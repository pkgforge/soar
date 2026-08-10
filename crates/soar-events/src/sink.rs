use std::{
    io::{self, Write},
    sync::{
        mpsc::{self, Receiver, Sender},
        Mutex,
    },
};

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
    events: Mutex<Vec<SoarEvent>>,
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

/// Writes each event as one JSON object per line.
///
/// A line is a complete event, so a reader needs no closing bracket and a
/// stream cut short still parses up to the last full line.
pub struct JsonLinesSink<W: Write + Send + Sync> {
    writer: Mutex<W>,
}

impl<W: Write + Send + Sync> JsonLinesSink<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer: Mutex::new(writer),
        }
    }
}

impl JsonLinesSink<io::Stdout> {
    /// Writes to stdout, where a frontend expects the stream.
    pub fn stdout() -> Self {
        Self::new(io::stdout())
    }
}

impl JsonLinesSink<io::Stderr> {
    /// Writes beside the answer, for a command whose stdout carries one JSON
    /// document.
    pub fn stderr() -> Self {
        Self::new(io::stderr())
    }
}

impl<W: Write + Send + Sync> EventSink for JsonLinesSink<W> {
    fn emit(&self, event: SoarEvent) {
        let Ok(line) = serde_json::to_string(&event) else {
            return;
        };
        let Ok(mut writer) = self.writer.lock() else {
            return;
        };
        // Flushed per event: a frontend needs it now, not when the buffer fills.
        let _ = writeln!(writer, "{line}");
        let _ = writer.flush();
    }
}
