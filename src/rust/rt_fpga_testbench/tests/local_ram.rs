#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use rt_fpga_testbench::{bits::Bits, drivers::state::State, model::Probe, sim::Sim, suites};
use shoop_wasm_test_support::shoop_test;

fn access(sim: &mut Sim<'_>, state: bool, address: u64, value: Option<&Bits>, error: bool) -> Bits {
    sim.model.set_all(&[
        ("local_valid", 1),
        ("local_state", u64::from(state)),
        ("local_write", u64::from(value.is_some())),
        ("local_address", address),
    ]);
    sim.model.set_bits(
        "local_write_data",
        &value.cloned().unwrap_or_else(|| Bits::zero(129)),
    );
    sim.model.eval();
    assert_eq!(sim.model.get("local_ready"), 1);
    assert_eq!(sim.model.get("cfg_ready"), 0);
    sim.idle(1);
    assert_eq!(sim.model.get("local_response_valid"), 1);
    assert_eq!(sim.model.get("local_response_error"), u64::from(error));
    let data = sim.model.bits("local_read_data");
    sim.model.set("local_valid", 0);
    sim.idle(1);
    assert_eq!(sim.model.get("local_response_valid"), 0);
    data
}

fn empty_quantum(sim: &mut Sim<'_>, frame: u64) {
    sim.model.set_all(&[
        ("run_valid", 1),
        ("run_instruction_count", 0),
        ("run_sync_count", 0),
        ("run_state_count", 0),
        ("run_quantum", 32),
        ("run_chunk_frames", 1),
    ]);
    sim.model.eval();
    assert_eq!(sim.model.get("run_ready"), 1);
    sim.idle(1);
    sim.model.set("run_valid", 0);
    sim.until(24, "empty SP quantum", |model| model.get("done") != 0);
    assert_eq!(sim.model.get("error_code"), 0);
    assert_eq!(sim.model.get("sync_frame"), frame);
    sim.idle(1);
}

#[shoop_test]
fn program_upload_busy_and_response_do_not_own_sp_run_admission() {
    for regions in [1, 2] {
        let depths = [16, 4, 16];
        let compiled = suites::state::design(depths)
            .parameters(&[("PROGRAM_REGIONS", regions), ("DDR_PROGRAM", 1)])
            .build(&rt_fpga_testbench::root().join("work/marlin-suite/local-ram-admission"))
            .unwrap();
        let mut sim = Sim::new(compiled.session(None).unwrap(), &[("clk", 10_000, 0)]);
        let mut driver = if regions == 2 {
            State::loading(&mut sim, depths, 73)
        } else {
            State::new(&mut sim, depths, 73)
        };
        sim.model.set_all(&[
            ("local_valid", 0),
            ("memory_program_ready", 0),
            ("memory_program_write_ready", 0),
            ("memory_program_read_valid", 0),
            ("memory_program_response_valid", 0),
            ("memory_program_response_error", 0),
        ]);
        driver.reset(&mut sim);
        sim.model.set_all(&[
            ("cfg_valid", 1),
            ("cfg_loop", 0),
            ("cfg_state", 0),
            ("cfg_write", 1),
            ("cfg_address", if regions == 2 { 16 } else { 0 }),
        ]);
        let words = [0x1122_3344, 0xaabb_ccdd, 0x5566_7788, 0];
        let mut payload = Bits::zero(129);
        for (word, value) in words.iter().take(3).enumerate() {
            payload.set(word * 32, 32, *value);
        }
        sim.model.set_bits("cfg_write_data", &payload);
        sim.idle(1);
        sim.model.set("cfg_valid", 0);
        assert_eq!(sim.model.get("sync_frame"), 0);
        for frame in [32, 64, 96] {
            assert_eq!(sim.model.get("memory_program_valid"), 1);
            if regions == 2 {
                empty_quantum(&mut sim, frame);
            } else {
                sim.model.eval();
                assert_eq!(sim.model.get("run_ready"), 0);
                sim.idle(12);
                assert_eq!(sim.model.get("sync_frame"), 0);
            }
            assert_eq!(sim.model.get("cfg_response_valid"), 0);
        }
        sim.model.set("memory_program_ready", 1);
        sim.idle(1);
        sim.model.set_all(&[
            ("memory_program_ready", 0),
            ("memory_program_write_ready", 1),
        ]);
        for word in words {
            assert_eq!(sim.model.get("memory_program_write_valid"), 1);
            assert_eq!(sim.model.get("memory_program_write_data"), word);
            sim.idle(1);
        }
        sim.model.set_all(&[
            ("memory_program_write_ready", 0),
            ("memory_program_response_valid", 1),
        ]);
        sim.idle(1);
        sim.model.set("memory_program_response_valid", 0);
        assert_eq!(sim.model.get("cfg_response_valid"), 1);
        assert_eq!(sim.model.get("cfg_response_error"), 0);
        assert_eq!(
            sim.model.get("sync_frame"),
            if regions == 2 { 96 } else { 0 }
        );
        if regions == 2 {
            empty_quantum(&mut sim, 128);
        } else {
            sim.model.eval();
            assert_eq!(sim.model.get("run_ready"), 0);
            sim.idle(1);
            empty_quantum(&mut sim, 32);
        }
    }
}

