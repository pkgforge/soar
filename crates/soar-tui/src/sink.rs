//! Bridges soar's operations into the interface's message channel.

use std::sync::mpsc::Sender;

use soar_events::{EventSink, SoarEvent};

use crate::msg::Msg;

/// Forwards soar events into the interface's message channel.
pub(crate) struct TuiSink(Sender<Msg>);

impl TuiSink {
    /// Creates the sink that delivers events to `tx`.
    pub(crate) fn new(tx: Sender<Msg>) -> Self {
        Self(tx)
    }
}

impl EventSink for TuiSink {
    fn emit(&self, event: SoarEvent) {
        let _ = self.0.send(Msg::Soar(event));
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use soar_events::{EventSink as _, LogLevel, SoarEvent};

    use super::TuiSink;
    use crate::msg::Msg;

    #[test]
    fn emitted_events_arrive_as_soar_messages() {
        let (tx, rx) = mpsc::channel();
        let sink = TuiSink::new(tx);
        sink.emit(SoarEvent::Log {
            level: LogLevel::Info,
            message: "sync finished".into(),
        });
        assert!(matches!(rx.recv(), Ok(Msg::Soar(_))));
    }
}
