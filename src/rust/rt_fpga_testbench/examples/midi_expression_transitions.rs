#[cfg(feature = "sim")]
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let value = rt_fpga_testbench::suites::midi::data::benchmark::musical(args[1].parse().unwrap())
        .unwrap();
    std::fs::write(
        &args[2],
        serde_json::to_string_pretty(&value).unwrap() + "\n",
    )
    .unwrap();
}
#[cfg(not(feature = "sim"))]
fn main() {
    panic!("Enable sim");
}
