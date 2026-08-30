//! Shoop-owned tracing facade.
//!
//! Backend-specific Perfetto types stay private. Allocation-permitted code uses
//! the re-exported `tracing` macros and subscriber layer; realtime code uses the
//! statically named, explicitly gated direct helpers.

use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(not(target_arch = "wasm32"))]
use perfetto_everywhere_core::Tracer;
use perfetto_everywhere_core::{Category, Field, FieldName, FieldValue, StaticName, TrackId};
#[cfg(target_arch = "wasm32")]
use perfetto_everywhere_core::{FlowAttachment, TraceBackend};
#[cfg(target_arch = "wasm32")]
use std::{cell::RefCell, rc::Rc};

#[cfg(all(target_arch = "wasm32", feature = "ordinary-web"))]
mod browser;
#[cfg(not(target_arch = "wasm32"))]
pub mod capture;
#[cfg(all(target_arch = "wasm32", feature = "ordinary-web"))]
pub use browser::{
    append_bounded_browser_records, initialize_browser_tracing, wasm_test_trace_begin,
    wasm_test_trace_finish, wasm_test_trace_finish_failure, wasm_test_trace_finish_result,
    BrowserCalibration, BrowserCapture, BrowserHealth, BrowserMetadata, BrowserRealmData,
};
#[cfg(target_arch = "wasm32")]
mod raw;
#[cfg(target_arch = "wasm32")]
pub use raw::{RawProducerHealth, RawTraceProducer, TraceMetadata, REALTIME_METADATA};
#[cfg(not(target_arch = "wasm32"))]
mod test_capture;
#[cfg(not(target_arch = "wasm32"))]
pub use test_capture::{run_test, run_test_result};

pub use tracing::{
    debug, debug_span, error, error_span, event, field, info, info_span, instrument, span, trace,
    trace_span, warn, warn_span, Level,
};

static TRACING_ENABLED: AtomicBool = AtomicBool::new(false);
static TRACING_OUTPUT_ENABLED: AtomicBool = AtomicBool::new(true);
static ENGINE_DETAIL_ENABLED: AtomicBool = AtomicBool::new(false);

const REALTIME_CATEGORY: Category = Category::new("shoop.realtime");
#[cfg(target_arch = "wasm32")]
thread_local! {
    static RAW_BACKEND: RefCell<Option<Rc<perfetto_everywhere_raw::RawRingBackend>>> =
        const { RefCell::new(None) };
}
const VALUE_FIELD: FieldName = FieldName::new("value");

#[cfg(not(target_arch = "wasm32"))]
static NATIVE_TRACER: Tracer<perfetto_everywhere_native::NativeBackend> =
    Tracer::new(perfetto_everywhere_native::NativeBackend);

#[cfg(not(target_arch = "wasm32"))]
pub(crate) const COUNTER_NAMES: &[StaticName] = &[
    StaticName::new("engine.fx.bridge.midi_input_overflows"),
    StaticName::new("engine.fx.bridge.slot_occupancy"),
    StaticName::new("engine.fx.bridge.generation"),
    StaticName::new("engine.fx.bridge.deadline_misses"),
    StaticName::new("engine.fx.bridge.fallback_reason"),
    StaticName::new("engine.fx.global_midi.rejected"),
    StaticName::new("engine.fx.global_midi.pending_overwrites"),
    StaticName::new("engine.fx.global_midi.pending_drained"),
    StaticName::new("engine.fx.global_midi.capacity_deferrals"),
];

/// Enable or disable all application tracing.
pub fn set_tracing_enabled(enabled: bool) {
    TRACING_ENABLED.store(enabled, Ordering::Relaxed);
}

/// Temporarily allow or quiesce output while preserving the requested mode.
pub fn set_tracing_output_enabled(enabled: bool) {
    TRACING_OUTPUT_ENABLED.store(enabled, Ordering::Release);
}

/// Enable detailed per-node engine records. This remains subordinate to tracing.
pub fn set_engine_detail_enabled(enabled: bool) {
    ENGINE_DETAIL_ENABLED.store(enabled, Ordering::Relaxed);
}

