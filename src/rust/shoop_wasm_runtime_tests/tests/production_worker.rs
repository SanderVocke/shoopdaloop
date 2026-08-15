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

    #[wasm_bindgen(catch, js_name = runProcessingModeContracts)]
    async fn run_processing_mode_contracts(
        runtime: &str,
        asset_location: &str,
        protocol_version: u16,
        command_max_bytes: u32,
    ) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch, js_name = runProtocolAndShutdownContracts)]
    async fn run_protocol_and_shutdown_contracts(
        runtime: &str,
        asset_location: &str,
        protocol_version: u16,
        command_max_bytes: u32,
    ) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch, js_name = runFailureIsolationContracts)]
    async fn run_failure_isolation_contracts(
        runtime: &str,
        asset_location: &str,
        protocol_version: u16,
        command_max_bytes: u32,
    ) -> Result<JsValue, JsValue>;
}

fn runtime_and_assets() -> (&'static str, &'static str) {
    let browser = cfg!(feature = "wasm-test-browser");
    let runtime = if browser { "chrome" } else { "node" };
    let asset_location = if browser {
        option_env!("SHOOP_WASM_TEST_ASSET_BASE")
    } else {
        option_env!("SHOOP_WASM_TEST_ASSET_DIR")
    }
    .expect("run_wasm_tests.py must provide the staged asset location");
    (runtime, asset_location)
}

async fn checked(
    future: impl std::future::Future<Output = Result<JsValue, JsValue>>,
    expected: &str,
) {
    let result = future
        .await
        .unwrap_or_else(|error| panic!("production Worker contract failed: {error:?}"));
    assert_eq!(result.as_string().as_deref(), Some(expected));
}

#[wasm_bindgen_test]
async fn exact_production_worker_modules_process_and_isolate_instances() {
    let (runtime, assets) = runtime_and_assets();
    checked(
        run_production_worker_probe(
            runtime,
            assets,
            shoop_audio_protocol::PROTOCOL_VERSION,
            shoop_audio_protocol::COMMAND_MAX_BYTES as u32,
        ),
        "production Worker probe: ok",
    )
    .await;
}

#[wasm_bindgen_test]
async fn explicit_cooperative_and_realtime_modes_restart_cleanly() {
    let (runtime, assets) = runtime_and_assets();
    checked(
        run_processing_mode_contracts(
            runtime,
            assets,
            shoop_audio_protocol::PROTOCOL_VERSION,
            shoop_audio_protocol::COMMAND_MAX_BYTES as u32,
        ),
        "processing mode contracts: ok",
    )
    .await;
}

#[wasm_bindgen_test]
async fn protocol_ordering_midi_observation_and_production_shutdown_work() {
    let (runtime, assets) = runtime_and_assets();
    checked(
        run_protocol_and_shutdown_contracts(
            runtime,
            assets,
            shoop_audio_protocol::PROTOCOL_VERSION,
            shoop_audio_protocol::COMMAND_MAX_BYTES as u32,
        ),
        "protocol and shutdown contracts: ok",
    )
    .await;
}

#[wasm_bindgen_test]
async fn one_terminal_worker_failure_does_not_stop_its_peer() {
    let (runtime, assets) = runtime_and_assets();
    checked(
        run_failure_isolation_contracts(
            runtime,
            assets,
            shoop_audio_protocol::PROTOCOL_VERSION,
            shoop_audio_protocol::COMMAND_MAX_BYTES as u32,
        ),
        "failure isolation contracts: ok",
    )
    .await;
}
