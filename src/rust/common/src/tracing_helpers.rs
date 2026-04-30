use std::os::raw::{c_char, c_uint};
use std::slice;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;

static TRACING_ENABLED: AtomicBool = AtomicBool::new(false);

/// Global counter for assigning unique plot IDs sequentially from 0.
static NEXT_PLOT_ID: AtomicU32 = AtomicU32::new(0);

/// Registry of PlotName objects, indexed by their assigned ID.
static PLOT_REGISTRY: Mutex<Vec<Option<tracy_client::PlotName>>> =
    Mutex::new(Vec::new());

pub fn set_tracing_enabled(enabled: bool) {
    TRACING_ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn is_tracing_enabled() -> bool {
    TRACING_ENABLED.load(Ordering::Relaxed)
}

/// Register a plot name and return a unique numeric ID.
pub fn register_plot_name(name: &str) -> u32 {
    let id = NEXT_PLOT_ID.fetch_add(1, Ordering::Relaxed);
    let plot_name = tracy_client::PlotName::new_leak(name.to_string());

    let mut registry = PLOT_REGISTRY.lock().unwrap();
    if (id as usize) >= registry.len() {
        registry.resize(id as usize + 1, None);
    }
    registry[id as usize] = Some(plot_name);
    id
}

/// Plot a value to Tracy using a pre-registered plot ID.
pub fn plot_by_id(id: u32, value: f64) {
    if !is_tracing_enabled() {
        return;
    }

    let registry = PLOT_REGISTRY.lock().unwrap();
    if let Some(Some(plot_name)) = registry.get(id as usize) {
        if let Some(client) = tracy_client::Client::running() {
            client.plot(plot_name.clone(), value);
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
    if is_tracing_enabled() { 1 } else { 0 }
}

/// C-compatible wrapper: register a plot name (name, len) and return an ID.
/// len is C `unsigned` (c_uint).
#[no_mangle]
pub unsafe extern "C" fn shoop_tracing_register_plot_name(name: *const c_char, len: c_uint) -> c_uint {
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