/// Whether tracing was explicitly requested, independent of capture quiescence.
pub fn is_tracing_requested() -> bool {
    TRACING_ENABLED.load(Ordering::Relaxed)
}

/// Whether callback CPU clocks are available for auxiliary timing reports.
pub fn is_realtime_cpu_timing_enabled() -> bool {
    cfg!(not(target_arch = "wasm32")) && is_tracing_requested()
}

/// Whether ordinary tracing output may currently be emitted.
pub fn is_tracing_enabled() -> bool {
    TRACING_ENABLED.load(Ordering::Relaxed) && TRACING_OUTPUT_ENABLED.load(Ordering::Acquire)
}

/// Whether detailed engine tracing output may currently be emitted.
pub fn is_engine_detail_enabled() -> bool {
    is_tracing_enabled() && ENGINE_DETAIL_ENABLED.load(Ordering::Relaxed)
}

#[cfg(not(target_arch = "wasm32"))]
type NativeSpan =
    perfetto_everywhere_core::SpanGuard<'static, perfetto_everywhere_native::NativeBackend>;

/// A backend-neutral direct realtime span.
#[must_use]
pub struct RealtimeSpan {
    #[cfg(not(target_arch = "wasm32"))]
    inner: Option<NativeSpan>,
    #[cfg(target_arch = "wasm32")]
    backend: Option<Rc<perfetto_everywhere_raw::RawRingBackend>>,
    #[cfg(target_arch = "wasm32")]
    active: bool,
}

impl RealtimeSpan {
    fn disabled() -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            inner: None,
            #[cfg(target_arch = "wasm32")]
            backend: None,
            #[cfg(target_arch = "wasm32")]
            active: false,
        }
    }

    /// Whether this wrapper entered the active tracing backend.
    pub fn entered_tracing(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.inner.is_some()
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.active
        }
    }
}

