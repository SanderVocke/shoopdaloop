#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use rt_fpga_reference::records::{Identity, LoopRecord, Mode};
use rt_fpga_testbench::{
    bits::Bits,
    clock::Edges,
    drivers::{audio::Audio, state::State},
    model::Model,
    random::Random,
    reference,
    sim::{Agent, Sim},
    suites,
};
use serde_json::json;
use shoop_wasm_test_support::shoop_test;
use std::{cell::RefCell, rc::Rc};

#[derive(Default)]
struct Records {
    values: Vec<LoopRecord>,
    pending: Option<(u32, u32)>,
}
impl Agent for Records {
    fn drive(&mut self, model: &mut Model<'_>, edge: Edges) {
        if !edge.falls(0) {
            return;
        }
        let records = &*self;
        let mut response = Bits::zero(164);
        if let Some((slot, _)) = records.pending {
            if let Some(record) = records.values.iter().find(|r| r.id.slot == slot) {
                let mode = match record.mode {
                    Mode::Stopped => 0,
                    Mode::Playing => 1,
                    Mode::Recording => 2,
                    Mode::Invalid => 3,
                };
                response
                    .set(0, 1, 1)
                    .set(1, 32, u64::from(record.id.generation))
                    .set(33, 2, mode)
                    .set(35, 64, record.content_length)
                    .set(99, 64, record.position)
                    .set(163, 1, u64::from(record.begin_capture));
            }
        }
        model.set("loop_read_ready", 1);
        model.set(
            "loop_response_valid",
            u64::from(records.pending.is_some_and(|(_, n)| n == 0)),
        );
        model.set("loop_response_error", 0);
        model.set_bits("loop_response_data", &response);
    }
    fn sample(&mut self, model: &Model<'_>, edge: Edges) {
        if !edge.rises(0) {
            return;
        }
        let records = self;
        if model.get("reset") != 0 {
            records.pending = None;
            return;
        }
        if let Some((_, delay)) = &mut records.pending {
            if *delay == 0 {
                records.pending = None;
            } else {
                *delay -= 1;
            }
        }
        if model.get("loop_read_valid") != 0 && model.get("loop_read_ready") != 0 {
            assert!(records.pending.is_none());
            records.pending = Some((model.get("loop_read_slot") as u32, 2));
        }
    }
}
fn records(sim: &mut Sim<'_>) -> Rc<RefCell<Records>> {
    let records = Rc::new(RefCell::new(Records::default()));
    sim.agent(records.clone());
    records
}
fn record(slot: u32, generation: u32, mode: Mode, position: u64, first: bool) -> LoopRecord {
    LoopRecord {
        id: Identity { slot, generation },
        mode,
        position,
        content_length: 64,
        begin_capture: first,
    }
}
fn root() -> std::path::PathBuf {
    rt_fpga_testbench::root().join("work/marlin-suite/reference")
}

#[shoop_test]
fn rust_state_matches_rtl_all_opcodes_partial_failures_and_record_inputs() {
    let depths = [64, 16, 8];
    let design = suites::state::design(depths).build(&root()).unwrap();
    let mut sim = Sim::new(design.session(None).unwrap(), &[("clk", 10000, 0)]);
    let mut driver = State::new(&mut sim, depths, 20261115);
    let bank = records(&mut sim);
    let memory = vec![
        suites::state::word(u32::MAX as u64),
        suites::state::word(1),
        suites::state::word(2),
        suites::state::word(1),
        suites::state::ramp(33, 2),
        suites::state::word((17 << 32) | (1 << 16) | 9),
    ];
    let instructions = vec![
        json!({"kind":"process_input","port":2,"destination":0}),
        json!({"kind":"process_loop_playback","view":0,"position":1,"length":2,"enabled":3,"destination":1}),
        json!({"kind":"process_gain","ramp":4,"source":1,"destination":2}),
        json!({"kind":"process_logical_playback","loop_id":{"slot":0,"generation":7},"view":0,"destination":3}),
        json!({"kind":"process_logical_capture","loop_id":{"slot":1,"generation":9},"view":0,"source":2}),
        json!({"kind":"process_smoothing","loop_id":{"slot":0,"generation":7},"control":5,"source":3}),
        json!({"kind":"process_silence","destination":4}),
        json!({"kind":"process_constant","value":i32::MIN,"destination":5}),
        json!({"kind":"process_arithmetic","left":4,"right":5,"destination":6,"mode":"one_minus"}),
        json!({"kind":"process_mix","left":5,"right":6,"destination":7}),
        json!({"kind":"process_loop_capture","view":0,"enabled":3,"source":7}),
        json!({"kind":"process_output","source":7,"port":0}),
    ];
    for mode in [Mode::Stopped, Mode::Playing, Mode::Recording, Mode::Invalid] {
        for generation in [7, 8] {
            for position in [0, 32, 64, 96] {
                for first in [false, true] {
                    let rows = vec![
                        record(0, generation, mode, position, false),
                        record(1, 9, Mode::Recording, 0, first),
                    ];
                    bank.borrow_mut().values = rows.clone();
                    for budget in [0, 8, 9] {
                        let mut case = suites::state::case(
                            instructions.clone(),
                            memory.clone(),
                            8,
                            Some(budget),
                            32,
                        );
                        case["program"]["chunk_frames"] = json!(32);
                        driver.load(&mut sim, &case);
                        let status = driver.execute(&mut sim, &[]);
                        let jobs = driver.drain(&mut sim);
                        let state = driver.snapshot(&mut sim);
                        assert_eq!(
                            (status, state, jobs),
                            reference::state(&case, &rows),
                            "{case}, {rows:?}"
                        );
                    }
                }
            }
        }
    }
    // Run each instruction without FIFO exhaustion as well.
    let rows = vec![
        record(0, 7, Mode::Playing, 0, false),
        record(1, 9, Mode::Recording, 0, true),
    ];
    bank.borrow_mut().values = rows.clone();
    for instruction in instructions {
        let case = suites::state::case(vec![instruction], memory.clone(), 8, Some(2), 32);
        driver.load(&mut sim, &case);
        let status = driver.execute(&mut sim, &[]);
        let jobs = driver.drain(&mut sim);
        assert_eq!(
            (status, driver.snapshot(&mut sim), jobs),
            reference::state(&case, &rows)
        );
    }
}

