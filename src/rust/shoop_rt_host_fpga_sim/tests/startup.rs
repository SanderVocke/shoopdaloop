#![cfg(all(feature = "sim", target_os = "linux"))]
use shoop_rt_host_fpga_sim::{Config, start};
use shoop_wasm_test_support::shoop_test;

#[shoop_test]
fn missing_tool_and_sources_return_the_worker_diagnostic() {
    let root = std::env::temp_dir().join(format!("shoop-fpga-missing-{}", std::process::id()));
    let config = Config {
        verilator: root.join("no-verilator").into_os_string(),
        ..Default::default()
    };
    let error = start(&config).err().expect("missing executable must fail");
    assert!(
        error.contains("cannot run Verilator") && error.contains("5.032"),
        "{error}"
    );
    let config = Config {
        rtl_directory: root.join("no-rtl"),
        ..Default::default()
    };
    let error = start(&config).err().expect("missing sources must fail");
    assert!(error.contains("RTL source directory is missing"), "{error}");
}
