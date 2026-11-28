#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
#[path = "../../rt_fpga_compiler/tests/support/mono.rs"]
mod mono;
use rt_fpga_compiler::{CompileContext, compile};
#[path = "../../rt_fpga_compiler/tests/support/programs.rs"]
mod programs;
use rt_fpga_compiler::{Operation, StateProgramTarget, loading::Allocator};
use rt_fpga_reference::{
    fixed::Fixed, jobs::JobProcessor, pipeline::Pipeline, state::PlannedTransition,
    storage::MediaStore, target::RustTarget, transitions::Operation as Transition,
};
use rt_fpga_testbench::{
    drivers::{
        audio::Audio,
        loading::{Commands, Upload, descriptor},
    },
    loop_image,
    sim::Sim,
    suites,
    target::{ProgramImage, RtlTarget},
};
use serde_json::json;
use shoop_rt_protocol::immediate::{Code, Command, CommandRecord, Header};
use shoop_rt_transport::rt;
use shoop_wasm_test_support::shoop_test;
use std::{cell::RefCell, rc::Rc};

const CAP: [u64; 7] = [64, 32, 64, 16, 16, 1, 2];

#[shoop_test]
fn ddr_instruction_fault_discards_the_partial_prefetch_and_next_run_recovers() {
    use rt_fpga_testbench::{
        drivers::store::{Geometry, Store},
        memory::Mode,
    };
    let geometry = Geometry::new([8, 128, 16], 1);
    let design = suites::pipeline::design(CAP, &geometry, 64, Mode::Fixed, 1)
        .parameters(&[("DDR_PROGRAM", 1)]);
    let compiled = design
        .build(&rt_fpga_testbench::root().join("work/marlin-suite/program-fetch-fault"))
        .unwrap();
    let mut sim = Sim::new(compiled.session(None).unwrap(), &[("clk", 10000, 0)]);
    let mut audio = Audio::new(&mut sim, CAP, 219);
    audio.reset(&mut sim);
    let mut store = Store::new(&mut sim, geometry, Mode::Fixed, 223, "store_").unwrap();
    store.reserve_ddr_program(CAP[0] as usize, 1);
    store.reset(&mut sim);
    sim.model.set("maintenance_enable", 0);
    audio.program.load(
        &mut sim,
        &json!({
            "program": {
                "instructions": [
                    {"kind": "process_silence", "destination": 0},
                    {"kind": "process_output", "source": 0, "port": 0}
                ],
                "quantum": 16, "chunk_frames": 16, "jobs_per_quantum": 2
            },
            "initial_state": {"memory": []},
            "queue_capacity": CAP[2]
        }),
    );
    audio.buffers = 1;
    audio.outputs = 1;
    audio.frames = 16;
    store.memory.borrow_mut().control.fail_next = true;
    assert_eq!(audio.execute(&mut sim, &[], false), (1, 4));
    assert!(audio.consumed().is_empty());
    assert_eq!(audio.execute(&mut sim, &[], false), (2, 0));
    assert_eq!(audio.consumed().len(), 2);
}

