#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use rt_fpga_compiler::{CompileError, FixedProgram, Resource, StateProgramTarget};
use rt_fpga_reference::{
    compiler::RustTarget,
    jobs::JobQueue,
    state::{PlannedTransition, StateProcessor},
    transitions::Operation,
};
use rt_fpga_testbench::{
    compiler::RtlTarget,
    drivers::state::State,
    loop_image::{image, write_plans},
    sim::Sim,
    suites,
};
use serde_json::json;
use shoop_wasm_test_support::shoop_test;

#[shoop_test]
fn compiled_mono_fixture_matches_rust_with_distinct_state_addresses() {
    let depths = [64, 16, 32];
    let mut design = suites::state::design(depths);
    design.parameters.insert("LOOP_DEPTH".into(), "160".into());
    let model = design
        .build(&rt_fpga_testbench::root().join("work/marlin-suite/sync"))
        .unwrap();
    let mut sim = Sim::new(model.session(None).unwrap(), &[("clk", 10000, 0)]);
    let mut driver = State::new(&mut sim, depths, 918);
    for quantum in [1, 16, 128] {
        let logical = FixedProgram::MonoLoops { quantum, loops: 2 }
            .emit()
            .unwrap();
        let views = [23, 41];
        let rust = RustTarget::new(&views).build(&logical).unwrap();
        let mut target = RtlTarget::new([64, 16, 32, 128, 8, 1, 2], 160, &views);
        target.state_base = 3;
        let rtl = target.build(&logical).unwrap();
        for (r, v) in rust.bindings.iter().zip(&rtl.bindings) {
            assert_eq!(r.gain.unwrap().0 + 3, usize::from(v.gain.unwrap()));
            assert_eq!(r.view.unwrap().0 + 3, usize::from(v.view.unwrap()));
            assert_eq!(r.meter.unwrap().0, usize::from(v.meter.unwrap()));
        }
        driver.reset(&mut sim);
        driver.image.load_compiled(&mut sim, &rtl);
        sim.model.set("sync_event_ready", 1);
        let specs = rust.plan.image().description.loops.clone();
        let program = rust.program;
        let mut sp = StateProcessor::with_loops(rust.plan, rust.memory);
        let mut queue = JobQueue::new(rust.resources.queue_capacity as usize);
        for tick in 0..32 {
            for (index, spec) in specs.iter().enumerate() {
                let operation = match (tick, index) {
                    (0, 0) | (16, _) => Some(Operation::Record),
                    (4, 0) | (12, 1) | (20, _) => Some(Operation::Play {
                        position: 0,
                        retrigger: true,
                    }),
                    (28, _) => Some(Operation::Stop),
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
            sp.run(&program, &mut queue).unwrap();
            let expected_jobs = queue
                .iter()
                .map(|job| serde_json::to_value(job).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(driver.execute(&mut sim, &[]), 0);
            assert_eq!(
                driver.drain(&mut sim),
                expected_jobs,
                "Q={quantum} tick={tick}"
            );
            while queue.pop().is_some() {}
            let actual = driver.snapshot(&mut sim);
            let values = actual["memory"].as_array().unwrap();
            assert!(
                values[..3]
                    .iter()
                    .all(|v| *v == json!({"kind":"word","value":0}))
            );
            assert_eq!(json!(&values[3..]), json!(sp.values()));
            assert_eq!(sim.model.get("sync_frame"), sp.frame());
        }
    }
}

#[shoop_test]
fn targets_reject_missing_media_and_physical_capacity_before_loading() {
    let logical = FixedProgram::MonoLoops {
        quantum: 16,
        loops: 2,
    }
    .emit()
    .unwrap();
    assert_eq!(
        RustTarget::new(&[]).build(&logical).err(),
        Some(CompileError::MissingView(0))
    );
    let mut target = RtlTarget::new([64, 16, 32, 128, 8, 1, 2], 160, &[23]);
    assert_eq!(
        target.build(&logical).err(),
        Some(CompileError::MissingView(1))
    );
    target.state_base = 13;
    assert_eq!(
        target.build(&logical).err(),
        Some(CompileError::Capacity(Resource::State))
    );
    target.state_base = 0;
    target.capacities.jobs = 10;
    assert_eq!(
        target.build(&logical).err(),
        Some(CompileError::Capacity(Resource::Jobs))
    );
}
