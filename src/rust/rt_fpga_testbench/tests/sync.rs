#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use rt_fpga_reference::{
    fixed::{Fixed, Ramp},
    jobs::JobQueue,
    state::{Instruction, PlannedTransition, Program, Resolution, StateProcessor, Value},
    transitions::*,
};
use rt_fpga_testbench::{
    bits::Bits,
    clock::Edges,
    drivers::state::State,
    loop_image::{LoopImage, access, image, operation, read_loop, write_plans, write_words},
    model::Model,
    protocol,
    random::Random,
    sim::{Agent, Sim},
    suites,
};
use serde_json::json;
use shoop_wasm_test_support::shoop_test;
use std::{cell::RefCell, rc::Rc};

struct Events {
    rows: Vec<Bits>,
    random: Random,
    stalls: bool,
}
impl Agent for Events {
    fn drive(&mut self, m: &mut Model<'_>, e: Edges) {
        if e.falls(0) {
            m.set(
                "sync_event_ready",
                u64::from(!self.stalls || self.random.yes(4)),
            );
        }
    }
    fn sample(&mut self, m: &Model<'_>, e: Edges) {
        if e.rises(0)
            && m.get("reset") == 0
            && m.get("sync_event_valid") != 0
            && m.get("sync_event_ready") != 0
        {
            self.rows.push(m.bits("sync_event_data"));
        }
    }
}
fn id(slot: u32) -> LoopId {
    LoopId {
        slot,
        generation: 7,
    }
}
fn spec(slot: u32, length: u64, source: Option<u32>, repeat_sync: bool) -> LoopSpec {
    LoopSpec {
        id: id(slot),
        kind: LoopKind::Primitive,
        length,
        sync_source: source.map(id),
        repeat_sync,
        timeline: None,
    }
}
fn event(e: &Event) -> Bits {
    let (kind, payload, flags) = match e.kind {
        EventKind::Transition(op) => {
            let (kind, pos, retrigger) = operation(op);
            (0, pos, kind | (u64::from(retrigger) << 2))
        }
        EventKind::Wrap => (1, 0, 0),
        EventKind::Pulse => (2, 0, 0),
    };
    let mut b = Bits::zero(256);
    b.set(0, 8, kind)
        .set(32, 32, u64::from(e.target.slot))
        .set(64, 64, payload)
        .set(128, 64, flags)
        .set(192, 64, e.frame);
    b
}
fn reason(error: TransitionError) -> u64 {
    match error {
        TransitionError::InvalidConfig => 1,
        TransitionError::Capacity => 2,
        TransitionError::InvalidId => 3,
        TransitionError::DuplicateId => 4,
        TransitionError::InvalidState => 5,
        TransitionError::InvalidFrame => 6,
        TransitionError::MissingSync => 7,
        TransitionError::Exhausted => 8,
        TransitionError::EmptyContent => 9,
        TransitionError::Unsupported => 10,
        TransitionError::InstantaneousDependencyCycle => 11,
    }
}
fn resolution(result: &Resolution) -> Bits {
    let (outcome, why, frame) = match result.outcome {
        Outcome::Applied { frame } => (0, 0, frame),
        Outcome::Superseded { frame } => (1, 0, frame),
        Outcome::Cancelled { frame } => (2, 0, frame),
        Outcome::Failed { frame, reason: r } => (3, reason(r), frame),
    };
    let mut b = Bits::zero(256);
    b.set(0, 8, 3)
        .set(8, 8, why)
        .set(16, 8, outcome)
        .set(32, 32, u64::from(result.target.slot))
        .set(64, 64, result.plan_index as u64)
        .set(192, 64, frame);
    b
}
fn run_rtl(d: &mut State, s: &mut Sim<'_>, prefix: usize) -> u64 {
    s.model.set("job_ready", 0);
    let accepted = s.cycles;
    let mut after = d.start(s, &[("run_sync_count", prefix as u64)]);
    for _ in 0..1000 * (prefix + 1) + 500 {
        if after.get("done") != 0 {
            d.run_cycles = s.cycles - accepted;
            let code = after.get("error_code");
            assert_eq!(after.get("busy"), 0);
            assert_eq!(d.tick(s).1.get("done"), 0);
            return code;
        }
        after = d.tick(s).1;
    }
    panic!(
        "integrated SP timeout at pc {}",
        s.model.get("instruction_pc")
    );
}
fn exercise(
    s: &mut Sim<'_>,
    d: &mut State,
    events: &Rc<RefCell<Events>>,
    specs: Vec<LoopSpec>,
    q: u32,
    seed: u64,
) {
    let plan = TransitionPlan::compile(
        PlanDescription {
            quantum: q,
            loops: specs.clone(),
        },
        TransitionLimits {
            loop_slots: 160,

            events: 352,
        },
    )
    .unwrap();
    let mut memory = vec![Value::Word(0); specs.len()];
    memory.push(Value::Ramp(Ramp::new(Fixed(i32::MIN), Fixed(i32::MAX), 33)));
    memory.push(Value::Word((33 << 32) | (7 << 16) | 30));
    let mut instructions = vec![json!({"kind":"process_input","port":0,"destination":0})];
    for (i, spec) in specs.iter().enumerate().rev() {
        instructions.push(
            json!({"kind":"process_logical_playback","loop_id":spec.id,"view":i,"destination":i+1}),
        );
        instructions
            .push(json!({"kind":"process_logical_capture","loop_id":spec.id,"view":i,"source":0}));
    }
    instructions
        .push(json!({"kind":"process_gain","ramp":specs.len(),"source":1,"destination":20}));
    instructions.push(json!({"kind":"process_smoothing","loop_id":specs[0].id,"control":specs.len()+1,"source":20}));
    instructions.push(json!({"kind":"process_output","source":30,"port":0}));
    let program = Program {
        sync_instructions: plan.sync_instructions(),
        instructions: serde_json::from_value::<Vec<Instruction>>(json!(instructions)).unwrap(),
        quantum: q,
        chunk_frames: 1,
        jobs_per_quantum: 32,
    };
    let mut combined: Vec<_> = program
        .sync_instructions
        .iter()
        .map(|i| serde_json::to_value(i).unwrap())
        .collect();
    combined.extend(instructions);
    let case = suites::state::case(
        combined,
        serde_json::from_value(json!(memory)).unwrap(),
        32,
        Some(32),
        u64::from(q),
    );
    d.load(s, &case);
    events.borrow_mut().rows.clear();
    let mut sp = StateProcessor::with_loops(plan, memory);
    let mut image_check = LoopImage::default();
    for spec in &specs {
        write_words(
            &mut d.image,
            s,
            spec.id.slot,
            &image_check.stage(sp.loop_state(spec.id).unwrap(), spec, None),
            0,
            16,
        );
    }
    let mut random = Random::new(seed);
    let mut sequence = 0_u64;
    let mut queue = JobQueue::new(32);
    let mut cycle_samples = Vec::new();
    for quantum in 0..48 {
        for _ in 0..random.below(7) {
            let spec = &specs[random.below(specs.len() as u64) as usize];
            sequence += 1;
            let op = match random.below(7) {
                0 => Operation::Stop,
                1 | 2 => Operation::Record,
                3 | 4 => Operation::Play {
                    position: random.below(5) * u64::from(q),
                    retrigger: random.yes(2),
                },
                _ => Operation::Seek {
                    position: random.below(5) * u64::from(q),
                    retrigger: random.yes(2),
                },
            };
            let on_pulse = random.yes(2) && spec.sync_source.is_some();
            let remaining_ticks = if on_pulse { random.below(3) } else { 0 };
            let at = sp_frame(quantum, q) + random.below(3 * u64::from(q));
            let plan = PlannedTransition {
                sequence: if seed > 20 {
                    sequence.reverse_bits()
                } else {
                    sequence
                },
                operation: op,
                on_pulse,
                effective: at.div_ceil(u64::from(q)) * u64::from(q),
                remaining_ticks,
            };
            let mut plans = sp.loop_state(spec.id).unwrap().planned;
            if let Some(index) = plans.iter().position(Option::is_none) {
                plans[index] = Some(plan);
                sp.write_plans(spec.id, plans).unwrap();
            }
        }
        for spec in &specs {
            write_plans(
                &mut d.image,
                s,
                spec.id.slot,
                &image_check.stage(sp.loop_state(spec.id).unwrap(), spec, None),
            );
        }
        events.borrow_mut().rows.clear();
        let frame = sp.frame();
        sp.run(&program, &mut queue).unwrap();
        assert_eq!(
            run_rtl(d, s, program.sync_instructions.len()),
            0,
            "seed {seed}, quantum {quantum}"
        );
        cycle_samples.push(d.run_cycles);
        assert_eq!(s.model.get("sync_frame"), sp.frame());
        let jobs: Vec<_> = queue
            .iter()
            .map(|j| serde_json::to_value(j).unwrap())
            .collect();
        assert_eq!(d.drain(s), jobs, "jobs seed {seed}, quantum {quantum}");
        while queue.pop().is_some() {}
        assert_eq!(d.image.snapshot(s), json!({"memory":sp.values()}));
        let rows = events.borrow().rows.clone();
        image_check.observe(&rows);
        assert_eq!(
            rows.iter()
                .filter(|b| b.get(0, 8) != 3)
                .cloned()
                .collect::<Vec<_>>(),
            sp.events().iter().map(event).collect::<Vec<_>>(),
            "events seed {seed}, quantum {quantum}"
        );
        let mut actual: Vec<_> = rows.into_iter().filter(|b| b.get(0, 8) == 3).collect();
        let mut expected: Vec<_> = sp.resolutions().iter().map(resolution).collect();
        actual.sort_by_key(|b| (b.get(32, 32), b.get(64, 64)));
        expected.sort_by_key(|b| (b.get(32, 32), b.get(64, 64)));
        assert_eq!(actual, expected, "outcomes seed {seed}, quantum {quantum}");
        for spec in &specs {
            let slot = sp.loop_state(spec.id).unwrap();
            image_check.verify(&read_loop(&mut d.image, s, spec.id.slot), slot, spec, frame);
        }
    }
    println!(
        "SYNC_CYCLES {}",
        json!({"seed":seed,"quantum":q,"quanta":cycle_samples.len(),"cycles":cycle_samples.iter().sum::<u64>(),"min":cycle_samples.iter().min(),"max":cycle_samples.iter().max()})
    );
}
fn sp_frame(quantum: u64, q: u32) -> u64 {
    quantum * u64::from(q)
}

