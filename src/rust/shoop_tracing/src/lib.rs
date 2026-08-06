//! Shared tracing gates and direct Tracy helpers.
//!
//! The application keeps Tracy disabled by default. Direct calls made from a
//! realtime callback are debugging-only and are enclosed in the narrowest
//! practical allocation-permitted scope. Tracy's C++ client may still allocate
//! or lock internally; tracing mode is therefore not realtime-safe.

use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(feature = "tracy")]
#[doc(hidden)]
pub use tracy_client;

static TRACING_ENABLED: AtomicBool = AtomicBool::new(false);
static TRACING_OUTPUT_ENABLED: AtomicBool = AtomicBool::new(true);
static ENGINE_DETAIL_ENABLED: AtomicBool = AtomicBool::new(false);

/// Enable or disable all application tracing.
pub fn set_tracing_enabled(enabled: bool) {
    TRACING_ENABLED.store(enabled, Ordering::Relaxed);
}

/// Temporarily allow or quiesce output while preserving the requested mode.
pub fn set_tracing_output_enabled(enabled: bool) {
    TRACING_OUTPUT_ENABLED.store(enabled, Ordering::Release);
}

/// Enable detailed per-node engine zones. This remains subordinate to tracing.
pub fn set_engine_detail_enabled(enabled: bool) {
    ENGINE_DETAIL_ENABLED.store(enabled, Ordering::Relaxed);
}

/// Whether tracing was explicitly requested, independent of capture quiescence.
pub fn is_tracing_requested() -> bool {
    TRACING_ENABLED.load(Ordering::Relaxed)
}

/// Whether ordinary tracing output may currently be emitted.
pub fn is_tracing_enabled() -> bool {
    TRACING_ENABLED.load(Ordering::Relaxed) && TRACING_OUTPUT_ENABLED.load(Ordering::Acquire)
}

/// Whether detailed engine tracing output may currently be emitted.
pub fn is_engine_detail_enabled() -> bool {
    is_tracing_enabled() && ENGINE_DETAIL_ENABLED.load(Ordering::Relaxed)
}

/// A direct Tracy span whose end operation receives the same narrow allocation
/// exception as its begin operation.
#[cfg(feature = "tracy")]
#[must_use]
pub struct RealtimeSpan {
    inner: Option<tracy_client::Span>,
}

#[cfg(not(feature = "tracy"))]
#[must_use]
pub struct RealtimeSpan;

impl RealtimeSpan {
    fn disabled() -> Self {
        #[cfg(feature = "tracy")]
        {
            Self { inner: None }
        }
        #[cfg(not(feature = "tracy"))]
        {
            Self
        }
    }

    /// Whether this wrapper entered the direct Tracy API.
    pub fn entered_tracy(&self) -> bool {
        #[cfg(feature = "tracy")]
        {
            self.inner.is_some()
        }
        #[cfg(not(feature = "tracy"))]
        {
            false
        }
    }
}

#[cfg(feature = "tracy")]
impl Drop for RealtimeSpan {
    fn drop(&mut self) {
        let Some(span) = self.inner.take() else {
            return;
        };
        assert_no_alloc::permit_alloc(|| drop(span));
    }
}

#[doc(hidden)]
pub fn disabled_realtime_span() -> RealtimeSpan {
    RealtimeSpan::disabled()
}

/// Start a direct Tracy span if its gate is active.
///
/// `location` is lazy so even the source-location cache is untouched on the
/// disabled path. The returned span closes through the same scoped exception.
#[cfg(feature = "tracy")]
#[doc(hidden)]
pub fn begin_realtime_span<F>(detailed: bool, location: F, value: Option<u64>) -> RealtimeSpan
where
    F: FnOnce() -> &'static tracy_client::SpanLocation,
{
    let enabled = if detailed {
        is_engine_detail_enabled()
    } else {
        is_tracing_enabled()
    };
    if !enabled {
        return RealtimeSpan::disabled();
    }

    assert_no_alloc::permit_alloc(|| {
        let Some(client) = tracy_client::Client::running() else {
            return RealtimeSpan::disabled();
        };
        let span = client.span(location(), 0);
        if let Some(value) = value {
            span.emit_value(value);
        }
        RealtimeSpan { inner: Some(span) }
    })
}

/// Initialize a static source location before driver activation when practical.
#[cfg(feature = "tracy")]
pub fn prewarm_realtime_location<F>(location: F)
where
    F: FnOnce() -> &'static tracy_client::SpanLocation,
{
    assert_no_alloc::permit_alloc(|| {
        let _ = location();
    });
}

/// Name and prewarm the current callback thread before driver activation when possible.
///
/// This deliberately runs only for explicit tracing sessions. Tracy may allocate while
/// naming the thread and creating its producer queue, so the operation is kept inside the
/// same narrow diagnostic allocation exception as direct realtime instrumentation.
#[cfg(feature = "tracy")]
pub fn prewarm_realtime_thread(name: &str) {
    if !is_tracing_requested() {
        return;
    }
    assert_no_alloc::permit_alloc(|| {
        if let Some(client) = tracy_client::Client::running() {
            client.set_thread_name(name);
            client.secondary_frame_mark(tracy_client::frame_name!("engine.prewarm"));
        }
    });
}

