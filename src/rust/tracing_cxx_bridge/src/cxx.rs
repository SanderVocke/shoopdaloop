use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

/// Global counter for assigning unique plot IDs sequentially from 0.
static NEXT_PLOT_ID: AtomicU32 = AtomicU32::new(0);

/// Registry of PlotName objects, indexed by their assigned ID.
///
/// IDs are sequential so this is a Vec, giving O(1) index lookup.
/// The mutex is only contended when tracing is active; when disabled
/// (the common case), `is_tracing_enabled()` short-circuits before
/// we ever touch this lock.
static PLOT_REGISTRY: Mutex<Vec<Option<tracy_client::PlotName>>> =
    Mutex::new(Vec::new());

#[cxx::bridge(namespace = "shoop_tracing")]
mod ffi {
    extern "Rust" {
        /// Check if Tracy tracing is enabled globally.
        ///
        /// Compiles to a single atomic load. When tracing is disabled
        /// the branch predictor learns the false path, making this
        /// essentially zero overhead on hot paths.
        fn is_tracing_enabled() -> bool;

        /// Register a plot name and return a unique numeric ID.
        ///
        /// Call this during initialization (not on hot paths). The
        /// returned ID can be stored in C++ and used with `plot_by_id`
        /// for fast, string-free plotting on audio threads.
        ///
        /// Each new name allocates a `PlotName` (intentionally leaked
        /// for Tracy's `'static` requirement). Calling with an already-
        /// registered name returns a new ID (no deduplication).
        fn register_plot_name(name: &str) -> u32;

        /// Plot a value to Tracy using a pre-registered plot ID.
        ///
        /// This is the fastest plot path: no string handling, just an
        /// atomic check, a Vec lookup, and a Tracy client call. Safe
        /// to call from audio threads when tracing is enabled.
        fn plot_by_id(id: u32, value: f64);
    }
}

fn is_tracing_enabled() -> bool {
    tracy_client::Client::running().is_some()
}

fn register_plot_name(name: &str) -> u32 {
    let id = NEXT_PLOT_ID.fetch_add(1, Ordering::Relaxed);
    let plot_name = tracy_client::PlotName::new_leak(name.to_string());

    let mut registry = PLOT_REGISTRY.lock().unwrap();
    // Grow the Vec if needed (should only happen on first registration)
    if (id as usize) >= registry.len() {
        registry.resize(id as usize + 1, None);
    }
    registry[id as usize] = Some(plot_name);
    id
}

fn plot_by_id(id: u32, value: f64) {
    // Fast path: return immediately when tracing is disabled.
    // This atomic load is branch-predicted away in production builds.
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