#[shoop_test]
fn integrated_sp_sync_instructions_match_rust_jobs_cursors_events_and_planned_transitions() {
    let mut design = suites::state::design([64, 16, 32]);
    design.parameters.insert("LOOP_DEPTH".into(), "160".into());
    let compiled = design
        .build(&rt_fpga_testbench::root().join("work/marlin-suite/sync"))
        .unwrap();
    let mut sim = Sim::new(compiled.session(None).unwrap(), &[("clk", 10000, 0)]);
    let mut driver = State::new(&mut sim, [64, 16, 32], 20261116);
    let events = Rc::new(RefCell::new(Events {
        rows: vec![],
        random: Random::new(922),
        stalls: std::env::var_os("SHOOP_SYNC_NO_STALLS").is_none(),
    }));
    sim.agent(events.clone());
    sim.agent(protocol::ReadyValid::new(
        "sync_event_valid",
        "sync_event_ready",
        &["sync_event_data"],
    ));
    for seed in 1..=20 {
        let q = [1, 16, 32, 64, 128][(seed - 1) as usize % 5];
        let specs = vec![
            spec(129, 4 * u64::from(q), None, false),
            spec(0, 2 * u64::from(q), Some(129), true),
            spec(3, u64::from(q), Some(3), true),
        ];
        exercise(&mut sim, &mut driver, &events, specs, q, seed);
    }
}

