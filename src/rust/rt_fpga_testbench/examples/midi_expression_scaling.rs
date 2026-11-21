#[cfg(feature = "sim")]
fn main() {
    let value = rt_fpga_testbench::suites::midi::expression::scaling().unwrap();
    std::fs::write(
        std::env::args().nth(1).expect("output JSON path"),
        serde_json::to_string_pretty(&value).unwrap() + "\n",
    )
    .unwrap();
}
#[cfg(not(feature = "sim"))]
fn main() {
    panic!("Enable sim");
}