#[shoop_test]
fn background_readiness_without_valid_cannot_accept_or_mutate_execution_state() {
    for audio in [false, true] {
        for regions in [1, 2] {
            for ddr in [0, 1] {
                let mut design = if audio {
                    suites::audio::design([16, 4, 16, 32, 2, 1, 1])
                } else {
                    suites::state::design([16, 4, 16])
                };
                let prefix = if audio { "state_engine." } else { "" };
                let effects: Vec<_> = [
                    "cfg_accept",
                    "cfg_program_access",
                    "cfg_state_access",
                    "cfg_loop_access",
                ]
                .map(|name| format!("{prefix}{name}"))
                .into();
                design.probes = effects.iter().map(|name| Probe::observe(name)).collect();
                let compiled = design
                    .parameters(&[
                        ("PROGRAM_REGIONS", regions),
                        ("DDR_PROGRAM", ddr),
                        ("CAPTURE_SYNC", 1),
                        ("LOOP_DEPTH", 4),
                    ])
                    .build(&rt_fpga_testbench::root().join("work/marlin-suite/background-ready"))
                    .unwrap();
                let mut s = Sim::new(compiled.session(None).unwrap(), &[("clk", 10_000, 0)]);
                s.model.set_all(&[
                    ("reset", 1),
                    ("run_valid", 0),
                    ("cfg_valid", 0),
                    ("local_valid", 0),
                    ("capture_ready", 0),
                    ("run_quantum", 32),
                    ("run_chunk_frames", 32),
                    ("run_instruction_count", 0),
                    ("run_sync_count", 0),
                    ("run_state_count", 0),
                    ("sync_event_ready", 1),
                    ("capture_credits", 65535),
                ]);
                if audio {
                    s.model.set_all(&[("job_enable", 1), ("run_buffers", 2)]);
                } else {
                    s.model.set("job_ready", 1);
                }
                s.idle(2);
                s.model.set("reset", 0);
                s.idle(2);
                let mut marker = Bits::zero(129);
                marker.set(0, 64, 0x1234_5678_9abc_def0).set(128, 1, 1);
                for (state, address) in [(1, 3), (0, 63)] {
                    s.model.set_all(&[
                        ("cfg_valid", 1),
                        ("cfg_state", state),
                        ("cfg_loop", 1 - state),
                        ("cfg_write", 1),
                        ("cfg_address", address),
                    ]);
                    s.model.set_bits("cfg_write_data", &marker);
                    s.model.eval();
                    assert_eq!(s.model.get("cfg_ready"), 1);
                    s.idle(1);
                    assert_eq!(s.model.get("cfg_response_valid"), 1);
                    assert_eq!(s.model.get("cfg_response_error"), 0);
                    s.model.set("cfg_valid", 0);
                    s.idle(1);
                }
                s.model.set("run_valid", 1);
                s.model.eval();
                assert_eq!(s.model.get("run_ready"), 1);
                s.idle(1);
                s.model.set("run_valid", 0);
                s.until(100, "held empty sync capture", |m| {
                    m.get("capture_valid") != 0
                });
                let frame = s.model.get("sync_frame");
                s.model.set_bits(
                    "cfg_write_data",
                    &Bits::from([u32::MAX, u32::MAX, u32::MAX, u32::MAX, 1]),
                );
                for (state, loops, write, address) in [
                    (0, 0, 1, if regions == 2 { 16 } else { 0 }),
                    (1, 0, 1, 3),
                    (0, 1, 1, 63),
                    (0, 0, 0, 0),
                ] {
                    s.model.set_all(&[
                        ("cfg_state", state),
                        ("cfg_loop", loops),
                        ("cfg_write", write),
                        ("cfg_address", address),
                    ]);
                    for _ in 0..16 {
                        s.model.eval();
                        assert_eq!(
                            s.model.get("cfg_ready"),
                            u64::from(regions == 2 && state == 0 && loops == 0 && write == 1),
                            "audio={audio},regions={regions},ddr={ddr}"
                        );
                        for effect in &effects {
                            assert_eq!(s.model.get(effect), 0, "{effect}");
                        }
                        assert_eq!(s.model.get("memory_program_valid"), 0);
                        assert_eq!(s.model.get("cfg_response_valid"), 0);
                        assert_eq!(s.model.get("sync_frame"), frame);
                        assert_eq!(s.model.get("capture_valid"), 1);
                        s.idle(1);
                    }
                }
                assert_eq!(access(&mut s, true, 3, None, false), marker);
                assert_eq!(
                    access(&mut s, false, 63, None, false),
                    Bits::from_u64(marker.get(0, 64)).resized(129)
                );
                s.model.set_all(&[
                    ("cfg_valid", 1),
                    ("cfg_state", 0),
                    ("cfg_loop", 0),
                    ("cfg_write", 1),
                    ("cfg_address", if regions == 2 { 16 } else { 0 }),
                ]);
                s.model.eval();
                assert_eq!(s.model.get("cfg_ready"), u64::from(regions == 2));
                assert_eq!(s.model.get(&effects[0]), u64::from(regions == 2));
                s.idle(1);
                s.model.set("cfg_valid", 0);
                if regions == 2 && ddr == 1 {
                    assert_eq!(s.model.get("memory_program_valid"), 1);
                    s.idle(16);
                    assert_eq!(s.model.get("memory_program_valid"), 1);
                    assert_eq!(s.model.get("cfg_response_valid"), 0);
                    s.model.set("memory_program_ready", 1);
                    s.idle(1);
                    s.model.set_all(&[
                        ("memory_program_ready", 0),
                        ("memory_program_write_ready", 1),
                    ]);
                    for word in [u64::from(u32::MAX); 3].into_iter().chain([0]) {
                        assert_eq!(s.model.get("memory_program_write_valid"), 1);
                        assert_eq!(s.model.get("memory_program_write_data"), word);
                        s.idle(1);
                    }
                    s.model.set_all(&[
                        ("memory_program_write_ready", 0),
                        ("memory_program_response_valid", 1),
                    ]);
                    s.idle(1);
                    s.model.set("memory_program_response_valid", 0);
                    assert_eq!(s.model.get("cfg_response_valid"), 1);
                    assert_eq!(s.model.get("cfg_response_error"), 0);
                } else {
                    assert_eq!(s.model.get("cfg_response_valid"), u64::from(regions == 2));
                    assert_eq!(s.model.get("memory_program_valid"), 0);
                }
                assert_eq!(s.model.get("sync_frame"), frame);
                assert_eq!(s.model.get("capture_valid"), 1);
                s.idle(2);
                s.model.set("capture_ready", 1);
                s.until(100, "finish empty quantum", |m| m.get("done") != 0);
                assert_eq!(s.model.get("error_code"), 0);
            }
        }
    }
}

