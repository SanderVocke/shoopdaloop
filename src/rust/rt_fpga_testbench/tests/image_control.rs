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
