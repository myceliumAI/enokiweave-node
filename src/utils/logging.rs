use std::fmt;
use tracing::{Level, Subscriber};
use tracing_subscriber::{
    fmt::{
        self as fmt_subscriber,
        format::Writer,
        time::FormatTime,
        FmtContext, FormatEvent, FormatFields,
    },
    registry::LookupSpan,
    EnvFilter,
};

/// Custom event formatter that adds component names with colors
pub struct ComponentFormatter {
    timer: fmt_subscriber::time::SystemTime,
}

impl ComponentFormatter {
    pub fn new() -> Self {
        Self {
            timer: fmt_subscriber::time::SystemTime,
        }
    }
}

impl<S, N> FormatEvent<S, N> for ComponentFormatter
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &tracing::Event<'_>,
    ) -> fmt::Result {
        // Write timestamp
        self.timer.format_time(&mut writer)?;
        writer.write_char(' ')?;

        // Write log level with color
        let level = *event.metadata().level();
        match level {
            Level::ERROR => write!(writer, "\x1b[31mERROR\x1b[0m")?,
            Level::WARN => write!(writer, "\x1b[33mWARN\x1b[0m")?,
            Level::INFO => write!(writer, "\x1b[32mINFO\x1b[0m")?,
            Level::DEBUG => write!(writer, "\x1b[34mDEBUG\x1b[0m")?,
            Level::TRACE => write!(writer, "\x1b[90mTRACE\x1b[0m")?,
        }
        writer.write_char(' ')?;

        // Write component name in cyan
        if let Some(target) = event.metadata().target().split("::").last() {
            write!(writer, "\x1b[36m[{}]\x1b[0m ", target)?;
        }

        // Write the log message
        ctx.field_format().format_fields(writer.by_ref(), event)?;
        writeln!(writer)
    }
}

/// Initialize logging for a specific component
pub fn init_logging(component_name: Option<&str>) {
    let filter = match component_name {
        Some(name) => format!("info,{}=debug,enokiweave=debug", name),
        None => "info,enokiweave=debug".to_string(),
    };

    let env_filter = EnvFilter::try_new(&filter).unwrap();

    let subscriber = fmt_subscriber::Subscriber::builder()
        .with_target(true)  // Enable target (component name) display
        .with_thread_ids(false)
        .with_thread_names(false)
        .with_file(false)
        .with_line_number(false)
        .with_env_filter(env_filter)
        .event_format(ComponentFormatter::new())
        .try_init();

    if subscriber.is_err() {
        eprintln!("Warning: Failed to initialize logging, it might already be initialized");
    }
}

/// Get a logger for a specific component
#[macro_export]
macro_rules! get_logger {
    ($component:expr) => {{
        tracing::debug!(target: $component, "Logger initialized");
        $component
    }};
} 