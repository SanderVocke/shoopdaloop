#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

#[shoop_wasm_test_support::shoop_test]
fn shared_integration_binary_uses_selected_runtime() {
    assert_eq!("wasm".len(), 4);
}
