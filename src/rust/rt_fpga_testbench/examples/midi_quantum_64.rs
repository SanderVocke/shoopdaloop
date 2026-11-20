#[cfg(feature = "sim")]
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let option = |key: &str, default: usize| {
        args.windows(2)
            .find(|p| p[0] == key)
            .map_or(default, |p| p[1].parse().unwrap())
    };
    let value = rt_fpga_testbench::suites::midi::benchmark_64(
        !args.iter().any(|p| p == "--midi-only"),
        option("--loops", 64),
        option("--frames", 1_440_000),
        option("--quanta", 16),
        option("--ddr-delay", 7) as u64,
        args.iter().any(|arg| arg == "--profile"),
    )
    .unwrap();
    let text = serde_json::to_string_pretty(&value).unwrap();
    if let Some(path) = args.windows(2).find(|p| p[0] == "--output") {
        std::fs::write(&path[1], text + "\n").unwrap();
    } else {
        println!("{text}");
    }
}
#[cfg(not(feature = "sim"))]
fn main() {
    panic!("Enable sim for the RTL benchmark");
}
