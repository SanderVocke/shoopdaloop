#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use shoop_wasm_test_support::shoop_test;
#[shoop_test]
fn prepared_loader_input_loss_memory_fault_and_pending_response_recovery() {
    use rt_fpga_testbench::{
        memory::Mode,
        suites::{continuous::Config, demo},
    };
    let c = Config {
        q: 32,
        cf: 32,
        p: 32,
        channels: 16,
        slots: 1024,
        ticks: 512,
        monitoring: false,
        routing: "identity".into(),
        control_depth: 128,
        ..Config::default()
    };
    for mode in [Mode::Instant, Mode::Fixed, Mode::Dramsim3] {
        demo::prepared_demo_and_recovery(&c, mode).unwrap();
    }
}
