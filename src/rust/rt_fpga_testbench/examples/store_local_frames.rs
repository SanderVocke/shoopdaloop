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
    let case = rt_fpga_testbench::suites::media::local_frames::boundaries(
        args[2].parse().unwrap(),
        args.get(3).map_or(64, |v| v.parse().unwrap()),
        rt_fpga_testbench::memory::Mode::Fixed,
    )
    .unwrap();
    let result =
        serde_json::json!({"cases":[case],"provenance":rt_fpga_testbench::report::provenance()});
    std::fs::write(output, serde_json::to_vec_pretty(&result).unwrap()).unwrap();
}
#[cfg(not(feature = "sim"))]
fn main() {
    panic!("Enable sim to test local frame limits");
}
