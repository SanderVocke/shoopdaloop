//! The new engine backend's browser host: an AudioWorklet module around the
//! engine, built for `wasm32-unknown-unknown` without Wasm imports.
//!
//! The processor script instantiates the module, writes a configuration record
//! ([`config`]) and prepares the engine before its first render quantum. Per
//! quantum it copies audio and posted bytes into module memory and calls
//! [`WorkletHost::process`], which pumps a bounded number of protocol records
//! into the engine's control endpoint through a record server, renders and
//! collects the records and MIDI events to post back. Records travel over
//! `postMessage` in batches ([`framing`]); Web MIDI travels on its own lanes
//! ([`lane`]). The host contains no engine or application semantics.
//!
//! Without the default `exports` feature the crate only provides the formats,
//! for the main-thread half.
#![deny(unsafe_code)]

#[cfg(all(test, target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

#[cfg(test)]
#[global_allocator]
static ALLOCATOR: shoop_wasm_test_support::heap::TrackingAllocator =
    shoop_wasm_test_support::heap::TrackingAllocator;

#[cfg(feature = "exports")]
pub mod abi;
pub mod config;
pub mod framing;
mod host;
pub mod lane;
mod stream;

pub use config::{ConfigError, WorkletConfig};
pub use host::{Counters, Inbound, Outbound, PrepareFailure, Quantum, QuantumError, WorkletHost};
