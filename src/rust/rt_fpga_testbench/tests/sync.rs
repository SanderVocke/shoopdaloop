#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use rt_fpga_reference::{
    fixed::{Fixed, Ramp},
    jobs::JobQueue,
    state::{Instruction, LoopSlot, Program, StateProcessor, Value},
    transitions::*,
};
use rt_fpga_testbench::{
    bits::Bits,
    clock::Edges,
    drivers::state::State,
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
}
impl Agent for Events {
    fn drive(&mut self, m: &mut Model<'_>, e: Edges) {
        if e.falls(0) {
            m.set("sync_event_ready", u64::from(self.random.yes(4)));
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
fn operation(op: Operation) -> (u64, u64, bool) {
    match op {
        Operation::Stop => (0, 0, false),
        Operation::Play {
            position,
            retrigger,
        } => (1, position, retrigger),
        Operation::Record => (2, 0, false),
        Operation::Seek {
            position,
            retrigger,
        } => (3, position, retrigger),
    }
}
fn image(slot: &LoopSlot, spec: &LoopSpec, stamp: Option<u64>) -> Bits {
    let mut b = Bits::zero(1664);
    let s = slot.state;
    b.set(0, 1, 1)
        .set(1, 32, u64::from(s.id.generation))
        .set(
            33,
            2,
            match s.mode {
                Mode::Stopped => 0,
                Mode::Playing => 1,
                Mode::Recording => 2,
            },
        )
        .set(35, 1, u64::from(s.repeat_sync));
    if let Some(source) = spec.sync_source {
        b.set(36, 1, 1).set(37, 32, u64::from(source.slot)).set(
            69,
            32,
            u64::from(source.generation),
        );
    }
    b.set(101, 1, u64::from(slot.pulse_this_quantum))
        .set(102, 1, u64::from(s.last_pulse.is_some()))
        .set(103, 1, u64::from(slot.render.begin_capture))
        .set(104, 1, u64::from(stamp.is_some()));
    for (at, value) in [
        (128, s.position),
        (192, s.content_length),
        (256, s.capture_frames),
        (320, s.cycles),
        (384, s.content_revision),
        (448, s.capture_revision),
        (512, s.revision_watermark),
        (576, s.last_pulse.unwrap_or(0)),
        (640, slot.render.position),
        (704, slot.render.content_length),
        (1536, stamp.unwrap_or(0)),
    ] {
        b.set(at, 64, value);
    }
    for (i, plan) in slot.planned.iter().enumerate() {
        if let Some(p) = plan {
            let base = 768 + i * 384;
            let (op, position, retrigger) = operation(p.command.operation);
            b.set(base, 32, u64::from(p.request.slot))
                .set(base + 32, 32, u64::from(p.request.generation))
                .set(base + 64, 64, p.command.sequence)
                .set(base + 128, 64, p.effective)
                .set(base + 192, 64, p.remaining_ticks)
                .set(base + 256, 64, position)
                .set(base + 320, 1, 1)
                .set(
                    base + 321,
                    1,
                    u64::from(matches!(p.command.timing, Timing::AfterTicks { .. })),
                )
                .set(base + 322, 2, op)
                .set(base + 324, 1, u64::from(retrigger));
        }
    }
    b
}
fn access_loop(
    program: &mut rt_fpga_testbench::drivers::state::Program,
    s: &mut Sim<'_>,
    address: u64,
    value: Option<&Bits>,
) -> Bits {
    s.model.set("cfg_loop", 1);
    let result = program.configure(s, false, address, value, false);
    s.model.set("cfg_loop", 0);
    result
}
fn configure_loop(d: &mut State, s: &mut Sim<'_>, address: u64, value: Option<&Bits>) -> Bits {
    access_loop(&mut d.image, s, address, value)
}
fn write_words(d: &mut State, s: &mut Sim<'_>, slot: u32, row: &Bits, start: usize, end: usize) {
    for word in start..end {
        let mut b = Bits::zero(129);
        b.set(0, 64, row.get(word * 128, 64))
            .set(64, 64, row.get(word * 128 + 64, 64));
        configure_loop(d, s, u64::from(slot) * 16 + word as u64, Some(&b));
    }
}
fn read_loop(d: &mut State, s: &mut Sim<'_>, slot: u32) -> Bits {
    let mut row = Bits::zero(1664);
    for word in 0..13 {
        let b = configure_loop(d, s, u64::from(slot) * 16 + word as u64, None);
        row.set(word * 128, 64, b.get(0, 64))
            .set(word * 128 + 64, 64, b.get(64, 64));
    }
    row
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
fn resolution(request: RequestSnapshot) -> Bits {
    let (outcome, why, frame) = match request.outcome.unwrap() {
        Outcome::Applied { frame } => (0, 0, frame),
        Outcome::Superseded { frame } => (1, 0, frame),
        Outcome::Cancelled { frame } => (2, 0, frame),
        Outcome::Failed { frame, reason: r } => (3, reason(r), frame),
    };
    let mut b = Bits::zero(256);
    b.set(0, 8, 3)
        .set(8, 8, why)
        .set(16, 8, outcome)
        .set(32, 32, u64::from(request.command.target.slot))
        .set(64, 32, u64::from(request.id.slot))
        .set(96, 32, u64::from(request.id.generation))
        .set(128, 64, request.command.sequence)
        .set(192, 64, frame);
    b
}
fn run_rtl(d: &mut State, s: &mut Sim<'_>, prefix: usize) -> u64 {
    s.model.set("job_ready", 0);
    let mut after = d.start(s, &[("run_sync_count", prefix as u64)]);
    for _ in 0..1000 * (prefix + 1) + 500 {
        if after.get("done") != 0 {
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
            pending_requests: 32,
            events: 352,
        },
    )
    .unwrap();
    let mut tp = TransitionProcessor::prepare(&plan);
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
    for spec in &specs {
        write_words(
            d,
            s,
            spec.id.slot,
            &image(sp.loop_state(spec.id).unwrap(), spec, None),
            0,
            13,
        );
    }
    let mut random = Random::new(seed);
    let mut sequence = 0;
    let mut queue = JobQueue::new(32);
    for quantum in 0..48 {
        let done: Vec<_> = tp
            .request_slots(&sp)
            .flatten()
            .filter(|r| r.outcome.is_some())
            .map(|r| r.id)
            .collect();
        for id in done {
            tp.take_completion(id).unwrap();
        }
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
            let timing = if random.yes(2) {
                spec.sync_source
                    .map(|source| Timing::AfterTicks {
                        source,
                        skip: random.below(3),
                    })
                    .unwrap_or(Timing::At)
            } else {
                Timing::At
            };
            let _ = tp.submit(
                &mut sp,
                Command {
                    target: spec.id,
                    sequence,
                    at: sp_frame(quantum, q) + random.below(3 * u64::from(q)),
                    operation: op,
                    timing,
                },
            );
        }
        for spec in &specs {
            write_words(
                d,
                s,
                spec.id.slot,
                &image(sp.loop_state(spec.id).unwrap(), spec, None),
                6,
                12,
            );
        }
        let pending: Vec<_> = tp
            .request_slots(&sp)
            .flatten()
            .filter(|r| r.outcome.is_none())
            .map(|r| r.id)
            .collect();
        events.borrow_mut().rows.clear();
        let frame = sp.frame();
        sp.run(&program, &mut queue).unwrap();
        tp.harvest(&mut sp);
        assert_eq!(
            run_rtl(d, s, program.sync_instructions.len()),
            0,
            "seed {seed}, quantum {quantum}"
        );
        assert_eq!(s.model.get("sync_frame"), sp.frame());
        let jobs: Vec<_> = queue
            .iter()
            .map(|j| serde_json::to_value(j).unwrap())
            .collect();
        assert_eq!(d.drain(s), jobs, "jobs seed {seed}, quantum {quantum}");
        while queue.pop().is_some() {}
        assert_eq!(d.image.snapshot(s), json!({"memory":sp.values()}));
        let rows = events.borrow().rows.clone();
        assert_eq!(
            rows.iter()
                .filter(|b| b.get(0, 8) != 3)
                .cloned()
                .collect::<Vec<_>>(),
            sp.events().iter().map(event).collect::<Vec<_>>(),
            "events seed {seed}, quantum {quantum}"
        );
        let mut actual: Vec<_> = rows.into_iter().filter(|b| b.get(0, 8) == 3).collect();
        let mut expected: Vec<_> = tp
            .request_slots(&sp)
            .flatten()
            .filter(|r| pending.contains(&r.id) && r.outcome.is_some())
            .map(resolution)
            .collect();
        actual.sort_by_key(|b| (b.get(64, 32), b.get(96, 32)));
        expected.sort_by_key(|b| (b.get(64, 32), b.get(96, 32)));
        assert_eq!(actual, expected, "outcomes seed {seed}, quantum {quantum}");
        for spec in &specs {
            let slot = sp.loop_state(spec.id).unwrap();
            let expected = image(slot, spec, Some(frame));
            let mut actual = read_loop(d, s, spec.id.slot);
            for (i, p) in slot.planned.iter().enumerate() {
                if p.is_none() {
                    let base = 768 + i * 384;
                    assert_eq!(actual.get(base + 320, 1), 0);
                    for part in 0..6 {
                        actual.set(base + part * 64, 64, 0);
                    }
                }
            }
            assert_eq!(
                actual, expected,
                "loop {:?}, seed {seed}, quantum {quantum}",
                spec.id
            );
        }
    }
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
                pending_requests: 32,
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
                &mut driver,
                &mut sim,
                spec.id.slot,
                &image(sp.loop_state(spec.id).unwrap(), spec, None),
                0,
                13,
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
    }));
    sim.agent(events);
    let mut random = Random::new(20261116);
    for q in [1, 16, 32] {
        let spec = spec(0, 4 * q, None, false);
        let plan = TransitionPlan::compile(
            PlanDescription {
                quantum: q as u32,
                loops: vec![spec],
            },
            TransitionLimits {
                loop_slots: 1,
                pending_requests: 4,
                events: 6,
            },
        )
        .unwrap();
        let mut tp = TransitionProcessor::prepare(&plan);
        let memory = vec![
            Value::Word((33 << 32) | (7 << 16) | 2),
            Value::Ramp(Ramp::new(Fixed(i32::MAX), Fixed(i32::MIN), 47)),
        ];
        let instructions = vec![
            json!({"kind":"process_input","port":0,"destination":0}),
            json!({"kind":"process_gain","ramp":1,"source":0,"destination":1}),
            json!({"kind":"process_smoothing","loop_id":id(0),"control":0,"source":1}),
            json!({"kind":"process_output","source":2,"port":0}),
        ];
        let program = Program {
            sync_instructions: plan.sync_instructions(),
            instructions: serde_json::from_value(json!(instructions)).unwrap(),
            quantum: q as u32,
            chunk_frames: 1,
            jobs_per_quantum: 8,
        };
        let mut combined: Vec<_> = program
            .sync_instructions
            .iter()
            .map(|i| serde_json::to_value(i).unwrap())
            .collect();
        combined.extend(instructions);
        let mut case = suites::audio::case(
            capacities,
            q,
            combined,
            serde_json::from_value(json!(memory)).unwrap(),
            8,
            1,
            vec![vec![0; q as usize]],
        );
        case["program"]["jobs_per_quantum"] = json!(8);
        driver.load(&mut sim, &case);
        let mut sp = StateProcessor::with_loops(plan, memory);
        let row = image(sp.loop_state(id(0)).unwrap(), &spec, None);
        for word in 0..13 {
            let mut b = Bits::zero(129);
            b.set(0, 64, row.get(word * 128, 64))
                .set(64, 64, row.get(word * 128 + 64, 64));
            access_loop(&mut driver.program, &mut sim, word as u64, Some(&b));
        }
        let mut jp = JobProcessor::with_history_slots(q as usize, 8, 16);
        let mut queue = JobQueue::new(32);
        for quantum in 0..40 {
            let completed: Vec<_> = tp
                .request_slots(&sp)
                .flatten()
                .filter(|r| r.outcome.is_some())
                .map(|r| r.id)
                .collect();
            for id in completed {
                tp.take_completion(id).unwrap();
            }
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
                tp.submit(
                    &mut sp,
                    Command {
                        target: id(0),
                        sequence: quantum + 1,
                        at: 0,
                        timing: Timing::At,
                        operation,
                    },
                )
                .unwrap();
            }
            let row = image(sp.loop_state(id(0)).unwrap(), &spec, None);
            for word in 6..12 {
                let mut b = Bits::zero(129);
                b.set(0, 64, row.get(word * 128, 64))
                    .set(64, 64, row.get(word * 128 + 64, 64));
                access_loop(&mut driver.program, &mut sim, word as u64, Some(&b));
            }
            let samples: Vec<_> = (0..q).map(|_| Fixed(random.signed())).collect();
            let mut output = vec![Fixed(123); q as usize];
            sp.run(&program, &mut queue).unwrap();
            tp.harvest(&mut sp);
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
            sim.model.set("run_sync_count", 1);
            let actual = driver.quantum(&mut sim, &json!({"inputs":[samples],"writes":[]}), true);
            assert_eq!(actual, expected, "Q={q}, quantum={quantum}");
            assert_eq!(sim.model.get("sync_frame"), sp.frame());
        }
    }
}
