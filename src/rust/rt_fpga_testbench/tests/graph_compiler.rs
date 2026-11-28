#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
#[path = "../../rt_fpga_compiler/tests/support/composite.rs"]
mod composites;
#[path = "../../rt_fpga_compiler/tests/support/mod.rs"]
mod support;
use composites::{arbitration_graph, timing_loop};
use rt_fpga_compiler::shoop_graph::Graph;
use rt_fpga_compiler::{
    CompileContext, CompileError, Resource, StateProgramTarget, compile, compile_composites,
};
use rt_fpga_reference::{
    fixed::Fixed,
    jobs::{JobProcessor, JobQueue},
    pipeline::Pipeline,
    state::{PlannedTransition, StateProcessor},
    storage::MediaStore,
    target::RustTarget,
    transitions::Operation,
};
use rt_fpga_testbench::{
    drivers::{
        audio::Audio,
        state::State,
        store::{Geometry, Store},
    },
    loop_image::{image, read_loop, write_plans},
    memory::Mode,
    sim::Sim,
    suites,
    target::RtlTarget,
};
use serde_json::json;
use shoop_wasm_test_support::shoop_test;
use support::*;

fn composite_graph() -> Graph {
    let mut graph = session(1);
    graph.loops.pop();
    graph.strips[1].controls.input_monitoring = false;
    graph.loops.push(timing_loop(
        120,
        4,
        &[(121, 0, 2), (101, 1, 2), (101, 3, 4)],
    ));
    graph.loops.push(timing_loop(121, 2, &[(100, 0, 2)]));
    graph
}

fn canonical_jobs(
    mut jobs: Vec<serde_json::Value>,
) -> (
    Vec<serde_json::Value>,
    std::collections::BTreeMap<u64, Vec<serde_json::Value>>,
) {
    use std::collections::{BTreeMap, BTreeSet};
    let mut media = BTreeMap::<u64, Vec<serde_json::Value>>::new();
    let mut buffers = BTreeSet::new();
    let mut captures = BTreeSet::new();
    let mut starts = BTreeSet::new();
    let mut histories = BTreeSet::new();
    let mut outputs = BTreeSet::new();
    for job in &jobs {
        let kind = job["kind"].as_str().unwrap();
        let sources: &[&str] = match kind {
            "gain" | "smoothing" | "capture" | "output" => &["source"],
            "mix" => &["left", "right"],
            "input" | "silence" | "read_view" | "begin_capture" => &[],
            _ => panic!("unexpected audio job {job}"),
        };
        for field in sources {
            assert!(
                buffers.contains(&job[*field].as_u64().unwrap()),
                "read before write: {job}"
            );
        }
        if let Some(destination) = job.get("destination") {
            assert!(
                buffers.insert(destination.as_u64().unwrap()),
                "duplicate buffer write: {job}"
            );
        }
        if matches!(kind, "read_view" | "begin_capture" | "capture") {
            media
                .entry(job["view"].as_u64().unwrap())
                .or_default()
                .push(job.clone());
        }
        match kind {
            "begin_capture" => {
                let view = job["view"].as_u64().unwrap();
                assert!(
                    !captures.contains(&view),
                    "writer start after capture: {job}"
                );
                assert!(starts.insert(view), "duplicate writer start: {job}");
            }
            "capture" => assert!(captures.insert(job["view"].as_u64().unwrap())),
            "smoothing" => assert!(histories.insert(job["history"].as_u64().unwrap())),
            "output" => assert!(outputs.insert(job["port"].as_u64().unwrap())),
            _ => (),
        }
    }
    jobs.sort_by_cached_key(|job| {
        (
            job["kind"].as_str().unwrap().to_owned(),
            job.get("destination")
                .or_else(|| job.get("view"))
                .or_else(|| job.get("port"))
                .unwrap()
                .as_u64()
                .unwrap(),
        )
    });
    media.retain(|view, _| captures.contains(view) || starts.contains(view));
    (jobs, media)
}

