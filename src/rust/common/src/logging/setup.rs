use anyhow;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};
use std::collections::HashSet;
use std::sync::Mutex;
use lazy_static::lazy_static;

lazy_static! {
    static ref LOG_MODULES: Mutex<HashSet<&'static str>> = Mutex::new(HashSet::new());
}

pub fn register_log_module(name: &'static str) {
    let mut modules = LOG_MODULES.lock().unwrap();
    modules.insert(name);
}

use colored::Colorize;
use tracing::{Event, Subscriber};
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields};
use tracing_subscriber::registry::LookupSpan;

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
        let metadata = event.metadata();
        let level = metadata.level();
        let timestamp = humantime::format_rfc3339(std::time::SystemTime::now());

        let level_str = match *level {
            tracing::Level::TRACE => "TRACE".white(),
            tracing::Level::DEBUG => "DEBUG".blue(),
            tracing::Level::INFO => "INFO".green(),
            tracing::Level::WARN => "WARN".yellow(),
            tracing::Level::ERROR => "ERROR".red(),
        };

        write!(
            writer,
            "[{}] [{}] [{}] ",
            timestamp,
            metadata.target().magenta(),
            level_str
        )?;

        ctx.format_fields(writer.by_ref(), event)?;

        writeln!(writer)
    }
}

pub fn init_logging() -> Result<(), anyhow::Error> {
    let mut env_filter = EnvFilter::from_default_env();
    if let Ok(value) = std::env::var("SHOOP_LOG") {
        let mut filter_str = String::new();
        let modules = LOG_MODULES.lock().unwrap();
        
        for item in value.split(',') {
            let mut parts = item.splitn(2, '=');
            let module_or_level = parts.next().unwrap_or_default();
            let level = parts.next();
            
            if !filter_str.is_empty() {
                filter_str.push(',');
            }
            
            match level {
                Some(level_str) => {
                    if modules.contains(module_or_level) {
                        filter_str.push_str(&format!("{}={}", module_or_level, level_str));
                    } else {
                        // If it's not a registered module, we'll try to use it as a raw tracing filter entry
                        filter_str.push_str(item);
                    }
                }
                None => {
                    // Could be a global level or a module with default level
                    filter_str.push_str(module_or_level);
                }
            }
        }
        env_filter = EnvFilter::new(filter_str);
    }

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_ansi(true)
        .event_format(ShoopFmt)
        .with_writer(std::io::stdout)
        .with_filter(env_filter);

    let registry = tracing_subscriber::registry().with(fmt_layer);

    let registry = if crate::tracing_helpers::is_tracing_enabled() {
        registry.with(Some(tracing_tracy::TracyLayer::default()))
    } else {
        registry.with(None::<tracing_tracy::TracyLayer>)
    };

    registry.init();

    Ok(())
}
