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

#[derive(Default)]
struct MessageVisitor {
    message: Option<String>,
}

impl tracing::field::Visit for MessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = Some(format!("{value:?}"));
        }
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

        match *event.metadata().level() {
            Level::TRACE => write!(writer, "{} ", Colored(Magenta, "[TRACE]")),
            Level::DEBUG => write!(writer, "{} ", Colored(Blue, "[DEBUG]")),
            Level::INFO => write!(writer, ""),
            Level::WARN => write!(writer, "{} ", Colored(Yellow, "[WARN]")),
            Level::ERROR => write!(writer, "{} ", Colored(Red, "[ERROR]")),
        }?;

        if let Some(message) = visitor.message {
            writeln!(writer, "{message}")
        } else {
            writeln!(writer)
        }
    }
}

struct WriterBuilder {
    stdout: std::io::Stdout,
    stderr: std::io::Stderr,
}

impl WriterBuilder {
    fn new() -> Self {
        Self {
            stdout: std::io::stdout(),
            stderr: std::io::stderr(),
        }
    }
}

impl<'a> MakeWriter<'a> for WriterBuilder {
    type Writer = Box<dyn std::io::Write + 'a>;

    fn make_writer(&'a self) -> Self::Writer {
        Box::new(self.stdout.lock())
    }

    fn make_writer_for(&'a self, meta: &tracing::Metadata<'_>) -> Self::Writer {
        if meta.level() == &tracing::Level::INFO {
            Box::new(self.stdout.lock())
        } else {
            Box::new(self.stderr.lock())
        }
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
        .with_writer(WriterBuilder::new())
        .compact()
        .without_time();

    let subscriber: Box<dyn Subscriber + Send + Sync> = if args.json {
        Box::new(builder.json().flatten_event(true).finish())
    } else {
        Box::new(builder.event_format(CustomFormatter).finish())
    };

    tracing::subscriber::set_global_default(subscriber).expect("Failed to set tracing subscriber");
}
