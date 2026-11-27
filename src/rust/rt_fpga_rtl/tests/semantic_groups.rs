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

const SLOT: u16 = 3;
const GENERATION: u32 = 7;
const GAIN: u16 = 7;
fn run(test: impl FnOnce(&mut Sim<'_>)) {
    let root = rt_fpga_rtl::root();
    let source = root.join("fpga/host_bridge/rtl");
    let design = Design::new(
        "semantic_group_probe",
        vec![
            source.join("fifo_group_banks.sv"),
            source.join("fifo_edit_kernel.sv"),
            source.join("fifo_group_consumer.sv"),
            root.join("src/rust/rt_fpga_rtl/tests/support/semantic_group_probe.sv"),
        ],
    )
    .build(&root.join("work/cmd-fifo-groups"))
    .unwrap();
    let mut sim = Sim::new(design.session(None).unwrap(), &[("clk", 10_000, 0)]);
    sim.max_ps = 100_000_000_000;
    reset(&mut sim, 1);
    test(&mut sim);
}
fn reset(sim: &mut Sim<'_>, epoch: u64) {
    sim.model.set_all(&[
        ("reset", 1),
        ("epoch", epoch),
        ("pipeline_fault", 0),
        ("host_valid", 0),
        ("result_read_valid", 0),
        ("boundary_valid", 0),
        ("render_ready", 1),
        ("select_ready", 1),
        ("select_error", 0),
        ("handoff_valid", 0),
        ("handoff_error", 0),
        ("allow_memory", 1),
        ("inject_read_error", 0),
        ("inject_write_error", 0),
        ("cfg_valid", 0),
    ]);
    sim.idle(2);
    sim.model.set("reset", 0);
    sim.idle(1);
}
fn host(sim: &mut Sim<'_>, operation: u64, bank: u64, word: u32, sequence: u64) -> u64 {
    let epoch = sim.model.get("epoch");
    sim.model.set_all(&[
        ("host_valid", 1),
        ("host_operation", operation),
        ("host_bank", bank),
        ("host_epoch", epoch),
        ("host_word", u64::from(word)),
        ("host_sequence", sequence),
        ("host_strobe", 15),
    ]);
    sim.idle(1);
    assert_eq!(sim.model.get("host_response_valid"), 1);
    let code = sim.model.get("host_receipt");
    sim.model
        .set_all(&[("host_valid", 0), ("boundary_valid", 0)]);
    sim.idle(1);
    code
}
fn words(
    sim: &Sim<'_>,
    sequence: u64,
    revision: u64,
    deadline: Option<u64>,
    records: &[[u32; 12]],
) -> Vec<u32> {
    let mut words = vec![0; 16];
    words[0] = 0x53434631;
    words[1] = 16 | ((records.len() as u32) << 16);
    words[2] = sim.model.get("epoch") as u32;
    words[3] = u32::from(deadline.is_some());
    words[4] = sequence as u32;
    words[5] = (sequence >> 32) as u32;
    words[6] = revision as u32;
    words[7] = (revision >> 32) as u32;
    words[8] = revision as u32;
    words[9] = (revision >> 32) as u32;
    words[10] = deadline.unwrap_or(0) as u32;
    words[11] = (deadline.unwrap_or(0) >> 32) as u32;
    for record in records {
        words.extend(record);
    }
    words
}
fn fill(sim: &mut Sim<'_>, bank: u64, words: &[u32]) {
    assert_eq!(host(sim, 0, bank, 0, 0), 0);
    for &word in words {
        assert_eq!(host(sim, 1, bank, word, 0), 0);
    }
}
fn publish(
    sim: &mut Sim<'_>,
    bank: u64,
    sequence: u64,
    revision: u64,
    deadline: Option<u64>,
    records: &[[u32; 12]],
) {
    let packet = words(sim, sequence, revision, deadline, records);
    fill(sim, bank, &packet);
    assert_eq!(host(sim, 2, bank, 0, 0), 0);
}
fn start(sim: &mut Sim<'_>, frame: u64) {
    assert_eq!(sim.model.get("boundary_ready"), 1);
    sim.model
        .set_all(&[("boundary_valid", 1), ("boundary_frame", frame)]);
    sim.idle(1);
    sim.model.set("boundary_valid", 0);
}
fn quantum(sim: &mut Sim<'_>, frame: u64) -> u64 {
    let before = sim.model.get("renders");
    start(sim, frame);
    let mut elapsed = 0;
    while sim.model.get("renders") == before && elapsed < 4300 && sim.model.get("faulted") == 0 {
        sim.idle(1);
        elapsed += 1;
    }
    assert_eq!(
        sim.model.get("faulted"),
        0,
        "consumer fault at command cycle {}",
        sim.model.get("current_command_cycles")
    );
    assert_eq!(sim.model.get("renders"), before + 1, "boundary watchdog");
    sim.idle(2);
    elapsed
}
fn result(sim: &mut Sim<'_>, bank: u64, count: usize) -> Vec<u32> {
    let mut values = Vec::new();
    for word in 0..16 + count {
        sim.model.set_all(&[
            ("result_read_valid", 1),
            ("result_read_bank", bank),
            ("result_read_word", word as u64),
        ]);
        sim.idle(1);
        assert_eq!(sim.model.get("result_response_valid"), 1);
        assert_eq!(sim.model.get("result_response_error"), 0);
        values.push(sim.model.get("result_read_data") as u32);
    }
    sim.model.set("result_read_valid", 0);
    sim.idle(1);
    assert_eq!(values[0], 0x53465231);
    assert_eq!(values[1], 16 | ((count as u32) << 16));
    values
}
fn cfg(sim: &mut Sim<'_>, state: bool, address: u16, write: Option<&Bits>) -> Bits {
    sim.model.set_all(&[
        ("cfg_valid", 1),
        ("cfg_state", u64::from(state)),
        ("cfg_address", u64::from(address)),
        ("cfg_write", u64::from(write.is_some())),
    ]);
    if let Some(data) = write {
        sim.model.set_bits("cfg_write_data", data);
    }
    sim.idle(1);
    assert_eq!(sim.model.get("cfg_response_valid"), 1);
    let value = sim.model.bits("cfg_read_data");
    sim.model.set("cfg_valid", 0);
    value
}
fn store(sim: &mut Sim<'_>, slot: u16, row: [u64; 16]) {
    for (index, word) in row.into_iter().enumerate() {
        let mut value = Bits::zero(129);
        value.set(0, 64, word);
        cfg(sim, false, slot * 16 + index as u16, Some(&value));
    }
}
fn load(sim: &mut Sim<'_>, slot: u16) -> [u64; 16] {
    std::array::from_fn(|index| cfg(sim, false, slot * 16 + index as u16, None).get(0, 64))
}
fn row(class: u64, mode: u64) -> [u64; 16] {
    let mut row = [0; 16];
    row[0] = 1 | (u64::from(GENERATION) << 1) | (mode << 33);
    row[1] = (u64::from(class == 1) << 45) | (u64::from(class == 2) << 44);
    row[2] = 64;
    row[3] = 256;
    row[4] = 128;
    row[5] = 9;
    row[6] = if class == 1 { 32 } else { 11 };
    row[7] = 13;
    row
}
fn id() -> LoopSlotId {
    LoopSlotId(SlotId::new(u32::from(SLOT), GENERATION))
}
fn encode(command: Command, self_sync: bool) -> [u32; 12] {
    let mut record = [0; 12];
    record[1] = u32::from(SLOT);
    record[2] = GENERATION;
    record[3] = u32::from(self_sync);
    record[0] = match command {
        Command::Transition(_, plan) => {
            record[6] = plan.now.map_or(u32::MAX, |m| m as u32);
            record[7] = plan.first.map_or(u32::MAX, |p| p.mode as u32);
            record[8] = plan.first.map_or(0, |p| p.skip);
            record[9] = plan.second.map_or(u32::MAX, |p| p.mode as u32);
            record[10] = plan.second.map_or(0, |p| p.skip);
            0
        }
        Command::ClearPlannedTransitions(_, mask) => {
            record[6] = mask as u32;
            1
        }
        Command::LoopRepeatSync(_, repeat) => {
            record[6] = u32::from(repeat);
            2
        }
        Command::LoopGain(_, gain) => {
            record[1] |= u32::from(GAIN) << 16;
            record[5] = gain.to_bits();
            record[6] = Fixed::from_f32(gain).0 as u32;
            3
        }
        Command::LoopIteration(_, request) => {
            record[6] = request.iteration as u32;
            record[7] = (request.iteration >> 32) as u32;
            record[8] = u32::from(request.retrigger) | (u32::from(request.seek) << 1);
            4
        }
        _ => {
            record = [0; 12];
            record[4] = Code::Unsupported as u32;
            6
        }
    };
    record
}
fn oracle(mut row: [u64; 16], command: Command, self_sync: bool, frame: u64) -> (Code, [u64; 16]) {
    let original = row;
    let state = loop_state(
        Identity {
            slot: u32::from(SLOT),
            generation: GENERATION,
        },
        &row,
        None,
    );
    let source = (!self_sync).then_some(LoopId {
        slot: 0,
        generation: GENERATION,
    });
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
fn switch(revision: u64, region: u32, composites: bool) -> [u32; 12] {
    let mut record = [0; 12];
    record[0] = 5;
    record[6] = revision as u32;
    record[7] = (revision >> 32) as u32;
    record[8] = region | (2 << 16);
    record[9] = 1 | (1 << 16);
    record[10] = 1 | (u32::from(composites) << 16);
    record
}
fn handoff(sim: &mut Sim<'_>, epoch: u64, sequence: u64) {
    sim.model.set_all(&[
        ("handoff_valid", 1),
        ("handoff_epoch", epoch),
        ("handoff_sequence", sequence),
        ("handoff_error", 0),
    ]);
    sim.idle(1);
    sim.model.set("handoff_valid", 0);
    sim.idle(4);
}

#[shoop_test]
fn ordered_mixed_repeated_targets_match_the_semantic_oracle() {
    run(|sim| {
        let mut sequence = 1;
        for class in 0..3 {
            for current in 0..3 {
                for self_sync in [false, true] {
                    for skip in [0, 1] {
                        let frame = 1000 + sequence * 32;
                        let commands = [
                            Command::Transition(u32::from(SLOT), TransitionMode::Recording.into()),
                            Command::Transition(
                                u32::from(SLOT),
                                TransitionPlan {
                                    now: None,
                                    first: Some(PlannedTransition {
                                        mode: TransitionMode::Playing,
                                        skip,
                                    }),
                                    second: Some(PlannedTransition {
                                        mode: TransitionMode::Stopped,
                                        skip: skip + 1,
                                    }),
                                },
                            ),
                            Command::LoopRepeatSync(id(), true),
                            Command::ClearPlannedTransitions(u32::from(SLOT), ClearPlanned::Both),
                            Command::LoopGain(u32::from(SLOT), 1.000001),
                            Command::LoopIteration(
                                id(),
                                IterationTransition {
                                    iteration: 2,
                                    retrigger: true,
                                    seek: false,
                                },
                            ),
                            Command::Transition(u32::from(SLOT), TransitionMode::Stopped.into()),
                        ];
                        let mut expected = row(class, current);
                        store(sim, SLOT, expected);
                        let mut codes = Vec::new();
                        for command in commands {
                            let (code, next) = oracle(expected, command, self_sync, frame);
                            codes.push(code as u32);
                            expected = next;
                        }
                        let records: Vec<_> =
                            commands.into_iter().map(|c| encode(c, self_sync)).collect();
                        publish(sim, 0, sequence, 0, None, &records);
                        quantum(sim, frame);
                        let reply = result(sim, 0, commands.len());
                        assert_eq!(
                            &reply[16..],
                            codes,
                            "class={class}, mode={current}, self={self_sync}, skip={skip}"
                        );
                        assert_eq!(reply[6], frame as u32);
                        assert_eq!(reply[12], Code::Applied as u32);
                        assert_eq!(load(sim, SLOT), expected);
                        assert_eq!(cfg(sim, true, GAIN, None), gain_value(1.000001));
                        assert_eq!(
                            sim.model.get("last_gain_bits"),
                            u64::from(1.000001f32.to_bits())
                        );
                        assert_eq!(host(sim, 3, 0, 0, sequence), 0);
                        sequence += 1;
                    }
                }
            }
        }
        assert_eq!(sequence, 37);
    });
}

#[shoop_test]
fn sixteen_and_thirty_two_target_groups_include_decode_and_turnover_in_bounds() {
    run(|sim| {
        let mut sequence = 1;
        let mut reports = Vec::new();
        for targets in [16u16, 32] {
            for skip in [0, 1] {
                let mut records = Vec::new();
                let command = Command::Transition(
                    u32::from(SLOT),
                    TransitionPlan {
                        now: None,
                        first: Some(PlannedTransition {
                            mode: TransitionMode::Recording,
                            skip,
                        }),
                        second: None,
                    },
                );
                let (code, expected) = oracle(row(0, 0), command, false, 500);
                for slot in 1..=targets {
                    store(sim, slot, row(0, 0));
                    let mut record = encode(command, false);
                    record[1] = u32::from(slot);
                    records.push(record);
                }
                publish(sim, 0, sequence, 0, None, &records);
                let elapsed = quantum(sim, 500);
                assert_eq!(
                    sim.model.get("peak_command_cycles"),
                    elapsed.max(sim.model.get("peak_command_cycles"))
                );
                let reply = result(sim, 0, targets as usize);
                assert_eq!(&reply[16..], vec![code as u32; targets as usize]);
                for slot in 1..=targets {
                    assert_eq!(load(sim, slot), expected);
                }
                assert!(elapsed <= 4096);
                reports.push(serde_json::json!({"targets":targets,"skip":skip,"boundary_to_render_cycles":elapsed}));
                assert_eq!(host(sim, 3, 0, 0, sequence), 0);
                sequence += 1;
            }
        }
        let mut records = Vec::new();
        for slot in 1..=32 {
            let mut row = row(1, 1);
            row[3] = u64::MAX;
            row[6] = u64::from(u32::MAX);
            store(sim, slot, row);
            let mut record = encode(
                Command::LoopIteration(
                    id(),
                    IterationTransition {
                        iteration: u64::from(slot),
                        retrigger: true,
                        seek: true,
                    },
                ),
                false,
            );
            record[1] = u32::from(slot);
            records.push(record);
        }
        publish(sim, 0, sequence, 0, None, &records);
        let elapsed = quantum(sim, 800);
        assert_eq!(&result(sim, 0, 32)[16..], vec![Code::Applied as u32; 32]);
        assert_eq!(sim.model.get("peak_command_cycles"), elapsed);
        assert!(elapsed <= 4096);
        reports.push(serde_json::json!({"targets":32,"kind":"iteration_seek","boundary_to_render_cycles":elapsed}));
        let report = serde_json::json!({"scope":"bank/consumer/kernel with one-cycle local RAM; no SP/JP, AXI or DDR integration", "groups":reports});
        println!("{report}");
        std::fs::write(
            rt_fpga_rtl::root().join("work/cmd-fifo-investigation/production-group-cycles.json"),
            serde_json::to_string_pretty(&report).unwrap(),
        )
        .unwrap();
    });
}

#[shoop_test]
fn cutoff_deadline_header_priority_and_full_results_do_not_drive_processing_time() {
    run(|sim| {
        store(sim, SLOT, row(0, 0));
        let repeat = encode(Command::LoopRepeatSync(id(), true), false);
        let packet = words(sim, 1, 0, Some(120), &[repeat]);
        fill(sim, 0, &packet);
        sim.model
            .set_all(&[("boundary_valid", 1), ("boundary_frame", 100)]);
        assert_eq!(host(sim, 2, 0, 0, 0), 0);
        sim.idle(3);
        assert_eq!(sim.model.get("renders"), 1);
        assert_eq!(sim.model.get("groups_attempted"), 0);
        publish(sim, 1, 2, 9, Some(1), &[switch(1, 0, false), repeat]);
        publish(sim, 2, 3, 0, None, &[repeat, switch(1, 0, false), repeat]);
        publish(sim, 3, 4, 0, None, &[repeat]);
        for frame in [132, 164, 196, 228] {
            quantum(sim, frame);
        }
        assert_eq!(sim.model.get("free_mask"), 0);
        assert_eq!(sim.model.get("renders"), 5);
        assert_eq!(sim.model.get("groups_attempted"), 4);
        for _ in 0..10 {
            assert_eq!(host(sim, 0, 0, 0, 0), 1);
        }
        assert_eq!(sim.model.get("renders"), 5);
        quantum(sim, 260);
        assert_eq!(sim.model.get("renders"), 6);
        assert_eq!(load(sim, SLOT)[0] & (1 << 35), 1 << 35);
        for (bank, sequence, codes) in [
            (0, 1, vec![2]),
            (1, 2, vec![3, 3]),
            (2, 3, vec![7, 7, 7]),
            (3, 4, vec![1]),
        ] {
            let reply = result(sim, bank, codes.len());
            assert_eq!(&reply[16..], codes);
            assert_eq!(reply[12], codes[0]);
            assert_eq!(host(sim, 3, bank, 0, sequence), 0);
        }
    });
}

#[shoop_test]
fn selected_switches_wait_for_fences_without_stopping_ordinary_groups_or_retrying() {
    run(|sim| {
        store(sim, SLOT, row(0, 0));
        publish(sim, 0, 1, 0, None, &[switch(1, 0, false)]);
        publish(
            sim,
            1,
            2,
            1,
            None,
            &[encode(Command::LoopRepeatSync(id(), true), false)],
        );
        quantum(sim, 0);
        assert_eq!(sim.model.get("active_revision"), 1);
        assert_eq!(sim.model.get("result_ready_mask"), 0);
        quantum(sim, 32);
        assert_eq!(sim.model.get("renders"), 2);
        assert_eq!(sim.model.get("result_ready_mask"), 0);
        publish(sim, 2, 3, 1, None, &[switch(2, 1, false)]);
        quantum(sim, 64);
        assert_eq!(sim.model.get("active_revision"), 1);
        handoff(sim, 1, 99);
        assert_eq!(sim.model.get("result_ready_mask"), 0);
        handoff(sim, 1, 1);
        let reply = result(sim, 0, 1);
        assert_eq!(reply[15], 2);
        assert_eq!(reply[10], 1);
        assert_eq!(reply[13], 0);
        assert_eq!(host(sim, 3, 0, 0, 1), 0);
        assert_eq!(&result(sim, 1, 1)[16..], &[1]);
        assert_eq!(host(sim, 3, 1, 0, 2), 0);
        assert_eq!(&result(sim, 2, 1)[16..], &[7]);
        assert_eq!(host(sim, 3, 2, 0, 3), 0);
        publish(sim, 0, 4, 1, None, &[switch(2, 1, true)]);
        quantum(sim, 96);
        assert_eq!(&result(sim, 0, 1)[16..], &[23]);
        assert_eq!(sim.model.get("active_revision"), 1);
        reset(sim, 2);
        publish(sim, 0, 1, 0, None, &[switch(1, 0, true)]);
        quantum(sim, 128);
        handoff(sim, 1, 1);
        assert_eq!(sim.model.get("result_ready_mask"), 0);
        handoff(sim, 2, 1);
        assert_eq!(result(sim, 0, 1)[13], 128);
        assert_eq!(host(sim, 3, 0, 0, 1), 0);
        publish(sim, 0, 2, 1, None, &[switch(2, 1, false)]);
        quantum(sim, 160);
        assert_eq!(&result(sim, 0, 1)[16..], &[23]);
        assert_eq!(sim.model.get("active_composites"), 1);
    });
}

#[shoop_test]
fn value_validation_precedes_target_lookup_and_gain_metadata_preserves_original_float() {
    run(|sim| {
        store(sim, SLOT, row(0, 0));
        let good = encode(Command::LoopGain(u32::from(SLOT), 1234.125), false);
        let mut nan = good;
        nan[1] = 128 | (512 << 16);
        nan[5] = f32::NAN.to_bits();
        let mut missing = good;
        missing[1] = 128 | (512 << 16);
        let mut unknown = [0; 12];
        unknown[0] = u32::MAX;
        let mut bad_refusal = [0; 12];
        bad_refusal[0] = 6;
        bad_refusal[4] = 2;
        let mut refused = [0; 12];
        refused[0] = 6;
        refused[4] = 23;
        publish(
            sim,
            0,
            1,
            0,
            None,
            &[good, nan, missing, unknown, bad_refusal, refused],
        );
        quantum(sim, 100);
        assert_eq!(&result(sim, 0, 6)[16..], &[1, 5, 4, 6, 5, 23]);
        assert_eq!(sim.model.get("gain_updates"), 1);
        assert_eq!(
            sim.model.get("last_gain_bits"),
            u64::from(1234.125f32.to_bits())
        );
        assert_eq!(sim.model.get("last_gain_slot"), u64::from(SLOT));
        assert_eq!(cfg(sim, true, GAIN, None), gain_value(1234.125));
    });
}

#[shoop_test]
fn local_read_and_uncertain_write_errors_are_faults_not_semantic_refusals() {
    for write in [false, true] {
        run(|sim| {
            store(sim, SLOT, row(0, 0));
            publish(
                sim,
                0,
                1,
                0,
                None,
                &[encode(
                    Command::Transition(u32::from(SLOT), TransitionMode::Recording.into()),
                    false,
                )],
            );
            sim.model.set(
                if write {
                    "inject_write_error"
                } else {
                    "inject_read_error"
                },
                1,
            );
            start(sim, 0);
            for _ in 0..200 {
                if sim.model.get("faulted") != 0 {
                    break;
                }
                sim.idle(1);
            }
            assert_eq!(sim.model.get("faulted"), 1);
            sim.idle(10);
            assert_eq!(sim.model.get("renders"), 0);
            assert_eq!(sim.model.get("result_ready_mask"), 0);
            assert_eq!(sim.model.get("boundary_ready"), 0);
            sim.model.set("inspect_bank", 0);
            sim.idle(1);
            assert_eq!(sim.model.get("bank_state"), 6);
            let writes = sim.model.get("memory_writes");
            assert_eq!(writes, u64::from(write));
            sim.idle(20);
            assert_eq!(sim.model.get("memory_writes"), writes);
            assert_eq!(host(sim, 3, 0, 0, 1), 3);
        });
    }
    run(|sim| {
        publish(sim, 0, 1, 0, None, &[switch(1, 0, false)]);
        start(sim, 0);
        for _ in 0..200 {
            if sim.model.get("select_valid") != 0 {
                break;
            }
            sim.idle(1);
        }
        assert_eq!(sim.model.get("select_valid"), 1);
        sim.model.set_all(&[
            ("handoff_valid", 1),
            ("handoff_epoch", 1),
            ("handoff_sequence", 1),
            ("handoff_error", 1),
        ]);
        sim.idle(1);
        sim.model.set("handoff_valid", 0);
        sim.idle(10);
        assert_eq!(sim.model.get("faulted"), 1);
        assert_eq!(sim.model.get("renders"), 0);
        assert_eq!(sim.model.get("result_ready_mask"), 0);
        assert_eq!(sim.model.get("boundary_ready"), 0);
    });
}

#[shoop_test]
fn every_partial_packet_word_and_unread_result_remain_independent_of_boundaries() {
    run(|sim| {
        store(sim, SLOT, row(0, 0));
        let repeat = encode(Command::LoopRepeatSync(id(), true), false);
        let packet = words(sim, 1, 0, None, &[repeat; 32]);
        assert_eq!(host(sim, 0, 0, 0, 0), 0);
        for (index, word) in packet.into_iter().enumerate() {
            assert_eq!(host(sim, 1, 0, word, 0), 0);
            assert_eq!(sim.model.get("renders"), index as u64);
            sim.idle(7);
            quantum(sim, index as u64 * 32);
            assert_eq!(sim.model.get("groups_attempted"), 0);
            assert_eq!(sim.model.get("memory_writes"), 0);
        }
        assert_eq!(sim.model.get("renders"), 400);
        assert_eq!(host(sim, 2, 0, 0, 0), 0);
        quantum(sim, 12800);
        assert_eq!(sim.model.get("groups_attempted"), 1);
        for frame in [12832, 12864, 12896, 12928] {
            quantum(sim, frame);
        }
        assert_eq!(sim.model.get("renders"), 405);
        assert_eq!(&result(sim, 0, 32)[16..], vec![1; 32]);
        assert_eq!(load(sim, SLOT)[0] & (1 << 35), 1 << 35);
    });
}

#[shoop_test]
fn qualified_faults_preempt_finalizer_and_main_writes_but_stale_faults_do_not() {
    for (epoch, sequence, fails, pipeline_fault) in [
        (2, 1, false, false),
        (1, 99, false, false),
        (1, 1, true, false),
        (1, 1, true, true),
    ] {
        run(|sim| {
            store(sim, SLOT, row(0, 0));
            publish(sim, 0, 1, 0, None, &[switch(1, 0, false)]);
            quantum(sim, 0);
            publish(
                sim,
                1,
                2,
                1,
                None,
                &[encode(Command::LoopRepeatSync(id(), true), false)],
            );
            start(sim, 32);
            for _ in 0..200 {
                if sim.model.get("main_result_tail") != 0 {
                    break;
                }
                sim.idle(1);
            }
            assert_eq!(sim.model.get("main_result_tail"), 1);
            sim.model.set_all(&[
                ("handoff_valid", 1),
                ("handoff_epoch", 1),
                ("handoff_sequence", 1),
                ("handoff_error", 0),
            ]);
            sim.idle(1);
            assert_eq!(sim.model.get("final_result_selected"), 1);
            assert_eq!(sim.model.get("main_result_tail"), 1);
            assert_eq!(sim.model.get("result_ready_mask"), 0);
            let writes = sim.model.get("memory_writes");
            sim.model.set_all(&[
                ("handoff_epoch", epoch),
                ("handoff_sequence", sequence),
                ("handoff_error", u64::from(!pipeline_fault)),
                ("pipeline_fault", u64::from(pipeline_fault)),
            ]);
            sim.model.eval();
            assert_eq!(sim.model.get("fault_event"), u64::from(fails));
            if fails {
                assert_eq!(sim.model.get("fault_sequence"), 1);
                assert_eq!(
                    sim.model.get("fault_frame"),
                    0,
                    "qualified fence owns F0, not competing F32"
                );
                assert_eq!(sim.model.get("fault_opcode"), 5);
            }
            sim.idle(1);
            sim.model.set_all(&[
                ("handoff_valid", 0),
                ("handoff_error", 0),
                ("pipeline_fault", 0),
            ]);
            sim.idle(100);
            assert_eq!(sim.model.get("faulted"), u64::from(fails));
            assert_eq!(sim.model.get("memory_writes"), writes);
            if fails {
                assert_eq!(sim.model.get("renders"), 1);
                assert_eq!(sim.model.get("result_ready_mask"), 0);
                for bank in [0, 1] {
                    sim.model.set("inspect_bank", bank);
                    sim.idle(1);
                    assert_eq!(
                        sim.model.get("bank_state"),
                        if pipeline_fault && bank == 0 { 4 } else { 6 }
                    );
                    assert_eq!(host(sim, 3, bank, 0, bank + 1), 3);
                }
            } else {
                assert_eq!(sim.model.get("renders"), 2);
                assert_eq!(result(sim, 0, 1)[15], 2);
                assert_eq!(host(sim, 3, 0, 0, 1), 0);
                assert_eq!(&result(sim, 1, 1)[16..], &[1]);
            }
        });
    }
    run(|sim| {
        store(sim, SLOT, row(0, 0));
        publish(
            sim,
            0,
            1,
            0,
            None,
            &[encode(Command::LoopRepeatSync(id(), true), false)],
        );
        quantum(sim, 0);
        assert_eq!(host(sim, 3, 0, 0, 1), 0);
        publish(
            sim,
            1,
            2,
            1,
            None,
            &[encode(Command::LoopRepeatSync(id(), false), false)],
        );
        start(sim, 32);
        for _ in 0..200 {
            if sim.model.get("fault_sequence") == 2 {
                assert_eq!(
                    sim.model.get("fault_opcode"),
                    u64::from(u32::MAX),
                    "header refusal cannot inherit the previous packet's opcode"
                );
            }
            if sim.model.get("renders") == 2 {
                break;
            }
            sim.idle(1);
        }
        assert_eq!(result(sim, 1, 1)[12], 3);
    });
}

#[shoop_test]
fn fence_completion_arbitrates_with_a_maximal_group_without_early_result_visibility() {
    run(|sim| {
        let mut records = Vec::new();
        for slot in 1..=32 {
            let mut state = row(1, 1);
            state[3] = u64::MAX;
            state[6] = u64::from(u32::MAX);
            store(sim, slot, state);
            let mut record = encode(
                Command::LoopIteration(
                    id(),
                    IterationTransition {
                        iteration: u64::from(slot),
                        retrigger: true,
                        seek: true,
                    },
                ),
                false,
            );
            record[1] = u32::from(slot);
            records.push(record);
        }
        publish(sim, 0, 1, 0, None, &[switch(1, 0, true)]);
        quantum(sim, 0);
        assert_eq!(sim.model.get("result_ready_mask"), 0);
        publish(sim, 1, 2, 1, None, &records);
        let before = sim.model.get("renders");
        start(sim, 32);
        let mut elapsed = 0;
        while sim.model.get("main_result_tail") == 0 && elapsed < 4096 {
            sim.idle(1);
            elapsed += 1;
        }
        assert_eq!(sim.model.get("main_result_tail"), 1);
        assert_eq!(sim.model.get("result_ready_mask"), 0);
        sim.model.set_all(&[
            ("handoff_valid", 1),
            ("handoff_epoch", 1),
            ("handoff_sequence", 1),
        ]);
        sim.idle(1);
        elapsed += 1;
        sim.model.set("handoff_valid", 0);
        while sim.model.get("renders") == before && elapsed < 4096 {
            sim.idle(1);
            elapsed += 1;
        }
        assert_eq!(sim.model.get("faulted"), 0);
        assert_eq!(sim.model.get("renders"), before + 1);
        sim.idle(2);
        assert_eq!(elapsed, 3783);
        assert_eq!(sim.model.get("peak_command_cycles"), elapsed);
        let first = result(sim, 0, 1);
        assert_eq!(first[6], 0);
        assert_eq!(first[8], 0);
        assert_eq!(first[10], 1);
        assert_eq!(first[13], 0);
        assert_eq!(first[15], 2);
        assert_eq!(host(sim, 3, 0, 0, 1), 0);
        let second = result(sim, 1, 32);
        assert_eq!(second[6], 32);
        assert_eq!(&second[16..], vec![1; 32]);
        let report = serde_json::json!({
            "scope": "one-cycle local RAM; prior synthetic fence completes during maximal group result emission",
            "targets": 32, "kind": "iteration_seek", "boundary_to_render_cycles": elapsed,
        });
        println!("{report}");
        std::fs::write(
            rt_fpga_rtl::root()
                .join("work/cmd-fifo-investigation/production-fence-race-cycles.json"),
            serde_json::to_string_pretty(&report).unwrap(),
        )
        .unwrap();
    });
}
