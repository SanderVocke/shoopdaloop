#[cfg(feature = "sim")]
fn main() {
    let output = std::env::args().nth(1).expect("output path");
    assert!(
        !std::path::Path::new(&output).exists(),
        "Use a new output path"
    );
    rt_fpga_testbench::report::configure(
        &std::path::PathBuf::from(format!("{output}.evidence")),
        false,
    )
    .unwrap();
    let mut result = rt_fpga_testbench::suites::midi::store_area::boundaries().unwrap();
    result["provenance"] = rt_fpga_testbench::report::provenance();
    std::fs::write(output, serde_json::to_vec_pretty(&result).unwrap()).unwrap();
}
#[cfg(not(feature = "sim"))]
fn main() {
    panic!("Enable sim to check MIDI storage boundaries");
}
