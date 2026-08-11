use colored::Colorize;
use lazy_static::lazy_static;
use std::cell::RefCell;
use std::collections::HashSet;
use std::fmt::Write as _;
use std::sync::Mutex;
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_log::NormalizeEvent;
use tracing_subscriber::filter::filter_fn;
use tracing_subscriber::fmt::format::{DefaultFields, Writer};
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

#[derive(Default)]
struct ShoopTracyConfig {
    fields: DefaultFields,
}

impl tracing_tracy::Config for ShoopTracyConfig {
    type Formatter = DefaultFields;

    fn formatter(&self) -> &Self::Formatter {
        &self.fields
    }

    fn format_fields_in_zone_name(&self) -> bool {
        false
    }
}

const TRACY_MESSAGE_MAX_BYTES: usize = u16::MAX as usize - 1;
const TRACY_ERROR_COLOR: u32 = 0xFF000000;

thread_local! {
    static TRACY_EVENT_MESSAGES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

struct ShoopTracyEventFieldVisitor<'a> {
    dest: &'a mut String,
    frame_mark: bool,
    has_fields: bool,
}

impl Visit for ShoopTracyEventFieldVisitor<'_> {
    fn record_bool(&mut self, field: &Field, value: bool) {
        match (value, field.name()) {
            (_, "tracy.frame_mark") => self.frame_mark = value,
            (true, _) => self.record_str(field, "true"),
            (false, _) => self.record_str(field, "false"),
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.dest.push_str(", ");
        self.dest.push_str(field.name());
        self.dest.push_str(" = ");
        self.dest.push_str(value);
        self.has_fields = true;
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let _ = write!(self.dest, ", {} = {value:?}", field.name());
        self.has_fields = true;
    }
}

#[derive(Clone)]
struct ShoopTracyEventLayer {
    client: tracy_client::Client,
}

impl ShoopTracyEventLayer {
    fn new() -> Self {
        Self {
            client: tracy_client::Client::start(),
        }
    }

    fn emit_message(&self, message: &str) {
        if message.len() < TRACY_MESSAGE_MAX_BYTES {
            self.client.message(message, 0);
            return;
        }

        let mut end = TRACY_MESSAGE_MAX_BYTES;
        while !message.is_char_boundary(end) {
            end -= 1;
        }
        self.client.color_message(
            "event message is too long and was truncated",
            TRACY_ERROR_COLOR,
            0,
        );
        self.client.message(&message[..end], 0);
    }
}

impl<S: Subscriber> Layer<S> for ShoopTracyEventLayer {
    fn on_event(&self, event: &Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        let normalized_metadata = event.normalized_metadata();
        let metadata = normalized_metadata
            .as_ref()
            .unwrap_or_else(|| event.metadata());

        let mut message = TRACY_EVENT_MESSAGES
            .with(|messages| messages.borrow_mut().pop())
            .unwrap_or_else(|| String::with_capacity(64));
        message.clear();
        let _ = write!(message, "log.level = {}", metadata.level());

        let mut visitor = ShoopTracyEventFieldVisitor {
            dest: &mut message,
            frame_mark: false,
            has_fields: false,
        };
        event.record(&mut visitor);
        let has_fields = visitor.has_fields;
        let frame_mark = visitor.frame_mark;
        drop(visitor);

        if has_fields {
            self.emit_message(&message);
        }
        if frame_mark {
            self.client.frame_mark();
        }

        TRACY_EVENT_MESSAGES.with(|messages| messages.borrow_mut().push(message));
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

    let tracing_enabled = crate::tracing_helpers::is_tracing_enabled();
    let tracy_span_layer = tracing_enabled.then(|| {
        tracing_tracy::TracyLayer::new(ShoopTracyConfig::default()).with_filter(filter_fn(
            |metadata| metadata.is_span() && crate::tracing_helpers::is_tracing_enabled(),
        ))
    });
    let tracy_event_layer = tracing_enabled.then(|| {
        ShoopTracyEventLayer::new().with_filter(filter_fn(|metadata| {
            metadata.is_event() && crate::tracing_helpers::is_tracing_enabled()
        }))
    });

    tracing_subscriber::registry()
        .with(fmt_layer)
        .with(tracy_span_layer)
        .with(tracy_event_layer)
        .init();
    let _ = tracing_log::LogTracer::init();

    Ok(())
}
