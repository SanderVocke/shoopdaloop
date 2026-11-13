//! Standalone instruction, FIFO, configuration and retained-state regressions.
use crate::{Result, drivers::state::State, model::Design, sim::Sim};
use serde_json::{Value, json};

pub const CONFIGS: &[(&str, [u64; 3])] = &[
    ("default", [256, 256, 256]),
    ("small", [7, 11, 5]),
    ("minimum", [1, 1, 1]),
];
pub fn design(depths: [u64; 3]) -> Design {
    let root = crate::root();
    Design::new(
        "state_processor",
        "sp_ram sp_job_fifo sp_engine state_processor"
            .split_whitespace()
            .map(|n| root.join(format!("fpga/state_processor/rtl/{n}.sv")))
            .collect(),
    )
    .parameters(&[
        ("PROGRAM_DEPTH", depths[0]),
        ("STATE_DEPTH", depths[1]),
        ("JOB_DEPTH", depths[2]),
    ])
}
pub fn word(n: u64) -> Value {
    json!({"kind":"word","value":n})
}
pub fn ramp(duration: u64, elapsed: u64) -> Value {
    json!({"kind":"ramp","value":{"start":i32::MIN,"target":i32::MAX,"duration":duration,"elapsed":elapsed}})
}
pub fn case(
    instructions: Vec<Value>,
    memory: Vec<Value>,
    capacity: u64,
    budget: Option<u64>,
    quantum: u64,
) -> Value {
    json!({"program":{"instructions":instructions,"quantum":quantum,"chunk_frames":1,"jobs_per_quantum":budget.unwrap_or(instructions.len() as u64)},"initial_state":{"memory":memory},"queue_capacity":capacity})
}
pub fn run(name: &str, depths: [u64; 3]) -> Result<()> {
    let model = design(depths).build(&crate::root().join("work/marlin-suite/state"))?;
    let mut sim = Sim::new(model.session(None)?, &[("clk", 10_000, 0)]);
    let mut driver = State::new(&mut sim, depths, 20261110);
    instruction_vectors(&mut driver, &mut sim);
    fifo_order_wrap_and_partial_failure(&mut driver, &mut sim);
    configuration_and_error_protocol(&mut driver, &mut sim);
    nonempty_admission_and_reset(&mut driver, &mut sim);
    println!("state/{name}: 4 regression groups passed");
    Ok(())
}

