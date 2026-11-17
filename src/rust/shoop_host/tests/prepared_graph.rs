#![cfg(all(feature = "fpga-reference", not(target_arch = "wasm32")))]
#[path = "../examples/fpga_reference_client.rs"]
mod client;
use shoop_wasm_test_support::shoop_test;

#[shoop_test]
fn application_client_live_loads_and_operates_through_rt_backend() {
    let output = tempfile::tempdir().unwrap();
    client::run_with(
        env!("CARGO_BIN_EXE_shoop_host").into(),
        output.path().into(),
    )
    .unwrap_or_else(|error| {
        let log = std::fs::read_to_string(output.path().join("host.log"))
            .unwrap_or_else(|error| format!("could not read host log: {error}"));
        panic!("{error:#}\nhost log:\n{log}");
    });
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output.path().join("report.json")).unwrap()).unwrap();
    assert_eq!(report["result"], "passed");
    assert_eq!(report["live_graph_updates"], 3);
    assert_eq!(report["recycled_logical_slots"], true);
    assert!(report["recorded_frames"].as_u64().unwrap() > 0);
}
