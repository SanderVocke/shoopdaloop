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