fn parameters(d: &mut Audio, image: &ProgramImage) {
    let c = &image.compiled;
    d.frames = u64::from(c.quantum);
    d.buffers = u64::from(c.resources.buffers);
    d.outputs = u64::from(c.resources.output_ports);
    d.program.state_count = c.state.len();
    d.program.program = json!({"instructions":vec![0;c.instructions.len()],"quantum":c.quantum,
        "chunk_frames":c.chunk_frames,"sync_count":c.sync_count,"jobs_per_quantum":c.jobs_per_quantum});
}
fn upload(s: &mut Sim<'_>, image: &ProgramImage) -> Rc<RefCell<Upload>> {
    let u = Rc::new(RefCell::new(Upload::new(image, CAP[0])));
    s.agent(u.clone());
    u
}
fn finish_upload(s: &mut Sim<'_>, u: &Rc<RefCell<Upload>>) {
    for _ in 0..10000 {
        if u.borrow().complete() {
            return;
        }
        s.idle(1);
    }
    panic!("upload did not finish");
}
fn select(
    s: &mut Sim<'_>,
    bridge: &mut Commands,
    control: &mut rt::Control,
    execution: &mut rt::Execution,
    image: &ProgramImage,
    frame: u64,
) {
    select_with_boundary(s, bridge, control, execution, image, frame, "run_valid");
}
fn select_with_boundary(
    s: &mut Sim<'_>,
    bridge: &mut Commands,
    control: &mut rt::Control,
    execution: &mut rt::Execution,
    image: &ProgramImage,
    frame: u64,
    signal: &str,
) {
    control
        .commands
        .push(
            Header::new(image.layout.expected, image.layout.expected, None),
            &[CommandRecord::new(Command::SwitchProgram(descriptor(
                image,
            )))],
        )
        .unwrap();
    bridge.boundary(&mut s.model, execution, frame);
    s.idle(4);
    assert_eq!(
        s.model.get("active_revision"),
        image.layout.expected,
        "publication must not pace processing"
    );
    s.model.set(signal, 1);
    for _ in 0..10000 {
        s.idle(1);
        bridge.selected(&mut s.model);
        if s.model.get("active_revision") == image.layout.revision {
            break;
        }
    }
    assert_eq!(s.model.get("active_revision"), image.layout.revision);
    s.model.set(signal, 0);
    assert_eq!(s.model.get("sync_frame"), frame);
}
fn acknowledge(
    s: &mut Sim<'_>,
    bridge: &mut Commands,
    control: &mut rt::Control,
    execution: &mut rt::Execution,
    frame: u64,
) {
    s.until(20000, "switch fence", |m| {
        m.get("switch_response_valid") != 0
    });
    bridge.boundary(&mut s.model, execution, frame);
    let response = control.commands.response().unwrap();
    assert_eq!(response.header.code(), Code::Applied);
    assert_eq!(response.header.frame(), frame);
    drop(response);
    s.idle(1);
    s.model.set("switch_response_ready", 0);
}
fn quantum(
    d: &mut Audio,
    s: &mut Sim<'_>,
    p: &mut Pipeline,
    j: &mut JobProcessor,
    m: &mut MediaStore,
    tick: i32,
) {
    let input = (0..16)
        .map(|i| Fixed((i * 7919 + tick * 101) - 14000))
        .collect::<Vec<_>>();
    let mut output = vec![vec![Fixed::ZERO; 16]; d.outputs as usize];
    let mut outputs = output.iter_mut().map(Vec::as_mut_slice).collect::<Vec<_>>();
    p.process(j, m, &[&input], &mut outputs).unwrap();
    assert_eq!(
        d.execute(s, &[input.iter().map(|v| v.0).collect()], true).1,
        0
    );
    assert_eq!(
        d.read_samples(s).1,
        output
            .iter()
            .map(|port| port.iter().map(|v| v.0).collect::<Vec<_>>())
            .collect::<Vec<_>>()
    );
    assert_eq!(s.model.get("sync_frame"), p.state().frame());
}

