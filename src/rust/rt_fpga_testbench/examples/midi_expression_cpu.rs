#[cfg(feature = "sim")]
fn main() {
    let mut cases = Vec::new();
    for (audio, midi) in [(true, true), (true, false), (false, true)] {
        for state in [false, true] {
            cases.push(rt_fpga_testbench::suites::midi::data::benchmark::cpu(
                audio, midi, state, 1024,
            ));
        }
    }
    let output = std::env::args().nth(1).expect("output JSON path");
    std::fs::write(output, serde_json::to_string_pretty(&cases).unwrap() + "\n").unwrap();
}
#[cfg(not(feature = "sim"))]
fn main() {
    panic!("Enable sim");
}