#[shoop_test]
fn canonical_job_comparison_preserves_dependencies_writers_and_payloads() {
    use serde_json::json;
    let input = json!({"kind":"input","port":0,"destination":0});
    let other_input = json!({"kind":"input","port":1,"destination":1});
    let gain = json!({"kind":"gain","source":0,"destination":2,"ramp":{"target":1}});
    let other_gain = json!({"kind":"gain","source":1,"destination":3,"ramp":{"target":2}});
    let start = json!({"kind":"begin_capture","view":7});
    let capture = json!({"kind":"capture","view":7,"source":1});
    let output = json!({"kind":"output","port":0,"source":2});
    let other_output = json!({"kind":"output","port":1,"source":3});
    let valid = vec![
        input.clone(),
        other_input.clone(),
        gain.clone(),
        other_gain.clone(),
        start.clone(),
        capture.clone(),
        output.clone(),
        other_output.clone(),
    ];
    let reordered = vec![
        other_input.clone(),
        start.clone(),
        capture.clone(),
        other_gain,
        input.clone(),
        gain.clone(),
        other_output,
        output.clone(),
    ];
    assert_eq!(canonical_jobs(valid.clone()), canonical_jobs(reordered));
    let mut duplicate_output = valid.clone();
    duplicate_output.push(output);
    for invalid in [
        vec![gain, input.clone()],
        vec![input.clone(), input.clone()],
        vec![other_input.clone(), capture.clone(), start],
        vec![other_input, capture.clone(), capture],
        duplicate_output,
        vec![
            input,
            json!({"kind":"smoothing","source":0,"destination":4,"history":1}),
            json!({"kind":"smoothing","source":0,"destination":5,"history":1}),
        ],
    ] {
        assert!(std::panic::catch_unwind(|| canonical_jobs(invalid)).is_err());
    }
    let mut changed = valid.clone();
    changed[2]["ramp"]["target"] = json!(3);
    assert_ne!(canonical_jobs(valid.clone()), canonical_jobs(changed));
    let mut missing = valid.clone();
    missing.pop();
    assert_ne!(canonical_jobs(valid), canonical_jobs(missing));
    let input = json!({"kind":"input","port":0,"destination":0});
    let capture = json!({"kind":"capture","view":7,"source":0});
    let read =
        json!({"kind":"read_view","view":7,"destination":1,"position_chunks":0,"length_chunks":1});
    assert_ne!(
        canonical_jobs(vec![input.clone(), read.clone(), capture.clone()]),
        canonical_jobs(vec![input, capture, read]),
    );
}

