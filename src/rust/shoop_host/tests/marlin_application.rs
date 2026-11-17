#![cfg(all(feature = "rtl-sim", target_os = "linux"))]
#[path = "../examples/fpga_reference_client.rs"]
#[allow(dead_code)]
mod client;
use shoop_wasm_test_support::shoop_test;
#[shoop_test(no_wasm = "native simulator host/application socket")]
fn rtl_application_records_and_reloads_through_rt_backend() {
    for transport in ["socket", "shared_memory"] {
        let output = tempfile::tempdir().unwrap();
        client::run_configuration(
            env!("CARGO_BIN_EXE_shoop_host").into(),
            output.path().into(),
            "rtl-sim",
            transport,
        )
        .unwrap_or_else(|error| {
            panic!(
                "{error:#}\n{}",
                std::fs::read_to_string(output.path().join("host.log")).unwrap_or_default()
            )
        });
        let report: serde_json::Value =
            serde_json::from_slice(&std::fs::read(output.path().join("report.json")).unwrap())
                .unwrap();
        assert_eq!(report["result"], "passed");
    }
}
