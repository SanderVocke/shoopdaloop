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
    let mut parameters: std::collections::BTreeMap<String, String> = args
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
    let frames = option("--frames", "1440000").parse().unwrap();
    let quanta = option("--quanta", "16").parse().unwrap();
    let delay = option("--ddr-delay", "7").parse().unwrap();
    if let Some(value) = args.windows(2).find(|p| p[0] == "--record-bursts") {
        let enabled = match value[1].as_str() {
            "on" => "1",
            "off" => "0",
            _ => panic!("--record-bursts must be on or off"),
        };
        parameters.insert("MIDI_RECORD_BURSTS".into(), enabled.into());
    }
    if let Some(value) = args.windows(2).find(|p| p[0] == "--split-midi-jobs") {
        let enabled = match value[1].as_str() {
            "on" => "1",
            "off" => "0",
            _ => panic!("--split-midi-jobs must be on or off"),
        };
        parameters.insert("SPLIT_MIDI_JOBS".into(), enabled.into());
    }
    use rt_fpga_testbench::suites::midi::data::benchmark;
    let mut value = match option("--profile", "sparse").as_str() {
        "sparse" => benchmark::run_parameters(
            audio,
            midi,
            option("--loops", "64").parse().unwrap(),
            frames,
            quanta,
            delay,
            option("--state", "off") == "on",
            &parameters,
        ),
        "tracks" => {
            assert!(midi, "tracks profile requires MIDI");
            let merge_output = match option("--merge-output", "on").as_str() {
                "on" => true,
                "off" => false,
                _ => panic!("--merge-output must be on or off"),
            };
            let interleave = match option("--interleave-jobs", "on").as_str() {
                "on" => true,
                "off" => false,
                _ => panic!("--interleave-jobs must be on or off"),
            };
            let shared_ddr = args
                .windows(2)
                .find(|p| p[0] == "--shared-ddr")
                .map(|p| std::path::Path::new(&p[1]));
            benchmark::tracks(
                audio,
                frames,
                quanta,
                delay,
                benchmark::JobOptions {
                    merge_output,
                    interleave,
                    shared_ddr,
                },
                &parameters,
            )
        }
        _ => panic!("--profile must be sparse or tracks"),
    }
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
