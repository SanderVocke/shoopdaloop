#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use shoop_wasm_test_support::shoop_test;
#[shoop_test]
fn staged_images_copy_live_state_at_activation_and_parameters_use_actual_ramps() {
    rt_fpga_testbench::suites::image_control::staging_and_ramps().unwrap();
}

#[shoop_test]
fn full_size_image_activation_is_resident_bounded_and_copies_every_state_slot() {
    rt_fpga_testbench::suites::image_control::full_capacity_activation().unwrap();
}

#[shoop_test]
fn resident_graph_and_parameter_actions_run_without_service_and_retain_exact_cuts() {
    rt_fpga_testbench::suites::image_control::resident_configuration_actions().unwrap();
}

#[shoop_test]
fn shared_configuration_owner_preserves_credits_generations_and_bounded_device_work() {
    rt_fpga_testbench::suites::image_control::configuration_owner_and_budget().unwrap();
}

#[shoop_test]
fn full_image_and_parameter_cuts_preserve_continuous_audio_with_full_replies_and_partial_ingress() {
    rt_fpga_testbench::suites::live_image::continuous_large_image().unwrap();
}

#[shoop_test]
fn inactive_image_banks_and_private_actual_state_updates_preserve_graph_continuity() {
    rt_fpga_testbench::suites::image_control::canonical_staging_banks().unwrap();
}

#[shoop_test]
fn canonical_parameters_follow_actual_bindings_and_freeze_when_graphs_drop_them() {
    rt_fpga_testbench::suites::image_control::canonical_parameters().unwrap();
}

#[shoop_test]
fn staged_activation_guards_preserve_identities_and_commit_only_actual_device_epochs() {
    rt_fpga_testbench::suites::stages::guards().unwrap();
}

#[shoop_test]
fn initial_and_graph_activation_use_actual_canonical_state_with_autonomous_audio() {
    rt_fpga_testbench::suites::resident_configuration::activation().unwrap();
}

#[shoop_test]
fn initial_activation_after_an_earlier_graph_uses_canonical_fields_and_preserves_the_clock() {
    rt_fpga_testbench::suites::resident_configuration::graph_before_initial().unwrap();
}
