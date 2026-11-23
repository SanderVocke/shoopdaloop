#[cfg(feature = "sim")]
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let output = std::path::Path::new(&args[2]);
    assert!(!output.exists(), "Use a new output path");
    rt_fpga_testbench::report::configure(
        &std::path::PathBuf::from(format!("{}.evidence", output.display())),
        false,
    )
    .unwrap();
    let mut value = rt_fpga_testbench::suites::midi::data::benchmark::musical(args[1].parse().unwrap())
        .unwrap();
    value["provenance"] = rt_fpga_testbench::report::provenance();
    std::fs::write(
        output,
        serde_json::to_string_pretty(&value).unwrap() + "\n",
    )
    .unwrap();
}
#[cfg(not(feature = "sim"))]
fn main() {
    panic!("Enable sim");
}
