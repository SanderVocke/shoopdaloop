#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use shoop_wasm_test_support::shoop_test;
#[shoop_test]
fn shared_preparation_preserves_the_pilot_scenario() {
    let actual = rt_fpga_testbench::prepared::scenario(16, 8, 2, true, "reverse");
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/continuous-audio.json")).unwrap();
    assert_eq!(actual, expected);
}
#[shoop_test]
fn rust_loader_records_match_the_prepared_hardware_roms() {
    let root = rt_fpga_testbench::root();
    let prepared = rt_fpga_testbench::prepared::prepare(
        rt_fpga_testbench::prepared::scenario(32, 32, 16, false, "identity"),
        32,
        1024,
    )
    .unwrap();
    for file in ["boot.hex", "events.hex"] {
        assert_eq!(
            std::fs::read(prepared.directory.join(file)).unwrap(),
            std::fs::read(root.join("fpga/audio_io/examples/default").join(file)).unwrap(),
            "Rust and hardware preparation differ: {file}"
        );
    }
}
#[shoop_test]
fn continuous_record_playback_matches_production_through_clocks_stalls_and_overrun() {
    use rt_fpga_testbench::{
        memory::Mode,
        suites::continuous::{self, Config},
    };
    let output = rt_fpga_testbench::root().join("work/marlin-continuous");
    let baseline: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/continuous-baseline.json")).unwrap();
    for (name, period_ps, phase_ps, stalls, overrun) in [
        ("coincident", 10000, 0, false, false),
        ("stalls", 13000, 3000, true, false),
        ("overrun", 13000, 3000, true, true),
    ] {
        let actual = continuous::run(
            name,
            &Config {
                period_ps,
                phase_ps,
                stalls,
                overrun,
                ..Config::default()
            },
            Mode::Fixed,
            &output,
        )
        .unwrap();
        let expected = baseline["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["name"] == name)
            .unwrap();
        for key in [
            "frames",
            "states",
            "media",
            "processed",
            "missed",
            "discarded",
            "rx_high_water",
            "tx_high_water",
        ] {
            assert_eq!(
                actual[key], expected[key],
                "{name}: historical baseline {key}"
            );
        }
        assert!(
            actual["control_stalls"].as_u64().unwrap() > 0,
            "bounded controls never stalled"
        );
    }
}
