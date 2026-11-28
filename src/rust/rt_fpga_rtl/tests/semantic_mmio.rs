#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use rt_fpga_rtl::{host::semantic_registers::*, model::Design, sim::Sim};
use shoop_wasm_test_support::shoop_test;

#[path = "support/semantic_trace.rs"]
mod trace;

fn run(test: impl FnOnce(&mut Sim<'_>)) {
    run_profile(false, test);
}
fn run_profile(trace: bool, test: impl FnOnce(&mut Sim<'_>)) {
    let root = rt_fpga_rtl::root();
    let rtl = root.join("fpga/host_bridge/rtl");
    let support = root.join("src/rust/rt_fpga_rtl/tests/support");
    let design = Design::new(
        "semantic_mmio_probe",
        vec![
            rtl.join("semantic_registers.sv"),
            rtl.join("fifo_mmio.sv"),
            rtl.join("fifo_group_banks.sv"),
            rtl.join("fifo_edit_kernel.sv"),
            rtl.join("fifo_group_consumer.sv"),
            rtl.join("fifo_snapshots.sv"),
            support.join("semantic_group_probe.sv"),
            support.join("semantic_mmio_probe.sv"),
        ],
    )
    .parameters(&[("TRACE_REGISTERS", u64::from(trace))])
    .build(&root.join("work/cmd-fifo-mmio"))
    .unwrap();
    let mut sim = Sim::new(design.session(None).unwrap(), &[("clk", 10_000, 0)]);
    sim.max_ps = 100_000_000_000;
    sim.model.set_all(&[
        ("reset", 1),
        ("runtime_reset", 0),
        ("epoch", 1),
        ("s_axi_awvalid", 0),
        ("s_axi_wvalid", 0),
        ("s_axi_bready", 0),
        ("s_axi_arvalid", 0),
        ("s_axi_rready", 0),
        ("boundary_valid", 0),
        ("render_ready", 1),
        ("handoff_valid", 0),
        ("handoff_error", 0),
        ("pipeline_fault", 0),
        ("diagnostic_sequence", 0x1122334455667788),
        ("capture_valid", 0),
        ("capture_quantum", 32),
        ("trace_valid", 0),
        ("trace_epoch", 1),
    ]);
    sim.idle(2);
    sim.model.set("reset", 0);
    sim.idle(2);
    test(&mut sim);
}
fn channel(sim: &mut Sim<'_>, valid: &str, ready: &str) {
    sim.model.set(valid, 1);
    assert_eq!(sim.tick(&[ready]).0.get(ready), 1, "{ready}");
    sim.model.set(valid, 0);
}
fn aw(sim: &mut Sim<'_>, address: u32) {
    sim.model.set("s_axi_awaddr", u64::from(address));
    channel(sim, "s_axi_awvalid", "s_axi_awready");
    sim.model.set("s_axi_awaddr", 0xfff);
}
fn w(sim: &mut Sim<'_>, value: u32, strobes: u64) {
    sim.model
        .set_all(&[("s_axi_wdata", u64::from(value)), ("s_axi_wstrb", strobes)]);
    channel(sim, "s_axi_wvalid", "s_axi_wready");
    sim.model.set("s_axi_wdata", 0xdeadbeef);
}
fn wait(sim: &mut Sim<'_>, signal: &str) {
    for _ in 0..16 {
        if sim.model.get(signal) != 0 {
            return;
        }
        sim.idle(1);
    }
    panic!("MMIO response watchdog: {signal}");
}
fn finish_write(sim: &mut Sim<'_>) -> u64 {
    wait(sim, "s_axi_bvalid");
    let code = sim.model.get("s_axi_bresp");
    sim.idle(3);
    assert_eq!(sim.model.get("s_axi_bvalid"), 1);
    assert_eq!(sim.model.get("s_axi_bresp"), code);
    assert_eq!(sim.model.get("s_axi_awready"), 0);
    assert_eq!(sim.model.get("s_axi_wready"), 0);
    sim.model.set("s_axi_bready", 1);
    sim.idle(1);
    sim.model.set("s_axi_bready", 0);
    code
}
fn write_word(sim: &mut Sim<'_>, address: u32, value: u32, strobes: u64) -> u64 {
    if address & 4 == 0 {
        aw(sim, address);
        w(sim, value, strobes);
    } else {
        w(sim, value, strobes);
        aw(sim, address);
    }
    finish_write(sim)
}
fn write(sim: &mut Sim<'_>, address: u32, value: u32) {
    assert_eq!(write_word(sim, address, value, 15), 0, "write {address:x}");
}
fn ar(sim: &mut Sim<'_>, address: u32) {
    sim.model.set("s_axi_araddr", u64::from(address));
    channel(sim, "s_axi_arvalid", "s_axi_arready");
    sim.model.set("s_axi_araddr", 0xfff);
}
fn finish_read(sim: &mut Sim<'_>) -> (u64, u32) {
    wait(sim, "s_axi_rvalid");
    let code = sim.model.get("s_axi_rresp");
    let data = sim.model.get("s_axi_rdata") as u32;
    sim.idle(3);
    assert_eq!(sim.model.get("s_axi_rvalid"), 1);
    assert_eq!(sim.model.get("s_axi_rresp"), code);
    assert_eq!(sim.model.get("s_axi_rdata"), u64::from(data));
    assert_eq!(sim.model.get("s_axi_arready"), 0);
    sim.model.set("s_axi_rready", 1);
    sim.idle(1);
    sim.model.set("s_axi_rready", 0);
    (code, data)
}
fn read(sim: &mut Sim<'_>, address: u32) -> u32 {
    ar(sim, address);
    let (code, value) = finish_read(sim);
    assert_eq!(code, 0, "read {address:x}");
    value
}
fn packet(sim: &Sim<'_>, sequence: u64, revision: u64, records: &[[u32; 12]]) -> Vec<u32> {
    let mut words = vec![0; 16];
    words[0] = 0x53434631;
    words[1] = 16 | ((records.len() as u32) << 16);
    words[2] = sim.model.get("epoch") as u32;
    words[4] = sequence as u32;
    words[5] = (sequence >> 32) as u32;
    words[6] = revision as u32;
    words[7] = (revision >> 32) as u32;
    words[8] = revision as u32;
    words[9] = (revision >> 32) as u32;
    for record in records {
        words.extend(record);
    }
    words
}
fn refusal() -> [u32; 12] {
    let mut record = [0; 12];
    record[0] = 6;
    record[4] = 4;
    record
}
fn begin_write_only(sim: &mut Sim<'_>, bank: u32) {
    write(sim, REG_COMMAND_BANK, bank);
    let epoch = sim.model.get("epoch") as u32;
    write(sim, REG_COMMAND_BEGIN, epoch);
}
fn begin(sim: &mut Sim<'_>, bank: u32) {
    begin_write_only(sim, bank);
    assert_eq!(read(sim, REG_COMMAND_RECEIPT), RECEIPT_ACCEPTED);
}
fn publish_write_only(
    sim: &mut Sim<'_>,
    bank: u32,
    sequence: u64,
    revision: u64,
    records: &[[u32; 12]],
) {
    let words = packet(sim, sequence, revision, records);
    begin_write_only(sim, bank);
    for word in words {
        write(sim, REG_COMMAND_DATA, word);
    }
    let epoch = sim.model.get("epoch") as u32;
    write(sim, REG_COMMAND_PUBLISH, epoch);
    assert_eq!(sim.model.get("last_admitted"), sequence);
}
fn publish(sim: &mut Sim<'_>, bank: u32, sequence: u64, revision: u64, records: &[[u32; 12]]) {
    publish_write_only(sim, bank, sequence, revision, records);
    assert_eq!(read(sim, REG_COMMAND_RECEIPT), RECEIPT_ACCEPTED);
}
fn quantum(sim: &mut Sim<'_>, frame: u64) {
    let before = sim.model.get("renders");
    assert_eq!(sim.model.get("boundary_ready"), 1);
    sim.model
        .set_all(&[("boundary_valid", 1), ("boundary_frame", frame)]);
    sim.idle(1);
    sim.model.set("boundary_valid", 0);
    for _ in 0..4096 {
        if sim.model.get("renders") != before {
            break;
        }
        sim.idle(1);
    }
    assert_eq!(sim.model.get("faulted"), 0);
    assert_eq!(sim.model.get("renders"), before + 1);
    sim.idle(2);
}
fn ack_write_only(sim: &mut Sim<'_>, snapshot: bool, sequence: u64, epoch: u32) -> u32 {
    let (low, high, authority, doorbell, receipt) = if snapshot {
        (
            REG_SNAPSHOT_ACK_LOW,
            REG_SNAPSHOT_ACK_HIGH,
            REG_SNAPSHOT_ACK_EPOCH,
            REG_SNAPSHOT_ACK,
            REG_SNAPSHOT_RECEIPT,
        )
    } else {
        (
            REG_RESULT_ACK_LOW,
            REG_RESULT_ACK_HIGH,
            REG_RESULT_ACK_EPOCH,
            REG_RESULT_ACK,
            REG_RESULT_RECEIPT,
        )
    };
    write(sim, low, sequence as u32);
    write(sim, high, (sequence >> 32) as u32);
    write(sim, authority, epoch);
    write(sim, doorbell, 1);
    receipt
}
fn ack(sim: &mut Sim<'_>, snapshot: bool, sequence: u64, epoch: u32) -> u32 {
    let receipt = ack_write_only(sim, snapshot, sequence, epoch);
    read(sim, receipt)
}
fn capture(sim: &mut Sim<'_>, frame: u64, revision: u64, initial: bool) {
    let before = sim.model.get("snapshot_last_sequence");
    sim.model.set_all(&[
        ("capture_valid", 1),
        ("capture_initial", u64::from(initial)),
        ("capture_frame", frame),
        ("capture_revision", revision),
    ]);
    for _ in 0..8192 {
        sim.model.eval();
        if sim.model.get("capture_ready") != 0 {
            break;
        }
        sim.idle(1);
    }
    assert_eq!(sim.model.get("snapshot_last_sequence"), before + 1);
    assert_eq!(sim.model.get("capture_ready"), 1);
    assert_eq!(sim.model.get("snapshot_fault"), 0);
    sim.model.set("capture_valid", 0);
    sim.idle(2);
}

#[shoop_test]
fn indexed_packet_mmio_preserves_every_prefix_and_runs_with_a_held_publish_response() {
    run(|sim| {
        assert_eq!(rt_fpga_rtl::host::registers::HOST_VERSION, 15);
        assert_eq!(SEMANTIC_VERSION, 16);
        for (register, value) in [
            (REG_SEMANTIC_ID, SEMANTIC_ID),
            (REG_SEMANTIC_CAPABILITIES, 0x7f),
            (REG_GROUP_BANKS, 4),
            (REG_COMMANDS_PER_GROUP, 32),
            (REG_SNAPSHOT_LOOP_LIMIT, 128),
            (REG_OBSERVATION_ROW_WORDS, 19),
            (REG_COMMAND_CYCLE_LIMIT, 4096),
            (REG_SNAPSHOT_CYCLE_LIMIT, 8192),
            (REG_QUANTUM_CYCLE_LIMIT, 60000),
            (REG_FAULT_SEQUENCE_HIGH, 0x11223344),
        ] {
            assert_eq!(read(sim, register), value);
        }
        assert_eq!(read(sim, REG_FREE_BANKS), 15);
        begin(sim, 3);
        assert_eq!(write_word(sim, REG_COMMAND_DATA, 0x53434631, 3), 0);
        assert_eq!(read(sim, REG_COMMAND_RECEIPT), RECEIPT_INCOMPLETE);
        assert_eq!(read(sim, REG_COMMAND_WRITTEN), 0);
        let words = packet(sim, 1, 0, &[refusal(); 32]);
        for (index, word) in words.into_iter().enumerate() {
            write(sim, REG_COMMAND_PUBLISH, 1);
            assert_eq!(
                read(sim, REG_COMMAND_RECEIPT),
                if index < 2 {
                    RECEIPT_INVALID
                } else {
                    RECEIPT_INCOMPLETE
                }
            );
            write(sim, REG_COMMAND_DATA, word);
            assert_eq!(read(sim, REG_COMMAND_WRITTEN), index as u32 + 1);
            if index % 40 == 0 {
                quantum(sim, index as u64 * 32);
                assert_eq!(read(sim, REG_GROUPS_ATTEMPTED_LOW), 0);
            }
        }
        aw(sim, REG_COMMAND_PUBLISH);
        w(sim, 1, 15);
        wait(sim, "s_axi_bvalid");
        quantum(sim, 16384);
        assert_eq!(sim.model.get("s_axi_bvalid"), 1);
        assert_eq!(sim.model.get("s_axi_bresp"), 0);
        assert_eq!(finish_write(sim), 0);
        assert_eq!(read(sim, REG_RESULT_READY), 8);
        assert_eq!(read(sim, REG_GROUPS_ATTEMPTED_LOW), 1);
        assert_eq!(read(sim, REG_LAST_ADMITTED_LOW), 1);
        assert_eq!(read(sim, REG_NEXT_SEQUENCE_LOW), 2);
        assert_eq!(read(sim, REG_LAST_CUTOFF_LOW), 1);
        write(sim, REG_RESULT_BANK, 3);
        let result: Vec<_> = (0..48).map(|_| read(sim, REG_RESULT_DATA)).collect();
        assert_eq!(result[0], 0x53465231);
        assert_eq!(result[1], 16 | (32 << 16));
        assert_eq!(result[4], 1);
        assert_eq!(result[6], 16384);
        assert_eq!(result[12], 1);
        assert!(result[16..].iter().all(|&code| code == 4));
        for _ in 0..70 {
            assert_eq!(read(sim, REG_RESULT_DATA), 0);
            assert_eq!(read(sim, REG_RESULT_RECEIPT), RECEIPT_UNAVAILABLE);
        }
        write(sim, REG_RESULT_REWIND, 1);
        assert_eq!(read(sim, REG_RESULT_DATA), 0x53465231);
        assert_eq!(ack(sim, false, 1, 1), RECEIPT_ACCEPTED);
        assert_eq!(ack(sim, false, 1, 1), RECEIPT_ACCEPTED);
        assert_eq!(read(sim, REG_FREE_BANKS), 15);
        for (address, value, strobes) in [
            (REG_SEMANTIC_ID, 0, 15),
            (REG_COMMAND_BANK, 4, 15),
            (REG_RESULT_BANK, 0x10000, 15),
            (REG_SNAPSHOT_BANK, 2, 15),
            (REG_RESULT_REWIND, 0, 15),
            (REG_SNAPSHOT_REWIND, 2, 15),
            (REG_RESULT_ACK, 2, 15),
            (REG_SNAPSHOT_ACK, 1, 3),
            (REG_COMMAND_BEGIN, 1, 0),
            (REG_COMMAND_DATA + 1, 0, 15),
            (0xffc, 0, 15),
        ] {
            assert_eq!(write_word(sim, address, value, strobes), 2);
        }
        for address in [REG_SEMANTIC_ID + 1, REG_COMMAND_BEGIN, 0x184, 0xffc, 4] {
            ar(sim, address);
            assert_eq!(finish_read(sim).0, 2);
        }
    });
}

#[shoop_test]
fn runtime_reset_preserves_axi_responses_and_rejects_cross_incarnation_write_halves() {
    for address_first in [false, true] {
        run(|sim| {
            begin(sim, 0);
            write(sim, REG_COMMAND_DATA, 0x53434631);
            if address_first {
                aw(sim, REG_COMMAND_DATA);
            } else {
                w(sim, 0x53434631, 15);
            }
            sim.model.set_all(&[("runtime_reset", 1), ("epoch", 2)]);
            sim.idle(3);
            sim.model.set("runtime_reset", 0);
            sim.idle(1);
            if address_first {
                w(sim, 0x53434631, 15);
            } else {
                aw(sim, REG_COMMAND_DATA);
            }
            assert_eq!(finish_write(sim), 0);
            assert_eq!(read(sim, REG_COMMAND_RECEIPT), RECEIPT_EPOCH);
            assert_eq!(read(sim, REG_COMMAND_WRITTEN), 0);
            assert_eq!(read(sim, REG_FREE_BANKS), 15);
            begin(sim, 1);
            ar(sim, REG_FAULT_SEQUENCE_HIGH);
            wait(sim, "s_axi_rvalid");
            let copied = sim.model.get("s_axi_rdata");
            aw(sim, REG_COMMAND_BEGIN);
            w(sim, 2, 15);
            wait(sim, "s_axi_bvalid");
            sim.model.set_all(&[
                ("runtime_reset", 1),
                ("epoch", 3),
                ("diagnostic_sequence", 0xaabbccddeeff0011),
            ]);
            sim.idle(3);
            assert_eq!(sim.model.get("s_axi_rvalid"), 1);
            assert_eq!(sim.model.get("s_axi_rdata"), copied);
            assert_eq!(sim.model.get("s_axi_bvalid"), 1);
            assert_eq!(sim.model.get("s_axi_bresp"), 0);
            sim.model.set("runtime_reset", 0);
            sim.idle(2);
            assert_eq!(finish_read(sim), (0, copied as u32));
            assert_eq!(finish_write(sim), 0);
            assert_eq!(read(sim, REG_COMMAND_BANK), 0);
            assert_eq!(read(sim, REG_COMMAND_WRITTEN), 0);
            assert_eq!(read(sim, REG_COMMAND_RECEIPT), RECEIPT_EPOCH);
            begin(sim, 2);
            assert_eq!(read(sim, REG_COMMAND_BANK_STATE), 1);
            assert_eq!(read(sim, REG_FAULT_SEQUENCE_HIGH), 0xaabbccdd);
            sim.model.set("epoch", 4);
            sim.idle(3);
            write(sim, REG_COMMAND_BEGIN, 4);
            assert_eq!(read(sim, REG_COMMAND_RECEIPT), RECEIPT_EPOCH);
            assert_eq!(read(sim, REG_NEXT_SEQUENCE_LOW), 1);
            assert_eq!(sim.model.get("renders"), 0);
        });
    }
}

#[shoop_test]
fn runtime_reset_refuses_pending_packet_and_fill_callbacks_without_dropping_axi_owners() {
    for snapshot in [false, true] {
        run(|sim| {
            let (data, receipt, ready) = if snapshot {
                capture(sim, 0, 0, true);
                (REG_SNAPSHOT_DATA, REG_SNAPSHOT_RECEIPT, REG_SNAPSHOT_READY)
            } else {
                publish(sim, 0, 1, 0, &[refusal()]);
                quantum(sim, 0);
                (REG_RESULT_DATA, REG_RESULT_RECEIPT, REG_RESULT_READY)
            };
            write(sim, REG_COMMAND_BANK, 1);
            aw(sim, REG_COMMAND_BEGIN);
            w(sim, 1, 15);
            ar(sim, data);
            assert_eq!(sim.model.get("s_axi_rvalid"), 0);
            assert_eq!(sim.model.get("s_axi_bvalid"), 0);
            sim.model.set_all(&[("runtime_reset", 1), ("epoch", 2)]);
            sim.idle(2);
            assert_eq!(sim.model.get("s_axi_rvalid"), 1);
            assert_eq!(sim.model.get("s_axi_rresp"), 0);
            assert_eq!(sim.model.get("s_axi_rdata"), 0);
            assert_eq!(sim.model.get("s_axi_bvalid"), 1);
            assert_eq!(finish_write(sim), 0);
            assert_eq!(finish_read(sim), (0, 0));
            sim.model.set("runtime_reset", 0);
            sim.idle(2);
            assert_eq!(read(sim, receipt), RECEIPT_EPOCH);
            assert_eq!(read(sim, REG_COMMAND_RECEIPT), RECEIPT_EPOCH);
            assert_eq!(read(sim, ready), 0);
            assert_eq!(read(sim, REG_COMMAND_BANK), 0);
            assert_eq!(read(sim, REG_COMMAND_BANK_STATE), 0);
            assert_eq!(read(sim, REG_COMMAND_WRITTEN), 0);
            assert_eq!(ack(sim, snapshot, 1, 1), RECEIPT_EPOCH);
            assert_eq!(sim.model.get("renders"), 0);
        });
    }
}

#[shoop_test]
fn result_reads_keep_accepted_owner_across_selection_ack_reuse_and_delayed_fence() {
    run(|sim| {
        let mut switch = [0; 12];
        switch[0] = 5;
        switch[6..11].copy_from_slice(&[1, 0, 1 << 16, 1 | (1 << 16), 1]);
        publish(sim, 3, 1, 0, &[switch]);
        for (bank, sequence) in [(1, 2), (2, 3), (0, 4)] {
            publish(sim, bank, sequence, 1, &[refusal()]);
        }
        quantum(sim, 0);
        assert_eq!(sim.model.get("active_revision"), 1);
        assert_eq!(sim.model.get("handoff_pending"), 1);
        quantum(sim, 32);
        assert_eq!(read(sim, REG_RESULT_READY), 0);
        write(sim, REG_RESULT_BANK, 1);
        assert_eq!(read(sim, REG_RESULT_DATA), 0);
        assert_eq!(read(sim, REG_RESULT_RECEIPT), RECEIPT_UNAVAILABLE);
        assert_eq!(ack(sim, false, 2, 1), RECEIPT_UNAVAILABLE);
        sim.model.set_all(&[
            ("handoff_valid", 1),
            ("handoff_epoch", 1),
            ("handoff_sequence", 1),
        ]);
        sim.idle(1);
        sim.model.set("handoff_valid", 0);
        sim.idle(4);
        assert_eq!(read(sim, REG_RESULT_READY), 8);
        write(sim, REG_RESULT_BANK, 3);
        aw(sim, REG_RESULT_BANK);
        w(sim, 1, 15);
        ar(sim, REG_RESULT_DATA);
        assert_eq!(finish_write(sim), 0);
        wait(sim, "s_axi_rvalid");
        assert_eq!(sim.model.get("s_axi_rdata"), 0x53465231);
        assert_eq!(sim.model.get("s_axi_rvalid"), 1);
        write(sim, REG_RESULT_BANK, 3);
        ack_write_only(sim, false, 1, 1);
        assert_eq!(sim.model.get("result_ready_mask"), 2);
        publish_write_only(sim, 3, 5, 1, &[refusal()]);
        for frame in [64, 96, 128] {
            quantum(sim, frame);
        }
        assert_eq!(sim.model.get("s_axi_rdata"), 0x53465231);
        assert_eq!(sim.model.get("s_axi_rvalid"), 1);
        assert_eq!(finish_read(sim), (0, 0x53465231));
        assert_eq!(read(sim, REG_RESULT_RECEIPT), RECEIPT_ACCEPTED);
        assert_eq!(read(sim, REG_COMMAND_RECEIPT), RECEIPT_ACCEPTED);
        assert_eq!(read(sim, REG_RESULT_READY), 2);
        write(sim, REG_RESULT_BANK, 1);
        assert_eq!(read(sim, REG_RESULT_DATA), 0x53465231);
        aw(sim, REG_RESULT_REWIND);
        w(sim, 1, 15);
        ar(sim, REG_RESULT_DATA);
        assert_eq!(finish_write(sim), 0);
        assert_eq!(finish_read(sim).0, 0);
        assert_eq!(read(sim, REG_RESULT_DATA), 0x53465231);
        assert_eq!(read(sim, REG_GROUPS_ATTEMPTED_LOW), 5);
        assert_eq!(sim.model.get("faulted"), 0);
    });
}

#[shoop_test]
fn faulted_mmio_keeps_complete_packets_readable_and_refuses_effects_without_axi_errors() {
    run(|sim| {
        publish(sim, 0, 1, 0, &[refusal()]);
        quantum(sim, 0);
        capture(sim, 0, 0, true);
        let renders = sim.model.get("renders");
        sim.model.set("pipeline_fault", 1);
        sim.idle(3);
        assert_eq!(sim.model.get("faulted"), 1);
        assert_eq!(sim.model.get("boundary_ready"), 0);
        write(sim, REG_RESULT_BANK, 0);
        write(sim, REG_SNAPSHOT_BANK, 0);
        assert_eq!(read(sim, REG_RESULT_DATA), 0x53465231);
        assert_eq!(read(sim, REG_SNAPSHOT_DATA), 0x53465331);
        assert_eq!(ack(sim, false, 1, 1), RECEIPT_FAULT);
        assert_eq!(ack(sim, true, 1, 1), RECEIPT_FAULT);
        assert_eq!(read(sim, REG_RESULT_READY), 1);
        assert_eq!(read(sim, REG_SNAPSHOT_READY), 1);
        write(sim, REG_COMMAND_BANK, 1);
        for register in [REG_COMMAND_BEGIN, REG_COMMAND_DATA, REG_COMMAND_PUBLISH] {
            write(sim, register, 1);
            assert_eq!(read(sim, REG_COMMAND_RECEIPT), RECEIPT_FAULT);
        }
        assert_eq!(read(sim, REG_COMMAND_BANK_STATE), 0);
        assert_eq!(read(sim, REG_COMMAND_WRITTEN), 0);
        assert_eq!(read(sim, REG_LAST_ADMITTED_LOW), 1);
        assert_eq!(sim.model.get("renders"), renders);
    });
}

#[shoop_test]
fn concurrent_packet_reads_do_not_hide_refused_acknowledgements() {
    for snapshot in [false, true] {
        run(|sim| {
            let (bank, low, high, authority, doorbell, data, receipt, ready, magic) = if snapshot {
                capture(sim, 0, 0, true);
                (
                    REG_SNAPSHOT_BANK,
                    REG_SNAPSHOT_ACK_LOW,
                    REG_SNAPSHOT_ACK_HIGH,
                    REG_SNAPSHOT_ACK_EPOCH,
                    REG_SNAPSHOT_ACK,
                    REG_SNAPSHOT_DATA,
                    REG_SNAPSHOT_RECEIPT,
                    REG_SNAPSHOT_READY,
                    0x53465331,
                )
            } else {
                publish(sim, 0, 1, 0, &[refusal()]);
                quantum(sim, 0);
                (
                    REG_RESULT_BANK,
                    REG_RESULT_ACK_LOW,
                    REG_RESULT_ACK_HIGH,
                    REG_RESULT_ACK_EPOCH,
                    REG_RESULT_ACK,
                    REG_RESULT_DATA,
                    REG_RESULT_RECEIPT,
                    REG_RESULT_READY,
                    0x53465231,
                )
            };
            write(sim, bank, 0);
            write(sim, low, 1);
            write(sim, high, 0);
            write(sim, authority, 2);
            aw(sim, doorbell);
            w(sim, 1, 15);
            ar(sim, data);
            assert_eq!(finish_write(sim), 0);
            assert_eq!(finish_read(sim), (0, magic));
            assert_eq!(read(sim, receipt), RECEIPT_EPOCH);
            assert_eq!(read(sim, ready), 1);
            write(sim, authority, 1);
            aw(sim, doorbell);
            w(sim, 1, 15);
            ar(sim, data);
            assert_eq!(finish_write(sim), 0);
            assert_eq!(
                finish_read(sim),
                (0, if snapshot { 16 } else { 16 | (1 << 16) })
            );
            assert_eq!(read(sim, receipt), RECEIPT_ACCEPTED);
            assert_eq!(read(sim, ready), 0);
        });
    }
}

#[shoop_test]
fn snapshot_mmio_copies_all_128_rows_and_reuses_acked_storage_while_r_is_held() {
    run(|sim| {
        capture(sim, 0, 0, true);
        capture(sim, 32, 5, false);
        capture(sim, 64, 5, false);
        assert_eq!(read(sim, REG_SNAPSHOT_READY), 3);
        assert_eq!(read(sim, REG_SNAPSHOT_SKIPPED_LOW), 1);
        write(sim, REG_SNAPSHOT_BANK, 0);
        assert_eq!(read(sim, REG_SNAPSHOT_DATA), 0x53465331);
        ar(sim, REG_SNAPSHOT_DATA);
        wait(sim, "s_axi_rvalid");
        assert_eq!(sim.model.get("s_axi_rdata"), 16);
        ack_write_only(sim, true, 1, 1);
        assert_eq!(sim.model.get("snapshot_ready_mask"), 2);
        capture(sim, 96, 6, false);
        ack_write_only(sim, true, 1, 1);
        assert_eq!(sim.model.get("snapshot_ready_mask"), 3);
        assert_eq!(sim.model.get("s_axi_rdata"), 16);
        assert_eq!(finish_read(sim), (0, 16));
        assert_eq!(read(sim, REG_SNAPSHOT_RECEIPT), RECEIPT_OLD_SEQUENCE);
        write(sim, REG_SNAPSHOT_BANK, 0);
        let mut expected = vec![
            0x53465331,
            16 | (1 << 16),
            1,
            32,
            4,
            0,
            96,
            0,
            6,
            0,
            128,
            19,
            1,
            0,
            0,
            0,
        ];
        for row in 0..128u32 {
            for word in 0..16u32 {
                let raw = 0x0123456700000000u64 | u64::from(row * 16 + word);
                expected.extend([raw as u32, (raw >> 32) as u32]);
            }
            let pulse = 0x1020304050600000u64 | u64::from(row);
            expected.extend([
                0x3f800000 | row,
                0x70000000 | row,
                pulse as u32,
                (pulse >> 32) as u32,
                1,
                0,
            ]);
        }
        assert_eq!(expected.len(), 4880);
        for (index, word) in expected.into_iter().enumerate() {
            assert_eq!(read(sim, REG_SNAPSHOT_DATA), word, "snapshot word {index}");
        }
        for _ in 0..20 {
            assert_eq!(read(sim, REG_SNAPSHOT_DATA), 0);
        }
        assert_eq!(read(sim, REG_SNAPSHOT_RECEIPT), RECEIPT_UNAVAILABLE);
        write(sim, REG_SNAPSHOT_REWIND, 1);
        assert_eq!(read(sim, REG_SNAPSHOT_DATA), 0x53465331);
        assert_eq!(ack(sim, true, 4, 2), RECEIPT_EPOCH);
        assert_eq!(read(sim, REG_SNAPSHOT_READY), 3);
        assert_eq!(ack(sim, true, 4, 1), RECEIPT_ACCEPTED);
        assert_eq!(read(sim, REG_SNAPSHOT_READY), 2);
        assert_eq!(read(sim, REG_SNAPSHOT_STATUS), 0);
        assert!(read(sim, REG_PEAK_SNAPSHOT_CYCLES) <= 8192);
        assert_eq!(sim.model.get("renders"), 0);
    });
}