#[cfg(not(feature = "tracy"))]
pub fn prewarm_realtime_thread(_name: &str) {}

/// Emit a named callback frame mark when tracing output is active.
#[cfg(feature = "tracy")]
#[doc(hidden)]
pub fn emit_realtime_frame_mark(name: tracy_client::FrameName) {
    if !is_tracing_enabled() {
        return;
    }
    assert_no_alloc::permit_alloc(|| {
        if let Some(client) = tracy_client::Client::running() {
            client.secondary_frame_mark(name);
        }
    });
}

/// Create a coarse direct Tracy span guarded by the application tracing flags.
#[cfg(feature = "tracy")]
#[macro_export]
macro_rules! realtime_span {
    ($name:literal) => {
        $crate::begin_realtime_span(false, || $crate::tracy_client::span_location!($name), None)
    };
    ($name:literal, value = $value:expr) => {
        $crate::begin_realtime_span(
            false,
            || $crate::tracy_client::span_location!($name),
            Some($value as u64),
        )
    };
}

#[cfg(not(feature = "tracy"))]
#[macro_export]
macro_rules! realtime_span {
    ($name:literal) => {{
        let _ = $name;
        $crate::disabled_realtime_span()
    }};
    ($name:literal, value = $value:expr) => {{
        let _ = ($name, $value);
        $crate::disabled_realtime_span()
    }};
}

/// Create a detailed direct Tracy span guarded by the engine-detail flag.
#[cfg(feature = "tracy")]
#[macro_export]
macro_rules! realtime_span_detail {
    ($name:literal) => {
        $crate::begin_realtime_span(true, || $crate::tracy_client::span_location!($name), None)
    };
    ($name:literal, value = $value:expr) => {
        $crate::begin_realtime_span(
            true,
            || $crate::tracy_client::span_location!($name),
            Some($value as u64),
        )
    };
}

#[cfg(not(feature = "tracy"))]
#[macro_export]
macro_rules! realtime_span_detail {
    ($name:literal) => {{
        let _ = $name;
        $crate::disabled_realtime_span()
    }};
    ($name:literal, value = $value:expr) => {{
        let _ = ($name, $value);
        $crate::disabled_realtime_span()
    }};
}

/// Emit a secondary frame mark from a realtime callback.
#[cfg(feature = "tracy")]
#[macro_export]
macro_rules! realtime_frame_mark {
    ($name:literal) => {
        $crate::emit_realtime_frame_mark($crate::tracy_client::frame_name!($name))
    };
}

#[cfg(not(feature = "tracy"))]
#[macro_export]
macro_rules! realtime_frame_mark {
    ($name:literal) => {{
        let _ = $name;
    }};
}

/// Prewarm the source location used by a realtime span macro invocation.
#[cfg(feature = "tracy")]
#[macro_export]
macro_rules! prewarm_realtime_span {
    ($name:literal) => {
        $crate::prewarm_realtime_location(|| $crate::tracy_client::span_location!($name))
    };
}

#[cfg(not(feature = "tracy"))]
#[macro_export]
macro_rules! prewarm_realtime_span {
    ($name:literal) => {{
        let _ = $name;
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn gates_keep_detail_subordinate_and_quiesce_output() {
        let _guard = TEST_LOCK.lock().unwrap();
        set_tracing_enabled(false);
        set_tracing_output_enabled(true);
        set_engine_detail_enabled(true);
        assert!(!is_tracing_enabled());
        assert!(!is_engine_detail_enabled());

        set_tracing_enabled(true);
        assert!(is_tracing_enabled());
        assert!(is_engine_detail_enabled());

        set_tracing_output_enabled(false);
        assert!(!is_tracing_enabled());
        assert!(!is_engine_detail_enabled());

        set_tracing_output_enabled(true);
        set_engine_detail_enabled(false);
        assert!(is_tracing_enabled());
        assert!(!is_engine_detail_enabled());
        set_tracing_enabled(false);
    }

    #[test]
    fn disabled_span_does_not_enter_tracy() {
        let _guard = TEST_LOCK.lock().unwrap();
        set_tracing_enabled(false);
        set_tracing_output_enabled(true);
        let span = crate::realtime_span!("engine.rt.disabled_test");
        assert!(!span.entered_tracy());
    }

    #[cfg(feature = "tracy")]
    #[test]
    fn prewarm_reuses_the_static_location() {
        fn location() -> &'static tracy_client::SpanLocation {
            tracy_client::span_location!("engine.rt.prewarm_test")
        }

        let before = location() as *const tracy_client::SpanLocation;
        prewarm_realtime_location(location);
        let after = location() as *const tracy_client::SpanLocation;
        assert_eq!(before, after);
    }
}
