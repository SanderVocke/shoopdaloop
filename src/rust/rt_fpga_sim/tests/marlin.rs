#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use rt_audio_math::Q824;
use rt_backend_testbench::{
    production, rtl,
    spec::{Action, Samples, Scenario},
};
use rt_fpga_sim::{Simulator, runtime, value_words};
use shoop_wasm_test_support::shoop_test;
use std::path::Path;

#[shoop_test]
fn integrated_gain_and_silence_with_stalls_and_independent_models() {
    let cache = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../work/marlin-tests");
    std::fs::create_dir_all(&cache).unwrap();
    let rt = runtime(&cache).unwrap();
    let wave = cache.join("smoke.vcd");
    let mut sim = Simulator::new(&rt, Some(&wave), true).unwrap();
    let mut independent = Simulator::new(&rt, None, false).unwrap();
    for source in [
        include_str!("../fixtures/instant-gains.json"),
        include_str!("../../../../fpga/verification/scenarios/silence.json"),
    ] {
        let case: Scenario = serde_json::from_str(source).unwrap();
        let expected = production::run::<Q824>(&case).unwrap();
        let image = rtl::compile(&case).unwrap();
        sim.load(&image).unwrap();
        for (i, step) in case.steps.iter().enumerate() {
            for action in &step.actions {
                let Action::Gain { node, target, .. } = action else {
                    panic!("gain fixture")
                };
                for &address in &image.gains.iter().find(|b| b.id == *node).unwrap().ramps {
                    sim.state(
                        address,
                        Some(value_words(rtl::Value::Ramp(rtl::Ramp {
                            start: *target,
                            target: *target,
                            duration: 0,
                            elapsed: 0,
                        }))),
                        false,
                    )
                    .unwrap();
                }
            }
            assert_eq!(
                Samples::Q824(sim.run(&image, &step.inputs).unwrap()),
                expected.quanta[i].outputs
            );
            assert_eq!(sim.frame(), expected.quanta[i].next_frame);
            assert_eq!(independent.frame(), 0);
        }
    }
    assert!(sim.sample(3, 0, None).is_err());
    independent.timeout = 0;
    assert!(independent.command(0, 0, [0; 20]).is_err());
    sim.reset();
    assert_eq!(sim.frame(), 0);
    drop(sim);
    drop(independent);
    let vcd = std::fs::read_to_string(wave).unwrap();
    let clock = vcd
        .lines()
        .find(|l| l.contains(" clk "))
        .unwrap()
        .split_whitespace()
        .nth(3)
        .unwrap();
    assert!(vcd.contains(&format!("0{clock}\n")));
    assert!(vcd.contains(&format!("1{clock}\n")));
}

mod fifo {
    use marlin::{
        verilator::{VerilatedModelConfig, VerilatorRuntime, VerilatorRuntimeOptions, WideIn},
        verilog::prelude::*,
    };
    use shoop_wasm_test_support::shoop_test;
    use std::path::Path;
    #[verilog(src = "rtl/fifo_probe.sv", name = "probe_top")]
    struct Probe;
    fn tick(m: &mut Probe<'_>) {
        m.clk = 0;
        m.eval();
        m.clk = 1;
        m.eval();
        m.clk = 0;
        m.eval();
    }
    #[shoop_test]
    fn odd_depth_fifo_wrap_wide_words_and_backpressure() {
        let here = Path::new(env!("CARGO_MANIFEST_DIR"));
        let root = here.join("../../..");
        let rt = VerilatorRuntime::new2(
            root.join("work/marlin-fifo"),
            &[
                here.join("rtl/fifo_probe.sv"),
                root.join("fpga/audio_io/rtl/audio_command_fifo.sv"),
                root.join("fpga/state_processor/rtl/sp_ram.sv"),
            ],
            &[] as &[&Path],
            [],
            VerilatorRuntimeOptions::default()
                .verilator_executable(here.join("verilator-wrapper").into()),
        )
        .unwrap();
        let mut m = rt
            .create_model::<Probe>(&VerilatedModelConfig::default())
            .unwrap();
        m.reset = 1;
        tick(&mut m);
        tick(&mut m);
        m.reset = 0;
        tick(&mut m);
        for ordinal in 0..20u32 {
            let mut words = core::array::from_fn::<_, 24, _>(|i| {
                ordinal.wrapping_mul(12345) ^ (i as u32).wrapping_mul(0x9e3779b9)
            });
            words[23] &= 0x3ff;
            assert_eq!(m.push_ready, 1);
            m.push_data = WideIn::new(words);
            m.push_valid = 1;
            tick(&mut m);
            m.push_valid = 0;
            for _ in 0..8 {
                if m.pop_valid != 0 {
                    break;
                }
                tick(&mut m);
            }
            assert_eq!(m.pop_valid, 1);
            assert_eq!(*m.pop_data.value(), words);
            for _ in 0..ordinal % 3 {
                tick(&mut m);
                assert_eq!(*m.pop_data.value(), words);
            }
            m.pop_ready = 1;
            tick(&mut m);
            m.pop_ready = 0;
            assert_eq!(m.count, 0);
        }
    }
}
