//! File codecs and document conversion owned by session clients.
#![deny(unsafe_code)]
#[cfg(all(test, target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
pub mod codec;
mod document;
pub mod worker;

pub mod scripts;
