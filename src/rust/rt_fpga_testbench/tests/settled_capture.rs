#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use rt_fpga_testbench::{bits::Bits, codec, sim::Sim, suites};
use serde_json::json;
use shoop_wasm_test_support::shoop_test;

fn configure(s: &mut Sim<'_>, space: u64, address: u64, value: &Bits) {
    s.model.set_all(&[
        ("cfg_valid", 1),
        ("cfg_state", u64::from(space == 1)),
        ("cfg_loop", u64::from(space == 2)),
        ("cfg_write", 1),
        ("cfg_address", address),
    ]);
    s.model.set_bits("cfg_write_data", &value.resized(129));
    s.model.eval();
    assert_eq!(s.model.get("cfg_ready"), 1);
    s.idle(1);
    assert_eq!(s.model.get("cfg_response_valid"), 1);
    assert_eq!(s.model.get("cfg_response_error"), 0);
    s.model.set("cfg_valid", 0);
    s.idle(1);
}
fn reset(s: &mut Sim<'_>) {
    s.model.set_all(&[
        ("reset", 1),
        ("run_valid", 0),
        ("cfg_valid", 0),
        ("local_valid", 0),
        ("capture_ready", 0),
        ("run_quantum", 32),
        ("run_chunk_frames", 32),
        ("run_inputs", 1),
        ("run_outputs", 1),
        ("run_buffers", 2),
        ("job_enable", 1),
        ("sample_input_data", 1 << 22),
        ("sync_event_ready", 1),
        ("capture_credits", 65535),
    ]);
    s.idle(2);
    s.model.set("reset", 0);
    s.idle(2);
}
fn start(s: &mut Sim<'_>, instructions: u64, loops: u64) {
    s.model.set_all(&[
        ("run_instruction_count", instructions),
        ("run_sync_count", loops),
        ("run_state_count", 1),
        ("run_valid", 1),
    ]);
    s.model.eval();
    assert_eq!(s.model.get("run_ready"), 1);
    s.idle(1);
    s.model.set("run_valid", 0);
}
fn held(s: &Sim<'_>) {
    assert_eq!(s.model.get("capture_valid"), 1);
    assert_eq!(s.model.get("state_busy"), 1);
    assert_eq!(s.model.get("monitor_valid"), 0);
    assert_eq!(s.model.get("queued_jobs"), 0);
    assert_eq!(s.model.get("jobs_busy"), 0);
    assert_eq!(s.model.get("sample_output_write"), 0);
    assert_eq!(s.model.get("done"), 0);
}
fn local(s: &mut Sim<'_>, state: bool, address: u64, write: bool) -> Bits {
    held(s);
    s.model.set_all(&[
        ("local_valid", 1),
        ("local_state", u64::from(state)),
        ("local_write", u64::from(write)),
        ("local_address", address),
    ]);
    s.model
        .set_bits("local_write_data", &Bits::from_u64(u64::MAX).resized(129));
    s.model.eval();
    assert_eq!(s.model.get("local_ready"), 1);
    s.idle(1);
    assert_eq!(s.model.get("local_response_valid"), 1);
    assert_eq!(s.model.get("local_response_error"), u64::from(write));
    let data = s.model.bits("local_read_data");
    s.model.set("local_valid", 0);
    s.idle(1);
    held(s);
    data
}
fn finish(s: &mut Sim<'_>, sample: u64) {
    s.model.set("capture_ready", 1);
    s.idle(1);
    s.model.set("capture_ready", 0);
    let mut output = [None; 32];
    for _ in 0..10_000 {
        s.model.eval();
        if s.model.get("sample_output_write") != 0 {
            let address = s.model.get("sample_output_address") as usize;
            assert!(address < output.len());
            output[address] = Some(s.model.get("sample_output_data"));
        }
        if s.model.get("done") != 0 {
            break;
        }
        assert_eq!(s.model.get("capture_valid"), 0);
        s.idle(1);
    }
    assert_eq!(s.model.get("done"), 1);
    assert_eq!(s.model.get("error_code"), 0);
    assert_eq!(output, [Some(sample); 32]);
    s.idle(2);
}
fn work() -> Vec<Bits> {
    [
        json!({"kind":"process_input","port":0,"destination":0}),
        json!({"kind":"process_gain","ramp":0,"source":0,"destination":1}),
        json!({"kind":"process_output","source":1,"port":0}),
    ]
    .iter()
    .map(|j| codec::instruction(j).unwrap())
    .collect()
}

