#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use rt_fpga_testbench::{
    bits::Bits,
    codec,
    drivers::store::Geometry,
    memory::{Memory, Mode},
    model::Design,
    sim::Sim,
    suites,
};
use serde_json::json;
use shoop_wasm_test_support::shoop_test;

fn host(s: &mut Sim<'_>, operation: u64, bank: u64, word: u32, sequence: u64) -> u64 {
    s.model.set_all(&[
        ("host_valid", 1),
        ("host_operation", operation),
        ("host_bank", bank),
        ("host_word", u64::from(word)),
        ("host_epoch", 1),
        ("host_strobe", 15),
        ("host_sequence", sequence),
    ]);
    s.idle(1);
    assert_eq!(s.model.get("host_response_valid"), 1);
    let receipt = s.model.get("host_receipt");
    s.model.set("host_valid", 0);
    s.idle(1);
    receipt
}
fn publish(s: &mut Sim<'_>, bank: u64, sequence: u32, revision: u32, commands: &[[u32; 12]]) {
    let mut words = vec![0; 16];
    words[0] = 0x53434631;
    words[1] = 16 | ((commands.len() as u32) << 16);
    words[2] = 1;
    words[4] = sequence;
    words[6] = revision;
    words[8] = revision;
    for command in commands {
        words.extend(command);
    }
    assert_eq!(host(s, 0, bank, 0, 0), 0);
    for word in words {
        assert_eq!(host(s, 1, bank, word, 0), 0);
    }
    assert_eq!(host(s, 2, bank, 0, 0), 0);
}
fn cfg(s: &mut Sim<'_>, state: bool, loops: bool, address: u64, value: Option<&Bits>) -> Bits {
    s.model.set_all(&[
        ("cfg_valid", 1),
        ("cfg_state", u64::from(state)),
        ("cfg_loop", u64::from(loops)),
        ("cfg_write", u64::from(value.is_some())),
        ("cfg_address", address),
    ]);
    if let Some(value) = value {
        s.model.set_bits("cfg_write_data", &value.resized(129));
    }
    s.model.eval();
    assert_eq!(s.model.get("cfg_ready"), 1);
    s.idle(1);
    assert_eq!(s.model.get("cfg_response_valid"), 1);
    assert_eq!(s.model.get("cfg_response_error"), 0);
    let value = s.model.bits("cfg_read_data");
    s.model.set("cfg_valid", 0);
    s.idle(1);
    value
}
fn quantum(s: &mut Sim<'_>, frame: u64, sample: u64, gains: &mut Vec<(u64, u64)>) -> u64 {
    quantum_samples(s, frame, &[sample; 32], gains)
}
fn quantum_samples(
    s: &mut Sim<'_>,
    frame: u64,
    samples: &[u64; 32],
    gains: &mut Vec<(u64, u64)>,
) -> u64 {
    s.model.eval();
    assert_eq!(s.model.get("run_ready"), 1);
    assert_eq!(s.model.get("sync_frame"), frame);
    s.model.set("run_valid", 1);
    s.idle(1);
    s.model.set("run_valid", 0);
    let mut output = [None; 32];
    let mut cycles = 1;
    while s.model.get("done") == 0 && s.model.get("faulted") == 0 && cycles < 60_000 {
        s.model.eval();
        if s.model.get("sample_output_write") != 0 {
            let address = s.model.get("sample_output_address") as usize;
            assert!(address < output.len());
            output[address] = Some(s.model.get("sample_output_data"));
        }
        if s.model.get("gain_update_valid") != 0 {
            gains.push((
                s.model.get("gain_update_slot"),
                s.model.get("gain_update_bits"),
            ));
        }
        s.idle(1);
        cycles += 1;
    }
    assert_eq!(s.model.get("faulted"), 0);
    assert_eq!(s.model.get("done"), 1);
    assert_eq!(s.model.get("error_code"), 0);
    assert_eq!(s.model.get("sync_frame"), frame + 32);
    assert_eq!(output, samples.map(Some));
    assert!(s.model.get("peak_command_cycles") <= 4096);
    s.idle(2);
    cycles
}
fn result(s: &mut Sim<'_>, bank: u64, count: usize) -> Vec<u32> {
    let mut words = vec![];
    for word in 0..16 + count {
        s.model.set_all(&[
            ("result_read_valid", 1),
            ("result_read_bank", bank),
            ("result_read_word", word as u64),
        ]);
        s.idle(1);
        assert_eq!(s.model.get("result_response_valid"), 1);
        assert_eq!(s.model.get("result_response_error"), 0);
        words.push(s.model.get("result_read_data") as u32);
    }
    s.model.set("result_read_valid", 0);
    s.idle(1);
    assert_eq!(words[0], 0x53465231);
    assert_eq!(words[12], 1);
    words
}