#[doc(hidden)]
pub fn begin_realtime_span(detailed: bool, name: &'static str, value: Option<u64>) -> RealtimeSpan {
    let enabled = if detailed {
        is_engine_detail_enabled()
    } else {
        is_tracing_enabled()
    };
    if !enabled {
        return RealtimeSpan::disabled();
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        return assert_no_alloc::permit_alloc(|| {
            let value_field = value.map(|value| Field::new(VALUE_FIELD, FieldValue::U64(value)));
            let fields = value_field.as_slice();
            let span = NATIVE_TRACER.span(REALTIME_CATEGORY, StaticName::new(name), fields);
            span.status()
                .was_recorded()
                .then_some(RealtimeSpan { inner: Some(span) })
                .unwrap_or_else(RealtimeSpan::disabled)
        });
    }

    #[cfg(target_arch = "wasm32")]
    {
        RAW_BACKEND.with(|slot| {
            let Some(backend) = slot.borrow().as_ref().cloned() else {
                return RealtimeSpan::disabled();
            };
            let value_field = value.map(|value| Field::new(VALUE_FIELD, FieldValue::U64(value)));
            let fields = value_field.as_slice();
            let status = backend.span_begin(
                REALTIME_CATEGORY,
                StaticName::new(name),
                TrackId::CURRENT,
                fields,
                FlowAttachment::None,
            );
            RealtimeSpan {
                backend: Some(backend),
                active: status.was_recorded(),
            }
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for RealtimeSpan {
    fn drop(&mut self) {
        if let Some(span) = self.inner.take() {
            assert_no_alloc::permit_alloc(|| drop(span));
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for RealtimeSpan {
    fn drop(&mut self) {
        if self.active {
            if let Some(backend) = &self.backend {
                let _ = backend.span_end(TrackId::CURRENT);
            }
        }
    }
}

/// Pre-register a static realtime name before driver activation when practical.
pub fn prewarm_realtime_name(name: &'static str) {
    let _ = StaticName::new(name);
}

/// Name/prewarm the current callback producer before driver activation when possible.
pub fn prewarm_realtime_thread(_name: &str) {}

#[doc(hidden)]
pub fn emit_realtime_event(name: &'static str) {
    if !is_tracing_enabled() {
        return;
    }
    #[cfg(not(target_arch = "wasm32"))]
    assert_no_alloc::permit_alloc(|| {
        let _ = NATIVE_TRACER.event(REALTIME_CATEGORY, StaticName::new(name), &[]);
    });
    #[cfg(target_arch = "wasm32")]
    RAW_BACKEND.with(|slot| {
        if let Some(backend) = slot.borrow().as_ref() {
            let _ = backend.event(
                REALTIME_CATEGORY,
                StaticName::new(name),
                TrackId::CURRENT,
                &[],
                FlowAttachment::None,
            );
        }
    });
}

#[doc(hidden)]
pub fn emit_plot_i64(detailed: bool, name: &'static str, value: i64) {
    let enabled = if detailed {
        is_engine_detail_enabled()
    } else {
        is_tracing_enabled()
    };
    if !enabled {
        return;
    }
    #[cfg(not(target_arch = "wasm32"))]
    assert_no_alloc::permit_alloc(|| {
        let _ = NATIVE_TRACER.counter_i64(StaticName::new(name), TrackId::CURRENT, value);
    });
    #[cfg(target_arch = "wasm32")]
    RAW_BACKEND.with(|slot| {
        if let Some(backend) = slot.borrow().as_ref() {
            let _ = backend.counter_i64(StaticName::new(name), TrackId::CURRENT, value);
        }
    });
}

#[doc(hidden)]
pub fn emit_plot_f64(detailed: bool, name: &'static str, value: f64) {
    let enabled = if detailed {
        is_engine_detail_enabled()
    } else {
        is_tracing_enabled()
    };
    if !enabled {
        return;
    }
    #[cfg(not(target_arch = "wasm32"))]
    assert_no_alloc::permit_alloc(|| {
        let _ = NATIVE_TRACER.counter_f64(StaticName::new(name), TrackId::CURRENT, value);
    });
    #[cfg(target_arch = "wasm32")]
    RAW_BACKEND.with(|slot| {
        if let Some(backend) = slot.borrow().as_ref() {
            let _ = backend.counter_f64(StaticName::new(name), TrackId::CURRENT, value);
        }
    });
}

/// Build the native Perfetto compatibility layer without exposing its backend type.
#[cfg(not(target_arch = "wasm32"))]
pub fn subscriber_layer<S>() -> impl tracing_subscriber::Layer<S>
where
    S: tracing::Subscriber + for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
{
    perfetto_everywhere_tracing::PerfettoLayer::new(perfetto_everywhere_native::NativeBackend)
}

/// Create a coarse direct span guarded by application tracing flags.
#[macro_export]
macro_rules! realtime_span {
    ($name:literal) => {
        $crate::begin_realtime_span(false, $name, None)
    };
    ($name:literal, value = $value:expr) => {
        $crate::begin_realtime_span(false, $name, Some($value as u64))
    };
}

/// Create a detailed direct span guarded by the engine-detail flag.
#[macro_export]
macro_rules! realtime_span_detail {
    ($name:literal) => {
        $crate::begin_realtime_span(true, $name, None)
    };
    ($name:literal, value = $value:expr) => {
        $crate::begin_realtime_span(true, $name, Some($value as u64))
    };
}

/// Emit a detailed i64 counter plot from a realtime callback.
#[macro_export]
macro_rules! realtime_plot_i64_detail {
    ($name:literal, $value:expr) => {
        $crate::emit_plot_i64(true, $name, $value as i64)
    };
}

/// Emit a detailed f64 counter plot from a realtime callback.
#[macro_export]
macro_rules! realtime_plot_f64_detail {
    ($name:literal, $value:expr) => {
        $crate::emit_plot_f64(true, $name, $value as f64)
    };
}

/// Compatibility alias while integer/floating callsites are made explicit.
#[macro_export]
macro_rules! realtime_plot_detail {
    ($name:literal, $value:expr) => {
        $crate::emit_plot_f64(true, $name, $value as f64)
    };
}

/// Emit an instant callback-boundary event.
#[macro_export]
macro_rules! realtime_frame_mark {
    ($name:literal) => {
        $crate::emit_realtime_event($name)
    };
}

/// Pre-register the static name used by a realtime span invocation.
#[macro_export]
macro_rules! prewarm_realtime_span {
    ($name:literal) => {
        $crate::prewarm_realtime_name($name)
    };
}
