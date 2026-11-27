use super::{
    channel, design, first_fault, publish_hold, quantum, read, read_hold, release_read,
    release_write, reset, wait_initial, write, write_hold,
};
use rt_fpga_rtl::host::semantic_registers::*;
use rt_fpga_testbench::{
    bits::Bits,
    drivers::store::Geometry,
    memory::{Memory, Mode},
    sim::Sim,
};
use shoop_wasm_test_support::shoop_test;

#[shoop_test(
    no_wasm = "Native source-qualified abort/audio/watchdog with actual held DDR ownership"
)]
fn abort_and_hardware_faults_keep_their_causes_without_retiring_or_replaying_backend_work() {
    let root = rt_fpga_testbench::root();
    let compiled = design()
        .build(&root.join("work/marlin-suite/semantic-mmio-audio"))
        .unwrap();
    for (abort, audio, watchdog, generic, cause) in [
        (true, 0, 0, 0, 16),
        (false, 1, 0, 0, 2),
        (false, 0, 1, 0, 32),
        (false, 1, 1, 0, 34),
        (false, 0, 1, 1, 96),
    ] {
        let mut s = Sim::new(compiled.session(None).unwrap(), &[("clk", 10_000, 0)]);
        let geometry = Geometry::new([8, 64, 32], 2);
        let memory = Memory::attach(
            &mut s,
            Mode::Fixed,
            geometry.words() + 128,
            0x0100_0000,
            &root.join("work/marlin-suite/semantic-mmio-audio/memory"),
            93,
        )
        .unwrap();
        memory.borrow_mut().control.stalls = true;
        reset(&mut s);
        memory.borrow_mut().control.hold_aw = true;
        s.model.set_all(&[
            ("cfg_valid", 1),
            ("cfg_state", 0),
            ("cfg_loop", 0),
            ("cfg_write", 1),
            ("cfg_address", 0),
        ]);
        s.model
            .set_bits("cfg_write_data", &Bits::from_u64(0).resized(129));
        s.model.eval();
        assert_eq!(s.model.get("cfg_ready"), 1);
        s.idle(1);
        s.model.set("cfg_valid", 0);
        s.until(10_000, "actual upload AW held before local fault", |m| {
            m.get("m_axi_awvalid") != 0
        });
        let address = s.model.get("m_axi_awaddr");
        if abort {
            s.model.set_all(&[
                ("s_axi_awaddr", u64::from(REG_ABORT)),
                ("s_axi_wdata", 1),
                ("s_axi_wstrb", 3),
            ]);
            channel(&mut s, "s_axi_wvalid", "s_axi_wready");
            channel(&mut s, "s_axi_awvalid", "s_axi_awready");
            s.until(32, "malformed ABORT strobe", |m| m.get("s_axi_bvalid") != 0);
            assert_eq!(s.model.get("s_axi_bresp"), 2);
            s.model.set("s_axi_bready", 1);
            s.idle(1);
            s.model.set("s_axi_bready", 0);
            assert_eq!(read(&mut s, REG_SUBMISSION_RESULT), RECEIPT_UNAVAILABLE);
            write(&mut s, REG_ABORT, 0);
            assert_eq!(read(&mut s, REG_SUBMISSION_RESULT), RECEIPT_EPOCH);
            s.model.set("s_axi_awaddr", u64::from(REG_ABORT));
            channel(&mut s, "s_axi_awvalid", "s_axi_awready");
        }
        quantum(&mut s, 0, 0, &mut vec![]);
        assert_eq!(s.model.get("faulted"), 0);
        assert_eq!(s.model.get("sync_frame"), 32);
        assert_eq!(read(&mut s, REG_COMPLETED_QUANTA), 1);
        if abort {
            s.model.set_all(&[("s_axi_wdata", 1), ("s_axi_wstrb", 15)]);
            channel(&mut s, "s_axi_wvalid", "s_axi_wready");
        } else {
            s.model.set_all(&[
                ("audio_fault", audio),
                ("watchdog_fault", watchdog),
                ("pipeline_fault", generic),
            ]);
        }
        s.model.eval();
        assert_eq!(s.model.get("diagnostic_valid"), 1);
        assert_eq!(s.model.get("diagnostic_cause"), cause);
        assert_eq!(s.model.get("gain_update_valid"), 0);
        assert_eq!(s.model.get("sample_output_write"), 0);
        s.idle(1);
        s.model.set_all(&[
            ("audio_fault", 0),
            ("watchdog_fault", 0),
            ("pipeline_fault", 0),
        ]);
        s.idle(8);
        assert_eq!(s.model.get("faulted"), 1);
        assert_eq!(s.model.get("run_ready"), 0);
        assert_eq!(
            s.model.get("recovery_state"),
            0,
            "local halt is not recovery"
        );
        assert_eq!(s.model.get("runtime_reset"), 0);
        assert_eq!(s.model.get("runtime_epoch"), 1);
        assert_eq!(s.model.get("m_axi_awvalid"), 1);
        assert_eq!(s.model.get("m_axi_awaddr"), address);
        let saved = first_fault(&s);
        assert_eq!(saved[0], 1);
        assert_eq!(saved[1], cause);
        assert_eq!(saved[3], u64::from(u32::MAX));
        assert_eq!(saved[7], 0);
        assert_eq!(saved[9], 32);
        assert_eq!(read(&mut s, REG_STATUS), 4);
        assert_eq!(s.model.get("irq"), 1);
        assert_eq!(read(&mut s, REG_FAULT_CAUSE), cause as u32);
        if abort {
            assert_eq!(s.model.get("s_axi_bvalid"), 1);
            assert_eq!(read(&mut s, REG_SUBMISSION_RESULT), RECEIPT_ACCEPTED);
            release_write(&mut s);
        }
        s.model.set("pipeline_fault", 1);
        s.idle(2);
        s.model.set("pipeline_fault", 0);
        assert_eq!(
            first_fault(&s),
            saved,
            "later generic faults must not relabel the cause"
        );
        assert_eq!(read_hold(&mut s, REG_FAULT_CAUSE), cause as u32);
        write_hold(&mut s, REG_RECOVERY_REQUEST, 1);
        s.idle(64);
        assert_eq!(s.model.get("recovery_state"), 1);
        assert_eq!(s.model.get("runtime_epoch"), 1);
        assert_eq!(s.model.get("runtime_reset"), 0);
        assert_eq!(s.model.get("m_axi_awvalid"), 1);
        memory.borrow_mut().control.hold_aw = false;
        s.until(
            20_000,
            "external retirement before fault recovery reset",
            |m| m.get("recovery_state") == 3,
        );
        s.until(
            8192,
            "fresh initial capture with old cause R/recovery B held",
            |m| m.get("run_ready") != 0,
        );
        assert!(memory.borrow().writes() > 0);
        assert_eq!(s.model.get("runtime_epoch"), 2);
        assert_eq!(s.model.get("sync_frame"), 0);
        assert_eq!(
            first_fault(&s),
            saved,
            "reset/epoch advance is not a new event"
        );
        release_read(&mut s, cause as u32);
        release_write(&mut s);
        assert_eq!(read(&mut s, REG_FAULT_EPOCH), 1);
        assert_eq!(read(&mut s, REG_FAULT_CAUSE), cause as u32);
        assert_eq!(read_hold(&mut s, REG_FAULT_CAUSE), cause as u32);
        s.model.set("watchdog_fault", 1);
        s.idle(1);
        s.model.set("watchdog_fault", 0);
        s.idle(4);
        assert_eq!(s.model.get("first_fault_epoch"), 2);
        assert_eq!(s.model.get("first_fault_cause"), 32);
        assert_eq!(s.model.get("first_fault_frame"), 0);
        release_read(&mut s, cause as u32);
        assert_eq!(read(&mut s, REG_FAULT_CAUSE), 32);
    }
}

