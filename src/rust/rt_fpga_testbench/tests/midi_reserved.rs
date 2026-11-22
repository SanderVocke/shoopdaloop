#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use shoop_wasm_test_support::shoop_test;

#[shoop_test]
fn midi_descriptor_reserved_words_survive_updates_and_clear_on_reuse() {
    rt_fpga_testbench::suites::midi::store_area::reserved_words().unwrap();
}

#[shoop_test]
fn midi_word_interface_relocation_and_address_wrap_keep_descriptor_data() {
    rt_fpga_testbench::suites::midi::store_area::relocated_words().unwrap();
}
