//! Turning Ctrl-C into a cancel request for the running operation.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use soar_utils::cancel::CancelToken;

/// What a Ctrl-C means at this point.
#[derive(Debug, PartialEq, Eq)]
enum OnInterrupt {
    /// Ask the operation to stop at its next safe point.
    Cancel,
    /// Quit now.
    Exit,
}

/// First press cancels. A second press, or any press after the operation is
/// over, exits.
fn on_interrupt(presses: u32, finished: bool) -> OnInterrupt {
    if presses == 1 && !finished {
        OnInterrupt::Cancel
    } else {
        OnInterrupt::Exit
    }
}

/// Turns Ctrl-C into cancelling `token` while the guard lives.
///
/// tokio never removes a signal handler once installed, so after the guard
/// drops, Ctrl-C exits at once instead of being ignored.
pub struct InterruptGuard {
    finished: Arc<AtomicBool>,
}

impl InterruptGuard {
    /// Cancels `token` on the first Ctrl-C until the returned guard drops.
    pub fn install(token: CancelToken) -> InterruptGuard {
        let finished = Arc::new(AtomicBool::new(false));
        tokio::spawn({
            let finished = Arc::clone(&finished);
            async move {
                let mut presses = 0u32;
                loop {
                    if tokio::signal::ctrl_c().await.is_err() {
                        return;
                    }
                    presses += 1;
                    match on_interrupt(presses, finished.load(Ordering::Relaxed)) {
                        OnInterrupt::Cancel => {
                            token.cancel();
                            if !crate::utils::json_enabled() {
                                crate::progress::suspend(|| {
                                    eprintln!("Aborting. Press Ctrl-C again to quit now.")
                                });
                            }
                        }
                        OnInterrupt::Exit => {
                            crate::progress::stop();
                            std::process::exit(130);
                        }
                    }
                }
            }
        });
        InterruptGuard {
            finished,
        }
    }
}

impl Drop for InterruptGuard {
    fn drop(&mut self) {
        self.finished.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_press_cancels() {
        assert_eq!(on_interrupt(1, false), OnInterrupt::Cancel);
    }

    #[test]
    fn a_second_press_exits() {
        assert_eq!(on_interrupt(2, false), OnInterrupt::Exit);
    }

    #[test]
    fn a_press_after_the_operation_is_over_exits() {
        assert_eq!(on_interrupt(1, true), OnInterrupt::Exit);
    }
}
