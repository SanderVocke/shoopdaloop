#[cfg(feature = "sim")]
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let output = args
        .windows(2)
        .find(|p| p[0] == "--output")
        .map(|p| std::path::PathBuf::from(&p[1]))
        .expect("--output is required");
    assert!(!output.exists(), "Use a new output path");
    let width = args
        .windows(2)
        .find(|p| p[0] == "--width")
        .map_or(64, |p| p[1].parse().unwrap());
    assert!(width == 32 || width == 64);
    rt_fpga_testbench::report::configure(
        &std::path::PathBuf::from(format!("{}.evidence", output.display())),
        false,
    )
    .unwrap();
    let result = rt_fpga_testbench::suites::metadata::translation_area_workloads(width).unwrap();
    std::fs::write(
        output,
        serde_json::to_string_pretty(&result).unwrap() + "\n",
    )
    .unwrap();
}
#[cfg(not(feature = "sim"))]
fn main() {
    panic!("Enable sim to run the RTL benchmark");
}