#[shoop_test]
fn live_regions_preserve_ramps_history_plans_and_frames_with_concurrent_uploads() {
    let mut design = suites::audio::design(CAP);
    design.parameters.insert("LIVE_LOADING".into(), "1".into());
    design
        .parameters
        .insert("PROGRAM_REGIONS".into(), "2".into());
    let model = design
        .build(&rt_fpga_testbench::root().join("work/marlin-suite/loading"))
        .unwrap();
    let mut s = Sim::new(model.session(None).unwrap(), &[("clk", 10000, 0)]);
    let mut d = Audio::loading(&mut s, CAP, 419);
    d.reset(&mut s);
    s.model.set_all(&[
        ("sync_event_ready", 1),
        ("handoff_complete", 1),
        ("switch_boundary", 0),
        ("local_valid", 0),
        ("switch_valid", 0),
        ("switch_response_ready", 0),
    ]);
    let mut target = RtlTarget::new(CAP, 32, &[]);
    target.state_base = 3;
    let mut limits = target.limits();
    limits.state_values -= 3;
    limits.jobs = CAP[2] as u32;
    let mut allocator = Allocator::new(limits);
    let logical = programs::sync_smoothing(16);
    let a = allocator.stage(&logical).unwrap();
    let mut rust = RustTarget::new(&[]);
    rust.limits = limits;
    let rust_image = rust.build_image(&logical, a.clone()).unwrap();
    let spec = rust_image.compiled.plan.image().description.loops[0].clone();
    let mut p = Pipeline::from_image(rust_image, limits).unwrap();
    let physical = a.loop_id(logical.loops[0].id).unwrap();
    let id = rt_fpga_reference::transitions::LoopId {
        slot: physical.slot,
        generation: physical.generation,
    };
    p.write_plans(
        id,
        [
            Some(PlannedTransition {
                sequence: 1,
                operation: Transition::Play {
                    position: 0,
                    retrigger: true,
                },
                on_pulse: false,
                effective: 0,
                remaining_ticks: 0,
            }),
            Some(PlannedTransition {
                sequence: 2,
                operation: Transition::Seek {
                    position: 16,
                    retrigger: true,
                },
                on_pulse: false,
                effective: 16 * 7,
                remaining_ticks: 0,
            }),
        ],
    )
    .unwrap();
    let mut image = target.build_image(&logical, a.clone()).unwrap();
    let row = loop_image::image(p.state().loop_state(id).unwrap(), &spec, None);
    for i in 0..16 {
        image.new_loops[0].words[i] = row.get(i * 64, 64);
    }
    parameters(&mut d, &image);
    let u = upload(&mut s, &image);
    finish_upload(&mut s, &u);
    let (mut control, mut execution) = rt::channel(8, 2, 128).unwrap();
    let mut bridge = Commands::default();
    allocator.publish(a.revision).unwrap();
    select_with_boundary(
        &mut s,
        &mut bridge,
        &mut control,
        &mut execution,
        &image,
        0,
        "switch_boundary",
    );
    acknowledge(&mut s, &mut bridge, &mut control, &mut execution, 0);
    allocator.complete(a.revision).unwrap();
    let mut jobs = JobProcessor::with_history_slots(16, 16, 16);
    let mut media = MediaStore::new(1).unwrap();
    s.model.set_all(&[
        ("cfg_valid", 1),
        ("cfg_state", 0),
        ("cfg_loop", 0),
        ("cfg_write", 1),
        ("cfg_address", CAP[0]),
    ]);
    for tick in 0..3 {
        quantum(&mut d, &mut s, &mut p, &mut jobs, &mut media, tick);
    }
    s.model.set("cfg_valid", 0);
    s.idle(2);
    let mut next = logical.clone();
    next.loops[0].sync_source = Some(next.loops[0].id);
    if let rt_fpga_compiler::InitialValue::Smoothing { destination, .. } =
        &mut next.initial_state[0]
    {
        *destination = 5;
    }
    for op in &mut next.work_pass {
        if let Operation::Output { source, .. } = op {
            *source = 5;
        }
    }
    let b = allocator.stage(&next).unwrap();
    let next_image = target.build_image(&next, b.clone()).unwrap();
    assert!(next_image.initializers.is_empty() && next_image.new_loops.is_empty());
    let u = upload(&mut s, &next_image);
    u.borrow_mut().during_execution();
    for tick in 3..6 {
        quantum(&mut d, &mut s, &mut p, &mut jobs, &mut media, tick);
    }
    finish_upload(&mut s, &u);
    assert!(u.borrow().accepted_during_execution > 0);
    let mut rust_image = rust.build_image(&next, b.clone()).unwrap();
    s.model.set("handoff_complete", 0);
    allocator.publish(b.revision).unwrap();
    select_with_boundary(
        &mut s,
        &mut bridge,
        &mut control,
        &mut execution,
        &next_image,
        96,
        "switch_boundary",
    );
    s.idle(20);
    assert_eq!(s.model.get("switch_response_valid"), 0);
    assert!(control.commands.response().is_none());
    assert!(allocator.stage(&logical).is_err());
    assert_eq!(allocator.current().unwrap().revision, a.revision);
    p.switch_image(&mut rust_image).unwrap();
    parameters(&mut d, &next_image);
    assert_eq!(
        s.model.get("run_ready"),
        1,
        "a pending handoff must not hold rendering"
    );
    for tick in 6..9 {
        quantum(&mut d, &mut s, &mut p, &mut jobs, &mut media, tick);
        assert_eq!(s.model.get("switch_response_valid"), 0);
        assert!(control.commands.response().is_none());
        assert_eq!(allocator.current().unwrap().revision, a.revision);
    }
    s.model.set("handoff_complete", 1);
    s.until(100, "handoff", |m| m.get("switch_response_valid") != 0);
    for tick in 9..11 {
        quantum(&mut d, &mut s, &mut p, &mut jobs, &mut media, tick);
    }
    assert_eq!(
        s.model.get("switch_response_valid"),
        1,
        "completion is held under backpressure"
    );
    acknowledge(&mut s, &mut bridge, &mut control, &mut execution, 96);
    allocator.complete(b.revision).unwrap();
    let actual = loop_image::read_loop(&mut d.program, &mut s, physical.slot);
    let expected = loop_image::image(p.state().loop_state(id).unwrap(), &spec, Some(160));
    for word in 2..10 {
        assert_eq!(
            actual.get(word * 64, 64),
            expected.get(word * 64, 64),
            "word {word}"
        );
    }
    control
        .commands
        .push(
            Header::new(a.revision, a.revision, None),
            &[CommandRecord::new(Command::SwitchProgram(descriptor(
                &image,
            )))],
        )
        .unwrap();
    bridge.boundary(&mut s.model, &mut execution, 176);
    assert_eq!(
        control.commands.response().unwrap().header.code(),
        Code::StaleConfiguration
    );
    control
        .commands
        .push(
            Header::new(b.revision, b.revision, Some(175)),
            &[CommandRecord::new(Command::SwitchProgram(descriptor(
                &next_image,
            )))],
        )
        .unwrap();
    bridge.boundary(&mut s.model, &mut execution, 176);
    assert_eq!(
        control.commands.response().unwrap().header.code(),
        Code::DeadlineMissed
    );
    let c = allocator.stage(&logical).unwrap();
    let cancelled = target.build_image(&logical, c.clone()).unwrap();
    let u = upload(&mut s, &cancelled);
    finish_upload(&mut s, &u);
    allocator.cancel(c.revision).unwrap();
    quantum(&mut d, &mut s, &mut p, &mut jobs, &mut media, 11);
    assert_eq!(s.model.get("active_revision"), b.revision);

    for generation in 8..12 {
        let mut replacement = next.clone();
        replacement.loops[0].id.generation = generation;
        replacement.loops[0].sync_source = Some(replacement.loops[0].id);
        replacement.sync_pass[0].generation = generation;
        replacement.bindings[0].id.generation = generation;
        for binding in &mut replacement.state_bindings {
            if let rt_fpga_compiler::shoop_graph::Owner::Loop(id) = &mut binding.key.owner {
                id.generation = generation;
            }
        }
        for op in &mut replacement.work_pass {
            if let Operation::Smoothing { loop_id, .. } = op {
                loop_id.generation = generation;
            }
        }
        let layout = allocator.stage(&replacement).unwrap();
        let replacement_image = target.build_image(&replacement, layout.clone()).unwrap();
        assert!(!replacement_image.initializers.is_empty());
        assert_eq!(replacement_image.new_loops.len(), 1);
        let u = upload(&mut s, &replacement_image);
        finish_upload(&mut s, &u);
        let frame = p.state().frame();
        allocator.publish(layout.revision).unwrap();
        select(
            &mut s,
            &mut bridge,
            &mut control,
            &mut execution,
            &replacement_image,
            frame,
        );
        acknowledge(&mut s, &mut bridge, &mut control, &mut execution, frame);
        allocator.complete(layout.revision).unwrap();
        let mut ri = rust.build_image(&replacement, layout.clone()).unwrap();
        p.switch_image(&mut ri).unwrap();
        for &history in layout.new_histories() {
            jobs.initialize_history(history as usize).unwrap();
        }
        parameters(&mut d, &replacement_image);
        quantum(
            &mut d,
            &mut s,
            &mut p,
            &mut jobs,
            &mut media,
            generation as i32,
        );
    }
    let mut empty = logical.clone();
    empty.loops.clear();
    empty.sync_pass.clear();
    empty.initial_state.clear();
    empty.state_bindings.clear();
    empty.bindings.clear();
    empty.work_pass = vec![
        Operation::Silence { destination: 0 },
        Operation::Output { source: 0, port: 0 },
    ];
    empty.jobs_per_quantum = 2;
    empty.resources.queue_capacity = 2;
    empty.resources.buffers = 1;
    empty.resources.history_slots = 0;
    for program in [&empty, &logical] {
        let layout = allocator.stage(program).unwrap();
        let image = target.build_image(program, layout.clone()).unwrap();
        let u = upload(&mut s, &image);
        finish_upload(&mut s, &u);
        let frame = p.state().frame();
        allocator.publish(layout.revision).unwrap();
        select(
            &mut s,
            &mut bridge,
            &mut control,
            &mut execution,
            &image,
            frame,
        );
        acknowledge(&mut s, &mut bridge, &mut control, &mut execution, frame);
        allocator.complete(layout.revision).unwrap();
        let mut ri = rust.build_image(program, layout.clone()).unwrap();
        p.switch_image(&mut ri).unwrap();
        for &history in layout.new_histories() {
            jobs.initialize_history(history as usize).unwrap();
        }
        parameters(&mut d, &image);
        for tick in 20..23 {
            quantum(&mut d, &mut s, &mut p, &mut jobs, &mut media, tick);
        }
    }
    d.reset(&mut s);
    assert_eq!(s.model.get("active_revision"), 0);
    assert_eq!(s.model.get("switch_response_valid"), 0);
    assert_eq!(s.model.get("sync_frame"), 0);
    for _ in 0..3 {
        assert_eq!(d.execute(&mut s, &[vec![0; 16]], false).1, 0);
    }
    assert_eq!(s.model.get("active_revision"), 0);
    assert_eq!(s.model.get("sync_frame"), 48);
}

