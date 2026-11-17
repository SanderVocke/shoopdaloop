//! The model is created, clocked and destroyed exclusively on its owner thread.
#[cfg(all(feature = "sim", not(target_arch = "wasm32")))]
mod bridge;
#[cfg(all(feature = "sim", not(target_arch = "wasm32")))]
mod owner;
#[cfg(all(feature = "sim", not(target_arch = "wasm32")))]
pub use bridge::{prepare, Bridge, Endpoint, Prepared};

#[cfg(test)]
#[global_allocator]
static ALLOCATOR: shoop_wasm_test_support::heap::TrackingAllocator =
    shoop_wasm_test_support::heap::TrackingAllocator;
