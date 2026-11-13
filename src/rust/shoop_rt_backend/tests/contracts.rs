//! The shared backend contracts against the adapter, the in-process carrier, the engine and
//! the offline host.

mod support;

use shoop_backend::contracts::{backend_contracts, harness_contracts};
use shoop_wasm_test_support::shoop_test;
use support::*;

#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

#[shoop_test]
fn adapter_satisfies_backend_contracts() {
    backend_contracts(&mut stack(&description()), &profile());
}

#[shoop_test]
fn adapter_satisfies_harness_contracts() {
    harness_contracts(RtTimingHarness::new);
}
