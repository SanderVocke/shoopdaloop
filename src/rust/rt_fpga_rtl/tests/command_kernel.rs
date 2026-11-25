#![cfg(all(feature = "sim", feature = "rt-interface", not(target_arch = "wasm32")))]
use rt_fpga_compiler::Identity;
use rt_fpga_reference::{fixed::Fixed, transitions::LoopId};
use rt_fpga_rtl::{
    bits::Bits,
    host::control::{
        apply_iteration, apply_plan, clear_plan, gain_value, loop_state, set_repeat_sync,
    },
    model::Design,
    sim::Sim,
};
use shoop_rt_protocol::{
    LoopSlotId, SlotId,
    immediate::{
        ClearPlanned, Code, Command, IterationTransition, PlannedTransition, TransitionMode,
        TransitionPlan,
    },
};
use shoop_wasm_test_support::shoop_test;
use std::collections::BTreeMap;

const SLOT: u16 = 3;
const GAIN: u16 = 7;
const GENERATION: u32 = 7;
fn identity() -> Identity {
    Identity {
        slot: u32::from(SLOT),
        generation: GENERATION,
    }
}
fn id() -> LoopSlotId {
    LoopSlotId(SlotId::new(u32::from(SLOT), GENERATION))
}
fn args(command: Command) -> (u64, Bits) {
    let mut words = [0u32; 5];
    let kind = match command {
        Command::Transition(_, plan) => {
            words = [
                plan.now.map_or(u32::MAX, |m| m as u32),
                plan.first.map_or(u32::MAX, |s| s.mode as u32),
                plan.first.map_or(0, |s| s.skip),
                plan.second.map_or(u32::MAX, |s| s.mode as u32),
                plan.second.map_or(0, |s| s.skip),
            ];
            0
        }
        Command::ClearPlannedTransitions(_, mask) => {
            words[0] = mask as u32;
            1
        }
        Command::LoopRepeatSync(_, value) => {
            words[0] = u32::from(value);
            2
        }
        Command::LoopGain(_, value) => {
            words[0] = Fixed::from_f32(value).0 as u32;
            3
        }
        Command::LoopIteration(_, request) => {
            words[0] = request.iteration as u32;
            words[1] = (request.iteration >> 32) as u32;
            words[2] = u32::from(request.retrigger) | (u32::from(request.seek) << 1);
            4
        }
        _ => 7,
    };
    let mut result = Bits::zero(160);
    for (i, word) in words.into_iter().enumerate() {
        result.set(i * 32, 32, u64::from(word));
    }
    (kind, result)
}
fn oracle(mut row: [u64; 16], command: Command, self_sync: bool, frame: u64) -> (Code, [u64; 16]) {
    let state = loop_state(identity(), &row, None);
    let source = (!self_sync).then_some(LoopId {
        slot: 0,
        generation: GENERATION,
    });
    let original = row;
    let result = match command {
        Command::Transition(_, plan) => apply_plan(&mut row, state, source, frame, plan),
        Command::ClearPlannedTransitions(_, mask) => {
            clear_plan(&mut row, mask);
            Ok(Code::Applied)
        }
        Command::LoopRepeatSync(_, value) => {
            set_repeat_sync(&mut row, value);
            Ok(Code::Applied)
        }
        Command::LoopGain(..) => Ok(Code::Applied),
        Command::LoopIteration(_, request) => apply_iteration(&mut row, state, frame, request),
        _ => Err(Code::Unsupported),
    };
    match result {
        Ok(code) => (code, row),
        Err(code) => (code, original),
    }
}
fn cfg(sim: &mut Sim<'_>, state: bool, address: u16, write: Option<&Bits>) -> Bits {
    sim.model.set_all(&[
        ("cfg_valid", 1),
        ("cfg_state", u64::from(state)),
        ("cfg_address", u64::from(address)),
        ("cfg_write", u64::from(write.is_some())),
    ]);
    if let Some(write) = write {
        sim.model.set_bits("cfg_write_data", write);
    }
    sim.idle(1);
    assert_eq!(sim.model.get("cfg_response_valid"), 1);
    let value = sim.model.bits("cfg_read_data");
    sim.model.set("cfg_valid", 0);
    value
}
fn store(sim: &mut Sim<'_>, slot: u16, row: [u64; 16]) {
    for (i, word) in row.into_iter().enumerate() {
        let mut value = Bits::zero(129);
        value.set(0, 64, word);
        cfg(sim, false, slot * 16 + i as u16, Some(&value));
    }
}
fn load(sim: &mut Sim<'_>, slot: u16) -> [u64; 16] {
    std::array::from_fn(|i| cfg(sim, false, slot * 16 + i as u16, None).get(0, 64))
}
fn execute(
    sim: &mut Sim<'_>,
    command: Command,
    self_sync: bool,
    frame: u64,
    stalled: bool,
) -> (Code, u64) {
    execute_at(sim, SLOT, command, self_sync, frame, stalled)
}
fn execute_at(
    sim: &mut Sim<'_>,
    slot: u16,
    command: Command,
    self_sync: bool,
    frame: u64,
    stalled: bool,
) -> (Code, u64) {
    let (kind, args) = args(command);
    sim.model.set_bits("command_args", &args);
    sim.model.set_all(&[
        ("command_valid", 1),
        ("command_kind", kind),
        ("command_slot", u64::from(slot)),
        ("command_generation", u64::from(GENERATION)),
        ("command_self_sync", u64::from(self_sync)),
        ("command_frame", frame),
        ("command_gain_address", u64::from(GAIN)),
    ]);
    assert_eq!(sim.model.get("command_ready"), 1);
    sim.idle(1);
    sim.model.set("command_valid", 0);
    for cycle in 0..2000 {
        sim.model
            .set("allow_memory", u64::from(!stalled || cycle % 11 >= 7));
        if sim.model.get("result_valid") != 0 {
            break;
        }
        sim.idle(1);
    }
    assert_eq!(sim.model.get("result_valid"), 1, "kernel watchdog");
    let code = Code::from_raw(sim.model.get("result_code") as u32).unwrap();
    let cycles = sim.model.get("command_cycles");
    assert_eq!(sim.model.get("write_fault"), 0);
    sim.idle(3);
    assert_eq!(sim.model.get("result_code"), code as u64);
    assert_eq!(sim.model.get("command_cycles"), cycles);
    sim.model
        .set_all(&[("result_ready", 1), ("allow_memory", 1)]);
    sim.idle(1);
    sim.model.set("result_ready", 0);
    (code, cycles)
}
fn random(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}
fn row(seed: &mut u64, class: u8, mode: u64) -> [u64; 16] {
    let mut row = std::array::from_fn(|_| random(seed));
    row[0] =
        (row[0] & !((3 << 33) | 0x1ffffffff)) | 1 | (u64::from(GENERATION) << 1) | (mode << 33);
    row[1] = (row[1] & !((1 << 44) | (1 << 45)))
        | (u64::from(class == 1) << 45)
        | (u64::from(class == 2) << 44);
    row[2] = 64;
    row[3] = 256;
    row[4] = 128;
    row[5] = 9;
    row[6] = if class == 1 { 32 } else { 11 };
    row[7] = 13;
    row
}
fn mode(value: u8) -> Option<TransitionMode> {
    match value {
        0 => None,
        1 => Some(TransitionMode::Stopped),
        2 => Some(TransitionMode::Playing),
        _ => Some(TransitionMode::Recording),
    }
}
fn step(value: u8, skip: u32) -> Option<PlannedTransition> {
    mode(value).map(|mode| PlannedTransition { mode, skip })
}
fn compare(
    sim: &mut Sim<'_>,
    original: [u64; 16],
    command: Command,
    self_sync: bool,
    frame: u64,
    stalled: bool,
) -> (Code, u64) {
    store(sim, SLOT, original);
    let (expected_code, expected) = oracle(original, command, self_sync, frame);
    let (code, cycles) = execute(sim, command, self_sync, frame, stalled);
    let actual = load(sim, SLOT);
    assert_eq!(
        code, expected_code,
        "{command:?}, self={self_sync}, frame={frame}, row={original:x?}"
    );
    assert_eq!(
        actual, expected,
        "{command:?}, self={self_sync}, frame={frame}, code={code:?}"
    );
    (code, cycles)
}

