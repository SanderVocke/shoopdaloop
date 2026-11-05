//! Authoritative portable session service. UI and scripting are client responsibilities.
#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;

#[cfg(test)]
extern crate std;

#[cfg(all(test, target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

mod host_graph;
mod service;
pub use host_graph::{HostGraphController, HostGraphError};
pub use service::{ConnectionId, Service, ServiceOptions};
