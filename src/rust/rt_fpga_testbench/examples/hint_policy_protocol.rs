#[cfg(feature = "sim")]
fn main() {
    use rt_fpga_testbench::{drivers::store::Geometry, memory::Mode, suites::pipeline};
    use serde_json::json;
    let args: Vec<_> = std::env::args().collect();
    let policy: u64 = args[1].parse().unwrap();
    assert!(policy <= 2);
    let output = std::path::Path::new(&args[2]);
    assert!(!output.exists(), "Use a new output path");
    let mut rows = Vec::new();
    for mode in [Mode::Instant, Mode::Fixed, Mode::Dramsim3, Mode::Legacy] {
        for blocked in [false, true] {
            let outcome = std::panic::catch_unwind(|| {
                pipeline::run_parameters(
                    "hint-policy-probe",
                    [32, 16, 32, 32, 16, 2, 2],
                    if blocked {
                        Geometry::new([16, 32 * 32, 4], 32)
                    } else {
                        Geometry::new([32, 32, 32], 1)
                    },
                    64,
                    mode,
                    1,
                    if blocked { 0 } else { 20 },
                    &[("READ_HINT_POLICY", policy)],
                )
                .unwrap();
            });
            let failure = outcome.as_ref().err().map(|e| {
                e.downcast_ref::<String>()
                    .cloned()
                    .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_else(|| "non-string panic".to_owned())
            });
            rows.push(json!({"memory_mode":format!("{mode:?}"),"blocked":blocked,
                "passed":outcome.is_ok(),"failure":failure}));
        }
    }
    std::fs::write(output, serde_json::to_vec_pretty(&json!({
        "policy":policy,"scope":"original publication/fault/reset/progress fixture; no relaxed default assertion",
        "cases":rows,"provenance":rt_fpga_testbench::report::provenance()
    })).unwrap()).unwrap();
}
#[cfg(not(feature = "sim"))]
fn main() {
    panic!("Enable sim to probe hint policies");
}
