//! Production-resolver comparisons, order validation and terminal transport faults.
use crate::{
    Result,
    bits::Bits,
    drivers::transition::{self as abi, Transition},
    model::{Design, Probe},
    sim::Sim,
};
use rt_backend_testbench::transition_fixture as production;
use serde_json::{Value, json};
mod control;

pub const CONFIGS: &[(&str, [u64; 3])] = &[
    ("default", [64, 64, 192]),
    ("small", [7, 11, 25]),
    ("minimum", [1, 1, 3]),
    ("wide", [130, 130, 390]),
    ("deployment", [64, 272, 400]),
];
pub fn design(depths: [u64; 3]) -> Design {
    let root = crate::root();
    let design = Design::new(
        "transition_processor",
        ["tp_ram", "tp_priority", "transition_processor"]
            .iter()
            .map(|n| root.join(format!("fpga/transition_processor/rtl/{n}.sv")))
            .collect(),
    )
    .parameters(&[
        ("LOOP_DEPTH", depths[0]),
        ("REQUEST_DEPTH", depths[1]),
        ("EVENT_DEPTH", depths[2]),
    ])
    .probes(vec![
        Probe::inject("frame"),
        Probe::inject("phase"),
        Probe::inject("stage_counter"),
    ]);
    design
}
pub fn lid(slot: u64) -> Value {
    json!({"slot":slot,"generation":3})
}
pub fn spec(slot: u64, length: u64, source: Option<u64>, repeat: bool) -> Value {
    json!({"id":lid(slot),"kind":"primitive","length":length,"sync_source":source.map(lid),"repeat_sync":repeat,"timeline":null})
}
pub fn play(position: u64, retrigger: bool) -> Value {
    json!({"kind":"play","position":position,"retrigger":retrigger})
}
pub fn seek(position: u64, retrigger: bool) -> Value {
    json!({"kind":"seek","position":position,"retrigger":retrigger})
}
pub fn command(
    target: u64,
    sequence: u64,
    operation: Value,
    at: u64,
    source: Option<u64>,
    skip: u64,
) -> Value {
    json!({"target":lid(target),"sequence":sequence,"operation":operation,"at":at,"timing":source.map(|s| json!({"kind":"after_ticks","source":lid(s),"skip":skip})).unwrap_or(json!({"kind":"at"}))})
}
pub fn step(commands: Vec<Value>) -> Value {
    json!({"collect_completed":true,"collect":[],"cancel":[],"commands":commands})
}
pub fn case(
    name: &str,
    loops: Vec<Value>,
    steps: Vec<Value>,
    q: u64,
    slots: Option<u64>,
    requests: u64,
) -> Value {
    let slots = slots.unwrap_or_else(|| {
        loops
            .iter()
            .map(|v| v["id"]["slot"].as_u64().unwrap() + 1)
            .max()
            .unwrap_or(1)
    });
    json!({"format_version":1,"name":name,"plan":{"description":{"quantum":q,"loops":loops},"limits":{"loop_slots":slots,"pending_requests":requests,"events":2*slots+requests}},"steps":steps})
}
pub fn directed(depths: [u64; 3]) -> Vec<Value> {
    let catalog: Vec<Value> =
        serde_json::from_str(include_str!("../../fixtures/transition-directed.json")).unwrap();
    catalog
        .into_iter()
        .filter_map(|row| {
            if row["minimum_loops"].as_u64().unwrap() > depths[0]
                || row["minimum_requests"].as_u64().unwrap() > depths[1]
            {
                return None;
            }
            let mut case = row["case"].clone();
            if row["bounded_requests"].as_bool().unwrap() {
                let requests = depths[1].min(32);
                case["plan"]["limits"]["pending_requests"] = json!(requests);
                case["plan"]["limits"]["events"] =
                    json!(2 * case["plan"]["limits"]["loop_slots"].as_u64().unwrap() + requests);
            }
            Some(case)
        })
        .collect()
}
pub fn seeded(depths: [u64; 3], seed: u64) -> Value {
    struct Lcg(u64);
    impl Lcg {
        fn below(&mut self, n: u64) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (self.0 ^ (self.0 >> 29)) % n
        }
    }
    let mut random = Lcg(seed + 1);
    let q = [1, 16, 32, 64, 128][seed as usize % 5];
    let slots = depths[0].min(12);
    let n = 1 + random.below(slots);
    let loops: Vec<_> = (0..n)
        .map(|i| {
            let source = if random.below(4) != 0 {
                Some(random.below(i + 1))
            } else {
                None
            };
            spec(i, random.below(5) * q, source, random.below(2) != 0)
        })
        .collect();
    let mut sequence = 0;
    let steps = (0..80)
        .map(|quantum| {
            let commands = (0..random.below(6))
                .map(|_| {
                    let target = random.below(n);
                    sequence += 1;
                    let op = match random.below(7) {
                        0 => json!({"kind":"stop"}),
                        1 | 2 => json!({"kind":"record"}),
                        3 | 4 => play(random.below(6) * q, random.below(2) != 0),
                        _ => seek(random.below(6) * q, random.below(2) != 0),
                    };
                    let (source, skip) = if random.below(2) != 0
                        && !loops[target as usize]["sync_source"].is_null()
                    {
                        (
                            Some(
                                loops[target as usize]["sync_source"]["slot"]
                                    .as_u64()
                                    .unwrap(),
                            ),
                            random.below(3),
                        )
                    } else {
                        (None, 0)
                    };
                    command(
                        target,
                        sequence,
                        op,
                        quantum * q + random.below(3 * q),
                        source,
                        skip,
                    )
                })
                .collect();
            step(commands)
        })
        .collect();
    case(
        &format!("seed-{seed}"),
        loops,
        steps,
        q,
        Some(slots),
        depths[1].min(32),
    )
}
pub fn workloads() -> Vec<Value> {
    let loops = (0..64)
        .map(|i| {
            spec(
                i,
                if i % 8 == 0 { 128 } else { 32 * (1 + i % 4) },
                if i % 8 == 0 { None } else { Some(i / 8 * 8) },
                i % 8 != 0,
            )
        })
        .collect();
    let first = (0..64)
        .map(|i| {
            command(
                i,
                i + 1,
                if i % 8 != 0 && i % 3 == 0 {
                    json!({"kind":"record"})
                } else {
                    play(0, true)
                },
                0,
                None,
                0,
            )
        })
        .collect();
    let next = (0..64)
        .map(|i| {
            command(
                i,
                i + 65,
                play(0, true),
                if i % 8 == 0 { 128 } else { 32 },
                if i % 8 == 0 { None } else { Some(i / 8 * 8) },
                0,
            )
        })
        .collect();
    let mut steps = vec![step(first), step(next)];
    steps.extend((0..14).map(|_| step(vec![])));
    let grouped = case(
        "8x8-tracks-record-play-and-repeat-sync",
        loops,
        steps,
        32,
        None,
        64,
    );
    let loops = (0u64..64)
        .map(|i| spec(i, 32, Some(i.saturating_sub(1)), false))
        .collect();
    let first = (0..64)
        .map(|i| command(i, i + 1, play(0, true), 0, None, 0))
        .collect();
    let next = (0u64..64)
        .map(|i| command(i, i + 65, seek(0, true), 0, Some(i.saturating_sub(1)), 0))
        .collect();
    let mut steps = vec![step(first), step(next)];
    steps.extend((0..6).map(|_| step(vec![])));
    vec![
        grouped,
        case(
            "64-member-sync-chain-all-active",
            loops,
            steps,
            32,
            None,
            64,
        ),
    ]
}
pub fn compare(d: &mut Transition, s: &mut Sim<'_>, value: &Value) -> Result<Vec<u64>> {
    let case: production::TransitionCase = serde_json::from_value(value.clone())?;
    let expected = production::run_transition_oracle(case.clone())
        .map_err(|e| format!("production transition oracle: {e:?}"))?;
    let image = production::compile(&case.plan)
        .map_err(|e| format!("production transition compile: {e:?}"))?;
    d.load(s, &serde_json::to_value(image)?);
    let start = d.phase_cycles.len();
    for (i, (step, expected)) in value["steps"]
        .as_array()
        .unwrap()
        .iter()
        .zip(expected.results)
        .enumerate()
    {
        let observed = d.step(s, step);
        let expected = serde_json::to_value(expected)?;
        if observed != expected {
            let directory = crate::root().join("work/marlin-suite/transition/failure");
            std::fs::create_dir_all(&directory)?;
            for (name, value) in [
                ("case", value),
                ("expected", &expected),
                ("observed", &observed),
            ] {
                std::fs::write(
                    directory.join(format!("{name}.json")),
                    serde_json::to_vec_pretty(value)?,
                )?;
            }
            panic!(
                "transition {} step {i}: {observed} != {expected}",
                value["name"]
            );
        }
    }
    Ok(d.phase_cycles[start..].to_vec())
}
pub fn run(name: &str, depths: [u64; 3], random_cases: u64) -> Result<()> {
    let compiled = design(depths).build(&crate::root().join("work/marlin-suite/transition"))?;
    let mut s = Sim::new(compiled.session(None)?, &[("clk", 10_000, 0)]);
    s.agent(crate::protocol::ReadyValid::new(
        "response_valid",
        "response_ready",
        &["response_error", "response_data"],
    ));
    let mut d = Transition::new(depths);
    for case in directed(depths) {
        compare(&mut d, &mut s, &case)?;
    }
    for seed in 0..random_cases {
        compare(&mut d, &mut s, &seeded(depths, seed))?;
    }
    indexed_install_guards(&mut d, &mut s)?;
    control::held_replies(&mut d, &mut s)?;
    control::publication(&mut d, &mut s)?;
    transport_and_negative_cases(&mut d, &mut s)?;
    dependency_order_validation(&mut d, &mut s);
    scratch_initialization_and_reset(&mut d, &mut s)?;
    let mut cases = workloads();
    if depths[1] >= 130 {
        let mut spare = cases[1].clone();
        spare["name"] = json!("64-loop-chain-with-130-request-slots");
        spare["plan"]["limits"]["pending_requests"] = json!(130);
        spare["plan"]["limits"]["events"] = json!(258);
        cases.push(spare);
    }
    if depths[0] >= 64 && depths[1] >= 64 && depths[2] >= 192 {
        for case in cases {
            let cycles = compare(&mut d, &mut s, &case)?;
            let output = crate::root().join(format!(
                "work/marlin-suite/transition/{name}-{}.json",
                case["name"].as_str().unwrap()
            ));
            std::fs::write(
                output,
                serde_json::to_vec_pretty(
                    &json!({"quantum_frames":32,"clock_hz":100_000_000,"quantum_cycles":cycles}),
                )?,
            )?;
        }
    }
    println!("transition/{name}: 9 regression groups, {random_cases} seeds x 80 quanta");
    Ok(())
}
fn transport_case() -> Value {
    case(
        "transport",
        vec![spec(0, 32, None, false)],
        vec![step(vec![command(0, 1, play(0, true), 0, None, 0)])],
        16,
        None,
        1,
    )
}
fn load(d: &mut Transition, s: &mut Sim<'_>, value: &Value) -> Result<()> {
    let c: production::TransitionCase = serde_json::from_value(value.clone())?;
    let image = production::compile(&c.plan).map_err(|e| format!("transition compile: {e:?}"))?;
    d.load(s, &serde_json::to_value(image)?);
    Ok(())
}
fn raw_requests(d: &mut Transition, s: &mut Sim<'_>) -> Vec<Bits> {
    (0..d.requests)
        .map(|slot| d.raw_command(s, 13, slot, &Bits::default(), None, false).1)
        .collect()
}
fn indexed_install_guards(d: &mut Transition, s: &mut Sim<'_>) -> Result<()> {
    fn reserve(
        d: &mut Transition,
        s: &mut Sim<'_>,
        original: [u32; 20],
    ) -> rt_fpga_adapter::transition_service::Transfer {
        let mut owner = d.service.take().unwrap();
        let first = owner.begin(4, 0, original).unwrap();
        let final_transfer = abi::ready_install(&mut owner, d, s, first);
        d.service = Some(owner);
        final_transfer
    }
    fn finish(d: &mut Transition, reply: (u64, Bits)) {
        assert_eq!(reply.0, 0);
        assert!(matches!(
            d.service
                .as_mut()
                .unwrap()
                .finish(rt_fpga_adapter::looping::TpReply {
                    error: 0,
                    words: reply.1.0.try_into().unwrap()
                })
                .unwrap(),
            rt_fpga_adapter::transition_service::Progress::Reply(_)
        ));
    }
    let value = transport_case();
    d.reset(s);
    d.ok(s, 1, 0, &abi::spec(&spec(0, 64, None, false)));
    d.ok(s, 2, 0, &Bits::default());
    let mut prepare = Bits(vec![1, 1, 3, 1, 16, 0]);
    assert_eq!(d.raw_command(s, 3, 0, &prepare, None, false).0, 1);
    assert_eq!(d.status(s)["configured"], false);
    prepare.set(
        160,
        32,
        u64::from(rt_fpga_adapter::transition_service::wire::ABI),
    );
    assert_eq!(d.raw_command(s, 3, 0, &prepare, None, false).0, 0);
    load(d, s, &value)?;
    let original: [u32; 20] = abi::command(&value["steps"][0]["commands"][0])
        .0
        .try_into()
        .unwrap();
    let transfer = reserve(d, s, original);
    let empty = raw_requests(d, s);
    let mut packet = Bits(transfer.words.to_vec());
    for (slot, generation) in [(1, 1), (0, 0), (0, 2)] {
        packet.set(1, 32, generation);
        assert_eq!(d.raw_command(s, 4, slot, &packet, None, false).0, 3);
        assert_eq!(raw_requests(d, s), empty);
    }
    packet.set(1, 32, 1);
    let reply = d.raw_command(s, 4, 0, &packet, None, false);
    finish(d, reply);
    let pending = raw_requests(d, s);
    assert_eq!(d.raw_command(s, 4, 0, &packet, None, false).0, 5);
    assert_eq!(raw_requests(d, s), pending);
    d.ok(s, 9, 0, &Bits::default());
    let terminal = raw_requests(d, s);
    assert_eq!(d.raw_command(s, 4, 0, &packet, None, false).0, 5);
    assert_eq!(raw_requests(d, s), terminal);
    d.ok(s, 6, 0, &Bits::from_u64(1));
    assert_eq!(d.raw_command(s, 4, 0, &packet, None, false).0, 3);
    let next = reserve(d, s, original);
    packet = Bits(next.words.to_vec());
    let reply = d.raw_command(s, 4, 0, &packet, None, false);
    finish(d, reply);
    d.ok(s, 5, 0, &Bits::from_u64(2));
    d.ok(s, 6, 0, &Bits::from_u64(2));
    let next = reserve(d, s, original);
    packet = Bits(next.words.to_vec());
    let before = raw_requests(d, s);
    s.model.inject("phase", Some(&Bits::from_u64(2)));
    assert_eq!(d.raw_command(s, 4, 0, &packet, None, false).0, 5);
    assert_eq!(raw_requests(d, s), before);
    s.model.inject("phase", None);
    Ok(())
}
fn transport_and_negative_cases(d: &mut Transition, s: &mut Sim<'_>) -> Result<()> {
    let case = transport_case();
    load(d, s, &case)?;
    let before = d.states(s);
    for opcode in [7, 8, 10, 14, 255] {
        assert_eq!(
            d.command(s, opcode, 0, &Bits::default(), Some(4), true).0,
            10
        );
        assert_eq!(d.states(s), before);
    }
    let mut bad = case["steps"][0]["commands"][0].clone();
    bad["target"]["generation"] = json!(0);
    assert_eq!(d.command(s, 4, 0, &abi::command(&bad), None, false).0, 3);
    d.ok(s, 4, 0, &abi::command(&case["steps"][0]["commands"][0]));
    d.command(s, 9, 0, &Bits::default(), Some(5), true);
    assert_eq!(d.status(s)["frame"], 16);
    assert_eq!(d.states(s)[0]["position"], 16);
    s.model
        .inject("frame", Some(&Bits::from_u64(u64::MAX - 15)));
    assert_eq!(d.command(s, 9, 0, &Bits::default(), None, false).0, 8);
    let status = d.status(s);
    assert_eq!(status["phase"], 3);
    assert_eq!(status["fault"], 8);
    assert_eq!(d.command(s, 9, 0, &Bits::default(), None, false).0, 8);
    assert_eq!(
        d.command(
            s,
            4,
            0,
            &abi::command(&command(0, 2, play(0, true), 0, None, 0)),
            None,
            false
        )
        .0,
        5
    );
    s.model.inject("frame", None);
    load(d, s, &case)?;
    assert_eq!(d.status(s)["fault"], 0);
    d.ok(s, 9, 0, &Bits::default());
    assert_eq!(d.status(s)["frame"], 16);
    Ok(())
}
fn prepare(
    d: &mut Transition,
    s: &mut Sim<'_>,
    loops: Vec<Value>,
    order: &[u64],
    slots: Option<u64>,
) -> u64 {
    let slots = slots.unwrap_or(loops.len() as u64);
    d.reset(s);
    for i in 0..slots {
        d.ok(
            s,
            1,
            i,
            &abi::spec(
                loops
                    .iter()
                    .find(|v| v["id"]["slot"] == i)
                    .unwrap_or(&Value::Null),
            ),
        );
    }
    for (i, &slot) in order.iter().enumerate() {
        d.ok(s, 2, i as u64, &Bits::from_u64(slot));
    }
    d.command(
        s,
        3,
        0,
        &Bits(vec![
            slots as u32,
            1,
            (2 * slots + 1) as u32,
            order.len() as u32,
            16,
            rt_fpga_adapter::transition_service::wire::ABI,
        ]),
        None,
        false,
    )
    .0
}
fn dependency_order_validation(d: &mut Transition, s: &mut Sim<'_>) {
    assert_eq!(
        prepare(d, s, vec![spec(0, 16, Some(0), false)], &[0], None),
        0
    );
    d.reset(s);
    assert_eq!(
        d.command(s, 2, 0, &Bits::from_u64(d.depths[0]), None, false)
            .0,
        3
    );
    assert_eq!(
        d.command(s, 2, 0, &Bits::from_u64(1 << 32), None, false).0,
        1
    );
    assert_eq!(
        d.command(s, 2, d.depths[0], &Bits::default(), None, false)
            .0,
        2
    );
    assert_eq!(prepare(d, s, vec![spec(0, 16, None, false)], &[], None), 1);
    if d.depths[0] < 2 {
        return;
    }
    assert_eq!(
        prepare(
            d,
            s,
            vec![spec(0, 16, None, false), spec(1, 16, Some(0), false)],
            &[0, 1],
            None
        ),
        0
    );
    assert_eq!(
        prepare(
            d,
            s,
            vec![spec(0, 16, None, false), spec(1, 16, Some(0), false)],
            &[1, 0],
            None
        ),
        1
    );
    assert_eq!(
        prepare(
            d,
            s,
            vec![spec(0, 16, None, false), spec(1, 16, None, false)],
            &[0, 0],
            None
        ),
        4
    );
    assert_eq!(
        prepare(d, s, vec![spec(0, 16, None, false)], &[0, 1], Some(2)),
        3
    );
    for order in [[0, 1], [1, 0]] {
        assert_eq!(
            prepare(
                d,
                s,
                vec![spec(0, 16, Some(1), false), spec(1, 16, Some(0), false)],
                &order,
                None
            ),
            1
        );
        assert_eq!(d.status(s)["configured"], false);
    }
    if d.depths[0] >= 3 {
        let loops = vec![
            spec(0, 16, Some(2), false),
            spec(1, 16, Some(0), false),
            spec(2, 16, Some(2), false),
        ];
        assert_eq!(prepare(d, s, loops.clone(), &[2, 0, 1], None), 0);
        assert_eq!(prepare(d, s, loops, &[0, 1, 2], None), 1);
        assert_eq!(
            prepare(
                d,
                s,
                vec![
                    spec(0, 16, Some(2), false),
                    spec(1, 16, Some(0), false),
                    spec(2, 16, Some(1), false)
                ],
                &[0, 1, 2],
                None
            ),
            1
        );
    }
}
fn scratch_initialization_and_reset(d: &mut Transition, s: &mut Sim<'_>) -> Result<()> {
    let value = transport_case();
    load(d, s, &value)?;
    d.ok(s, 4, 0, &abi::command(&value["steps"][0]["commands"][0]));
    s.model.set_all(&[
        ("command_valid", 1),
        ("command_opcode", 9),
        ("response_ready", 0),
    ]);
    s.idle(1);
    s.model.set("command_valid", 0);
    assert_ne!(s.model.get("busy"), 0);
    d.reset(s);
    assert_eq!(d.status(s)["configured"], false);
    load(d, s, &value)?;
    let result = d.step(s, &value["steps"][0]);
    assert_eq!(result["next_frame"], 16);
    assert_eq!(result["states"][0]["position"], 16);
    Ok(())
}

