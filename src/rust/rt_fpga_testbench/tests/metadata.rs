#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use shoop_wasm_test_support::shoop_test;
#[shoop_test]
fn indexed_metadata_detached_roots_and_coherent_shared_cache_transport() {
    use rt_fpga_testbench::suites::metadata;
    for width in [32, 64] {
        for large in 0..3 {
            metadata::tables(width, large).unwrap();
        }
        for words in [16, 64] {
            metadata::coherent_cache_and_transport(width, words).unwrap();
        }
    }
}
#[shoop_test]
fn prefetch_resolver_wrap_missing_metadata_and_independent_backpressure() {
    for c in [[32, 32, 32], [16, 8, 4], [5, 5, 8]] {
        rt_fpga_testbench::suites::resolver::resolver_conformance(c).unwrap();
    }
}