#[shoop_test(no_wasm = "Isolated experimental RTL edit kernel differential test")]
fn retained_command_kernel_matches_rust_and_reports_cycle_bounds() {
    let root = rt_fpga_rtl::root();
    let source = root.join("fpga/experiments/command_fifo");
    let compiled = Design::new(
        "fifo_edit_probe",
        vec![source.join("edit_kernel.sv"), source.join("probe.sv")],
    )
    .parameters(&[("LOOPS", 128), ("STATES", 512)])
    .build(&root.join("work/cmd-fifo-kernel"))
    .unwrap();
    let mut sim = Sim::new(compiled.session(None).unwrap(), &[("clk", 10_000, 0)]);
    sim.max_ps = 100_000_000_000;
    sim.model.set_all(&[("reset", 1), ("allow_memory", 1)]);
    sim.idle(2);
    sim.model.set("reset", 0);
    sim.idle(1);
    let mut seed = 0xa43fcd7219b60e85;
    let mut stats: BTreeMap<String, (u64, u64, u64)> = BTreeMap::new();
    let mut count = 0;
    for class in 0..3 {
        for current in 0..3 {
            for self_sync in [false, true] {
                for now in 0..4 {
                    for first in 0..4 {
                        for second in 0..4 {
                            for skip in [0, 1] {
                                let original = row(&mut seed, class, current);
                                let command = Command::Transition(
                                    u32::from(SLOT),
                                    TransitionPlan {
                                        now: mode(now),
                                        first: step(first, skip),
                                        second: step(second, skip + 1),
                                    },
                                );
                                let (code, cycles) =
                                    compare(&mut sim, original, command, self_sync, 100, false);
                                let key = format!("transition/class{class}/{code:?}");
                                let e = stats.entry(key).or_insert((0, u64::MAX, 0));
                                e.0 += 1;
                                e.1 = e.1.min(cycles);
                                e.2 = e.2.max(cycles);
                                count += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    for iteration in 0..1024 {
        let class = (random(&mut seed) % 3) as u8;
        let current = random(&mut seed) % 3;
        let mut original = row(&mut seed, class, current);
        let edge = [
            0,
            1,
            32,
            u64::from(u32::MAX),
            u64::from(u32::MAX) + 1,
            u64::MAX,
        ];
        for index in [2, 3, 4, 6, 7] {
            original[index] = edge[(random(&mut seed) % edge.len() as u64) as usize];
        }
        let self_sync = random(&mut seed) & 1 != 0;
        let frame = edge[(random(&mut seed) % edge.len() as u64) as usize];
        let command = match iteration % 7 {
            0 => Command::Transition(
                u32::from(SLOT),
                TransitionPlan {
                    now: mode((random(&mut seed) % 4) as u8),
                    first: step((random(&mut seed) % 4) as u8, u32::MAX),
                    second: step((random(&mut seed) % 4) as u8, 0),
                },
            ),
            1 => Command::ClearPlannedTransitions(
                u32::from(SLOT),
                [
                    ClearPlanned::First,
                    ClearPlanned::Second,
                    ClearPlanned::Both,
                ][(random(&mut seed) % 3) as usize],
            ),
            2 => Command::LoopRepeatSync(id(), random(&mut seed) & 1 != 0),
            3 => Command::LoopGain(
                u32::from(SLOT),
                (random(&mut seed) as i32 as f32) / 16777216.,
            ),
            4 | 5 => Command::LoopIteration(
                id(),
                IterationTransition {
                    iteration: edge[(random(&mut seed) % edge.len() as u64) as usize],
                    retrigger: random(&mut seed) & 1 != 0,
                    seek: random(&mut seed) & 1 != 0,
                },
            ),
            _ => Command::InputGain(0, 1.),
        };
        let (code, cycles) = compare(&mut sim, original, command, self_sync, frame, false);
        if let Command::LoopGain(_, value) = command {
            assert_eq!(cfg(&mut sim, true, GAIN, None), gain_value(value));
        }
        let key = format!("kind{}/{code:?}", args(command).0);
        let e = stats.entry(key).or_insert((0, u64::MAX, 0));
        e.0 += 1;
        e.1 = e.1.min(cycles);
        e.2 = e.2.max(cycles);
        count += 1;
    }
    for i in 0..512 {
        let mut original = row(&mut seed, 1, (i % 3) as u64);
        original[3] = if i % 2 == 0 {
            u64::MAX
        } else {
            random(&mut seed) % 4096
        };
        original[6] = random(&mut seed) & u64::from(u32::MAX);
        let command = Command::LoopIteration(
            id(),
            IterationTransition {
                iteration: if i % 2 == 0 {
                    random(&mut seed) % 65536
                } else {
                    random(&mut seed)
                },
                retrigger: i & 1 != 0,
                seek: i & 2 != 0,
            },
        );
        let (code, cycles) = compare(&mut sim, original, command, false, 100, false);
        let e = stats
            .entry(format!("multiply/{code:?}"))
            .or_insert((0, u64::MAX, 0));
        e.0 += 1;
        e.1 = e.1.min(cycles);
        e.2 = e.2.max(cycles);
        count += 1;
    }
    let mut group_cycles = Vec::new();
    for targets in [16u16, 32] {
        for skip in [0, 1] {
            let original = row(&mut seed, 0, 0);
            let command = Command::Transition(
                u32::from(SLOT),
                TransitionPlan {
                    first: step(3, skip),
                    ..Default::default()
                },
            );
            let (expected_code, expected) = oracle(original, command, false, 500);
            for slot in 1..=targets {
                store(&mut sim, slot, original);
            }
            let mut cycles = 0;
            for slot in 1..=targets {
                let (code, elapsed) = execute_at(&mut sim, slot, command, false, 500, false);
                assert_eq!(code, expected_code);
                cycles += elapsed;
                count += 1;
            }
            for slot in 1..=targets {
                assert_eq!(load(&mut sim, slot), expected);
            }
            group_cycles.push(serde_json::json!({ "targets": targets, "skip": skip, "kernel_cycles_sum": cycles, "scope": "sequential kernel invocations at one supplied frame, not a hardware group FIFO" }));
        }
    }
    let mut original = row(&mut seed, 0, 0);
    let commands = [
        Command::Transition(u32::from(SLOT), TransitionMode::Recording.into()),
        Command::Transition(
            u32::from(SLOT),
            TransitionPlan {
                first: step(2, 1),
                second: step(1, 2),
                ..Default::default()
            },
        ),
        Command::ClearPlannedTransitions(u32::from(SLOT), ClearPlanned::Both),
        Command::LoopRepeatSync(id(), true),
    ];
    store(&mut sim, SLOT, original);
    for i in 0..32 {
        let command = commands[i % commands.len()];
        let (code, expected) = oracle(original, command, false, 200);
        let (actual, _) = execute(&mut sim, command, false, 200, true);
        assert_eq!(actual, code);
        assert_eq!(load(&mut sim, SLOT), expected);
        original = expected;
        count += 1;
    }
    let mut invalid = original;
    invalid[0] &= !1;
    store(&mut sim, SLOT, invalid);
    assert_eq!(
        execute(&mut sim, commands[0], false, 200, false).0,
        Code::TargetUnavailable
    );
    assert_eq!(load(&mut sim, SLOT), invalid);
    let mut stale = original;
    stale[0] ^= 1 << 1;
    store(&mut sim, SLOT, stale);
    assert_eq!(
        execute(&mut sim, commands[0], false, 200, false).0,
        Code::TargetUnavailable
    );
    assert_eq!(load(&mut sim, SLOT), stale);
    let report = serde_json::json!({ "cases": count + 2, "seed": "a43fcd7219b60e85", "cycles": stats, "group_work": group_cycles, "scope": "isolated unconnected kernel; no FIFO, audio, switch or production-path claim" });
    std::fs::create_dir_all(root.join("work/cmd-fifo-investigation")).unwrap();
    std::fs::write(
        root.join("work/cmd-fifo-investigation/kernel-cycles.json"),
        serde_json::to_string_pretty(&report).unwrap(),
    )
    .unwrap();
    println!("{report}");
}
