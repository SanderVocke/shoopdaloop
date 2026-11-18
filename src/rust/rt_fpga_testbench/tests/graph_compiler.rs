#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
#[path = "../../rt_fpga_compiler/tests/support/mod.rs"]
mod support;
use rt_fpga_compiler::{
    StateProgramTarget,
    graph::{CompileContext, compile},
};
use rt_fpga_reference::{
    compiler::RustTarget,
    fixed::Fixed,
    jobs::{JobProcessor, JobQueue},
    pipeline::Pipeline,
    state::{PlannedTransition, StateProcessor},
    storage::MediaStore,
    transitions::Operation,
};
use rt_fpga_testbench::{
    compiler::RtlTarget,
    drivers::{audio::Audio, state::State},
    loop_image::{image, write_plans},
    sim::Sim,
    suites,
};
use shoop_wasm_test_support::shoop_test;
use support::*;

#[shoop_test]
fn multichannel_recording_smoothing_and_bus_programs_emit_identical_rust_and_rtl_jobs() {
    let depths = [128, 64, 128];
    let mut design = suites::state::design(depths);
    design.parameters.insert("LOOP_DEPTH".into(), "160".into());
    let model = design
        .build(&rt_fpga_testbench::root().join("work/marlin-suite/graph-compiler"))
        .unwrap();
    let mut sim = Sim::new(model.session(None).unwrap(), &[("clk", 10000, 0)]);
    let mut driver = State::new(&mut sim, depths, 20261118);
    for quantum in [1, 16, 128] {
        let mut graph = session(2);
        graph.loops[0].boundary_smoothing_frames = 33;
        graph.loops[1].sync_source = Some(id(100));
        graph.loops[0].balance = 0.25;
        graph.buses[0].gain_db = -6.;
        let logical = compile(
            &graph,
            CompileContext {
                quantum,
                limits: Default::default(),
            },
        )
        .unwrap();
        let views = [23, 41, 52, 68, 79, 85];
        let rust = RustTarget::new(&views).build(&logical).unwrap();
        let mut target = RtlTarget::new([128, 64, 128, 128, 64, 2, 2], 160, &views);
        target.state_base = 3;
        let rtl = target.build(&logical).unwrap();
        driver.reset(&mut sim);
        driver.image.load_compiled(&mut sim, &rtl);
        sim.model.set("sync_event_ready", 1);
        let specs = rust.plan.image().description.loops.clone();
        let mut sp = StateProcessor::with_loops(rust.plan, rust.memory);
        let mut jobs = JobQueue::new(rust.resources.queue_capacity as usize);
        for tick in 0..16 {
            for spec in &specs {
                let operation = match tick {
                    0 => Some(Operation::Record),
                    4 => Some(Operation::Play {
                        position: 0,
                        retrigger: true,
                    }),
                    12 => Some(Operation::Stop),
                    _ => None,
                };
                if let Some(operation) = operation {
                    sp.write_plans(
                        spec.id,
                        [
                            Some(PlannedTransition {
                                sequence: tick + 1,
                                operation,
                                on_pulse: false,
                                effective: 0,
                                remaining_ticks: 0,
                            }),
                            None,
                        ],
                    )
                    .unwrap();
                }
                write_plans(
                    &mut driver.image,
                    &mut sim,
                    spec.id.slot,
                    &image(sp.loop_state(spec.id).unwrap(), spec, None),
                );
            }
            sp.run(&rust.program, &mut jobs).unwrap();
            let expected = jobs
                .iter()
                .map(|j| serde_json::to_value(j).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(driver.execute(&mut sim, &[]), 0);
            assert_eq!(driver.drain(&mut sim), expected, "Q={quantum} tick={tick}");
            while jobs.pop().is_some() {}
        }
    }
}
#[shoop_test]
fn general_compiler_gain_balance_monitoring_and_multibus_samples_match_on_rtl() {
    let cap = [128, 64, 128, 32, 64, 2, 2];
    let model = suites::audio::design(cap)
        .build(&rt_fpga_testbench::root().join("work/marlin-suite/graph-compiler-audio"))
        .unwrap();
    let mut sim = Sim::new(model.session(None).unwrap(), &[("clk", 10000, 0)]);
    let mut driver = Audio::new(&mut sim, cap, 20261118);
    for quantum in [1, 16, 32] {
        let mut graph = session(2);
        graph.loops.clear();
        graph.strips[0].controls.input_monitoring = true;
        graph.strips[0].controls.input_gain_db = -3.;
        graph.strips[0].controls.output_balance = 0.5;
        graph.buses[0].gain_db = -6.;
        graph.buses[1].balance = -0.25;
        let logical = compile(
            &graph,
            CompileContext {
                quantum,
                limits: Default::default(),
            },
        )
        .unwrap();
        let rust = RustTarget::new(&[]).build(&logical).unwrap();
        let rtl = RtlTarget::new(cap, 160, &[]).build(&logical).unwrap();
        driver.load_compiled(&mut sim, &rtl);
        let r = rust.resources;
        let mut pipeline = Pipeline::prepare(
            rust.plan,
            rust.program,
            rust.memory,
            r.queue_capacity as usize,
        )
        .unwrap();
        let mut jobs = JobProcessor::with_history_slots(
            quantum as usize,
            r.buffers as usize,
            r.history_slots as usize,
        );
        let mut media = MediaStore::new(1).unwrap();
        for tick in 0..4 {
            let inputs = (0..2)
                .map(|ch| {
                    (0..quantum)
                        .map(|f| Fixed((f as i32 * 7919 + tick * 101 + ch * 13331) - 14000))
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            let mut output = vec![vec![Fixed::ZERO; quantum as usize]; 2];
            pipeline
                .process(
                    &mut jobs,
                    &mut media,
                    &inputs.iter().map(Vec::as_slice).collect::<Vec<_>>(),
                    &mut output.iter_mut().map(Vec::as_mut_slice).collect::<Vec<_>>(),
                )
                .unwrap();
            assert_eq!(
                driver
                    .execute(
                        &mut sim,
                        &inputs
                            .iter()
                            .map(|p| p.iter().map(|s| s.0).collect())
                            .collect::<Vec<_>>(),
                        true
                    )
                    .1,
                0
            );
            assert_eq!(
                driver.read_samples(&mut sim).1,
                output
                    .iter()
                    .map(|p| p.iter().map(|s| s.0).collect::<Vec<_>>())
                    .collect::<Vec<_>>()
            );
        }
    }
}