pub fn coherent_observations() -> Result<()> {
    let compiled = design([1, 2, 4])
        .parameters(&[("INTEGRATED", 1)])
        .build(&crate::root().join("work/marlin-suite/coherent-observations"))?;
    let mut s = Sim::new(compiled.session(None)?, &[("clk", 10000, 0)]);
    let mut d = Transition::new([1, 2, 4]);
    let image = case(
        "coherent-cut",
        vec![spec(0, 0, Some(0), true)],
        vec![],
        32,
        Some(1),
        2,
    );
    load(&mut d, &mut s, &image)?;
    d.ok(
        &mut s,
        4,
        0,
        &abi::command(&command(0, 1, json!({"kind":"record"}), 0, None, 0)),
    );
    let held = d.hold_raw_command(&mut s, 0, 0, &Bits::default(), false);
    let stable = |s: &Sim<'_>| {
        assert_eq!(s.model.get("response_valid"), 1);
        assert_eq!(s.model.get("response_error"), held.0);
        assert_eq!(s.model.bits("response_data"), held.1);
    };
    let execute = |s: &mut Sim<'_>, finish: bool| {
        s.model
            .set_all(&[("execute_valid", 1), ("execute_finish", u64::from(finish))]);
        s.until(1000, "independent execution ready", |m| {
            m.get("execute_ready") != 0
        });
        s.idle(1);
        s.model.set("execute_valid", 0);
        s.until(1000, "independent execution done", |m| {
            m.get("execute_done") != 0
        });
        assert_eq!(s.model.get("execute_error"), 0);
        s.idle(1);
    };
    let observe = |s: &mut Sim<'_>, kind: u64, index: u64, frame: u64, phase: u64| {
        s.model.set_all(&[
            ("observe_valid", 1),
            ("observe_kind", kind),
            ("observe_index", index),
        ]);
        s.until(1000, "observation ready", |m| m.get("observe_ready") != 0);
        s.idle(1);
        s.model.set("observe_valid", 0);
        assert_eq!(s.model.get("observe_response_valid"), 1);
        assert_eq!(s.model.get("observe_frame"), frame);
        assert_eq!(s.model.get("observe_phase"), phase);
        let row = (
            s.model.get("observe_response_error"),
            s.model.bits("observe_data"),
        );
        s.idle(1);
        row
    };
    let resident = |s: &mut Sim<'_>, opcode: u64, index: u64, data: &Bits| {
        s.model.set_all(&[
            ("resident_valid", 1),
            ("resident_opcode", opcode),
            ("resident_index", index),
        ]);
        s.model.set_bits("resident_data", data);
        s.until(1000, "resident command ready", |m| {
            m.get("resident_ready") != 0
        });
        s.idle(1);
        s.model.set("resident_valid", 0);
        s.until(1000, "resident command result", |m| {
            m.get("resident_done") != 0
        });
        let reply = (
            s.model.get("resident_error"),
            s.model.bits("resident_result"),
        );
        s.idle(1);
        reply
    };
    execute(&mut s, false);
    stable(&s);
    let (error, row) = observe(&mut s, 0, 0, 0, 2);
    assert_eq!(error, 0);
    assert_eq!(row.get(33, 2), 2);
    assert_eq!(row.get(192, 64), 0);
    stable(&s);
    assert_eq!(resident(&mut s, 12, 0, &Bits::default()), (0, row));
    assert_eq!(
        resident(&mut s, 12, u32::MAX.into(), &Bits::default()),
        (3, Bits::zero(832))
    );
    stable(&s);
    let (error, row) = observe(&mut s, 1, 0, 0, 2);
    assert_eq!(error, 0);
    assert_eq!(row.get(37, 3), 1);
    let (error, event) = observe(&mut s, 2, 0, 0, 2);
    assert_eq!(error, 0);
    assert_eq!(event.get(192, 2), 2);
    for kind in 0..4 {
        let (error, row) = observe(&mut s, kind, u32::MAX.into(), 0, 2);
        assert_eq!(error, 1);
        assert_eq!(row, Bits::zero(832));
        stable(&s);
    }
    s.model.set_all(&[
        ("observe_valid", 1),
        ("loop_read_valid", 1),
        ("loop_read_slot", 0),
    ]);
    s.model.eval();
    assert_eq!(s.model.get("observe_ready"), 0);
    assert_eq!(s.model.get("loop_read_ready"), 1);
    s.idle(1);
    s.model.set("loop_read_valid", 0);
    assert_eq!(s.model.get("loop_response_valid"), 1);
    assert_eq!(s.model.get("observe_response_valid"), 0);
    s.idle(1);
    s.model.set("observe_valid", 0);
    s.idle(1);
    s.model.set_all(&[
        ("observe_valid", 1),
        ("execute_valid", 1),
        ("execute_finish", 1),
    ]);
    s.model.eval();
    assert_eq!(s.model.get("observe_ready"), 0);
    assert_eq!(s.model.get("execute_ready"), 1);
    s.idle(1);
    s.model
        .set_all(&[("observe_valid", 0), ("execute_valid", 0)]);
    s.until(1000, "finish before observation", |m| {
        m.get("execute_done") != 0
    });
    s.idle(1);
    let (error, row) = observe(&mut s, 0, 0, 32, 0);
    assert_eq!(error, 0);
    assert_eq!(row.get(192, 64), 32);
    stable(&s);
    let (error, collected) = resident(&mut s, 6, 0, &Bits::from_u64(1));
    assert_eq!(error, 0);
    assert_eq!(collected.get(37, 3), 1);
    let (error, empty) = resident(&mut s, 13, 0, &Bits::default());
    assert_eq!(error, 0);
    assert_eq!((empty.get(0, 1), empty.get(1, 32)), (0, 2));
    stable(&s);
    s.model
        .set_all(&[("observe_valid", 1), ("resident_valid", 1), ("reset", 1)]);
    s.idle(1);
    assert_eq!(s.model.get("observe_response_valid"), 0);
    assert_eq!(s.model.get("observe_ready"), 0);
    assert_eq!(s.model.get("resident_ready"), 0);
    assert_eq!(s.model.get("resident_done"), 0);
    println!(
        "coherent observations: full loop/request/event rows, PROCESS/FINISH phase and frame, held host reply, invalid bounds, execution/SP priority and reset"
    );
    Ok(())
}
