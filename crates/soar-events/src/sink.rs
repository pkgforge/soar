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

/// Writes each event as one JSON object per line.
///
/// This is the shape a frontend driving soar over a pipe reads: a line is a
/// complete event, so a reader never has to buffer for a closing bracket, and
/// a stream cut short mid-operation still parses up to the last full line.
pub struct JsonLinesSink<W: std::io::Write + Send + Sync> {
    writer: std::sync::Mutex<W>,
}

impl<W: std::io::Write + Send + Sync> JsonLinesSink<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer: std::sync::Mutex::new(writer),
        }
    }
}

impl JsonLinesSink<std::io::Stdout> {
    /// A sink writing to stdout, which is where a frontend expects the stream.
    pub fn stdout() -> Self {
        Self::new(std::io::stdout())
    }
}

impl JsonLinesSink<std::io::Stderr> {
    /// A sink writing beside the answer rather than into it.
    ///
    /// A command answering with one JSON document cannot carry a stream on the
    /// same output, since a reader expecting a document would find a second
    /// thing after it.
    pub fn stderr() -> Self {
        Self::new(std::io::stderr())
    }
}

impl<W: std::io::Write + Send + Sync> EventSink for JsonLinesSink<W> {
    fn emit(&self, event: SoarEvent) {
        let Ok(line) = serde_json::to_string(&event) else {
            return;
        };
        let Ok(mut writer) = self.writer.lock() else {
            return;
        };
        // Flushed per event: a frontend rendering progress needs it now, not
        // when the buffer happens to fill.
        let _ = writeln!(writer, "{line}");
        let _ = writer.flush();
    }
}