#[shoop_test]
fn execution_ram_has_one_cycle_service_and_priority_even_with_a_stalled_program_loader() {
    for ddr in [0, 1] {
        let depths = [16, 4, 16];
        let design = suites::state::design(depths).parameters(&[
            ("PROGRAM_REGIONS", 2),
            ("LOOP_DEPTH", 4),
            ("DDR_PROGRAM", ddr),
        ]);
        let compiled = design
            .build(&rt_fpga_testbench::root().join("work/marlin-suite/local-ram"))
            .unwrap();
        let mut sim = Sim::new(compiled.session(None).unwrap(), &[("clk", 10_000, 0)]);
        let mut driver = State::loading(&mut sim, depths, 71);
        sim.model.set("local_valid", 0);
        sim.model.set_all(&[
            ("memory_program_ready", 0),
            ("memory_program_write_ready", 0),
            ("memory_program_read_valid", 0),
            ("memory_program_response_valid", 0),
            ("memory_program_response_error", 0),
        ]);
        driver.reset(&mut sim);
        if ddr != 0 {
            sim.model.set_all(&[
                ("cfg_valid", 1),
                ("cfg_loop", 0),
                ("cfg_state", 0),
                ("cfg_write", 1),
                ("cfg_address", 16),
            ]);
            sim.model.set("cfg_write_data", 0);
            assert_eq!(sim.model.get("cfg_ready"), 1);
            sim.idle(1);
            sim.model.set("cfg_valid", 0);
            assert_eq!(sim.model.get("memory_program_valid"), 1);
        }
        let mut loop_word = Bits::zero(129);
        loop_word.set(0, 64, 0xdead_beef_1234_5678);
        let mut state_word = loop_word.clone();
        state_word.set(64, 64, 0x1020_3040_5060_7080).set(128, 1, 1);
        for round in 0..32 {
            sim.model.set_all(&[
                ("cfg_valid", 1),
                ("cfg_loop", 1),
                ("cfg_write", 0),
                ("cfg_address", 63),
            ]);
            access(&mut sim, false, 63, Some(&loop_word), false);
            access(&mut sim, true, 3, Some(&state_word), false);
            assert_eq!(access(&mut sim, false, 63, None, false), loop_word);
            assert_eq!(access(&mut sim, true, 3, None, false), state_word);
            assert_eq!(
                access(&mut sim, false, 127, Some(&Bits::zero(129)), true),
                Bits::zero(129)
            );
            assert_eq!(
                access(&mut sim, true, 7, Some(&Bits::zero(129)), true),
                Bits::zero(129)
            );
            assert_eq!(access(&mut sim, false, 63, None, false), loop_word);
            assert_eq!(access(&mut sim, true, 3, None, false), state_word);
            if ddr != 0 {
                assert_eq!(sim.model.get("memory_program_valid"), 1);
                assert_eq!(sim.model.get("cfg_response_valid"), 0);
            }
            loop_word.set(0, 32, round);
        }
        sim.model.set("cfg_valid", 0);
        sim.model.set_all(&[
            ("local_valid", 1),
            ("local_state", 0),
            ("local_write", 0),
            ("local_address", 63),
            ("run_valid", 1),
        ]);
        sim.model.eval();
        assert_eq!(
            sim.model.get("local_ready"),
            0,
            "processing owns the RAM before a local request"
        );
        sim.model.set_all(&[("local_valid", 0), ("run_valid", 0)]);
        sim.model.set("reset", 1);
        sim.idle(1);
        assert_eq!(sim.model.get("local_response_valid"), 0);
        assert_eq!(sim.model.get("local_ready"), 0);
    }
}