#[shoop_test]
fn live_media_graph_switch_preserves_recording_and_fences_axi_work() {
    use rt_fpga_compiler::Identity;
    use rt_fpga_testbench::{
        drivers::store::{Geometry, Store},
        memory::Mode,
    };
    for (mode, width, ddr_program) in [
        (Mode::Instant, 64, false),
        (Mode::Fixed, 64, true),
        (Mode::Fixed, 32, true),
    ] {
        let geometry = Geometry::new([8, 128, 16], 1);
        let mut design = suites::pipeline::design(CAP, &geometry, width, mode, 1);
        design
            .parameters
            .insert("DDR_PROGRAM".into(), u8::from(ddr_program).to_string());
        design.parameters.insert("LIVE_LOADING".into(), "1".into());
        design
            .parameters
            .insert("PROGRAM_REGIONS".into(), "2".into());
        let model = design
            .build(&rt_fpga_testbench::root().join("work/marlin-suite/loading-media"))
            .unwrap();
        let mut s = Sim::new(model.session(None).unwrap(), &[("clk", 10000, 0)]);
        let mut d = Audio::loading(&mut s, CAP, 733);
        d.reset(&mut s);
        let mut store = Store::new(&mut s, geometry, mode, 991, "store_").unwrap();
        if ddr_program {
            store.reserve_ddr_program(CAP[0] as usize, 2);
        }
        store.load(&mut s,&json!({"chunk_frames":16,"chunk_limit":128,"block_chunks":1,"samples":[],"free_chunks":[],
            "views":[{"chunks":[],"frames":0}]}));
        let mut logical = compile(
            &mono::session(
                &(vec![mono::MonoLoop {
                    id: Identity {
                        slot: 0,
                        generation: 1,
                    },
                    source: None,
                    input: true,
                    output: true,
                    gain: 0.5,
                    repeat_sync: false,
                }]),
            ),
            CompileContext {
                quantum: 16,
                limits: Default::default(),
            },
        )
        .unwrap();
        logical.chunk_frames = 16;
        let mut target = RtlTarget::new(CAP, 32, &[0]);
        target.state_base = 3;
        let mut limits = target.limits();
        limits.state_values -= 3;
        limits.jobs = CAP[2] as u32;
        let mut allocator = Allocator::new(limits);
        let a = allocator.stage(&logical).unwrap();
        let mut rust = RustTarget::new(&[0]);
        rust.limits = limits;
        let ri = rust.build_image(&logical, a.clone()).unwrap();
        let spec = ri.compiled.plan.image().description.loops[0].clone();
        let id = spec.id;
        let mut p = Pipeline::from_image(ri, limits).unwrap();
        p.write_plans(
            id,
            [
                Some(PlannedTransition {
                    sequence: 1,
                    operation: Transition::Record,
                    on_pulse: false,
                    effective: 0,
                    remaining_ticks: 0,
                }),
                Some(PlannedTransition {
                    sequence: 2,
                    operation: Transition::Play {
                        position: 0,
                        retrigger: true,
                    },
                    on_pulse: false,
                    effective: 64,
                    remaining_ticks: 0,
                }),
            ],
        )
        .unwrap();
        let mut image = target.build_image(&logical, a.clone()).unwrap();
        let row = loop_image::image(p.state().loop_state(id).unwrap(), &spec, None);
        for i in 0..16 {
            image.new_loops[0].words[i] = row.get(i * 64, 64);
        }
        parameters(&mut d, &image);
        let u = upload(&mut s, &image);
        finish_upload(&mut s, &u);
        let (mut control, mut execution) = rt::channel(8, 2, 128).unwrap();
        let mut bridge = Commands::default();
        allocator.publish(a.revision).unwrap();
        select(&mut s, &mut bridge, &mut control, &mut execution, &image, 0);
        acknowledge(&mut s, &mut bridge, &mut control, &mut execution, 0);
        allocator.complete(a.revision).unwrap();
        let mut jobs = JobProcessor::with_history_slots(16, 16, 16);
        let mut media = MediaStore::new(16).unwrap();
        media.create_recording().unwrap();
        for tick in 0..3 {
            quantum(&mut d, &mut s, &mut p, &mut jobs, &mut media, tick);
        }
        let mut next = logical.clone();
        next.work_pass
            .insert(0, Operation::Silence { destination: 7 });
        next.resources.buffers = 8;
        next.jobs_per_quantum += 1;
        next.resources.queue_capacity += 1;
        let b = allocator.stage(&next).unwrap();
        let image = target.build_image(&next, b.clone()).unwrap();
        assert!(image.initializers.is_empty() && image.new_loops.is_empty());
        let u = upload(&mut s, &image);
        finish_upload(&mut s, &u);
        let input = vec![Fixed(12345); 16];
        let mut left = vec![Fixed::ZERO; 16];
        let mut right = left.clone();
        p.process(
            &mut jobs,
            &mut media,
            &[&input],
            &mut [&mut left, &mut right],
        )
        .unwrap();
        assert_eq!(
            d.execute(&mut s, &[input.iter().map(|v| v.0).collect()], true)
                .1,
            0
        );
        store.memory.borrow_mut().control.pause_responses = true;
        allocator.publish(b.revision).unwrap();
        select(
            &mut s,
            &mut bridge,
            &mut control,
            &mut execution,
            &image,
            64,
        );
        if mode == Mode::Fixed {
            s.idle(128);
            assert_eq!(
                s.model.get("switch_response_valid"),
                0,
                "AXI writes must complete before ownership handoff"
            );
            assert_eq!(allocator.current().unwrap().revision, a.revision);
            assert!(control.commands.response().is_none());
        }
        store.memory.borrow_mut().control.pause_responses = false;
        acknowledge(&mut s, &mut bridge, &mut control, &mut execution, 64);
        allocator.complete(b.revision).unwrap();
        let mut ri = rust.build_image(&next, b).unwrap();
        p.switch_image(&mut ri).unwrap();
        parameters(&mut d, &image);
        for tick in 4..10 {
            quantum(&mut d, &mut s, &mut p, &mut jobs, &mut media, tick);
        }
        assert_eq!(p.state().loop_state(id).unwrap().state.content_length, 64);
        let result = store.command(
            &mut s,
            &json!({"kind":"read","view":0,"position":0,"frames":64,"length":64}),
            true,
            true,
            true,
        );
        assert_eq!(result["status"]["kind"], "completed");
        let mut expected = vec![Fixed::ZERO; 64];
        media
            .read_loop(rt_fpga_reference::storage::ViewId(0), 0, 64, &mut expected)
            .unwrap();
        assert_eq!(
            result["value"]["value"],
            json!(expected.iter().map(|v| v.0).collect::<Vec<_>>())
        );
    }
}