#[shoop_test]
fn integrated_sp_preserves_full_sequence_priority_with_out_of_order_plan_writes() {
    let mut design = suites::state::design([64, 16, 32]);
    design.parameters.insert("LOOP_DEPTH".into(), "160".into());
    let compiled = design
        .build(&rt_fpga_testbench::root().join("work/marlin-suite/sync"))
        .unwrap();
    let mut sim = Sim::new(compiled.session(None).unwrap(), &[("clk", 10000, 0)]);
    let mut driver = State::new(&mut sim, [64, 16, 32], 20261116);
    let events = Rc::new(RefCell::new(Events {
        rows: vec![],
        random: Random::new(142),
        stalls: true,
    }));
    sim.agent(events.clone());
    for seed in 21..=24 {
        exercise(
            &mut sim,
            &mut driver,
            &events,
            vec![
                spec(129, 64, None, false),
                spec(0, 32, Some(129), true),
                spec(3, 16, Some(3), true),
            ],
            16,
            seed,
        );
    }
}

#[shoop_test]
fn integrated_sp_keeps_wide_ram_counters_and_checks_overflow() {
    let mut design = suites::state::design([64, 16, 32]);
    design.parameters.insert("LOOP_DEPTH".into(), "160".into());
    let compiled = design
        .build(&rt_fpga_testbench::root().join("work/marlin-suite/sync"))
        .unwrap();
    let mut sim = Sim::new(compiled.session(None).unwrap(), &[("clk", 10000, 0)]);
    let mut driver = State::new(&mut sim, [64, 16, 32], 44);
    let events = Rc::new(RefCell::new(Events {
        rows: vec![],
        random: Random::new(14),
        stalls: true,
    }));
    sim.agent(events.clone());
    let spec = spec(0, 32, None, false);
    let plan = TransitionPlan::compile(
        PlanDescription {
            quantum: 16,
            loops: vec![spec],
        },
        TransitionLimits {
            loop_slots: 160,

            events: 352,
        },
    )
    .unwrap();
    let sp = StateProcessor::with_loops(plan, vec![]);
    let case = suites::state::case(
        vec![json!({"kind":"process_loop","loop_id":id(0)})],
        vec![],
        32,
        Some(0),
        16,
    );
    let high = (1_u64 << 48) + 32;
    for scenario in 0..6 {
        driver.load(&mut sim, &case);
        events.borrow_mut().rows.clear();
        let mut slot = sp.loop_state(id(0)).unwrap().clone();
        slot.state.revision_watermark = high;
        slot.state.capture_revision = high;
        let operation = match scenario {
            0 => {
                slot.state.revision_watermark = u64::MAX;
                slot.state.capture_revision = u64::MAX;
                Some(Operation::Record)
            }
            1 => {
                slot.state.mode = Mode::Recording;
                slot.state.capture_frames = high;
                Some(Operation::Play {
                    position: high - 16,
                    retrigger: true,
                })
            }
            2 | 3 => {
                slot.state.mode = Mode::Playing;
                slot.state.position = 32;
                slot.state.cycles = if scenario == 2 { high } else { u64::MAX };
                None
            }
            4 => {
                slot.state.mode = Mode::Recording;
                slot.state.capture_frames = u64::MAX;
                None
            }
            _ => Some(Operation::Record),
        };
        if let Some(operation) = operation {
            slot.planned[0] = Some(PlannedTransition {
                sequence: u64::MAX,
                operation,
                on_pulse: false,
                effective: 0,
                remaining_ticks: 0,
            });
        }
        let row = image(&slot, &spec, None);
        write_words(&mut driver.image, &mut sim, 0, &row, 0, 16);
        assert_eq!(
            run_rtl(&mut driver, &mut sim, 1),
            if matches!(scenario, 3 | 4) { 2 } else { 0 },
            "scenario {scenario}"
        );
        if matches!(scenario, 3 | 4) {
            assert_eq!(sim.model.get("run_ready"), 0);
            continue;
        }
        let row = read_loop(&mut driver.image, &mut sim, 0);
        let rows = events.borrow().rows.clone();
        match scenario {
            0 => {
                assert_eq!(row.get(33, 2), 0);
                assert_eq!(row.get(7 * 64, 64), u64::MAX);
                assert_eq!(rows.len(), 1);
                assert_eq!(
                    (rows[0].get(0, 8), rows[0].get(8, 8), rows[0].get(16, 8)),
                    (3, 8, 3)
                );
            }
            1 => {
                assert_eq!(row.get(2 * 64, 64), high);
                assert_eq!(row.get(3 * 64, 64), high);
                assert_eq!(row.get(6 * 64, 64), high);
                assert_eq!(row.get(8 * 64, 64), high - 16);
                assert_eq!(row.get(5 * 64, 64), 0);
            }
            2 => {
                assert_eq!(row.get(2 * 64, 64), 16);
                assert_eq!(row.get(5 * 64, 64), high + 1);
                assert_eq!(
                    rows.iter().map(|r| r.get(0, 8)).collect::<Vec<_>>(),
                    vec![1, 2]
                );
            }
            _ => {
                assert_eq!(row.get(7 * 64, 64), high + 1);
                assert_eq!(row.get(4 * 64, 64), 16);
                assert_eq!(row.get(54, 1), 1);
            }
        }
        // Config writes ignore the upper bus bits for compact loop words.
        let mut word = Bits::zero(129);
        word.set(0, 64, high).set(64, 64, u64::MAX).set(128, 1, 1);
        access(&mut driver.image, &mut sim, 15, Some(&word));
        let actual = access(&mut driver.image, &mut sim, 15, None);
        assert_eq!(actual.get(0, 64), high);
        assert_eq!(actual.get(64, 64), 0);
        assert_eq!(actual.get(128, 1), 0);
    }
}