#[shoop_test]
fn production_consumer_edits_execution_ram_and_renders_while_results_and_handoff_wait() {
    let root = rt_fpga_testbench::root();
    let audio = suites::audio::design([64, 16, 128, 32, 4, 1, 1]);
    let mut sources = audio.sources;
    for module in [
        "fifo_group_banks",
        "fifo_edit_kernel",
        "fifo_group_consumer",
        "fifo_audio_pipeline",
    ] {
        sources.push(root.join(format!("fpga/host_bridge/rtl/{module}.sv")));
    }
    let mut design = Design::new("fifo_audio_pipeline", sources);
    design.parameters = audio.parameters;
    design = design.parameters(&[("LOOP_DEPTH", 32), ("EXTERNAL_IO", 1)]);
    let compiled = design
        .build(&root.join("work/marlin-suite/semantic-audio"))
        .unwrap();
    let mut s = Sim::new(compiled.session(None).unwrap(), &[("clk", 10_000, 0)]);
    s.model.set_all(&[
        ("reset", 1),
        ("epoch", 1),
        ("pipeline_fault", 0),
        ("host_valid", 0),
        ("result_read_valid", 0),
        ("run_valid", 0),
        ("cfg_valid", 0),
        ("audio_cfg_valid", 0),
        ("handoff_valid", 0),
        ("handoff_error", 0),
        ("handoff_epoch", 1),
        ("handoff_sequence", 1),
        ("run_quantum", 32),
        ("run_chunk_frames", 32),
        ("run_inputs", 1),
        ("run_outputs", 1),
        ("capture_credits", 65535),
        ("job_enable", 1),
        ("sample_input_data", 1 << 22),
        ("drain_ready", 0),
    ]);
    s.idle(2);
    s.model.set("reset", 0);
    s.idle(2);
    for slot in 0..32 {
        for word in 0..16 {
            let value = match word {
                0 => 1 | (7 << 1),
                3 => 256,
                _ => 0,
            };
            let mut bits = Bits::zero(129);
            bits.set(0, 64, value);
            cfg(&mut s, false, true, slot * 16 + word, Some(&bits));
        }
        let mut instruction = Bits::zero(96);
        instruction
            .set(0, 8, 12)
            .set(8, 16, slot)
            .set(24, 32, 7)
            .set(56, 16, 65535)
            .set(72, 16, 1);
        cfg(&mut s, false, false, slot, Some(&instruction));
    }
    for (i, instruction) in [
        json!({"kind":"process_input","port":0,"destination":0}),
        json!({"kind":"process_gain","ramp":0,"source":0,"destination":1}),
        json!({"kind":"process_output","source":1,"port":0}),
    ]
    .iter()
    .enumerate()
    {
        cfg(
            &mut s,
            false,
            false,
            32 + i as u64,
            Some(&codec::instruction(instruction).unwrap()),
        );
    }
    cfg(
        &mut s,
        true,
        false,
        0,
        Some(&codec::state(&suites::audio::ramp(1 << 24, 1 << 24, 0, 0))),
    );
    let mut select = [0; 12];
    select[0] = 5;
    select[6] = 1;
    select[8] = 35 << 16;
    select[9] = 32 | (1 << 16);
    select[10] = 2;
    publish(&mut s, 0, 1, 0, &[select]);
    assert_eq!(
        s.model.get("sync_frame"),
        0,
        "uploads and publication do not pace processing"
    );
    let mut gains = vec![];
    let mut cycles = vec![quantum(&mut s, 0, 1 << 22, &mut gains)];
    assert_eq!(s.model.get("handoff_pending"), 1);
    assert_eq!(s.model.get("result_ready_mask"), 0);
    s.model
        .set_all(&[("handoff_valid", 1), ("handoff_sequence", 99)]);
    s.idle(3);
    assert_eq!(s.model.get("handoff_pending"), 1);
    assert_eq!(s.model.get("result_ready_mask"), 0);
    s.model.set_all(&[
        ("handoff_epoch", 2),
        ("handoff_sequence", 1),
        ("handoff_error", 1),
    ]);
    let mut gain = [0; 12];
    gain[0] = 3;
    gain[2] = 7;
    gain[3] = 1;
    gain[5] = 0.5f32.to_bits();
    gain[6] = 1 << 23;
    publish(&mut s, 1, 2, 1, &[gain]);
    cycles.push(quantum(&mut s, 32, 1 << 21, &mut gains));
    assert_eq!(gains, [(0, u64::from(0.5f32.to_bits()))]);
    assert_eq!(s.model.get("handoff_pending"), 1);
    assert_eq!(s.model.get("result_ready_mask"), 0);
    s.model.set("handoff_valid", 0);
    let arms = (0..32)
        .map(|slot| {
            let mut command = [0; 12];
            command[1] = slot;
            command[2] = 7;
            command[3] = 1;
            command[6] = 2;
            command[7] = u32::MAX;
            command[9] = u32::MAX;
            command
        })
        .collect::<Vec<_>>();
    publish(&mut s, 2, 3, 1, &arms);
    cycles.push(quantum(&mut s, 64, 1 << 21, &mut gains));
    for slot in 0..32 {
        assert_eq!(
            (cfg(&mut s, false, true, slot * 16, None).get(0, 64) >> 33) & 3,
            2
        );
        assert_eq!(cfg(&mut s, false, true, slot * 16 + 4, None).get(0, 64), 32);
        assert_eq!(cfg(&mut s, false, true, slot * 16 + 9, None).get(0, 64), 64);
    }
    cycles.push(quantum(&mut s, 96, 1 << 21, &mut gains));
    let mut refusal = [0; 12];
    refusal[0] = 6;
    refusal[4] = 5;
    publish(&mut s, 3, 4, 1, &[refusal]);
    cycles.push(quantum(&mut s, 128, 1 << 21, &mut gains));
    assert_eq!(s.model.get("free_mask"), 0);
    assert_eq!(s.model.get("result_ready_mask"), 0);
    s.model.set_all(&[
        ("host_valid", 1),
        ("host_operation", 0),
        ("host_bank", 0),
        ("result_read_valid", 1),
        ("result_read_bank", 0),
        ("result_read_word", 0),
    ]);
    cycles.push(quantum(&mut s, 160, 1 << 21, &mut gains));
    assert_eq!(s.model.get("host_receipt"), 1);
    assert_eq!(s.model.get("result_response_error"), 1);
    assert_eq!(s.model.get("handoff_pending"), 1);
    assert_eq!(s.model.get("groups_attempted"), 4);
    s.model.set_all(&[
        ("host_valid", 0),
        ("result_read_valid", 0),
        ("handoff_valid", 1),
        ("handoff_epoch", 1),
        ("handoff_sequence", 1),
        ("handoff_error", 0),
    ]);
    s.until(100, "qualified handoff result", |m| {
        m.get("result_ready_mask") == 1
    });
    let selected = result(&mut s, 0, 1);
    assert_eq!(selected[6], 0);
    assert_eq!(selected[15], 2);
    assert_eq!(selected[16], 1);
    assert_eq!(host(&mut s, 3, 0, 0, 1), 0);
    let gain_result = result(&mut s, 1, 1);
    assert_eq!(gain_result[6], 32);
    assert_eq!(gain_result[16], 1);
    assert_eq!(host(&mut s, 3, 1, 0, 2), 0);
    let arm_result = result(&mut s, 2, 32);
    assert_eq!(arm_result[6], 64);
    assert_eq!(&arm_result[16..], &[1; 32]);
    assert_eq!(host(&mut s, 3, 2, 0, 3), 0);
    assert_eq!(result(&mut s, 3, 1)[16], 5);
    assert_eq!(host(&mut s, 3, 3, 0, 4), 0);
    assert_eq!(s.model.get("free_mask"), 15);
    assert_eq!(s.model.get("sync_frame"), 192);
    println!(
        "SEMANTIC_AUDIO_LOCAL_QUANTUM_CYCLES {cycles:?}; command peak {}",
        s.model.get("peak_command_cycles")
    );
}

