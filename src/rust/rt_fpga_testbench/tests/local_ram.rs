#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use rt_fpga_testbench::{bits::Bits, drivers::state::State, sim::Sim, suites};
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
        let mut driver = State::new(&mut sim, depths, 71);
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
