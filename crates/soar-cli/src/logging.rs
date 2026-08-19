use nu_ansi_term::Color::{Blue, Magenta, Red, Yellow};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::{
    fmt::{
        self,
        format::{FmtSpan, Writer},
        FmtContext, FormatEvent, FormatFields, MakeWriter,
    },
    registry::LookupSpan,
};

use crate::{cli::Args, utils::Colored};

/// Collects an event's message and the fields recorded alongside it.
///
/// The fields are what say which repository or database a record is about, so a
/// log that drops them leaves every repetition of a message looking the same.
#[derive(Default)]
struct MessageVisitor {
    message: Option<String>,
    fields: Vec<(&'static str, String)>,
}

impl MessageVisitor {
    fn record(&mut self, field: &tracing::field::Field, value: String) {
        if field.name() == "message" {
            self.message = Some(value);
        } else {
            self.fields.push((field.name(), value));
        }
    }
}

impl tracing::field::Visit for MessageVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.record(field, value.to_string());
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.record(field, format!("{value:?}"));
    }
}

pub struct CustomFormatter;

impl<S, N> FormatEvent<S, N> for CustomFormatter
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        _: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> std::fmt::Result {
        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);

        let level = *event.metadata().level();
        match level {
            Level::TRACE => write!(writer, "{} ", Colored(Magenta, "[TRACE]")),
            Level::DEBUG => write!(writer, "{} ", Colored(Blue, "[DEBUG]")),
            Level::INFO => write!(writer, ""),
            Level::WARN => write!(writer, "{} ", Colored(Yellow, "[WARN]")),
            Level::ERROR => write!(writer, "{} ", Colored(Red, "[ERROR]")),
        }?;

        if let Some(message) = visitor.message {
            write!(writer, "{message}")?;
        }

        // Info is soar's own output, where the fields carry what `--json` prints
        // rather than anything a reader of the line needs appended to it.
        if level != Level::INFO {
            for (name, value) in visitor.fields {
                write!(writer, " {}={value}", Colored(Blue, name))?;
            }
        }

        writeln!(writer)
    }
}

/// Chooses which stream log records go to.
///
/// Info goes to stdout and the rest to stderr, unless soar is emitting an
/// event stream, in which case stdout carries only that.
struct WriterBuilder {
    logs_to_stderr: bool,
}

impl WriterBuilder {
    fn new(logs_to_stderr: bool) -> Self {
        Self {
            logs_to_stderr,
        }
    }
}

/// A writer that buffers output and prints it properly, suspending the progress
/// display to avoid interfering with progress rendering.
struct SuspendingWriter {
    buffer: Vec<u8>,
    use_stderr: bool,
}

impl SuspendingWriter {
    fn new(use_stderr: bool) -> Self {
        Self {
            buffer: Vec::new(),
            use_stderr,
        }
    }
}

impl std::io::Write for SuspendingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buffer.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Drop for SuspendingWriter {
    fn drop(&mut self) {
        if self.buffer.is_empty() {
            return;
        }

        let output = String::from_utf8_lossy(&self.buffer);
        // Remove trailing newline since println adds one
        let output = output.trim_end_matches('\n');

        let use_stderr = self.use_stderr;
        let output = output.to_string();
        crate::progress::suspend(|| {
            if use_stderr {
                eprintln!("{}", output);
            } else {
                println!("{}", output);
            }
        });
    }
}

impl<'a> MakeWriter<'a> for WriterBuilder {
    type Writer = SuspendingWriter;

    fn make_writer(&'a self) -> Self::Writer {
        SuspendingWriter::new(self.logs_to_stderr)
    }

    fn make_writer_for(&'a self, meta: &tracing::Metadata<'_>) -> Self::Writer {
        SuspendingWriter::new(self.logs_to_stderr || meta.level() != &tracing::Level::INFO)
    }
}

pub fn setup_logging(args: &Args) {
    let filter_level = if args.quiet {
        Level::ERROR
    } else if args.verbose >= 2 {
        Level::TRACE
    } else if args.verbose == 1 {
        Level::DEBUG
    } else {
        Level::INFO
    };

    let builder = fmt::Subscriber::builder()
        .with_env_filter(format!("soar={filter_level}"))
        .with_target(false)
        .with_thread_ids(false)
        .with_thread_names(false)
        .with_file(false)
        .with_line_number(false)
        .with_span_events(FmtSpan::NONE)
        .with_writer(WriterBuilder::new(args.json))
        .compact()
        .without_time();

    let subscriber: Box<dyn Subscriber + Send + Sync> = if args.json {
        Box::new(builder.json().flatten_event(true).finish())
    } else {
        Box::new(builder.event_format(CustomFormatter).finish())
    };

    tracing::subscriber::set_global_default(subscriber).expect("Failed to set tracing subscriber");
}
