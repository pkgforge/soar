//! Cooperative cancellation.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// A shared flag that asks running work to stop at its next safe point.
///
/// Clones share the flag. Cancelling cannot be undone; start new work with
/// a new token.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    /// A token that is not cancelled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Asks every holder of this token to stop.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    /// Whether [`cancel`](Self::cancel) has been called on any clone.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clone_sees_the_cancel() {
        let token = CancelToken::new();
        let shared = token.clone();
        assert!(!shared.is_cancelled());
        token.cancel();
        assert!(shared.is_cancelled());
    }

    #[test]
    fn a_fresh_token_is_not_cancelled() {
        assert!(!CancelToken::new().is_cancelled());
        assert!(!CancelToken::default().is_cancelled());
    }
}
