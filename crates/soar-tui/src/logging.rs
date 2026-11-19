//! Routes soar's log records into the interface instead of onto the screen.
//!
//! While the interface runs, nothing else may write to the terminal. A
//! subscriber formats each record as one plain line and hands it to a callback,
//! which feeds the interface's log.

use std::{io::Write, sync::Arc};

use tracing_subscriber::{filter::LevelFilter, fmt::MakeWriter};

use crate::TuiOptions;

/// The callback that receives each formatted log line.
type OnLine = Arc<dyn Fn(String) + Send + Sync>;

/// Creates the writer each log record is formatted into.
struct LineWriter {
    on_line: OnLine,
}

impl LineWriter {
    fn new(on_line: impl Fn(String) + Send + Sync + 'static) -> Self {
        Self {
            on_line: Arc::new(on_line),
        }
    }
}

impl<'a> MakeWriter<'a> for LineWriter {
    type Writer = LineBuffer;

    fn make_writer(&'a self) -> Self::Writer {
        LineBuffer {
            buffer: Vec::new(),
            on_line: Arc::clone(&self.on_line),
        }
    }
}

/// Collects the bytes of one record and hands each of its lines to the callback
/// once the record is fully formatted.
struct LineBuffer {
    buffer: Vec<u8>,
    on_line: OnLine,
}

impl Write for LineBuffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buffer.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Drop for LineBuffer {
    fn drop(&mut self) {
        let output = String::from_utf8_lossy(&self.buffer);
        for line in output.lines() {
            if !line.is_empty() {
                (self.on_line)(line.to_string());
            }
        }
    }
}

/// Installs the interface's log subscriber.
///
/// The level follows the command line: errors only when quiet, debug with one
/// `-v`, trace with two or more, and info otherwise. Lines are plain text whose
/// prefix names the level, so they can be shown in the interface's log.
/// Installing does nothing when a subscriber is already set.
pub(crate) fn init(options: &TuiOptions, on_line: impl Fn(String) + Send + Sync + 'static) {
    let level = if options.quiet {
        LevelFilter::ERROR
    } else if options.verbosity >= 2 {
        LevelFilter::TRACE
    } else if options.verbosity == 1 {
        LevelFilter::DEBUG
    } else {
        LevelFilter::INFO
    };

    let _ = tracing_subscriber::fmt()
        .with_ansi(false)
        .without_time()
        .with_target(false)
        .with_level(true)
        .with_max_level(level)
        .with_writer(LineWriter::new(on_line))
        .try_init();
}

#[cfg(test)]
mod tests {
    use std::{
        io::Write as _,
        sync::{Arc, Mutex},
    };

    use tracing_subscriber::fmt::MakeWriter as _;

    use super::LineWriter;

    #[test]
    fn lines_reach_the_callback_when_the_writer_drops() {
        let lines: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&lines);
        let writer = LineWriter::new(move |line| seen.lock().unwrap().push(line));
        let mut sink = writer.make_writer();
        sink.write_all(b"WARN a\nINFO b\n").unwrap();
        drop(sink);
        assert_eq!(*lines.lock().unwrap(), ["WARN a", "INFO b"]);
    }
}
