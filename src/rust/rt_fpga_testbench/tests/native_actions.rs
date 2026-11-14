#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use shoop_wasm_test_support::shoop_test;
#[shoop_test]
fn native_execution_credits_and_initial_activation_do_not_require_arm_continuations() {
    rt_fpga_testbench::suites::native_actions::cells_and_initial_activation().unwrap();
}

#[shoop_test]
fn public_rows_publish_complete_payloads_and_private_writeback_preserves_identity_and_cuts() {
    rt_fpga_testbench::suites::native_actions::public_rows_and_private_writeback().unwrap();
}

#[shoop_test]
fn resident_public_commands_reuse_execution_cells_while_arm_service_is_paused() {
    rt_fpga_testbench::suites::native_actions::public_delivery_runs_without_arm_collection()
        .unwrap();
}

#[shoop_test]
fn public_delivery_hooks_preserve_autonomous_continuous_stereo_capture_and_playback() {
    rt_fpga_testbench::suites::resident_continuous::delivery(
        rt_fpga_testbench::memory::Mode::Instant,
    )
    .unwrap();
}

#[shoop_test]
fn managed_control_cannot_bypass_native_or_signal_ownership() {
    rt_fpga_testbench::suites::control::managed_owner_isolation().unwrap();
}

#[shoop_test]
fn resident_public_delivery_survives_fixed_and_dram_memory_interference() {
    use rt_fpga_testbench::{memory::Mode, suites::resident_continuous};
    for mode in [Mode::Fixed, Mode::Dramsim3] {
        resident_continuous::delivery(mode).unwrap();
    }
}

#[shoop_test]
fn public_queries_retain_historical_cuts_and_acknowledge_ownership_before_reuse() {
    rt_fpga_testbench::suites::observations::historical_queries().unwrap();
}
