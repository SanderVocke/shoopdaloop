#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;
use wasm_bindgen_test::wasm_bindgen_test;

#[cfg(feature = "wasm-test-browser")]
wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen(module = "/js/worker_fixture.js")]
extern "C" {
    #[wasm_bindgen(catch, js_name = runProductionWorkerProbe)]
    async fn run_production_worker_probe(
        runtime: &str,
        asset_location: &str,
        protocol_version: u16,
        command_max_bytes: u32,
    ) -> Result<JsValue, JsValue>;
}

#[wasm_bindgen_test]
async fn exact_production_worker_modules_process_and_isolate_instances() {
    let browser = cfg!(feature = "wasm-test-browser");
    let runtime = if browser { "chrome" } else { "node" };
    let asset_location = if browser {
        option_env!("SHOOP_WASM_TEST_ASSET_BASE")
    } else {
        option_env!("SHOOP_WASM_TEST_ASSET_DIR")
    }
    .expect("run_wasm_tests.py must provide the staged asset location");
    let result = run_production_worker_probe(
        runtime,
        asset_location,
        shoop_audio_protocol::PROTOCOL_VERSION,
        shoop_audio_protocol::COMMAND_MAX_BYTES as u32,
    )
    .await
    .unwrap_or_else(|error| panic!("production Worker probe failed: {error:?}"));
    assert_eq!(
        result.as_string().as_deref(),
        Some("production Worker probe: ok")
    );
}