#[shoop_test(no_wasm = "Native reset-owned incarnation authority and held AXI transport")]
fn reset_owned_mmio_authority_ignores_boot_seed_changes_and_keeps_copied_responses() {
    let root = rt_fpga_testbench::root();
    let compiled = design()
        .build(&root.join("work/marlin-suite/semantic-mmio-audio"))
        .unwrap();
    let mut s = Sim::new(compiled.session(None).unwrap(), &[("clk", 10_000, 0)]);
    let geometry = Geometry::new([8, 64, 32], 2);
    let memory = Memory::attach(
        &mut s,
        Mode::Fixed,
        geometry.words() + 128,
        0x0100_0000,
        &root.join("work/marlin-suite/semantic-mmio-audio/memory"),
        94,
    )
    .unwrap();
    memory.borrow_mut().control.stalls = true;
    reset(&mut s);
    s.model.set("epoch", 99);
    s.idle(4);
    assert_eq!(s.model.get("runtime_epoch"), 1);
    assert_eq!(read(&mut s, REG_EPOCH), 1);
    for address in [REG_ABORT, REG_RECOVERY_REQUEST] {
        write(&mut s, address, 99);
        assert_eq!(read(&mut s, REG_SUBMISSION_RESULT), RECEIPT_EPOCH);
        assert_eq!(s.model.get("faulted"), 0);
        assert_eq!(s.model.get("recovery_state"), 0);
    }
    let mut command = [0; 12];
    command[0] = 6;
    command[4] = 4;
    publish_hold(&mut s, 0, 1, 0, &[command]);
    quantum(&mut s, 0, 0, &mut vec![]);
    assert_eq!(s.model.get("runtime_epoch"), 1);
    assert_eq!(read(&mut s, REG_RESULT_READY), 1);
    release_write(&mut s);
    assert_eq!(read_hold(&mut s, REG_EPOCH), 1);
    write_hold(&mut s, REG_RECOVERY_REQUEST, 1);
    let mut changes = 0;
    let mut reset_clocks = 0;
    let mut ready = false;
    for _ in 0..10_000 {
        let previous_epoch = s.model.get("runtime_epoch");
        if s.model.get("runtime_reset") != 0 {
            reset_clocks += 1;
        }
        s.idle(1);
        let epoch = s.model.get("runtime_epoch");
        if epoch != previous_epoch {
            assert_eq!((previous_epoch, epoch), (1, 2));
            assert_eq!(
                s.model.get("runtime_reset"),
                1,
                "runtime epoch must change under reset"
            );
            changes += 1;
        }
        assert_eq!(s.model.get("s_axi_rvalid"), 1);
        assert_eq!(s.model.get("s_axi_rdata"), 1);
        assert_eq!(s.model.get("s_axi_bvalid"), 1);
        assert_eq!(s.model.get("s_axi_bresp"), 0);
        if epoch == 2 && s.model.get("recovery_state") == 3 && s.model.get("run_ready") != 0 {
            ready = true;
            break;
        }
    }
    assert!(ready);
    assert_eq!(changes, 1);
    assert!(reset_clocks >= 2);
    assert_eq!(s.model.get("sync_frame"), 0);
    assert_eq!(s.model.get("last_admitted"), 0);
    release_read(&mut s, 1);
    release_write(&mut s);
    assert_eq!(read(&mut s, REG_EPOCH), 2);
    assert_eq!(read(&mut s, REG_COMPLETED_QUANTA), 0);
    assert_eq!(read(&mut s, REG_FAULT_EPOCH), 1);
    assert_eq!(read(&mut s, REG_FAULT_CAUSE), 0x90);
    assert_eq!(read(&mut s, REG_FAULT_FRAME_LOW), 32);
    s.model.set_all(&[("reset", 1), ("epoch", 0)]);
    s.idle(2);
    s.model.set("reset", 0);
    s.idle(32);
    s.model.set("epoch", 99);
    s.idle(64);
    assert_eq!(s.model.get("runtime_epoch"), 0);
    assert_eq!(s.model.get("run_ready"), 0);
    assert_eq!(read(&mut s, REG_SNAPSHOT_READY), 0);
    s.model.set("reset", 1);
    s.idle(2);
    s.model.set("reset", 0);
    wait_initial(&mut s);
    assert_eq!(s.model.get("runtime_epoch"), 99);
    assert_eq!(read(&mut s, REG_EPOCH), 99);
    assert_eq!(read(&mut s, REG_FAULT_EPOCH), 0);
}
