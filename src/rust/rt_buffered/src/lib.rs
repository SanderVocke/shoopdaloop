#![doc = include_str!("../README.md")]
#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

#[cfg(test)]
extern crate std;

#[cfg(all(test, target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

mod backend;
mod inline;
mod processor;
mod timing;

pub use backend::{Backend, Batch, BatchTag, ParameterEvent, SourceObserver};
pub use inline::InlineTestProcessor;
pub use processor::{BufferLimits, BufferedProcessor, Lifecycle, MidiLimits, MAX_STORAGE_BYTES};
pub use timing::{PreparedTiming, Timing, MAX_BATCH_FRAMES, MAX_LATENCY_FRAMES};