#[shoop_test]
fn composite_seek_arbitrates_shared_children_on_a_sync_pulse() {
    use rt_fpga_reference::transitions::LoopId;
    let parent = |slot| LoopId {
        slot,
        generation: 1,
    };
    let depths = [64, 16, 16];
    let target = RtlTarget::new([64, 16, 16, 16, 16, 0, 0], 16, &[]);
    let graph = arbitration_graph();
    let context = CompileContext {
        quantum: 16,
        limits: target.limits(),
    };
    let mut logical = compile_composites(&graph, context, 16).unwrap();
    for (slot, len) in [(0, 32), (4, 48), (5, 48)] {
        logical.loops[slot].length = len;
    }
    let rtl = target.build(&logical).unwrap();
    let rust = RustTarget::new(&[]).build(&logical).unwrap();
    let specs = rust.plan.image().description.loops.clone();
    let mut sp = StateProcessor::with_loops(rust.plan, rust.memory);
    let model = suites::state::design(depths)
        .build(&rt_fpga_testbench::root().join("work/marlin-suite/composite-arbitration"))
        .unwrap();
    let mut sim = Sim::new(model.session(None).unwrap(), &[("clk", 10000, 0)]);
    let mut driver = State::new(&mut sim, depths, 717);
    driver.reset(&mut sim);
    driver.image.load_compiled(&mut sim, &rtl);
    sim.model.set("sync_event_ready", 1);
    let play = Operation::Play {
        position: 0,
        retrigger: true,
    };
    let seek = Operation::Seek {
        position: 32,
        retrigger: false,
    };
    let mut queue = JobQueue::new(16);
    for tick in 0..5 {
        for spec in &specs {
            let operation = match (tick, spec.id.slot) {
                (0, 0..=2 | 6) => Some(play),
                (2, 1) => Some(seek),
                (2, 4) => Some(Operation::Stop),
                _ => None,
            };
            if let Some(operation) = operation {
                sp.write_plans(
                    spec.id,
                    [
                        Some(PlannedTransition {
                            sequence: tick + 1,
                            operation,
                            effective: tick * 16,
                            on_pulse: false,
                            remaining_ticks: 0,
                        }),
                        None,
                    ],
                )
                .unwrap();
                write_plans(
                    &mut driver.image,
                    &mut sim,
                    spec.id.slot,
                    &image(sp.loop_state(spec.id).unwrap(), spec, None),
                );
            }
        }
        sp.run(&rust.program, &mut queue).unwrap();
        assert_eq!(driver.execute(&mut sim, &[]), 0);
        for spec in &specs {
            let slot = sp.loop_state(spec.id).unwrap();
            let row = read_loop(&mut driver.image, &mut sim, spec.id.slot);
            let expected = image(slot, spec, Some(tick * 16));
            for (offset, width) in [(33, 2), (53, 1), (128, 64), (512, 64)] {
                assert_eq!(
                    row.get(offset, width),
                    expected.get(offset, width),
                    "slot {} bit {offset} at tick {tick}",
                    spec.id.slot
                );
            }
        }
        if tick == 0 {
            assert!(!sp.loop_state(parent(4)).unwrap().render.plays());
            assert!(!sp.loop_state(parent(6)).unwrap().render.plays());
            assert!(sp.resolutions().iter().any(|r| r.target == parent(6)
                && r.outcome == rt_fpga_reference::transitions::Outcome::Applied { frame: 0 }));
        }
        if tick == 1 {
            assert!(sp.loop_state(parent(4)).unwrap().render.plays());
        }
        if tick == 2 {
            assert!(sp.loop_state(parent(0)).unwrap().pulse_this_quantum);
            assert!(!sp.loop_state(parent(4)).unwrap().render.plays());
            let nested = sp.loop_state(parent(5)).unwrap();
            assert_eq!(nested.render.position, 32);
            assert!(!nested.pulse_this_quantum);
        }
        if tick == 3 {
            assert!(!sp.loop_state(parent(2)).unwrap().render.plays());
        }
        assert_eq!(driver.drain(&mut sim).len(), queue.iter().count());
        while queue.pop().is_some() {}
    }
}

