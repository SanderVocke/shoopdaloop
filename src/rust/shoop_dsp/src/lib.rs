//! Prepared host DSP; concrete dependencies and SoundFont assets are feature gated.
#[cfg(any(feature = "builtin-fx", feature = "oxisynth"))]
pub mod backends;
#[cfg(all(test, target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

#[cfg(feature = "builtin-fx")]
pub mod builtin_fx;
#[cfg(any(feature = "builtin-fx", feature = "oxisynth"))]
mod midi_cc;
pub mod midi_storage;
#[cfg(feature = "oxisynth")]
pub mod oxisynth;

#[cfg(test)]
#[global_allocator]
static ALLOCATOR: shoop_wasm_test_support::heap::TrackingAllocator =
    shoop_wasm_test_support::heap::TrackingAllocator;