#[shoop_test]
fn entire_128_loop_sync_settles_before_jobs_and_private_capture_reads_are_bounded_and_read_only() {
    let root = rt_fpga_testbench::root();
    let mut design = suites::audio::design([256, 16, 128, 32, 4, 1, 1]);
    design.probes.clear();
    let compiled = design
        .parameters(&[("CAPTURE_SYNC", 1), ("LOOP_DEPTH", 128), ("EXTERNAL_IO", 1)])
        .build(&root.join("work/marlin-suite/settled-capture"))
        .unwrap();
    let mut s = Sim::new(compiled.session(None).unwrap(), &[("clk", 10_000, 0)]);
    reset(&mut s);
    for slot in 0..128 {
        for word in 0..16 {
            let value = match word {
                0 => 1 | (7 << 1) | ((if slot == 127 { 2 } else { 1 }) << 33),
                2 => {
                    if slot == 127 {
                        0
                    } else {
                        slot
                    }
                }
                3 => {
                    if slot == 127 {
                        0
                    } else {
                        256
                    }
                }
                _ => 0,
            };
            configure(&mut s, 2, slot * 16 + word, &Bits::from_u64(value));
        }
        let mut instruction = Bits::zero(96);
        instruction
            .set(0, 8, 12)
            .set(8, 16, slot)
            .set(24, 32, 7)
            .set(56, 16, 65535)
            .set(72, 16, 1);
        configure(&mut s, 0, slot, &instruction);
    }
    for (i, instruction) in work().iter().enumerate() {
        configure(&mut s, 0, 128 + i as u64, instruction);
    }
    let ramp = codec::state(&suites::audio::ramp(1 << 24, 1 << 24, 0, 0));
    configure(&mut s, 1, 0, &ramp);
    for frame in [0, 32] {
        start(&mut s, 131, 128);
        s.until(20_000, "whole sync prefix", |m| {
            m.get("capture_valid") != 0 || m.get("done") != 0
        });
        held(&s);
        assert_eq!(s.model.get("capture_frame"), frame);
        assert_eq!(s.model.get("sync_frame"), frame + 32);
        s.model.set_all(&[
            ("cfg_valid", 1),
            ("cfg_loop", 1),
            ("cfg_state", 0),
            ("cfg_write", 1),
            ("cfg_address", 2),
        ]);
        s.model
            .set_bits("cfg_write_data", &Bits::from_u64(u64::MAX));
        for slot in 0..128 {
            let words: Vec<_> = (0..16)
                .map(|word| local(&mut s, false, slot * 16 + word, false).get(0, 64))
                .collect();
            assert_eq!(
                words[9], frame,
                "last slot must settle before capture opens"
            );
            if slot == 127 {
                assert_eq!(words[4], frame + 32);
                assert_eq!(
                    words[4] - 32,
                    frame,
                    "render capture length excludes next quantum"
                );
                assert_eq!(words[8], 0);
            } else {
                assert_eq!(words[8], slot + frame);
                assert_eq!(words[2], slot + frame + 32);
            }
            assert_eq!(
                s.model.get("cfg_ready"),
                0,
                "loader writes cannot change a settled capture"
            );
        }
        assert_eq!(local(&mut s, true, 0, false), ramp);
        assert_eq!(local(&mut s, false, 2, true), Bits::zero(129));
        assert_eq!(
            local(&mut s, false, 2, false).get(0, 64),
            frame + 32,
            "capture cannot write continuing state"
        );
        s.model.set("cfg_valid", 0);
        finish(&mut s, 1 << 22);
    }
}

