#[cfg(feature = "sim")]
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let output = std::path::Path::new(&args[1]);
    assert!(!output.exists(), "Use a new output path");
    rt_fpga_testbench::report::configure(
        &std::path::PathBuf::from(format!("{}.evidence", output.display())),
        false,
    )
    .unwrap();
    let mut result = rt_fpga_testbench::suites::media::area::operations_local_frames(
        args.get(2).map_or(64, |v| v.parse().unwrap()),
        args.get(3).map_or(32, |v| v.parse().unwrap()),
        args.get(4).map(|v| v.parse().unwrap()),
    )
    .unwrap();
    result["provenance"] = rt_fpga_testbench::report::provenance();
    std::fs::write(output, serde_json::to_vec_pretty(&result).unwrap()).unwrap();
}
#[cfg(not(feature = "sim"))]
fn main() {
    panic!("Enable sim to measure storage operations");
}