#[shoop_test]
fn integrated_sp_checks_sync_prefix_order_generations_bounds_and_latched_faults() {
    let mut design = suites::state::design([64, 16, 32]);
    design.parameters.insert("LOOP_DEPTH".into(), "160".into());
    let compiled = design
        .build(&rt_fpga_testbench::root().join("work/marlin-suite/sync"))
        .unwrap();
    let mut sim = Sim::new(compiled.session(None).unwrap(), &[("clk", 10000, 0)]);
    let mut driver = State::new(&mut sim, [64, 16, 32], 44);
    let loop_op = |slot, generation| json!({"kind":"process_loop","loop_id":{"slot":slot,"generation":generation}});
    for (instructions, prefix, error) in [
        (vec![loop_op(1, 7), loop_op(0, 7)], 2, 2),
        (vec![loop_op(0, 7), loop_op(0, 7)], 2, 2),
        (vec![loop_op(0, 8)], 1, 2),
        (vec![loop_op(160, 7)], 1, 2),
        (
            vec![json!({"kind":"process_silence","destination":0})],
            1,
            3,
        ),
        (vec![loop_op(0, 7)], 2, 3),
    ] {
        let specs = vec![spec(0, 32, None, false), spec(1, 32, Some(0), false)];
        let plan = TransitionPlan::compile(
            PlanDescription {
                quantum: 16,
                loops: specs.clone(),
            },
            TransitionLimits {
                loop_slots: 160,

                events: 352,
            },
        )
        .unwrap();
        let sp = StateProcessor::with_loops(plan, vec![]);
        let case = suites::state::case(instructions, vec![], 32, Some(0), 16);
        driver.load(&mut sim, &case);
        sim.model.set("sync_event_ready", 1);
        for spec in &specs {
            write_words(
                &mut driver.image,
                &mut sim,
                spec.id.slot,
                &image(sp.loop_state(spec.id).unwrap(), spec, None),
                0,
                16,
            );
        }
        sim.model.set("cfg_loop", 1);
        driver
            .image
            .configure(&mut sim, false, 160 * 16, None, true);
        sim.model.set("cfg_loop", 0);
        assert_eq!(run_rtl(&mut driver, &mut sim, prefix), error);
        assert_eq!(
            sim.model.get("run_ready"),
            0,
            "managed execution failure must latch"
        );
        assert_eq!(sim.model.get("sync_frame"), 0);
        assert!(driver.drain(&mut sim).is_empty());
    }
}