#[shoop_test]
fn composite_applies_sixty_four_child_actions_in_one_quantum() {
    const CHILDREN: u32 = 64;
    let depths = [256, 128, 16];
    let target = RtlTarget::new([256, 128, 16, 16, 16, 0, 0], 128, &[]);
    let mut graph = Graph::default();
    graph
        .loops
        .extend((0..CHILDREN).map(|slot| timing_loop(slot, 0, &[])));
    let children = (0..CHILDREN).map(|slot| (slot, 0, 1)).collect::<Vec<_>>();
    graph.loops.push(timing_loop(CHILDREN, 1, &children));
    let context = CompileContext {
        quantum: 16,
        limits: target.limits(),
    };
    let mut logical = compile_composites(&graph, context, 16).unwrap();
    for row in &mut logical.loops[..CHILDREN as usize] {
        row.length = 16;
    }
    let rtl = target.build(&logical).unwrap();
    assert_eq!(rtl.sync_count, CHILDREN as usize + 1);
    assert_eq!(
        rtl.instructions.len(),
        3 * CHILDREN as usize + 1 + logical.work_pass.len()
    );
    let rust = RustTarget::new(&[]).build(&logical).unwrap();
    let model = suites::state::design(depths)
        .build(&rt_fpga_testbench::root().join("work/marlin-suite/composite-fanout"))
        .unwrap();
    let mut sim = Sim::new(model.session(None).unwrap(), &[("clk", 10000, 0)]);
    let mut driver = State::new(&mut sim, depths, 636);
    driver.reset(&mut sim);
    driver.image.load_compiled(&mut sim, &rtl);
    sim.model.set("sync_event_ready", 1);
    let spec = *rust.plan.image().description.loops.last().unwrap();
    let mut sp = StateProcessor::with_loops(rust.plan, rust.memory);
    sp.write_plans(
        spec.id,
        [
            Some(PlannedTransition {
                effective: 0,
                sequence: 1,
                on_pulse: false,
                remaining_ticks: 0,
                operation: Operation::Play {
                    position: 0,
                    retrigger: true,
                },
            }),
            None,
        ],
    )
    .unwrap();
    write_plans(
        &mut driver.image,
        &mut sim,
        CHILDREN,
        &image(sp.loop_state(spec.id).unwrap(), &spec, None),
    );
    let mut jobs = JobQueue::new(rust.resources.queue_capacity as usize);
    sp.run(&rust.program, &mut jobs).unwrap();
    driver.start(&mut sim, &[]);
    let start = sim.cycles;
    sim.until(20_000, "64 child starts", |model| model.get("done") != 0);
    assert_eq!(sim.model.get("error_code"), 0);
    assert_eq!(
        sim.model.get("instruction_pc"),
        rtl.instructions.len() as u64
    );
    println!(
        "64 child starts in {} simulated SP cycles",
        sim.cycles - start
    );
    assert_eq!(sim.model.get("sync_frame"), 16);
    for child in 0..CHILDREN {
        let state = sp
            .loop_state(rt_fpga_reference::transitions::LoopId {
                slot: child,
                generation: 1,
            })
            .unwrap();
        assert!(state.render.plays());
        let row = read_loop(&mut driver.image, &mut sim, child);
        assert_eq!(row.get(33, 2), 1);
        assert_eq!(row.get(53, 1), 1);
        assert_eq!(row.get(576, 64), 0);
    }
}

#[shoop_test]
fn composite_schedule_uses_program_capacity_for_many_spans_and_parents() {
    let depths = [128, 16, 16];
    let target = RtlTarget::new([128, 16, 16, 16, 16, 0, 0], 16, &[]);
    let spans = (0..70).map(|i| (0, 2 * i, 2 * i + 1)).collect::<Vec<_>>();
    let mut graph = Graph::default();
    graph
        .loops
        .extend([timing_loop(0, 0, &[]), timing_loop(1, 140, &spans)]);
    graph
        .loops
        .extend((2..=9).map(|slot| timing_loop(slot, 2, &[(0, 0, 1)])));
    let limits = target.limits();
    let mut logical = compile_composites(
        &graph,
        CompileContext {
            quantum: 16,
            limits,
        },
        16,
    )
    .unwrap();
    logical.loops[0].length = 16;
    let rtl = target.build(&logical).unwrap();
    assert_eq!(rtl.sync_count, 10);
    assert_eq!(
        rtl.instructions.len(),
        10 + 1 + 70 + 8 * 2 + logical.work_pass.len()
    );
    let too_small = rt_fpga_compiler::ProgramLimits {
        instructions: rtl.instructions.len() as u32 - 1,
        ..limits
    };
    assert_eq!(
        logical.validate_logical(too_small),
        Err(CompileError::Capacity(Resource::Instructions))
    );

    let rust = RustTarget::new(&[]).build(&logical).unwrap();
    let specs = rust.plan.image().description.loops.clone();
    let mut sp = StateProcessor::with_loops(rust.plan, rust.memory);
    let model = suites::state::design(depths)
        .build(&rt_fpga_testbench::root().join("work/marlin-suite/composite-sparse"))
        .unwrap();
    let mut sim = Sim::new(model.session(None).unwrap(), &[("clk", 10_000, 0)]);
    let mut driver = State::new(&mut sim, depths, 636);
    driver.reset(&mut sim);
    driver.image.load_compiled(&mut sim, &rtl);
    sim.model.set("sync_event_ready", 1);
    let mut jobs = JobQueue::new(rust.resources.queue_capacity as usize);
    for (tick, (slot, position)) in [(9, 0), (1, 138 * 16)].into_iter().enumerate() {
        let spec = specs.iter().find(|spec| spec.id.slot == slot).unwrap();
        let plan = PlannedTransition {
            effective: tick as u64 * 16,
            sequence: 1,
            on_pulse: false,
            remaining_ticks: 0,
            operation: Operation::Play {
                position,
                retrigger: true,
            },
        };
        sp.write_plans(spec.id, [Some(plan), None]).unwrap();
        write_plans(
            &mut driver.image,
            &mut sim,
            slot,
            &image(sp.loop_state(spec.id).unwrap(), spec, None),
        );
        sp.run(&rust.program, &mut jobs).unwrap();
        driver.start(&mut sim, &[]);
        sim.until(20_000, "large composite schedule", |model| {
            model.get("done") != 0
        });
        assert_eq!(sim.model.get("error_code"), 0);
        assert_eq!(
            sim.model.get("instruction_pc"),
            rtl.instructions.len() as u64
        );
        let child = sp.loop_state(specs[0].id).unwrap();
        assert!(child.render.plays() && child.pulse_this_quantum);
        let row = read_loop(&mut driver.image, &mut sim, 0);
        assert_eq!(row.get(33, 2), 1);
        assert_eq!(row.get(53, 1), 1);
        assert_eq!(row.get(128, 64), child.state.position);
        assert_eq!(driver.drain(&mut sim).len(), jobs.iter().count());
        while jobs.pop().is_some() {}
    }
}

