//! Running package hooks and build commands.
//!
//! A hook or build command used to inherit the terminal, so its output went
//! straight to the screen and its stdin belonged to the user. Running it here
//! instead delivers each printed line as an event tied to the package, and
//! lets an operation abort the child.

use std::{
    io::{BufRead as _, BufReader},
    os::unix::process::CommandExt as _,
    process::{Command, ExitStatus, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use soar_events::{EventSinkHandle, OperationId, SoarEvent};
use soar_utils::cancel::CancelToken;

use crate::{
    SoarResult,
    error::{ErrorContext as _, SoarError},
};

/// How often the running child is polled for exit and cancellation.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// How long the readers may keep draining after the child has exited. The
/// command's own output is worth waiting for, but a process it daemonized
/// cannot hold the whole operation hostage over its leftover pipes.
const READER_GRACE: Duration = Duration::from_secs(10);

/// Where a child's output goes, and what can stop it.
pub struct ProcessContext<'a> {
    /// Sink the printed lines are reported through.
    pub events: &'a EventSinkHandle,
    /// Operation the child belongs to.
    pub op_id: OperationId,
    /// Name of the package the child works for.
    pub pkg_name: &'a str,
    /// Flag that asks the child to be stopped.
    pub cancel: &'a CancelToken,
}

/// Runs `cmd` to completion, reporting each line it prints as a
/// [`SoarEvent::ProcessOutput`].
///
/// The child gets no input. If `cancel` fires, the child is killed and
/// [`SoarError::Cancelled`] is returned.
pub fn run(mut cmd: Command, ctx: &ProcessContext) -> SoarResult<ExitStatus> {
    // The command must never start under a token that already fired, or a
    // fast command could finish before the first poll sees the cancellation.
    if ctx.cancel.is_cancelled() {
        return Err(SoarError::Cancelled);
    }

    let program = cmd.get_program().to_string_lossy().into_owned();
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // A group of its own lets a cancel take the command's own children
        // down with it, so the pipes close and the readers finish.
        .process_group(0);

    let mut child = cmd.spawn().with_context(|| format!("starting {program}"))?;

    let events = ctx.events.clone();
    let op_id = ctx.op_id;
    let pkg_name = ctx.pkg_name;

    thread::scope(|scope| {
        // Each reader marks its stream done, so the drain after a normal
        // exit can tell a closed pipe from one a leftover process still
        // holds without blocking on the join.
        let out_done = Arc::new(AtomicBool::new(false));
        let err_done = Arc::new(AtomicBool::new(false));
        if let Some(out) = child.stdout.take() {
            let events = events.clone();
            let done = Arc::clone(&out_done);
            scope.spawn(move || {
                read_lines(out, events, op_id, pkg_name);
                done.store(true, Ordering::Release);
            });
        } else {
            out_done.store(true, Ordering::Release);
        }
        if let Some(err) = child.stderr.take() {
            let done = Arc::clone(&err_done);
            scope.spawn(move || {
                read_lines(err, events, op_id, pkg_name);
                done.store(true, Ordering::Release);
            });
        } else {
            err_done.store(true, Ordering::Release);
        }

        loop {
            let status = child
                .try_wait()
                .with_context(|| format!("waiting for {program}"))?;
            if let Some(status) = status {
                // The child is gone, but a process it left behind may still
                // hold the pipes. Keep draining for a bounded grace, with
                // the token still live, so the command's own output is kept
                // without a daemonized leftover holding the operation
                // hostage. Killing the group closes the pipes and lets the
                // readers finish.
                let deadline = Instant::now() + READER_GRACE;
                let mut cancelled = false;
                while !out_done.load(Ordering::Acquire) || !err_done.load(Ordering::Acquire) {
                    if ctx.cancel.is_cancelled() {
                        cancelled = true;
                    }
                    if cancelled || Instant::now() >= deadline {
                        kill_group(child.id());
                        break;
                    }
                    thread::sleep(POLL_INTERVAL);
                }
                if cancelled {
                    return Err(SoarError::Cancelled);
                }
                return Ok(status);
            }
            if ctx.cancel.is_cancelled() {
                kill_group(child.id());
                let _ = child.wait();
                return Err(SoarError::Cancelled);
            }
            thread::sleep(POLL_INTERVAL);
        }
    })
}

/// Kills the process group the child led.
///
/// The child leads its own group, so the signal takes its own children down
/// with it and the pipes close. A group that already exited fails the call,
/// which is fine here.
fn kill_group(pid: u32) {
    let group = -(pid as i32);
    unsafe {
        nix::libc::kill(group, nix::libc::SIGKILL);
    }
}

/// Emits each line of `stream` as [`SoarEvent::ProcessOutput`].
///
/// Bytes that are not UTF-8 are replaced rather than dropping the line, and
/// a carriage return left behind by a CRLF ending is trimmed.
fn read_lines<R: std::io::Read>(
    stream: R,
    events: EventSinkHandle,
    op_id: OperationId,
    pkg_name: &str,
) {
    for chunk in BufReader::new(stream).split(b'\n') {
        let Ok(chunk) = chunk else {
            break;
        };
        let line = String::from_utf8_lossy(&chunk);
        events.emit(SoarEvent::ProcessOutput {
            op_id,
            pkg_name: pkg_name.to_string(),
            line: line.trim_end_matches('\r').to_string(),
        });
    }
}

#[cfg(test)]
mod tests {
    use std::{process::Command, sync::Arc, time::Instant};

    use soar_events::{CollectorSink, EventSinkHandle, NullSink, SoarEvent};
    use soar_utils::cancel::CancelToken;

    use super::*;

    fn context<'a>(events: &'a EventSinkHandle, cancel: &'a CancelToken) -> ProcessContext<'a> {
        ProcessContext {
            events,
            op_id: 1,
            pkg_name: "probe",
            cancel,
        }
    }

    fn shell(script: &str) -> Command {
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(script);
        cmd
    }

    fn output_lines(events: &CollectorSink) -> Vec<String> {
        events
            .events()
            .into_iter()
            .filter_map(|event| {
                match event {
                    SoarEvent::ProcessOutput {
                        line, ..
                    } => Some(line),
                    _ => None,
                }
            })
            .collect()
    }

    #[test]
    fn both_streams_arrive_as_lines() {
        let collector = Arc::new(CollectorSink::default());
        let events: EventSinkHandle = collector.clone();
        let cancel = CancelToken::new();
        let ctx = context(&events, &cancel);

        let status = run(shell("echo a; echo b >&2"), &ctx).unwrap();

        assert!(status.success());
        let mut lines = output_lines(&collector);
        lines.sort();
        assert_eq!(lines, vec!["a", "b"]);
    }

    #[test]
    fn an_already_cancelled_command_is_never_started() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("marker");
        let events: EventSinkHandle = Arc::new(NullSink);
        let cancel = CancelToken::new();
        cancel.cancel();
        let ctx = context(&events, &cancel);

        let result = run(shell(&format!("touch {}", marker.display())), &ctx);

        assert!(matches!(result, Err(SoarError::Cancelled)));
        assert!(!marker.exists(), "the command ran despite the cancellation");
    }

    #[test]
    fn a_cancelled_child_is_killed_and_reports_cancelled() {
        let events: EventSinkHandle = Arc::new(NullSink);
        let cancel = CancelToken::new();
        let ctx = context(&events, &cancel);

        let canceller = cancel.clone();
        let timer = thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            canceller.cancel();
        });

        let start = Instant::now();
        let result = run(shell("sleep 5"), &ctx);
        timer.join().unwrap();
        let elapsed = start.elapsed();

        assert!(matches!(result, Err(SoarError::Cancelled)));
        assert!(
            elapsed < Duration::from_secs(1),
            "cancellation took {elapsed:?}"
        );
    }

    #[test]
    #[cfg(unix)]
    fn a_daemonized_leftover_cannot_hold_a_finished_child_hostage() {
        let collector = Arc::new(CollectorSink::default());
        let events: EventSinkHandle = collector.clone();
        let cancel = CancelToken::new();
        let ctx = context(&events, &cancel);

        // The child itself exits at once, but the sleep it daemonized keeps
        // the pipes open. Cancelling after the start has to be seen by the
        // drain that follows the exit, since the poll loop is already done.
        let canceller = cancel.clone();
        let timer = thread::spawn(move || {
            thread::sleep(Duration::from_millis(300));
            canceller.cancel();
        });

        let start = Instant::now();
        let result = run(shell("echo hi; sleep 300 &"), &ctx);
        timer.join().unwrap();
        let elapsed = start.elapsed();

        assert!(matches!(result, Err(SoarError::Cancelled)));
        assert!(
            elapsed < Duration::from_secs(5),
            "a leftover holding the pipes stalled the call for {elapsed:?}"
        );
        assert!(output_lines(&collector).iter().any(|line| line == "hi"));
    }

    #[test]
    fn a_child_reading_stdin_finishes() {
        let events: EventSinkHandle = Arc::new(NullSink);
        let cancel = CancelToken::new();
        let ctx = context(&events, &cancel);

        let status = run(shell("read x; echo got"), &ctx).unwrap();

        assert!(status.success());
    }

    #[test]
    fn a_non_utf8_line_still_arrives() {
        let collector = Arc::new(CollectorSink::default());
        let events: EventSinkHandle = collector.clone();
        let cancel = CancelToken::new();
        let ctx = context(&events, &cancel);

        let status = run(shell("printf 'b\\377d\\n'"), &ctx).unwrap();

        assert!(status.success());
        assert_eq!(output_lines(&collector), ["b\u{fffd}d"]);
    }
}