struct ScratchMemory;
impl rt_fpga_reference::jobs::Memory for ScratchMemory {
    fn read(
        &mut self,
        _: rt_fpga_reference::storage::ViewId,
        _: u64,
        _: u64,
        _: &mut [Fixed],
    ) -> Result<(), rt_fpga_reference::Error> {
        Err(rt_fpga_reference::Error::InvalidView)
    }
    fn write(
        &mut self,
        _: rt_fpga_reference::storage::ViewId,
        _: &[Fixed],
    ) -> Result<(), rt_fpga_reference::Error> {
        Err(rt_fpga_reference::Error::InvalidView)
    }
}

#[shoop_test]
fn integrated_sync_and_audio_instructions_drive_jp_with_exact_rust_samples() {
    use rt_fpga_reference::jobs::{BufferId, JobProcessor};
    use rt_fpga_testbench::drivers::audio::Audio;
    let capacities = [32, 16, 32, 32, 16, 2, 2];
    let compiled = suites::audio::design(capacities)
        .build(&rt_fpga_testbench::root().join("work/marlin-suite/sync-audio"))
        .unwrap();
    let mut sim = Sim::new(compiled.session(None).unwrap(), &[("clk", 10000, 0)]);
    let mut driver = Audio::new(&mut sim, capacities, 77);
    let events = Rc::new(RefCell::new(Events {
        rows: vec![],
        random: Random::new(41),
        stalls: true,
    }));
    sim.agent(events.clone());
    let mut random = Random::new(20261116);
    for q in [1, 16, 32] {
        use rt_fpga_compiler::{FixedProgram, StateProgramTarget};
        use rt_fpga_reference::compiler::RustTarget;
        use rt_fpga_testbench::compiler::RtlTarget;
        let logical = FixedProgram::SyncSmoothing { quantum: q as u32 }
            .emit()
            .unwrap();
        let rust = RustTarget::new(&[]).build(&logical).unwrap();
        let mut target = RtlTarget::new(capacities, 1, &[]);
        target.state_base = 4;
        let rtl = target.build(&logical).unwrap();
        assert_eq!(rust.bindings[0].gain.unwrap().0, 1);
        assert_eq!(rtl.bindings[0].gain, Some(5));
        driver.load_compiled(&mut sim, &rtl);
        let spec = rust.plan.image().description.loops[0];
        let program = rust.program;
        let mut sp = StateProcessor::with_loops(rust.plan, rust.memory);
        let mut image_check = LoopImage::default();
        let row = image_check.stage(sp.loop_state(id(0)).unwrap(), &spec, None);
        assert_eq!(read_loop(&mut driver.program, &mut sim, 0), row);
        let mut jp = JobProcessor::with_history_slots(
            q as usize,
            rust.resources.buffers as usize,
            rust.resources.history_slots as usize,
        );
        let mut queue = JobQueue::new(rust.resources.queue_capacity as usize);
        for quantum in 0..40 {
            let op = match quantum % 8 {
                0 => Some(Operation::Play {
                    position: 0,
                    retrigger: true,
                }),
                2 | 6 => Some(Operation::Stop),
                3 => Some(Operation::Record),
                4 => Some(Operation::Play {
                    position: 0,
                    retrigger: false,
                }),
                5 => Some(Operation::Seek {
                    position: 0,
                    retrigger: true,
                }),
                _ => None,
            };
            if let Some(operation) = op {
                sp.write_plans(
                    id(0),
                    [
                        Some(PlannedTransition {
                            sequence: quantum + 1,
                            effective: 0,
                            remaining_ticks: 0,
                            on_pulse: false,
                            operation,
                        }),
                        None,
                    ],
                )
                .unwrap();
            }
            let row = image_check.stage(sp.loop_state(id(0)).unwrap(), &spec, None);
            write_plans(&mut driver.program, &mut sim, 0, &row);
            events.borrow_mut().rows.clear();
            let frame = sp.frame();
            let samples: Vec<_> = (0..q).map(|_| Fixed(random.signed())).collect();
            let mut output = vec![Fixed(123); q as usize];
            sp.run(&program, &mut queue).unwrap();
            let jobs: Vec<_> = queue.iter().copied().collect();
            jp.run(
                &mut queue,
                &mut ScratchMemory,
                &[&samples],
                &mut [&mut output],
            )
            .unwrap();
            let buffers: Vec<_> = (0..8).map(|i| jp.buffer(BufferId(i)).ok()).collect();
            let expected = json!({"jobs":jobs,"remaining_jobs":[],"state":{"memory":sp.values()},"buffers":buffers,"outputs":[output],"stage":"jobs","status":{"kind":"completed"}});
            let mut actual =
                driver.quantum(&mut sim, &json!({"inputs":[samples],"writes":[]}), true);
            let values = actual["state"]["memory"].as_array_mut().unwrap();
            assert!(
                values[..usize::from(target.state_base)]
                    .iter()
                    .all(|v| *v == json!({"kind":"word","value":0}))
            );
            values.drain(..usize::from(target.state_base));
            assert_eq!(actual, expected, "Q={q}, quantum={quantum}");
            assert_eq!(sim.model.get("sync_frame"), sp.frame());
            image_check.observe(&events.borrow().rows);
            image_check.verify(
                &read_loop(&mut driver.program, &mut sim, 0),
                sp.loop_state(id(0)).unwrap(),
                &spec,
                frame,
            );
        }
    }
}