#[shoop_test]
fn zero_sync_and_empty_programs_capture_before_work_without_manufacturing_another_frame() {
    let root = rt_fpga_testbench::root();
    let mut design = suites::audio::design([16, 4, 16, 32, 4, 1, 1]);
    design.probes.clear();
    let compiled = design
        .parameters(&[("CAPTURE_SYNC", 1), ("EXTERNAL_IO", 1)])
        .build(&root.join("work/marlin-suite/settled-capture-empty"))
        .unwrap();
    let mut s = Sim::new(compiled.session(None).unwrap(), &[("clk", 10_000, 0)]);
    reset(&mut s);
    for (i, instruction) in work().iter().enumerate() {
        configure(&mut s, 0, i as u64, instruction);
    }
    configure(
        &mut s,
        1,
        0,
        &codec::state(&suites::audio::ramp(1 << 24, 1 << 24, 0, 0)),
    );
    for (frame, instructions) in [(0, 3), (32, 0)] {
        start(&mut s, instructions, 0);
        s.until(100, "zero-sync gate", |m| m.get("capture_valid") != 0);
        held(&s);
        assert_eq!(s.model.get("capture_frame"), frame);
        assert_eq!(s.model.get("sync_frame"), frame + 32);
        s.idle(100);
        held(&s);
        assert_eq!(s.model.get("sync_frame"), frame + 32);
        finish(&mut s, if instructions == 0 { 0 } else { 1 << 22 });
    }
}

#[shoop_test]
fn strict_semantic_prefix_refuses_early_work_but_standalone_interleaving_still_runs() {
    let root = rt_fpga_testbench::root();
    for strict in [0, 1] {
        let mut design = suites::audio::design([16, 4, 16, 32, 4, 1, 1]);
        design.probes.clear();
        let compiled = design
            .parameters(&[("CAPTURE_SYNC", strict), ("EXTERNAL_IO", 1)])
            .build(&root.join("work/marlin-suite/settled-capture-prefix"))
            .unwrap();
        let mut s = Sim::new(compiled.session(None).unwrap(), &[("clk", 10_000, 0)]);
        reset(&mut s);
        for word in 0..16 {
            configure(
                &mut s,
                2,
                word,
                &Bits::from_u64(if word == 0 { 1 | (7 << 1) } else { 0 }),
            );
        }
        configure(
            &mut s,
            0,
            0,
            &codec::instruction(&json!({"kind":"process_input","port":0,"destination":0})).unwrap(),
        );
        let mut loop_instruction = Bits::zero(96);
        loop_instruction
            .set(0, 8, 12)
            .set(24, 32, 7)
            .set(56, 16, 65535)
            .set(72, 16, 1);
        configure(&mut s, 0, 1, &loop_instruction);
        configure(
            &mut s,
            0,
            2,
            &codec::instruction(&json!({"kind":"process_output","source":0,"port":0})).unwrap(),
        );
        start(&mut s, 3, 1);
        let mut output = [None; 32];
        for _ in 0..2000 {
            s.model.eval();
            assert_eq!(s.model.get("capture_valid"), 0);
            if s.model.get("sample_output_write") != 0 {
                let address = s.model.get("sample_output_address") as usize;
                assert!(address < output.len());
                output[address] = Some(s.model.get("sample_output_data"));
            }
            if s.model.get("done") != 0 {
                break;
            }
            s.idle(1);
        }
        assert_eq!(s.model.get("done"), 1);
        if strict == 0 {
            assert_eq!(s.model.get("error_code"), 0);
            assert_eq!(output, [Some(1 << 22); 32]);
            assert_eq!(s.model.get("sync_frame"), 32);
        } else {
            assert_ne!(s.model.get("error_code"), 0);
            assert_eq!(output, [None; 32]);
            assert_eq!(s.model.get("sync_frame"), 0);
        }
    }
}
