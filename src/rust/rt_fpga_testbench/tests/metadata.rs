#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use shoop_wasm_test_support::shoop_test;
#[shoop_test]
fn translation_boundaries_and_lifetimes_match_an_independent_map() {
    for width in [32, 64] {
        rt_fpga_testbench::suites::metadata::translation_area_workloads(width).unwrap();
    }
}

#[shoop_test]
fn directory_paths_cross_all_primary_levels_and_survive_deep_retirement() {
    rt_fpga_testbench::suites::metadata::directory_area_workloads(64).unwrap();
}
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

#[shoop_test]
fn primary_cache_capacity_preserves_fences_backpressure_faults_and_reset() {
    for width in [32, 64] {
        rt_fpga_testbench::suites::metadata::coherent_cache_lines(width, 16, 128).unwrap();
    }
}