#[shoop_test]
fn bounded_composite_spans_drive_existing_media_jobs_in_rust_and_rtl() {
    const Q: usize = 16;
    const CAPACITY: [u64; 7] = [128, 128, 128, Q as u64, 64, 1, 1];
    let views = [0, 1];
    let target = RtlTarget::new(CAPACITY, 128, &views);
    let context = CompileContext {
        quantum: Q as u32,
        limits: target.limits(),
    };
    let logical = compile_composites(&composite_graph(), context, Q as u32).unwrap();
    let rtl = target.build(&logical).unwrap();
    let rust = RustTarget::new(&views).build(&logical).unwrap();
    assert_eq!(rtl.sync_count, logical.sync_pass.len());
    assert_eq!(
        rtl.instructions.len(),
        logical.work_pass.len() + logical.sync_pass.len() + 7
    );
    assert_eq!(rtl.bindings.iter().filter(|b| b.view.is_some()).count(), 2);
    assert_eq!(rtl.bindings.iter().filter(|b| b.gain.is_some()).count(), 2);
    let too_small = CompileContext {
        limits: rt_fpga_compiler::ProgramLimits {
            instructions: rtl.instructions.len() as u32 - 1,
            ..context.limits
        },
        ..context
    };
    assert_eq!(
        compile_composites(&composite_graph(), too_small, Q as u32),
        Err(CompileError::Capacity(Resource::Instructions)),
    );
    let mut cyclic = composite_graph();
    cyclic.loops[2].sync_source = Some(id(100));
    assert_eq!(
        compile_composites(&cyclic, context, Q as u32),
        Err(CompileError::InvalidProgram)
    );

    let geometry = Geometry::new([8, 256, Q as u64], 8);
    let design = suites::pipeline::design(CAPACITY, &geometry, 64, Mode::Fixed, 1);
    let model = design
        .build(&rt_fpga_testbench::root().join("work/marlin-suite/composite-playback"))
        .unwrap();
    let mut sim = Sim::new(model.session(None).unwrap(), &[("clk", 10000, 0)]);
    let mut driver = Audio::new(&mut sim, CAPACITY, 217);
    driver.load_compiled(&mut sim, &rtl);
    let mut store = Store::new(&mut sim, geometry.clone(), Mode::Fixed, 31, "store_").unwrap();
    store.load(
        &mut sim,
        &json!({
            "chunk_frames": Q,
            "chunk_limit": geometry.slots,
            "block_chunks": geometry.block_chunks,
            "samples": [],
            "free_chunks": [],
            "views": [{"chunks": [], "frames": 0}, {"chunks": [], "frames": 0}],
        }),
    );
    sim.model.set("sync_event_ready", 1);
    let specs = rust.plan.image().description.loops.clone();
    let resources = rust.resources;
    let mut pipeline = Pipeline::prepare(
        rust.plan,
        rust.program,
        rust.memory,
        resources.queue_capacity as usize,
    )
    .unwrap();
    pipeline.trace_jobs(true);
    let mut jobs = JobProcessor::with_history_slots(
        Q,
        resources.buffers as usize,
        resources.history_slots as usize,
    );
    let mut media = MediaStore::with_block_chunks(Q, geometry.block_chunks as usize).unwrap();
    media.create_recordings(2).unwrap();
    let play = Operation::Play {
        position: 0,
        retrigger: true,
    };
    for (tick, expected) in [
        0, 0, 0, 0x080000, 0, 0x080000, 0x100000, 0x100000, 0x180000, 0,
    ]
    .into_iter()
    .enumerate()
    {
        for spec in &specs {
            let operation = match (tick, spec.id.slot) {
                (0, 100 | 101) => Some(Operation::Record),
                (1, 100 | 101) | (2, 100) | (9, 120) => Some(Operation::Stop),
                (2 | 7, 120) => Some(play),
                _ => None,
            };
            if let Some(operation) = operation {
                pipeline
                    .write_plans(
                        spec.id,
                        [
                            Some(PlannedTransition {
                                effective: (tick * Q) as u64,
                                sequence: tick as u64 + 1,
                                on_pulse: false,
                                remaining_ticks: 0,
                                operation,
                            }),
                            None,
                        ],
                    )
                    .unwrap();
                write_plans(
                    &mut driver.program,
                    &mut sim,
                    spec.id.slot,
                    &image(pipeline.state().loop_state(spec.id).unwrap(), spec, None),
                );
            }
        }
        let sample = if tick == 0 { 0x200000 } else { 0 };
        let mut output = [Fixed::ZERO; Q];
        pipeline
            .process(
                &mut jobs,
                &mut media,
                &[&[Fixed(sample); Q]],
                &mut [&mut output],
            )
            .unwrap();
        assert_eq!(output, [Fixed(expected); Q], "reference quantum {tick}");
        assert_eq!(
            driver.execute(&mut sim, &[vec![sample; Q]], false).1,
            0,
            "RTL quantum {tick}"
        );
        assert_eq!(
            driver.read_samples(&mut sim).1,
            vec![vec![expected; Q]],
            "RTL quantum {tick}"
        );
        assert_eq!(
            canonical_jobs(driver.consumed()),
            canonical_jobs(
                pipeline
                    .emitted_jobs()
                    .iter()
                    .map(|j| serde_json::to_value(j).unwrap())
                    .collect::<Vec<_>>()
            )
        );
        for spec in &specs {
            let state = pipeline.state().loop_state(spec.id).unwrap();
            let row = read_loop(&mut driver.program, &mut sim, spec.id.slot);
            let expected = image(state, spec, Some((tick * Q) as u64));
            assert_eq!(
                row.get(33, 2),
                expected.get(33, 2),
                "mode for {} at quantum {tick}",
                spec.id.slot
            );
            assert_eq!(
                row.get(53, 1),
                expected.get(53, 1),
                "pulse for {} at quantum {tick}",
                spec.id.slot
            );
            assert_eq!(
                row.get(128, 64),
                state.state.position,
                "cursor for {} at quantum {tick}",
                spec.id.slot
            );
        }
    }
}

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
            assert_eq!(
                canonical_jobs(driver.drain(&mut sim)),
                canonical_jobs(expected),
                "Q={quantum} tick={tick}"
            );
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

