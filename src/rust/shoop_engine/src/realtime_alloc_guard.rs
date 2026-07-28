//! Runtime allocation guard for realtime processing.
//!
//! The guard is intentionally opt-in for the application: installing
//! [`assert_no_alloc::AllocDisabler`] as the process global allocator is the
//! executable's responsibility, then [`set_enabled`] controls whether engine
//! process calls are wrapped. This keeps normal developer runs unchanged while
//! allowing the CLI to turn allocation aborts on for realtime callbacks.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Once;

static ENABLED: AtomicBool = AtomicBool::new(false);

/// Enable or disable the realtime allocation guard.
///
/// This only has an effect in binaries/tests that installed
/// `assert_no_alloc::AllocDisabler` as their global allocator. In release builds
/// the upstream crate's default `disable_release` feature makes these checks a
/// no-op.
pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}

/// Whether top-level realtime process calls should be checked for allocations.
pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Run a realtime section with allocations forbidden when the runtime guard is
/// enabled.
///
/// This is meant for top-level audio/MIDI process calls on realtime threads.
pub fn forbid_alloc_if_enabled<T, F: FnOnce() -> T>(f: F) -> T {
    if enabled() {
        assert_no_alloc::assert_no_alloc(f)
    } else {
        f()
    }
}

/// Reverse guard: run a section where allocations are explicitly allowed, even
/// if it is nested inside a realtime no-allocation section.
///
/// Use this only for exceptional paths where allocating on the realtime thread
/// is intentional and preferable to aborting under the developer guard.
pub fn allow_alloc<T, F: FnOnce() -> T>(f: F) -> T {
    assert_no_alloc::permit_alloc(f)
}

/// Run an exceptional allocation-permitted realtime section and emit a warning
/// once for that call site while the runtime guard is enabled.
pub fn allow_alloc_once<T, F: FnOnce() -> T>(site: &'static str, once: &'static Once, f: F) -> T {
    if enabled() {
        allow_alloc(|| {
            once.call_once(|| {
                eprintln!(
                    "[RealtimeAllocGuard] WARNING: realtime allocation temporarily allowed at {site}"
                );
            });
            f()
        })
    } else {
        f()
    }
}

/// Mark an exceptional realtime section as allocation-permitted and warn once
/// for this macro invocation while the runtime guard is enabled.
#[macro_export]
macro_rules! realtime_allow_alloc_once {
    ($site:literal, $body:expr) => {{
        static WARN_ONCE: std::sync::Once = std::sync::Once::new();
        $crate::realtime_alloc_guard::allow_alloc_once($site, &WARN_ONCE, $body)
    }};
}
