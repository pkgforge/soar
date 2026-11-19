//! The blocking loop that turns messages into frames, and the thread that
//! feeds it terminal input.

use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, RecvTimeoutError, Sender},
    },
    time::{Duration, Instant},
};

use ratatui::crossterm::event;

use crate::{app::App, msg::Msg, runner::Runner, terminal::Session, ui};

/// How often the state receives a tick, driving spinners and timeouts.
const TICK: Duration = Duration::from_millis(100);

/// The most messages handled between two redraws.
const MAX_BATCH: usize = 256;

/// How long pausing waits for the reader to drain its buffers.
const PAUSE_HANDSHAKE: Duration = Duration::from_millis(250);

/// Reads terminal events in the background and sends them into the message
/// channel.
pub(crate) struct InputThread {
    paused: Arc<AtomicBool>,
    drained: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
}

impl InputThread {
    /// Starts reading terminal events into `tx` on a background thread.
    pub(crate) fn spawn(tx: Sender<Msg>) -> io::Result<InputThread> {
        let paused = Arc::new(AtomicBool::new(false));
        let drained = Arc::new(AtomicBool::new(true));
        let stop = Arc::new(AtomicBool::new(false));
        std::thread::Builder::new()
            .name("soar-tui-input".into())
            .spawn({
                let paused = Arc::clone(&paused);
                let drained = Arc::clone(&drained);
                let stop = Arc::clone(&stop);
                move || input_loop(tx, paused, drained, stop)
            })?;
        Ok(InputThread {
            paused,
            drained,
            stop,
        })
    }

    /// Stops reading keystrokes while a suspended program uses the
    /// terminal, after asking the reader to throw away what was already
    /// buffered, so the suspended program starts from a clean terminal
    /// instead of receiving keystrokes meant for the interface. The call
    /// waits up to `PAUSE_HANDSHAKE` for that drain and proceeds
    /// regardless, so a wedged reader cannot stall the interface.
    pub(crate) fn pause(&self) {
        self.drained.store(false, Ordering::Relaxed);
        self.paused.store(true, Ordering::Relaxed);
        let deadline = Instant::now() + PAUSE_HANDSHAKE;
        while !self.drained.load(Ordering::Relaxed) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    /// Resumes reading keystrokes after a suspended program has finished.
    pub(crate) fn resume(&self) {
        self.paused.store(false, Ordering::Relaxed);
    }

    /// Ends the background thread.
    pub(crate) fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Polls for terminal events until asked to stop, giving up the terminal
/// while paused so a suspended program keeps its keystrokes. Taking the
/// pause throws away whatever keystrokes were buffered before it, so the
/// suspended program starts from a clean terminal.
fn input_loop(
    tx: Sender<Msg>,
    paused: Arc<AtomicBool>,
    drained: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
) {
    while !stop.load(Ordering::Relaxed) {
        if paused.load(Ordering::Relaxed) {
            // The flag, not a memory of having paused before, gates the
            // drain: a pause landing while this thread sleeps must
            // drain again, or stale keystrokes reach the suspended
            // program and the handshake never completes.
            if !drained.load(Ordering::Relaxed) {
                while event::poll(Duration::ZERO).unwrap_or(false) {
                    if event::read().is_err() {
                        break;
                    }
                }
                drained.store(true, Ordering::Relaxed);
            }
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }
        match event::poll(Duration::from_millis(50)) {
            Ok(true) => {
                match event::read() {
                    Ok(ev) => {
                        if tx.send(Msg::Input(ev)).is_err() {
                            break;
                        }
                    }
                    Err(error) => {
                        // The runner and the other senders keep the
                        // channel alive, so a dead reader would go
                        // unnoticed and leave the interface deaf.
                        let _ = tx.send(Msg::InputFailed(error.to_string()));
                        break;
                    }
                }
            }
            Ok(false) => {}
            Err(error) => {
                let _ = tx.send(Msg::InputFailed(error.to_string()));
                break;
            }
        }
    }
}

/// Drives the interface: redraws when the state changed, applies arriving
/// messages in batches so a burst causes one redraw, and hands the requested
/// work to the runner. The channel sender stays unused until suspended
/// effects report back through it.
pub(crate) fn run_loop(
    session: &mut Session,
    app: &mut App,
    rx: &Receiver<Msg>,
    runner: &mut Runner,
    input: &InputThread,
    _tx: &Sender<Msg>,
) -> io::Result<()> {
    let mut last_tick = Instant::now();
    loop {
        if app.dirty {
            session.terminal.draw(|f| ui::render(f, app))?;
            app.dirty = false;
        }
        let timeout = TICK.saturating_sub(last_tick.elapsed());
        let first = match rx.recv_timeout(timeout) {
            Ok(msg) => Some(msg),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        };
        let mut effects = Vec::new();
        let now = Instant::now();
        if let Some(msg) = first {
            effects.extend(app.update(msg, now));
            for msg in rx.try_iter().take(MAX_BATCH) {
                effects.extend(app.update(msg, now));
            }
        }
        if last_tick.elapsed() >= TICK {
            effects.extend(app.update(Msg::Tick, now));
            last_tick = Instant::now();
        }
        for effect in effects {
            runner.execute(effect, session, input)?;
        }
        if app.quit {
            return Ok(());
        }
    }
}
