use std::os::raw::{c_char, c_uint};
use std::slice;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};

use arc_swap::ArcSwap;

static TRACING_ENABLED: AtomicBool = AtomicBool::new(false);

/// Global counter for assigning unique plot IDs sequentially from 0.
static NEXT_PLOT_ID: AtomicU32 = AtomicU32::new(0);

/// Lock-free registry of PlotName objects, indexed by their assigned ID.
///
/// Uses `ArcSwap<Vec<Arc<PlotName>>>` so that the hot-path `plot_by_id`
/// can read a snapshot without acquiring a mutex.  Writers (register)
/// clone-and-append, which is fine for the ~20-50 plot names we expect.
type PlotRegistry = ArcSwap<Vec<Arc<tracy_client::PlotName>>>;

static PLOT_REGISTRY: OnceLock<PlotRegistry> = OnceLock::new();

pub fn set_tracing_enabled(enabled: bool) {
    TRACING_ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn is_tracing_enabled() -> bool {
    TRACING_ENABLED.load(Ordering::Relaxed)
}

/// Register a plot name and return a unique numeric ID.
pub fn register_plot_name(name: &str) -> u32 {
    let id = NEXT_PLOT_ID.fetch_add(1, Ordering::Relaxed);
    let plot_name = Arc::new(tracy_client::PlotName::new_leak(name.to_string()));

    let registry = PLOT_REGISTRY.get_or_init(|| ArcSwap::new(Arc::new(Vec::new())));

    // Clone current snapshot, append, and swap atomically.
    // Writers are rare (init time only), so the clone cost is negligible.
    let mut vec = registry.load().as_ref().clone();
    vec.push(plot_name);
    registry.store(Arc::new(vec));
    id
}

/// Plot a value to Tracy using a pre-registered plot ID.
///
/// Lock-free on the hot path: loads an Arc snapshot, then indexes
/// into the vector without any mutex.
pub fn plot_by_id(id: u32, value: f64) {
    if !is_tracing_enabled() {
        return;
    }

    // OnceLock::get() is lock-free after initialization.
    // ArcSwap::load() is lock-free — returns an Arc without blocking.
    if let Some(registry) = PLOT_REGISTRY.get() {
        let registry = registry.load();
        if let Some(plot_name) = registry.get(id as usize) {
            if let Some(client) = tracy_client::Client::running() {
                client.plot((**plot_name).clone(), value);
            }
        }
    }
}

// ── C-compatible exports for runtime registration ──────────────────────
// These are called via function pointers by shoopdaloop_backend.so
// after shoop_register_tracing() is invoked at startup.
// Signatures must match shoop_tracing_callbacks in libshoopdaloop_backend.h.

/// C-compatible wrapper: check if tracing is enabled.
/// Returns 1 if enabled, 0 otherwise (matches C `unsigned`).
#[no_mangle]
pub unsafe extern "C" fn shoop_tracing_is_enabled() -> c_uint {
    if is_tracing_enabled() {
        1
    } else {
        0
    }
}

/// C-compatible wrapper: register a plot name (name, len) and return an ID.
/// len is C `unsigned` (c_uint).
#[no_mangle]
pub unsafe extern "C" fn shoop_tracing_register_plot_name(
    name: *const c_char,
    len: c_uint,
) -> c_uint {
    let name_str = unsafe {
        assert!(!name.is_null());
        let bytes = slice::from_raw_parts(name as *const u8, len as usize);
        std::str::from_utf8(bytes).unwrap_or("<invalid>")
    };
    register_plot_name(name_str)
}

/// C-compatible wrapper: plot a value by ID.
#[no_mangle]
pub unsafe extern "C" fn shoop_tracing_plot_by_id(id: c_uint, value: f64) {
    plot_by_id(id, value);
}

/// Helper to efficiently plot values to Tracy, handling plot name caching and leakage.
pub struct TracyPlotter {
    suffix: String,
    last_base_identifier: Option<String>,
    plot_name: Option<tracy_client::PlotName>,
}

impl Default for TracyPlotter {
    fn default() -> Self {
        Self::new("")
    }
}

impl TracyPlotter {
    pub fn new(suffix: impl Into<String>) -> Self {
        Self {
            suffix: suffix.into(),
            last_base_identifier: None,
            plot_name: None,
        }
    }

    /// Plots a value to Tracy.
    ///
    /// If `base_identifier` changes, a new `PlotName` is created (and leaked).
    /// Safe to call frequently.
    pub fn plot(&mut self, value: f64, base_identifier: &str) {
        if !is_tracing_enabled() {
            return;
        }
        if let Some(client) = tracy_client::Client::running() {
            if self.plot_name.is_none()
                || self.last_base_identifier.as_deref() != Some(base_identifier)
            {
                let full_name = if self.suffix.is_empty() {
                    base_identifier.to_string()
                } else {
                    format!("{}/{}", base_identifier, self.suffix)
                };
                self.plot_name = Some(tracy_client::PlotName::new_leak(full_name));
                self.last_base_identifier = Some(base_identifier.to_string());
            }

            if let Some(plot_name) = &self.plot_name {
                client.plot(plot_name.clone(), value);
            }
        }
    }
}
