use std::sync::mpsc::{self, Receiver, Sender};

use crate::SoarEvent;

/// Trait for consuming events.
///
/// Each frontend provides its own implementation.
pub trait EventSink: Send + Sync {
    fn emit(&self, event: SoarEvent);
}

/// Channel-based event sink.
///
/// Sends events through a standard mpsc channel. The receiver end
/// can be polled by any consumer (GUI, test harness, etc.).
pub struct ChannelSink {
    sender: Sender<SoarEvent>,
}

impl ChannelSink {
    pub fn new() -> (Self, Receiver<SoarEvent>) {
        let (sender, receiver) = mpsc::channel();
        (
            Self {
                sender,
            },
            receiver,
        )
    }
}

impl EventSink for ChannelSink {
    fn emit(&self, event: SoarEvent) {
        let _ = self.sender.send(event);
    }
}

/// No-op event sink for tests or headless operation.
pub struct NullSink;

impl EventSink for NullSink {
    fn emit(&self, _event: SoarEvent) {}
}

/// Collector sink that stores all events for inspection.
///
/// Useful in tests to verify that expected events were emitted.
#[derive(Default)]
pub struct CollectorSink {
    events: std::sync::Mutex<Vec<SoarEvent>>,
}

impl CollectorSink {
    pub fn events(&self) -> Vec<SoarEvent> {
        self.events.lock().unwrap().clone()
    }

    pub fn len(&self) -> usize {
        self.events.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl EventSink for CollectorSink {
    fn emit(&self, event: SoarEvent) {
        self.events.lock().unwrap().push(event);
    }
}
