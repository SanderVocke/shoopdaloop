#[cfg(feature = "sim")]
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let option = |key: &str, default: &str| {
        args.windows(2)
            .find(|p| p[0] == key)
            .map_or_else(|| default.to_owned(), |p| p[1].clone())
    };
    let (audio, midi) = match option("--scenario", "mixed").as_str() {
        "mixed" => (true, true),
        "audio" => (true, false),
        "midi" => (false, true),
        _ => panic!("--scenario must be mixed, audio or midi"),
    };
    let parameters = args
        .windows(2)
        .find(|p| p[0] == "--parameters")
        .map(|p| serde_json::from_slice(&std::fs::read(&p[1]).unwrap()).unwrap())
        .unwrap_or_default();
    if let Some(path) = args.windows(2).find(|p| p[0] == "--output") {
        assert!(
            !std::path::Path::new(&path[1]).exists(),
            "Use a new output path"
        );
        let evidence = std::path::PathBuf::from(format!("{}.evidence", path[1]));
        rt_fpga_testbench::report::configure(&evidence, false).unwrap();
    }
    let mut value = rt_fpga_testbench::suites::midi::data::benchmark::run_parameters(
        audio,
        midi,
        option("--loops", "64").parse().unwrap(),
        option("--frames", "1440000").parse().unwrap(),
        option("--quanta", "16").parse().unwrap(),
        option("--ddr-delay", "7").parse().unwrap(),
        option("--state", "off") == "on",
        &parameters,
    )
    .unwrap();
    value["provenance"] = rt_fpga_testbench::report::provenance();
    let text = serde_json::to_string_pretty(&value).unwrap() + "\n";
    if let Some(path) = args.windows(2).find(|p| p[0] == "--output") {
        std::fs::write(&path[1], text).unwrap();
    } else {
        print!("{text}");
    }
}
#[cfg(not(feature = "sim"))]
fn main() {
    panic!("Enable sim to run the RTL benchmark");
}