#[shoop_test]
fn composite_record_preflight_preserves_media_on_rust_and_rtl() {
    use rt_fpga_compiler::shoop_graph::{CompositeKind, LoopMode};
    let mut graph = session(1);
    graph.loops.truncate(1);
    graph.loops[0].gain = 1.0;
    graph.strips[1].controls.input_monitoring = false;
    let mut parent = timing_loop(120, 3, &[(100, 0, 2), (100, 2, 3)]);
    let plan = parent.composite.as_mut().unwrap();
    plan.kind = CompositeKind::Explicit;
    plan.spans.to_mut()[0].mode = LoopMode::Recording;
    graph.loops.push(parent);

    const CAP: [u64; 7] = [128, 128, 128, 16, 64, 1, 1];
    let target = RtlTarget::new(CAP, 128, &[0]);
    let context = CompileContext {
        quantum: 16,
        limits: target.limits(),
    };
    let logical = compile_composites(&graph, context, 16).unwrap();
    let geometry = Geometry::new([8, 32, 16], 1);
    let model = suites::pipeline::design(CAP, &geometry, 64, Mode::Fixed, 1)
        .build(&rt_fpga_testbench::root().join("work/marlin-suite/composite-recording"))
        .unwrap();

    let old = 0x100000;
    let fresh = 0x200000;
    for (capacity, prior_take, expected) in [(1, true, old), (1, false, fresh), (2, true, fresh)] {
        let mut logical = logical.clone();
        let mut media = MediaStore::with_chunk_limit(16, capacity).unwrap();
        if prior_take {
            media.insert(&[Fixed(old); 16]).unwrap();
            logical.loops[0].length = 16;
        } else {
            media.create_recording().unwrap();
        }
        let rtl = target.build(&logical).unwrap();
        let rust = RustTarget::new(&[0]).build(&logical).unwrap();
        let spec = *rust
            .plan
            .image()
            .description
            .loops
            .iter()
            .find(|x| x.id.slot == 120)
            .unwrap();
        let resources = rust.resources;
        let mut pipeline = Pipeline::prepare(
            rust.plan,
            rust.program,
            rust.memory,
            resources.queue_capacity as usize,
        )
        .unwrap();
        pipeline.trace_jobs(true);
        let mut jobs = JobProcessor::new(16, resources.buffers as usize);
        let mut sim = Sim::new(model.session(None).unwrap(), &[("clk", 10000, 0)]);
        let mut driver = Audio::new(&mut sim, CAP, 417);
        driver.load_compiled(&mut sim, &rtl);
        let mut store = Store::new(&mut sim, geometry.clone(), Mode::Fixed, 514, "store_").unwrap();
        store.load(&mut sim, &serde_json::to_value(media.snapshot()).unwrap());
        sim.model.set("sync_event_ready", 1);

        pipeline
            .write_plans(
                spec.id,
                [
                    Some(PlannedTransition {
                        sequence: 1,
                        operation: Operation::Play {
                            position: 0,
                            retrigger: true,
                        },
                        effective: 0,
                        on_pulse: false,
                        remaining_ticks: 0,
                    }),
                    None,
                ],
            )
            .unwrap();
        write_plans(
            &mut driver.program,
            &mut sim,
            spec.id.slot,
            &image(pipeline.state().loop_state(spec.id).unwrap(), &spec, None),
        );
        for (tick, sample) in [fresh, fresh * 2, 0].into_iter().enumerate() {
            let mut output = [Fixed::ZERO; 16];
            pipeline
                .process(
                    &mut jobs,
                    &mut media,
                    &[&[Fixed(sample); 16]],
                    &mut [&mut output],
                )
                .unwrap();
            assert_eq!(output, [Fixed(if tick == 2 { expected } else { 0 }); 16]);
            assert_eq!(
                driver.execute(&mut sim, &[vec![sample; 16]], false).1,
                0,
                "recording step {tick}"
            );
            assert_eq!(
                driver.read_samples(&mut sim).1,
                vec![output.iter().map(|sample| sample.0).collect::<Vec<_>>()]
            );
            assert_eq!(
                driver.consumed(),
                pipeline
                    .emitted_jobs()
                    .iter()
                    .map(|job| serde_json::to_value(job).unwrap())
                    .collect::<Vec<_>>()
            );
        }
    }
}