#[shoop_test(no_wasm = "The RTL pipeline uses native Verilator and DRAMsim3")]
fn looping_workloads_with_thirty_two_frame_chunks_and_dram_timing() {
    use rt_fpga_compiler::{Identity, compile_with_chunk_frames};
    use rt_fpga_testbench::{
        drivers::store::{Geometry, Store},
        memory::Mode,
    };

    const Q: usize = 32;
    const QUANTUM_BUDGET_CYCLES: u64 = Q as u64 * 100_000_000 / 48_000;
    const CAPACITY: [u64; 7] = [1024, 512, 1024, Q as u64, 512, 1, 2];
    let geometry = Geometry::new([512, 8192, Q as u64], 32);
    let mut design = suites::pipeline::design(CAPACITY, &geometry, 64, Mode::Dramsim3, 1);
    design.parameters.insert("DDR_PROGRAM".into(), "1".into());
    design.parameters.insert("LIVE_LOADING".into(), "1".into());
    design
        .parameters
        .insert("PROGRAM_REGIONS".into(), "2".into());
    let compiled = design
        .build(&rt_fpga_testbench::root().join("work/marlin-suite/loading-ddr"))
        .unwrap();

    for loops in [16, 64] {
        let mut sim = Sim::new(compiled.session(None).unwrap(), &[("clk", 10_000, 0)]);
        let mut driver = Audio::loading(&mut sim, CAPACITY, 733);
        driver.reset(&mut sim);
        sim.model.set("sync_event_ready", 1);
        let mut store =
            Store::new(&mut sim, geometry.clone(), Mode::Dramsim3, 991, "store_").unwrap();
        store.reserve_ddr_program(CAPACITY[0] as usize, 2);
        store.load(&mut sim, &json!({"chunk_frames":Q,"chunk_limit":8192,"block_chunks":32,"samples":[],"free_chunks":[],
            "views":vec![json!({"chunks":[], "frames":0}); geometry.views as usize]}));
        let graph = mono::session(
            &(0..loops)
                .map(|slot| mono::MonoLoop {
                    id: Identity {
                        slot,
                        generation: 1,
                    },
                    source: None,
                    input: true,
                    output: true,
                    gain: 1.0 / loops as f32,
                    repeat_sync: false,
                })
                .collect::<Vec<_>>(),
        );
        let mut limits = RtlTarget::new(CAPACITY, 64, &[]).limits();
        limits.jobs = CAPACITY[2] as u32;
        let logical = compile_with_chunk_frames(
            &graph,
            CompileContext {
                quantum: Q as u32,
                limits,
            },
            Q as u32,
        )
        .unwrap();
        let mut allocator = Allocator::new(limits);
        let layout = allocator.stage(&logical).unwrap();
        let views = layout.media_views(&logical);
        assert!(views.iter().all(|&view| view < geometry.views));
        let mut rust = RustTarget::new(&views);
        rust.limits = limits;
        let reference_image = rust.build_image(&logical, layout.clone()).unwrap();
        let specs = reference_image
            .compiled
            .plan
            .image()
            .description
            .loops
            .clone();
        let mut reference = Pipeline::from_image(reference_image, limits).unwrap();
        let mut image = RtlTarget::new(CAPACITY, 64, &views)
            .build_image(&logical, layout)
            .unwrap();
        for (spec, row) in specs.iter().zip(&mut image.new_loops) {
            assert_eq!(spec.id.slot, row.id.slot);
            reference
                .write_plans(
                    spec.id,
                    [
                        Some(PlannedTransition {
                            sequence: 1,
                            operation: Transition::Record,
                            on_pulse: false,
                            effective: 0,
                            remaining_ticks: 0,
                        }),
                        Some(PlannedTransition {
                            sequence: 2,
                            operation: Transition::Play {
                                position: 0,
                                retrigger: true,
                            },
                            on_pulse: false,
                            effective: 128,
                            remaining_ticks: 0,
                        }),
                    ],
                )
                .unwrap();
            let words =
                loop_image::image(reference.state().loop_state(spec.id).unwrap(), spec, None);
            for (index, word) in row.words.iter_mut().enumerate() {
                *word = words.get(64 * index, 64);
            }
        }
        parameters(&mut driver, &image);
        let transfer = Rc::new(RefCell::new(Upload::new(&image, CAPACITY[0])));
        sim.agent(transfer.clone());
        finish_upload(&mut sim, &transfer);
        let (mut control, mut execution) = rt::channel(8, 2, 128).unwrap();
        let mut bridge = Commands::default();
        allocator.publish(1).unwrap();
        select(
            &mut sim,
            &mut bridge,
            &mut control,
            &mut execution,
            &image,
            0,
        );
        acknowledge(&mut sim, &mut bridge, &mut control, &mut execution, 0);
        allocator.complete(1).unwrap();
        let mut cycles = Vec::new();
        for index in 0..8 {
            let input = if index < 4 { 0x300000 } else { 0 };
            let start = sim.cycles;
            assert_eq!(
                driver.execute(&mut sim, &[vec![input; Q]], false).1,
                0,
                "{loops} loops, quantum {index}, media error {}",
                sim.model.get("store_error_code")
            );
            cycles.push(sim.cycles - start);
            if index >= 4 {
                assert_eq!(driver.read_samples(&mut sim).1, vec![vec![0x300000; Q]; 2]);
            }
        }
        assert!(store.memory.borrow().requests() > 0);
        println!(
            "{loops} loops, DRAMsim3 AXI64 cycles per quantum including input configuration: {cycles:?}"
        );
        assert!(
            cycles.iter().all(|&cycles| cycles <= QUANTUM_BUDGET_CYCLES),
            "{loops} loops exceeded the quantum budget of {QUANTUM_BUDGET_CYCLES} fabric cycles"
        );
    }
}
