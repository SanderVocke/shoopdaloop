use colored::Colorize;
use lazy_static::lazy_static;
use std::collections::HashSet;
use std::sync::Mutex;
use tracing::{Event, Subscriber};
use tracing_log::NormalizeEvent;
use tracing_subscriber::filter::filter_fn;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

lazy_static! {
    static ref LOG_MODULES: Mutex<HashSet<&'static str>> = Mutex::new(HashSet::new());
}

pub fn register_log_module(name: &'static str) {
    if let Ok(mut modules) = LOG_MODULES.lock() {
        modules.insert(name);
    }
}

struct ShoopFmt;

impl<S, N> FormatEvent<S, N> for ShoopFmt
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> std::fmt::Result {
        let normalized_metadata = event.normalized_metadata();
        let metadata = normalized_metadata
            .as_ref()
            .unwrap_or_else(|| event.metadata());
        let level = metadata.level();
        let timestamp = humantime::format_rfc3339(std::time::SystemTime::now());

        let level_str = match *level {
            tracing::Level::TRACE => "TRACE".white(),
            tracing::Level::DEBUG => "DEBUG".blue(),
            tracing::Level::INFO => "INFO".green(),
            tracing::Level::WARN => "WARN".yellow(),
            tracing::Level::ERROR => "ERROR".red(),
        };

        let thread = std::thread::current();
        let thread_id = thread.id();
        let thread_str = match thread.name() {
            Some(name) => format!("{name} ({thread_id:?})"),
            None => format!("{thread_id:?}"),
        };

        write!(
            writer,
            "[{}] [{}] [{}] [{}] ",
            timestamp,
            thread_str,
            metadata.target().magenta(),
            level_str
        )?;

        ctx.format_fields(writer.by_ref(), event)?;
        writeln!(writer)
    }
}

pub fn init_logging() -> Result<(), anyhow::Error> {
    let env_filter = if let Ok(value) = std::env::var("SHOOP_LOG") {
        let mut filter_str = String::new();
        let modules = LOG_MODULES
            .lock()
            .map_err(|e| anyhow::anyhow!("Could not lock global modules list: {e:?}"))?;

        for item in value.split(',') {
            let mut parts = item.splitn(2, '=');
            let module_or_level = parts.next().unwrap_or_default();
            let level = parts.next();

            if !filter_str.is_empty() {
                filter_str.push(',');
            }

            match level {
                Some(level_str) if modules.contains(module_or_level) => {
                    filter_str.push_str(&format!("{module_or_level}={level_str}"));
                }
                Some(_) => filter_str.push_str(item),
                None => filter_str.push_str(module_or_level),
            }
        }
        EnvFilter::new(filter_str)
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
    };

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_ansi(true)
        .event_format(ShoopFmt)
        .with_writer(std::io::stdout)
        .with_filter(env_filter);
    let perfetto_layer = shoop_tracing::subscriber_layer().with_filter(filter_fn(|metadata| {
        (metadata.is_span() || metadata.is_event()) && shoop_tracing::is_tracing_enabled()
    }));

    tracing_subscriber::registry()
        .with(fmt_layer)
        .with(perfetto_layer)
        .init();
    let _ = tracing_log::LogTracer::init();

    Ok(())
}