fn vector(
    d: &mut State,
    s: &mut Sim<'_>,
    instruction: Value,
    memory: Vec<Value>,
    jobs: Vec<Value>,
    status: u64,
    after: Option<Vec<Value>>,
    budget: Option<u64>,
    q: u64,
) {
    let value = case(vec![instruction], memory.clone(), d.depths[2], budget, q);
    d.load(s, &value);
    assert_eq!(d.execute(s, &[]), status, "vector {value}");
    assert_eq!(d.drain(s), jobs, "vector {value}");
    assert_eq!(d.snapshot(s), json!({"memory":after.unwrap_or(memory)}));
}
fn instruction_vectors(d: &mut State, s: &mut Sim<'_>) {
    for (instruction, job) in [
        (
            json!({"kind":"process_input","port":65535,"destination":417}),
            json!({"kind":"input","port":65535,"destination":417}),
        ),
        (
            json!({"kind":"process_mix","left":65535,"right":32768,"destination":91}),
            json!({"kind":"mix","left":65535,"right":32768,"destination":91}),
        ),
        (
            json!({"kind":"process_output","source":65534,"port":513}),
            json!({"kind":"output","source":65534,"port":513}),
        ),
        (
            json!({"kind":"process_silence","destination":633}),
            json!({"kind":"silence","destination":633}),
        ),
    ] {
        vector(d, s, instruction, vec![], vec![job], 0, None, None, 32);
    }
    let gain = json!({"kind":"process_gain","ramp":0,"source":91,"destination":633});
    for q in [0, 1, 32, u64::from(u32::MAX)] {
        for (duration, elapsed) in [
            (0, 0),
            (0, u32::MAX as u64),
            (1, 0),
            (33, 1),
            (u32::MAX as u64, u32::MAX as u64 - 15),
            (u32::MAX as u64, u32::MAX as u64),
        ] {
            let memory = ramp(duration, elapsed);
            let mut after = memory.clone();
            after["value"]["elapsed"] = json!(duration.min(elapsed + q));
            vector(
                d,
                s,
                gain.clone(),
                vec![memory.clone()],
                if q == 0 {
                    vec![]
                } else {
                    vec![
                        json!({"kind":"gain","source":91,"destination":633,"ramp":memory["value"]}),
                    ]
                },
                if q == 0 { 2 } else { 0 },
                Some(vec![if q == 0 { memory } else { after }]),
                None,
                q,
            );
        }
    }
    vector(d, s, gain.clone(), vec![word(0)], vec![], 2, None, None, 32);
    let mut bad = gain.clone();
    bad["ramp"] = json!(65535);
    vector(d, s, bad, vec![], vec![], 2, None, None, 32);
    let mut play = json!({"kind":"process_loop_playback","view":65535,"position":65535,"length":65535,"enabled":0,"destination":633});
    vector(
        d,
        s,
        play.clone(),
        vec![word(0)],
        vec![json!({"kind":"silence","destination":633})],
        0,
        None,
        None,
        32,
    );
    let capture = json!({"kind":"process_loop_capture","view":65535,"enabled":0,"source":911});
    vector(
        d,
        s,
        capture.clone(),
        vec![word(0)],
        vec![],
        0,
        None,
        Some(0),
        32,
    );
    vector(
        d,
        s,
        play.clone(),
        vec![ramp(100, 0)],
        vec![],
        2,
        None,
        None,
        32,
    );
    if d.depths[1] >= 4 {
        let memory = vec![
            word(u32::MAX as u64),
            word(1 << 63),
            word(u64::MAX - 1),
            word(u64::MAX),
        ];
        play["view"] = json!(0);
        play["enabled"] = json!(1);
        play["position"] = json!(2);
        play["length"] = json!(3);
        vector(
            d,
            s,
            play.clone(),
            memory.clone(),
            vec![
                json!({"kind":"read_view","destination":633,"view":u32::MAX,"position_chunks":u64::MAX-1,"length_chunks":u64::MAX}),
            ],
            0,
            None,
            None,
            32,
        );
        let mut capture = capture;
        capture["view"] = json!(0);
        capture["enabled"] = json!(1);
        vector(
            d,
            s,
            capture,
            memory.clone(),
            vec![json!({"kind":"capture","source":911,"view":u32::MAX})],
            0,
            None,
            None,
            32,
        );
        for field in ["view", "enabled", "position", "length"] {
            let mut bad = play.clone();
            bad[field] = json!(65535);
            vector(d, s, bad, memory.clone(), vec![], 2, None, None, 32);
        }
        for (position, length) in [(0, 0), (9, 9), (10, 9)] {
            let mut bad = memory.clone();
            bad[2] = word(position);
            bad[3] = word(length);
            vector(d, s, play.clone(), bad, vec![], 2, None, None, 32);
        }
    }
    let mut last = gain.clone();
    last["ramp"] = json!(d.depths[1] - 1);
    let mut memory = vec![word(9); d.depths[1] as usize - 1];
    memory.push(ramp(100, 0));
    let mut after = memory.clone();
    *after.last_mut().unwrap() = ramp(100, 32);
    vector(
        d,
        s,
        last,
        memory,
        vec![json!({"kind":"gain","source":91,"destination":633,"ramp":ramp(100,0)["value"]})],
        0,
        Some(after),
        None,
        32,
    );
    vector(
        d,
        s,
        gain,
        vec![ramp(100, 0)],
        vec![],
        1,
        None,
        Some(d.depths[2] + 1),
        32,
    );
    // Logical loop reads use an independently delayed record slave.
    logical_vectors(d, s);
}
fn logical_vectors(d: &mut State, s: &mut Sim<'_>) {
    use crate::{bits::Bits, clock::Edges, model::Model, random::Random, sim::Agent};
    use std::{cell::RefCell, rc::Rc};
    struct Record {
        response: Bits,
        pending: Option<u64>,
        random: Random,
    }
    impl Agent for Record {
        fn drive(&mut self, m: &mut Model<'_>, e: Edges) {
            if e.falls(0) {
                let resetting = m.get("reset") != 0;
                m.set(
                    "loop_read_ready",
                    u64::from(!resetting && self.random.yes(3)),
                );
                m.set(
                    "loop_response_valid",
                    u64::from(!resetting && self.pending == Some(0)),
                );
                m.set_bits("loop_response_data", &self.response);
            }
        }
        fn sample(&mut self, m: &Model<'_>, e: Edges) {
            if !e.rises(0) {
                return;
            }
            if m.get("reset") != 0 {
                self.pending = None;
                return;
            }
            let accepted = m.get("loop_read_valid") != 0 && m.get("loop_read_ready") != 0;
            if self.pending == Some(0) {
                self.pending = None;
            } else if let Some(n) = &mut self.pending {
                *n -= 1;
            }
            if accepted {
                assert_eq!(m.get("loop_read_slot"), 3);
                assert!(self.pending.is_none());
                self.pending = Some(3);
            }
        }
    }
    let record = Rc::new(RefCell::new(Record {
        response: Bits::zero(164),
        pending: None,
        random: Random::new(20261110),
    }));
    s.agent(record.clone());
    for mode in 0..4 {
        for generation in [0x81234567u64, 0x81234566] {
            let mut response = Bits::zero(164);
            response
                .set(0, 1, 1)
                .set(1, 32, generation)
                .set(33, 2, mode)
                .set(35, 64, 64)
                .set(99, 64, 32)
                .set(163, 1, 1);
            record.borrow_mut().response = response;
            let instruction = json!({"kind":"process_logical_playback","loop_id":{"slot":3,"generation":0x81234567u32},"view":0,"destination":91});
            let mut value = case(
                vec![instruction],
                vec![word(u32::MAX as u64)],
                d.depths[2],
                None,
                32,
            );
            value["program"]["chunk_frames"] = json!(32);
            d.load(s, &value);
            let status = d.execute(s, &[]);
            let jobs = d.drain(s);
            if generation != 0x81234567 || mode == 3 {
                assert_eq!((status, jobs), (2, vec![]));
            } else {
                assert_eq!(status, 0);
                assert_eq!(
                    jobs,
                    vec![if mode == 1 {
                        json!({"kind":"read_view","destination":91,"view":u32::MAX,"position_chunks":1,"length_chunks":2})
                    } else {
                        json!({"kind":"silence","destination":91})
                    }]
                );
            }
        }
    }
    if d.depths[2] >= 2 {
        for first in [false, true] {
            let mut response = Bits::zero(164);
            response
                .set(0, 1, 1)
                .set(1, 32, 0x81234567)
                .set(33, 2, 2)
                .set(163, 1, u64::from(first));
            record.borrow_mut().response = response;
            let value = case(
                vec![
                    json!({"kind":"process_logical_capture","loop_id":{"slot":3,"generation":0x81234567u32},"view":0,"source":91}),
                ],
                vec![word(37)],
                d.depths[2],
                Some(2),
                32,
            );
            d.load(s, &value);
            assert_eq!(d.execute(s, &[]), 0);
            let mut jobs = if first {
                vec![json!({"kind":"begin_capture","view":37})]
            } else {
                vec![]
            };
            jobs.push(json!({"kind":"capture","source":91,"view":37}));
            assert_eq!(d.drain(s), jobs);
        }
    }
}
fn fifo_order_wrap_and_partial_failure(d: &mut State, s: &mut Sim<'_>) {
    let count = d.depths[0].min(d.depths[2]);
    let instructions: Vec<_> = (0..count).map(|_| json!({"kind":"process_input","port":d.random.below(65536),"destination":d.random.below(65536)})).collect();
    let expected: Vec<_> = instructions
        .iter()
        .map(|i| json!({"kind":"input","port":i["port"],"destination":i["destination"]}))
        .collect();
    d.load(
        s,
        &case(instructions.clone(), vec![], d.depths[2], None, 32),
    );
    for _ in 0..2 * d.depths[2] / count + 3 {
        assert_eq!(d.execute(s, &[]), 0);
        assert_eq!(d.drain(s), expected);
    }
    if d.depths[0] > d.depths[2] {
        let mut program = instructions;
        program.push(json!({"kind":"process_gain","ramp":0,"source":0,"destination":0}));
        let value = case(program, vec![ramp(100, 0)], d.depths[2], Some(0), 32);
        d.load(s, &value);
        assert_eq!(d.execute(s, &[]), 1);
        assert_eq!(d.drain(s), expected);
        assert_eq!(d.snapshot(s), value["initial_state"]);
    }
    if d.depths[0] >= 2 {
        let value = case(
            vec![
                json!({"kind":"process_input","port":0,"destination":1}),
                json!({"kind":"process_gain","ramp":65535,"source":1,"destination":2}),
            ],
            vec![],
            d.depths[2],
            Some(0),
            32,
        );
        d.load(s, &value);
        assert_eq!(d.execute(s, &[]), 2);
        assert_eq!(
            d.drain(s),
            vec![json!({"kind":"input","port":0,"destination":1})]
        );
    }
}
fn configuration_and_error_protocol(d: &mut State, s: &mut Sim<'_>) {
    use crate::{bits::Bits, codec};
    let instruction = json!({"kind":"process_gain","ramp":0,"source":1,"destination":2});
    let value = case(
        vec![instruction.clone()],
        vec![ramp(100, 0)],
        d.depths[2],
        None,
        32,
    );
    d.load(s, &value);
    for (state, depth) in [(false, d.depths[0]), (true, d.depths[1])] {
        d.configure(s, state, depth, Some(&Bits::from_u64(123)), true);
        d.configure(s, state, depth, None, true);
        d.configure(s, state, 65535, None, true);
    }
    assert_eq!(d.snapshot(s), value["initial_state"]);
    assert_eq!(
        d.configure(s, false, 0, None, false).array::<3>(),
        codec::instruction(&instruction).unwrap().array::<3>()
    );
    d.run_inputs(s, &[]);
    s.model.set_all(&[
        ("cfg_valid", 1),
        ("cfg_state", 1),
        ("cfg_write", 0),
        ("cfg_address", 0),
    ]);
    let (before, after) = d.tick(s);
    assert_ne!(before.get("cfg_ready"), 0);
    assert_eq!(before.get("run_ready"), 0);
    assert_ne!(after.get("cfg_response_valid"), 0);
    assert_eq!(after.get("busy"), 0);
    s.model.set("cfg_valid", 0);
    assert_eq!(d.tick(s).0.get("run_ready"), 0);
    s.model.set("run_valid", 0);
    assert_ne!(d.start(s, &[]).get("busy"), 0);
    s.model.set_all(&[
        ("cfg_valid", 1),
        ("cfg_state", 1),
        ("cfg_write", 1),
        ("cfg_address", 0),
        ("cfg_write_data", 7),
        ("run_valid", 1),
        ("run_instruction_count", 0),
        ("job_ready", 1),
    ]);
    let mut done = false;
    for _ in 0..20 {
        let (before, after) = d.tick(s);
        if after.get("done") != 0 {
            assert_ne!(before.get("busy"), 0);
            assert_eq!(before.get("cfg_ready") | before.get("run_ready"), 0);
            done = true;
            break;
        }
    }
    assert!(done);
    s.model
        .set_all(&[("cfg_valid", 0), ("run_valid", 0), ("job_ready", 0)]);
    d.tick(s);
    assert_eq!(
        d.drain(s),
        vec![json!({"kind":"gain","source":1,"destination":2,"ramp":ramp(100,0)["value"]})]
    );
    assert_eq!(d.snapshot(s)["memory"][0]["value"]["elapsed"], 32);
    d.load(s, &value);
    assert_eq!(
        d.execute(s, &[("run_instruction_count", d.depths[0] + 1)]),
        3
    );
    assert_eq!(d.snapshot(s), value["initial_state"]);
    assert_eq!(d.execute(s, &[("run_state_count", d.depths[1] + 1)]), 2);
    assert_eq!(d.snapshot(s), value["initial_state"]);
    d.configure(s, false, 0, Some(&Bits::from_u64(255)), false);
    assert_eq!(d.execute(s, &[]), 3);
    assert!(d.drain(s).is_empty());
    assert_eq!(d.snapshot(s), value["initial_state"]);
    for bad in [json!(65536), json!(u32::MAX), json!(-1)] {
        assert!(
            codec::instruction(&json!({"kind":"process_input","port":bad,"destination":0}))
                .is_err()
        );
    }
}
fn nonempty_admission_and_reset(d: &mut State, s: &mut Sim<'_>) {
    let instruction = json!({"kind":"process_gain","ramp":0,"source":1,"destination":2});
    let value = case(
        vec![instruction.clone()],
        vec![ramp(100, 0)],
        d.depths[2],
        None,
        32,
    );
    d.load(s, &value);
    assert_ne!(d.start(s, &[]).get("busy"), 0);
    s.model.set("reset", 1);
    let (_, after) = d.tick(s);
    assert_eq!(
        after.get("busy") | after.get("job_valid") | after.get("queued_jobs"),
        0
    );
    s.model.set("reset", 0);
    d.tick(s);
    assert_eq!(d.snapshot(s), value["initial_state"]);
    assert_eq!(d.execute(s, &[]), 0);
    let updated = d.snapshot(s);
    assert_eq!(s.model.get("queued_jobs"), 1);
    assert_eq!(d.execute(s, &[]), 1);
    assert_eq!(d.snapshot(s), updated);
    assert_eq!(s.model.get("queued_jobs"), 1);
    s.model.set("reset", 1);
    d.tick(s);
    s.model.set("reset", 0);
    d.tick(s);
    assert_eq!(s.model.get("queued_jobs"), 0);
    assert_eq!(d.snapshot(s), updated);
    assert_eq!(
        d.configure(s, false, 0, None, false).array::<3>(),
        crate::codec::instruction(&instruction)
            .unwrap()
            .array::<3>()
    );
    assert_eq!(d.execute(s, &[]), 0);
    assert_eq!(
        d.drain(s),
        vec![
            json!({"kind":"gain","source":1,"destination":2,"ramp":updated["memory"][0]["value"]})
        ]
    );
    assert_eq!(d.snapshot(s)["memory"][0]["value"]["elapsed"], 64);
}