#[shoop_test]
fn rust_jobs_match_rtl_samples_buffers_ramps_smoothing_and_failures() {
    let capacities = [32, 16, 32, 32, 16, 2, 2];
    let design = suites::audio::design(capacities).build(&root()).unwrap();
    let mut sim = Sim::new(design.session(None).unwrap(), &[("clk", 10000, 0)]);
    let mut driver = Audio::new(&mut sim, capacities, 20261115);
    let bank = records(&mut sim);
    let mut random = Random::new(20261115);
    for iteration in 0..80 {
        let frames = [1, 7, 32][iteration % 3];
        let duration = [0, 1, 33, u32::MAX][iteration % 4];
        let mode = ["multiply", "left_balance", "right_balance", "one_minus"][iteration % 4];
        let instructions = vec![
            json!({"kind":"process_input","port":0,"destination":0}),
            json!({"kind":"process_gain","ramp":0,"source":0,"destination":1}),
            json!({"kind":"process_constant","value":random.next() as i32,"destination":2}),
            json!({"kind":"process_arithmetic","left":1,"right":2,"destination":3,"mode":mode}),
            json!({"kind":"process_smoothing","loop_id":{"slot":0,"generation":7},"control":1,"source":3}),
            json!({"kind":"process_smoothing","loop_id":{"slot":1,"generation":9},"control":2,"source":3}),
            json!({"kind":"process_mix","left":4,"right":5,"destination":if iteration%11==0 {0}else{6}}),
            json!({"kind":"process_output","source":6,"port":if iteration%13==0 {3}else{0}}),
        ];
        let memory = vec![
            json!({"kind":"ramp","value":{"start":random.next() as i32,"target":random.next() as i32,"duration":duration,"elapsed":0}}),
            suites::state::word(
                (u64::from(duration) << 32) | (if iteration % 19 == 0 { 16 } else { 14 } << 16) | 4,
            ),
            suites::state::word((7 << 32) | (15 << 16) | 5),
        ];
        let mut case = suites::audio::case(
            capacities,
            frames,
            instructions,
            memory,
            8,
            1,
            vec![vec![0; frames as usize]],
        );
        case["history_slots"] = json!(capacities[4]);
        driver.load(&mut sim, &case);
        let mut cpu = reference::Audio::new(&case);
        for quantum in 0..6 {
            let rows = vec![
                record(
                    0,
                    if iteration % 17 == 0 && quantum == 4 {
                        8
                    } else {
                        7
                    },
                    if quantum == 3 {
                        Mode::Stopped
                    } else {
                        Mode::Playing
                    },
                    if quantum % 3 == 0 { 0 } else { 32 },
                    false,
                ),
                record(
                    1,
                    9,
                    Mode::Playing,
                    if quantum % 2 == 0 { 0 } else { 32 },
                    false,
                ),
            ];
            bank.borrow_mut().values = rows.clone();
            let inputs: Vec<i32> = (0..frames).map(|_| random.next() as i32).collect();
            let step = json!({"inputs":[inputs],"writes":[]});
            let expected = cpu.quantum(&step, &rows);
            let actual = driver.quantum(&mut sim, &step, quantum % 2 == 0);
            assert_eq!(
                actual, expected,
                "case {iteration}, quantum {quantum}: {case}"
            );
        }
    }
}