#[shoop_test]
fn real_media_fences_and_private_loader_streams_do_not_own_audio_admission() {
    fn command(s: &mut Sim<'_>, opcode: u64, view: u64, count: u64) {
        s.model.set_all(&[
            ("store_cmd_valid", 1),
            ("store_cmd_opcode", opcode),
            ("store_cmd_view", view),
            ("store_cmd_start", 0),
            ("store_cmd_length", if opcode == 6 { count } else { 0 }),
            ("store_cmd_count", count),
        ]);
        s.model.eval();
        assert_eq!(s.model.get("store_cmd_ready"), 1);
        s.idle(1);
        s.model.set("store_cmd_valid", 0);
    }
    #[track_caller]
    fn complete(s: &mut Sim<'_>) {
        s.until(100_000, "private media completion", |m| {
            m.get("store_done") != 0 || m.get("faulted") != 0
        });
        assert_eq!(
            s.model.get("faulted"),
            0,
            "pipeline {}/{}; store {}; memory fault {}; frame {}",
            s.model.get("error_code"),
            s.model.get("error_stage"),
            s.model.get("store_error_code"),
            s.model.get("memory_fault"),
            s.model.get("sync_frame")
        );
        assert_eq!(s.model.get("store_error_code"), 0);
    }
    fn acknowledge(s: &mut Sim<'_>) {
        s.model.set("store_response_ready", 1);
        s.idle(1);
        s.model.set("store_response_ready", 0);
    }
    fn exercise(mode: Mode, fail_fence: bool) {
        let root = rt_fpga_testbench::root();
        let geometry = Geometry::new([8, 64, 32], 2);
        let mut design = suites::pipeline::design(
            [16, 8, 32, 32, 4, 1, 1],
            &geometry,
            if mode == Mode::Legacy { 32 } else { 64 },
            mode,
            1,
        );
        design.top = if mode == Mode::Legacy {
            "fifo_media_audio_pipeline"
        } else {
            "fifo_media_audio_pipeline_axi"
        }
        .into();
        if mode != Mode::Legacy {
            design
                .sources
                .push(root.join("fpga/axi_memory/rtl/fifo_media_audio_pipeline_axi.sv"));
        }
        design.probes.clear();
        for module in [
            "fifo_group_banks",
            "fifo_edit_kernel",
            "fifo_group_consumer",
            "fifo_audio_pipeline",
            "fifo_media_loader",
            "fifo_media_arbiter",
            "fifo_media_audio_pipeline",
        ] {
            design
                .sources
                .push(root.join(format!("fpga/host_bridge/rtl/{module}.sv")));
        }
        design = design.parameters(&[("LOOP_DEPTH", 1), ("EXTERNAL_IO", 1)]);
        let compiled = design
            .build(&root.join("work/marlin-suite/semantic-media"))
            .unwrap();
        let mut s = Sim::new(compiled.session(None).unwrap(), &[("clk", 10_000, 0)]);
        let memory = Memory::attach(
            &mut s,
            mode,
            geometry.words(),
            0x0100_0000,
            &root.join("work/marlin-suite/semantic-media/memory"),
            91,
        )
        .unwrap();
        memory.borrow_mut().control.stalls = true;
        s.model.set_all(&[
            ("reset", 1),
            ("epoch", 1),
            ("pipeline_fault", 0),
            ("host_valid", 0),
            ("result_read_valid", 0),
            ("run_valid", 0),
            ("cfg_valid", 0),
            ("audio_cfg_valid", 0),
            ("store_cfg_valid", 0),
            ("store_cmd_valid", 0),
            ("store_write_valid", 0),
            ("store_read_ready", 0),
            ("store_response_ready", 0),
            ("arena_base", 0x0100_0000),
            ("maintenance_enable", 0),
            ("maintenance_tick", 0),
            ("run_quantum", 32),
            ("run_chunk_frames", 32),
            ("run_inputs", 1),
            ("run_outputs", 1),
            ("job_enable", 1),
            ("sample_input_data", 1 << 22),
            ("drain_ready", 0),
        ]);
        s.idle(2);
        s.model.set("reset", 0);
        s.idle(2);
        for word in 0..16 {
            let value = match word {
                0 => 1 | (7 << 1),
                3 => 256,
                _ => 0,
            };
            cfg(&mut s, false, true, word, Some(&Bits::from_u64(value)));
        }
        let instructions = [
            json!({"kind":"process_input","port":0,"destination":0}),
            json!({"kind":"process_gain","ramp":0,"source":0,"destination":1}),
            json!({"kind":"process_output","source":1,"port":0}),
        ];
        for (i, instruction) in instructions.iter().enumerate() {
            cfg(
                &mut s,
                false,
                false,
                i as u64,
                Some(&codec::instruction(instruction).unwrap()),
            );
        }
        cfg(
            &mut s,
            true,
            false,
            0,
            Some(&codec::state(&suites::audio::ramp(1 << 24, 1 << 24, 0, 0))),
        );
        command(&mut s, 0, 0, 0);
        complete(&mut s);
        let view = s.model.get("store_result_view");
        acknowledge(&mut s);
        let mut select = [0; 12];
        select[0] = 5;
        select[6] = 1;
        select[8] = 3 << 16;
        select[9] = 1 << 16;
        select[10] = 2;
        publish(&mut s, 0, 1, 0, &[select]);
        let mut gains = vec![];
        let mut costs = vec![quantum(&mut s, 0, 1 << 22, &mut gains)];
        s.until(100_000, "initial real fence", |m| {
            m.get("result_ready_mask") == 1
        });
        assert_eq!(result(&mut s, 0, 1)[15], 2);
        assert_eq!(host(&mut s, 3, 0, 0, 1), 0);
        let writes = memory.borrow().writes();
        command(&mut s, 5, view, 32);
        let mut frame = 32;
        for index in 0..32 {
            assert_eq!(
                s.model.get("store_busy"),
                0,
                "partial chunk must not acquire the real store"
            );
            assert_eq!(s.model.get("store_done"), 0);
            costs.push(quantum(&mut s, frame, 1 << 22, &mut gains));
            frame += 32;
            s.model.set_all(&[
                ("store_write_valid", 1),
                ("store_write_data", 0x100 + index),
            ]);
            s.model.eval();
            assert_eq!(s.model.get("store_write_ready"), 1);
            s.idle(1);
            s.model.set("store_write_valid", 0);
        }
        memory.borrow_mut().control.pause_responses = true;
        costs.push(quantum(&mut s, frame, 1 << 22, &mut gains));
        frame += 32;
        memory.borrow_mut().control.pause_responses = false;
        complete(&mut s);
        memory.borrow_mut().control.pause_responses = true;
        assert_eq!(
            s.model.get("store_cmd_ready"),
            0,
            "unread private response retains its bank only"
        );
        for (i, instruction) in instructions.iter().enumerate() {
            cfg(
                &mut s,
                false,
                false,
                16 + i as u64,
                Some(&codec::instruction(instruction).unwrap()),
            );
        }
        select[6] = 2;
        select[8] = 1 | (3 << 16);
        publish(&mut s, 0, 2, 1, &[select]);
        let selected_frame = frame;
        costs.push(quantum(&mut s, frame, 1 << 22, &mut gains));
        frame += 32;
        assert_eq!(s.model.get("handoff_pending"), 1);
        assert_eq!(s.model.get("result_ready_mask"), 0);
        assert_eq!(s.model.get("active_revision"), 2);
        assert!(
            memory.borrow().writes() > writes,
            "real dirty sample-cache writeback must start"
        );
        assert!(memory.borrow().active().is_some_and(|active| active.write
            && active.index < (geometry.slots * geometry.chunk_frames) as usize));
        let mut gain = [0; 12];
        gain[0] = 3;
        gain[2] = 7;
        gain[3] = 1;
        gain[5] = 0.5f32.to_bits();
        gain[6] = 1 << 23;
        publish(&mut s, 1, 3, 2, &[gain]);
        s.model.set_all(&[
            ("host_valid", 1),
            ("host_operation", 0),
            ("host_bank", 0),
            ("result_read_valid", 1),
            ("result_read_bank", 0),
            ("result_read_word", 0),
        ]);
        costs.push(quantum(&mut s, frame, 1 << 21, &mut gains));
        frame += 32;
        assert_eq!(s.model.get("host_receipt"), 1);
        assert_eq!(s.model.get("result_response_error"), 1);
        s.model
            .set_all(&[("host_valid", 0), ("result_read_valid", 0)]);
        assert_eq!(gains, [(0, u64::from(0.5f32.to_bits()))]);
        assert_eq!(s.model.get("handoff_pending"), 1);
        assert_eq!(s.model.get("result_ready_mask"), 0);
        assert_eq!(
            s.model.get("store_done"),
            1,
            "ARM response still unread during actual cache/DMA fence"
        );
        if fail_fence {
            memory.borrow_mut().control.fail_next = true;
        }
        memory.borrow_mut().control.pause_responses = false;
        if fail_fence {
            s.until(100_000, "actual fence memory failure", |m| {
                m.get("faulted") != 0
            });
            assert_eq!(s.model.get("memory_fault"), 1);
            assert_eq!(s.model.get("result_ready_mask"), 0);
            assert_eq!(
                s.model.get("active_revision"),
                2,
                "selection is not ownership completion"
            );
            assert_eq!(
                host(&mut s, 3, 0, 0, 2),
                3,
                "fault cannot acknowledge the selected image as fenced"
            );
            s.model.set("run_valid", 1);
            s.model.eval();
            assert_eq!(s.model.get("run_ready"), 0);
            s.idle(10);
            assert_eq!(s.model.get("sync_frame"), frame);
            assert_eq!(s.model.get("result_ready_mask"), 0);
            println!(
                "SEMANTIC_MEDIA_{mode:?}_FENCE_MEMORY_FAULT frame {frame}; no result or replay"
            );
            return;
        }
        s.until(100_000, "actual sample and metadata fence", |m| {
            m.get("result_ready_mask") == 1
        });
        assert_eq!(s.model.get("handoff_pending"), 0);
        let switched = result(&mut s, 0, 1);
        assert_eq!(switched[4], 2);
        assert_eq!(u64::from(switched[6]), selected_frame);
        assert_eq!(switched[15], 2);
        assert_eq!(switched[16], 1);
        assert_eq!(host(&mut s, 3, 0, 0, 2), 0);
        assert_eq!(result(&mut s, 1, 1)[16], 1);
        assert_eq!(host(&mut s, 3, 1, 0, 3), 0);
        acknowledge(&mut s);
        command(&mut s, 6, view, 32);
        complete(&mut s);
        assert_eq!(s.model.get("store_result_count"), 32);
        assert_eq!(s.model.get("store_read_data"), 0x100);
        costs.push(quantum(&mut s, frame, 1 << 21, &mut gains));
        frame += 32;
        acknowledge(&mut s);
        assert_eq!(
            s.model.get("store_cmd_ready"),
            0,
            "early response ACK cannot discard unread data"
        );
        costs.push(quantum(&mut s, frame, 1 << 21, &mut gains));
        frame += 32;
        s.model.set("store_read_ready", 1);
        for index in 0..32 {
            s.model.eval();
            assert_eq!(s.model.get("store_read_valid"), 1);
            assert_eq!(s.model.get("store_read_data"), 0x100 + index);
            s.idle(1);
        }
        s.model.set("store_read_ready", 0);
        assert_eq!(s.model.get("store_cmd_ready"), 1);
        assert_eq!(s.model.get("sync_frame"), frame);
        assert_eq!(s.model.get("faulted"), 0);
        for (address, value) in [(1, view), (2, 0), (3, 1), (4, 1)] {
            cfg(
                &mut s,
                true,
                false,
                address,
                Some(&codec::state(&json!({"kind":"word","value":value}))),
            );
        }
        let playback = [
            json!({"kind":"process_loop_playback","view":1,"position":2,"length":3,"enabled":4,"destination":0}),
            instructions[1].clone(),
            instructions[2].clone(),
        ];
        for (i, instruction) in playback.iter().enumerate() {
            cfg(
                &mut s,
                false,
                false,
                i as u64,
                Some(&codec::instruction(instruction).unwrap()),
            );
        }
        select[6] = 3;
        select[8] = 3 << 16;
        select[9] = 5 << 16;
        publish(&mut s, 0, 4, 2, &[select]);
        let samples = std::array::from_fn(|i| (0x100 + i as u64) / 2);
        costs.push(quantum_samples(&mut s, frame, &samples, &mut gains));
        frame += 32;
        s.until(100_000, "playback program fence", |m| {
            m.get("result_ready_mask") == 1
        });
        assert_eq!(result(&mut s, 0, 1)[15], 2);
        assert_eq!(host(&mut s, 3, 0, 0, 4), 0);
        let capture = [
            instructions[0].clone(),
            instructions[1].clone(),
            json!({"kind":"process_loop_capture","view":1,"enabled":4,"source":1}),
            instructions[2].clone(),
        ];
        for (i, instruction) in capture.iter().enumerate() {
            cfg(
                &mut s,
                false,
                false,
                16 + i as u64,
                Some(&codec::instruction(instruction).unwrap()),
            );
        }
        select[6] = 4;
        select[8] = 1 | (4 << 16);
        publish(&mut s, 0, 5, 3, &[select]);
        costs.push(quantum(&mut s, frame, 1 << 21, &mut gains));
        frame += 32;
        s.until(100_000, "capture program fence", |m| {
            m.get("result_ready_mask") == 1
        });
        assert_eq!(result(&mut s, 0, 1)[15], 2);
        assert_eq!(host(&mut s, 3, 0, 0, 5), 0);
        for (i, instruction) in instructions.iter().enumerate() {
            cfg(
                &mut s,
                false,
                false,
                i as u64,
                Some(&codec::instruction(instruction).unwrap()),
            );
        }
        select[6] = 5;
        select[8] = 3 << 16;
        publish(&mut s, 0, 6, 4, &[select]);
        costs.push(quantum(&mut s, frame, 1 << 21, &mut gains));
        frame += 32;
        s.until(100_000, "recorded sample/cache fence", |m| {
            m.get("result_ready_mask") == 1
        });
        assert_eq!(result(&mut s, 0, 1)[15], 2);
        assert_eq!(host(&mut s, 3, 0, 0, 6), 0);
        {
            let backing = memory.borrow();
            let words = match &backing.backend {
                rt_fpga_testbench::memory::Backend::Legacy(bus) => &bus.words,
                rt_fpga_testbench::memory::Backend::Axi(bus) => &bus.words,
                _ => unreachable!("external memory model required"),
            };
            assert_eq!(
                &words[..32],
                &(0..32).map(|i| 0x100 + i).collect::<Vec<u32>>()
            );
            assert_eq!(&words[32..64], &[1 << 21; 32]);
            assert!(
                backing.active().is_none(),
                "fenced data must be in the independent backing memory, not only cache readback"
            );
        }
        s.model.set_all(&[
            ("store_cmd_valid", 1),
            ("store_cmd_opcode", 6),
            ("store_cmd_view", view),
            ("store_cmd_start", 32),
            ("store_cmd_length", 64),
            ("store_cmd_count", 32),
        ]);
        s.model.eval();
        assert_eq!(s.model.get("store_cmd_ready"), 1);
        s.idle(1);
        s.model.set("store_cmd_valid", 0);
        complete(&mut s);
        assert_eq!(s.model.get("store_result_count"), 32);
        s.model.set("store_read_ready", 1);
        for _ in 0..32 {
            s.model.eval();
            assert_eq!(s.model.get("store_read_valid"), 1);
            assert_eq!(s.model.get("store_read_data"), 1 << 21);
            s.idle(1);
        }
        s.model.set("store_read_ready", 0);
        acknowledge(&mut s);
        assert_eq!(s.model.get("sync_frame"), frame);
        assert_eq!(s.model.get("faulted"), 0);
        println!(
            "SEMANTIC_MEDIA_{mode:?}_QUANTUM_CYCLES {costs:?}; peak command {}",
            s.model.get("peak_command_cycles")
        );
    }
    for mode in [Mode::Legacy, Mode::Fixed] {
        exercise(mode, false);
    }
    exercise(Mode::Fixed, true);
}
