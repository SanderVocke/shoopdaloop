#![cfg(all(feature = "fpga-reference", not(target_arch = "wasm32")))]
#[path = "../examples/fpga_reference_client.rs"]
mod client;
use shoop_wasm_test_support::shoop_test;

#[shoop_test]
fn application_client_prepares_once_and_operates_through_rt_backend() {
    let output = tempfile::tempdir().unwrap();
    client::run_with(
        env!("CARGO_BIN_EXE_shoop_host").into(),
        output.path().into(),
    )
    .unwrap();
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output.path().join("report.json")).unwrap()).unwrap();
    assert_eq!(report["result"], "passed");
    assert!(report["recorded_frames"].as_u64().unwrap() > 0);
}