#[shoop_test]
fn rust_state_and_jobs_match_rtl_record_playback_wrap_and_retake() {
    use rt_fpga_reference::{
        fixed::Fixed,
        storage::{MediaStore, ViewId},
    };
    use rt_fpga_testbench::{
        drivers::{pipeline::Pipeline, store::Geometry},
        memory::Mode,
    };
    let capacities = [32, 16, 32, 32, 16, 2, 2];
    let geometry = Geometry::new([16, 32, 4], 1);
    let compiled = suites::pipeline::design(capacities, &geometry, 64, Mode::Legacy, 3)
        .build(&root())
        .unwrap();
    let mut sim = Sim::new(compiled.session(None).unwrap(), &[("clk", 10000, 0)]);
    let mut driver = Pipeline::new(&mut sim, capacities, geometry, Mode::Legacy, 20261115).unwrap();
    for frames in [4, 8, 32] {
        let mut case = suites::pipeline::record_case(&driver, frames, 20261115 + frames);
        let first = case["steps"][0].clone();
        let second = json!({"writes":[],"inputs":[(0..frames).map(|i| i32::MAX-(i as i32)*999).collect::<Vec<_>>() ]});
        let play = |position| {
            json!({"writes":[
            {"address":2,"value":suites::state::word(0)},
            {"address":6,"value":suites::state::word(1)},
            {"address":4,"value":suites::state::word(position)},
            {"address":5,"value":suites::state::word(2*frames/4)}],"inputs":[vec![0;frames as usize]]})
        };
        let retake = case["steps"][2].clone();
        let last = case["steps"][3].clone();
        case["steps"] = json!([
            first,
            second,
            play(frames / 4 - 1),
            play(2 * frames / 4 - 1),
            retake,
            last
        ]);
        // The retake shortens the explicit playback window again.
        case["steps"][4]["writes"].as_array_mut().unwrap().extend([
            json!({"address":4,"value":suites::state::word(0)}),
            json!({"address":5,"value":suites::state::word(frames/4)}),
        ]);
        driver.load(&mut sim, &case);
        let mut cpu = reference::Audio::new(&case);
        let mut memory = MediaStore::with_chunk_limit(4, 32).unwrap();
        assert_eq!(memory.create_recording().unwrap(), ViewId(0));
        for step in case["steps"].as_array().unwrap() {
            if step["media_actions"].is_array() {
                memory.clear(ViewId(0)).unwrap();
            }
            let expected = cpu.quantum_with_memory(step, &[], &mut memory);
            let mut actual = driver.quantum(&mut sim, step, true);
            actual.as_object_mut().unwrap().remove("media");
            assert_eq!(actual, expected, "Q={frames}, {step}");
            let count = memory.frames(ViewId(0)).unwrap();
            if count != 0 {
                let mut samples = vec![Fixed::ZERO; count];
                memory
                    .read_loop(ViewId(0), 0, count as u64, &mut samples)
                    .unwrap();
                let read = driver.store.command(
                    &mut sim,
                    &json!({"kind":"read","view":0,"position":0,"length":count,"frames":count}),
                    true,
                    true,
                    true,
                );
                assert_eq!(read["value"], json!({"kind":"samples","value":samples}));
            }
        }
    }
}

#[shoop_test]
fn rust_jobs_match_media_disabled_rtl_refusals_and_remaining_jobs() {
    let capacities = [32, 16, 32, 32, 16, 2, 2];
    let compiled = suites::audio::design(capacities).build(&root()).unwrap();
    let mut sim = Sim::new(compiled.session(None).unwrap(), &[("clk", 10000, 0)]);
    let mut driver = Audio::new(&mut sim, capacities, 20261115);
    let bank = records(&mut sim);
    let rows = vec![record(0, 7, Mode::Recording, 0, true)];
    bank.borrow_mut().values = rows.clone();
    for operation in [
        json!({"kind":"process_loop_playback","view":0,"position":2,"length":3,"enabled":1,"destination":1}),
        json!({"kind":"process_loop_capture","view":0,"enabled":1,"source":0}),
        json!({"kind":"process_logical_capture","loop_id":{"slot":0,"generation":7},"view":0,"source":0}),
    ] {
        let instructions = vec![
            json!({"kind":"process_input","port":0,"destination":0}),
            operation,
            json!({"kind":"process_output","source":0,"port":0}),
        ];
        let case = suites::audio::case(
            capacities,
            32,
            instructions,
            vec![
                suites::state::word(0),
                suites::state::word(1),
                suites::state::word(0),
                suites::state::word(1),
            ],
            2,
            1,
            vec![vec![99; 32]],
        );
        driver.load(&mut sim, &case);
        let mut cpu = reference::Audio::new(&case);
        let step = &case["steps"][0];
        let expected = cpu.quantum(step, &rows);
        assert_eq!(expected["status"]["value"]["error"], "unsupported_job");
        assert_eq!(driver.quantum(&mut sim, step, true), expected);
    }
}
