//! Fixed-quantum audio/MIDI nodes with internal block processing and stream delay.
#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

#[cfg(test)]
extern crate std;

#[cfg(all(test, target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

mod inline;
mod processor;
mod timing;

pub use inline::InlineTest;
pub use processor::{External, MidiLimits, MAX_STORAGE_BYTES};
pub use timing::{PreparedTiming, Timing, MAX_BLOCK_FRAMES, MAX_LATENCY_FRAMES};
