#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use rt_fpga_testbench::{bits::Bits, codec, model::Design, sim::Sim, suites};
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
    assert_eq!(output, [Some(sample); 32]);
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
