//! Both ends of the browser MessagePort record transport.
#![forbid(unsafe_code)]

#[cfg(all(test, target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

pub mod client;
pub mod server;
pub use client::RecordBatches;
#[cfg(all(target_arch = "wasm32", feature = "browser"))]
pub mod browser;
