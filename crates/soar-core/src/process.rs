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
    thread,
    time::Duration,
};

use soar_events::{EventSinkHandle, OperationId, SoarEvent};
use soar_utils::cancel::CancelToken;

use crate::{
    SoarResult,
    error::{ErrorContext as _, SoarError},
};

/// How often the running child is polled for exit and cancellation.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

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
        if let Some(out) = child.stdout.take() {
            let events = events.clone();
            scope.spawn(move || read_lines(out, events, op_id, pkg_name));
        }
        if let Some(err) = child.stderr.take() {
            scope.spawn(move || read_lines(err, events, op_id, pkg_name));
        }

        loop {
            let status = child
                .try_wait()
                .with_context(|| format!("waiting for {program}"))?;
            if let Some(status) = status {
                return Ok(status);
            }
            if ctx.cancel.is_cancelled() {
                // The child leads its own process group, so the signal takes
                // its own children down with it and the pipes close. A group
                // that already exited fails the call, which is fine here.
                let group = -(child.id() as i32);
                unsafe {
                    nix::libc::kill(group, nix::libc::SIGKILL);
                }
                let _ = child.wait();
                return Err(SoarError::Cancelled);
            }
            thread::sleep(POLL_INTERVAL);
        }
    })
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
