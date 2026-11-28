#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use rt_fpga_rtl::{
    bits::Bits,
    model::{Design, Probe},
    sim::Sim,
};
use shoop_wasm_test_support::shoop_test;

fn run(loops: u64, inject: bool, test: impl FnOnce(&mut Sim<'_>)) {
    let root = rt_fpga_rtl::root();
    let mut design = Design::new(
        "fifo_snapshots",
        vec![root.join("fpga/host_bridge/rtl/fifo_snapshots.sv")],
    )
    .parameters(&[("LOOPS", loops)]);
    if inject {
        design = design.probes(vec![
            Probe::inject("last_sequence"),
            Probe::inject("skipped"),
        ]);
    }
    let compiled = design.build(&root.join("work/fifo-snapshots")).unwrap();
    let mut s = Sim::new(compiled.session(None).unwrap(), &[("clk", 10_000, 0)]);
    reset(&mut s, 1);
    test(&mut s);
}
fn reset(s: &mut Sim<'_>, epoch: u64) {
    s.model.set_all(&[
        ("reset", 1),
        ("epoch", epoch),
        ("fault", 0),
        ("capture_valid", 0),
        ("memory_ready", 1),
        ("memory_response_valid", 0),
        ("memory_response_error", 0),
        ("metadata_ready", 1),
        ("metadata_response_valid", 0),
        ("metadata_response_error", 0),
        ("read_valid", 0),
        ("ack_valid", 0),
    ]);
    s.idle(2);
    s.model.set("reset", 0);
    s.idle(1);
}
fn metadata(row: u64) -> Bits {
    let mut v = Bits::zero(129);
    v.set(0, 32, 0x3f00_0001 + row)
        .set(32, 32, 0xf000_0000 + row)
        .set(64, 64, 0xfedc_ba98_7654_0000 + row)
        .set(128, 1, row & 1);
    v
}
fn step(s: &mut Sim<'_>, base: u64) -> (bool, bool) {
    s.model.eval();
    let memory = s.model.get("memory_valid") != 0 && s.model.get("memory_ready") != 0;
    let address = s.model.get("memory_address");
    let observations = s.model.get("metadata_valid") != 0 && s.model.get("metadata_ready") != 0;
    let row = s.model.get("metadata_slot");
    s.idle(1);
    s.model.set("memory_response_valid", u64::from(memory));
    if memory {
        let mut data = Bits::zero(129);
        data.set(0, 64, base.wrapping_add(address)).set(128, 1, 1);
        s.model.set_bits("memory_read_data", &data);
    }
    s.model
        .set("metadata_response_valid", u64::from(observations));
    if observations {
        s.model.set_bits("metadata_read_data", &metadata(row));
    }
    (memory, observations)
}
fn begin(s: &mut Sim<'_>, initial: bool, frame: u64, revision: u64) {
    s.model.set_all(&[
        ("capture_valid", 1),
        ("capture_initial", u64::from(initial)),
        ("capture_frame", frame),
        ("capture_revision", revision),
        ("capture_quantum", 32),
    ]);
}
fn capture(
    s: &mut Sim<'_>,
    loops: u64,
    base: u64,
    initial: bool,
    frame: u64,
    revision: u64,
    bank: Option<u64>,
) -> u64 {
    begin(s, initial, frame, revision);
    let ready = s.model.get("ready_mask");
    let mut memory_reads = 0;
    let mut metadata_reads = 0;
    let mut elapsed = 0;
    if let Some(bank) = bank {
        s.model.set_all(&[("read_valid", 1), ("read_bank", bank)]);
    }
    for cycle in 0..8194 {
        s.model.eval();
        if s.model.get("capture_ready") != 0 {
            break;
        }
        assert_eq!(
            s.model.get("ready_mask"),
            ready,
            "partial capture became visible"
        );
        if bank.is_some() {
            s.model.set("read_word", cycle % (16 + loops * 38));
        }
        let (m, o) = step(s, base);
        memory_reads += u64::from(m);
        metadata_reads += u64::from(o);
        elapsed += 1;
        if bank.is_some() {
            assert_eq!(s.model.get("response_valid"), 1);
            assert_eq!(
                s.model.get("response_error"),
                1,
                "partial packet read succeeded"
            );
            assert_eq!(s.model.get("read_data"), 0);
        }
        if cycle == 0 {
            s.model.set_all(&[
                ("capture_frame", 999),
                ("capture_revision", 888),
                ("capture_quantum", 7),
            ]);
        }
    }
    assert_eq!(s.model.get("capture_ready"), 1);
    assert_eq!(s.model.get("capture_fault"), 0);
    assert!(elapsed <= 8192);
    assert_eq!(memory_reads, if bank.is_some() { loops * 16 } else { 0 });
    assert_eq!(metadata_reads, if bank.is_some() { loops } else { 0 });
    let sequence = s.model.get("last_sequence");
    s.model.set("read_valid", 0);
    s.idle(20);
    assert_eq!(
        s.model.get("last_sequence"),
        sequence,
        "held capture was retried"
    );
    s.model.set("capture_valid", 0);
    step(s, base);
    elapsed
}
fn read(s: &mut Sim<'_>, bank: u64, word: u64, error: bool) -> u64 {
    s.model
        .set_all(&[("read_valid", 1), ("read_bank", bank), ("read_word", word)]);
    s.idle(1);
    assert_eq!(s.model.get("response_valid"), 1);
    assert_eq!(s.model.get("response_error"), u64::from(error));
    let data = s.model.get("read_data");
    if error {
        assert_eq!(data, 0);
    }
    s.model.set("read_valid", 0);
    s.idle(1);
    data
}
fn packet(s: &mut Sim<'_>, loops: u64, bank: u64, header: &[u64; 8], base: u64) {
    for index in 0..8 + loops * 19 {
        let expected = if index < 8 {
            header[index as usize]
        } else {
            let row = (index - 8) / 19;
            let word = (index - 8) % 19;
            match word {
                0..=15 => base.wrapping_add(row * 16 + word),
                16 => metadata(row).get(0, 64),
                17 => metadata(row).get(64, 64),
                18 => row & 1,
                _ => unreachable!(),
            }
        };
        assert_eq!(
            read(s, bank, index * 2, false),
            expected & 0xffff_ffff,
            "low word {index}"
        );
        assert_eq!(
            read(s, bank, index * 2 + 1, false),
            expected >> 32,
            "high word {index}"
        );
    }
    read(s, bank, 16 + loops * 38, true);
    read(s, bank, 8191, true);
}
fn ack(s: &mut Sim<'_>, bank: u64, epoch: u64, sequence: u64, receipt: u64) {
    s.model.set_all(&[
        ("ack_valid", 1),
        ("ack_bank", bank),
        ("ack_epoch", epoch),
        ("ack_sequence", sequence),
    ]);
    s.idle(1);
    assert_eq!(s.model.get("ack_response_valid"), 1);
    assert_eq!(s.model.get("ack_receipt"), receipt);
    s.model.set("ack_valid", 0);
    s.idle(1);
}

#[shoop_test]
fn complete_128_row_packets_stay_immutable_and_full_readers_only_skip_publication() {
    run(128, false, |s| {
        read(s, 0, 0, true);
        let first = capture(s, 128, 0x1234_5678_9000_0000, true, 0, 17, Some(0));
        assert_eq!(s.model.get("ready_mask"), 1);
        let second = capture(
            s,
            128,
            0x8765_4321_0000_0000,
            false,
            0x100_0000_0000,
            18,
            Some(1),
        );
        assert_eq!(s.model.get("ready_mask"), 3);
        assert_eq!(first, second);
        println!(
            "SNAPSHOT_128_CAPTURE_CYCLES {first}; peak {}",
            s.model.get("peak_cycles")
        );
        for sequence in 3..=9 {
            assert_eq!(capture(s, 128, 0, false, sequence * 32, 19, None), 1);
            assert_eq!(s.model.get("last_sequence"), sequence);
            assert_eq!(s.model.get("skipped"), sequence - 2);
        }
        packet(
            s,
            128,
            0,
            &[
                0x10_5346_5331,
                32u64 << 32 | 1,
                1,
                0,
                17,
                19u64 << 32 | 128,
                0,
                0,
            ],
            0x1234_5678_9000_0000,
        );
        packet(
            s,
            128,
            1,
            &[
                0x1_0010_5346_5331,
                32u64 << 32 | 1,
                2,
                0x100_0000_0000,
                18,
                19u64 << 32 | 128,
                0,
                0,
            ],
            0x8765_4321_0000_0000,
        );
        assert_eq!(s.model.get("last_sequence"), 9);
        ack(s, 0, 1, 1, 0);
        ack(s, 0, 1, 1, 6);
        capture(s, 128, 0x9988_0000_1111_0000, false, 32, 20, Some(0));
        assert_eq!(s.model.get("sequence0"), 10);
        ack(s, 0, 1, 1, 6);
        ack(s, 0, 2, 10, 2);
        ack(s, 0, 1, 11, 4);
        ack(s, 1, 1, 10, 5);
        assert_eq!(s.model.get("ready_mask"), 3);
        packet(
            s,
            128,
            0,
            &[
                0x1_0010_5346_5331,
                32u64 << 32 | 1,
                10,
                32,
                20,
                19u64 << 32 | 128,
                7,
                0,
            ],
            0x9988_0000_1111_0000,
        );
        ack(s, 1, 1, 2, 0);
        ack(s, 0, 1, 10, 0);
        assert_eq!(s.model.get("free_mask"), 3);
    });
}

#[shoop_test]
fn actual_width_exhaustion_and_saturating_skip_counts_never_wrap_or_fault_render_release() {
    run(1, true, |s| {
        capture(s, 1, 1, false, 0, 1, Some(0));
        capture(s, 1, 2, false, 32, 1, Some(1));
        s.model.set_all(&[
            ("probe_skipped_force", 1),
            ("probe_skipped_value", u64::MAX - 1),
        ]);
        s.idle(1);
        s.model.set("probe_skipped_force", 0);
        capture(s, 1, 3, false, 64, 1, None);
        assert_eq!(s.model.get("skipped"), u64::MAX);
        capture(s, 1, 3, false, 96, 1, None);
        assert_eq!(s.model.get("skipped"), u64::MAX);
        ack(s, 0, 1, 1, 0);
        s.model.set_all(&[
            ("probe_last_sequence_force", 1),
            ("probe_last_sequence_value", u64::MAX - 1),
        ]);
        s.idle(1);
        s.model.set("probe_last_sequence_force", 0);
        capture(s, 1, 4, false, 128, 1, Some(0));
        assert_eq!(s.model.get("sequence_exhausted"), 1);
        assert_eq!(s.model.get("last_sequence"), u64::MAX);
        packet(
            s,
            1,
            0,
            &[
                0x1_0010_5346_5331,
                32u64 << 32 | 1,
                u64::MAX,
                128,
                1,
                19u64 << 32 | 1,
                u64::MAX,
                0,
            ],
            4,
        );
        ack(s, 0, 1, u64::MAX, 0);
        for _ in 0..3 {
            assert_eq!(capture(s, 1, 5, false, 160, 1, None), 1);
            assert_eq!(s.model.get("last_sequence"), u64::MAX);
            assert_eq!(s.model.get("ready_mask"), 2);
        }
        ack(s, 1, 1, 2, 0);
        assert_eq!(s.model.get("free_mask"), 3);
    });
}

#[shoop_test]
fn backend_errors_timeout_and_same_edge_pipeline_fault_never_publish_partial_headers() {
    for failure in [
        "memory",
        "metadata",
        "timeout",
        "memory_wait",
        "metadata_ready",
        "metadata_wait",
        "lost_gate",
        "commit",
    ] {
        run(1, false, |s| {
            begin(s, false, 0, 1);
            if failure == "timeout" {
                s.model.set("memory_ready", 0);
            }
            if failure == "metadata_ready" {
                s.model.set("metadata_ready", 0);
            }
            for _ in 0..8200 {
                s.model.eval();
                if failure == "memory" && s.model.get("memory_response_valid") != 0 {
                    s.model.set("memory_response_error", 1);
                }
                if failure == "metadata" && s.model.get("metadata_response_valid") != 0 {
                    s.model.set("metadata_response_error", 1);
                }
                if failure == "commit" && s.model.get("peak_cycles") == 46 {
                    s.model.set("fault", 1);
                }
                if failure == "memory_wait" {
                    s.model.set("memory_response_valid", 0);
                }
                if failure == "metadata_wait" {
                    s.model.set("metadata_response_valid", 0);
                }
                if failure == "lost_gate" && s.model.get("peak_cycles") >= 10 {
                    s.model.set("capture_valid", 0);
                }
                if s.model.get("capture_ready") != 0 {
                    break;
                }
                assert_eq!(s.model.get("ready_mask"), 0);
                step(s, 0x100);
            }
            s.model.eval();
            assert_eq!(s.model.get("capture_ready"), 1, "{failure}");
            assert_eq!(s.model.get("ready_mask"), 0, "{failure}");
            assert_eq!(s.model.get("capture_fault"), u64::from(failure != "commit"));
            if ["timeout", "memory_wait", "metadata_ready", "metadata_wait"].contains(&failure) {
                assert_eq!(s.model.get("peak_cycles"), 8192);
            }
            read(s, 0, 0, true);
            ack(s, 0, 1, 1, 3);
            reset(s, 2);
            assert_eq!(s.model.get("free_mask"), 3);
            assert_eq!(s.model.get("last_sequence"), 0);
            capture(s, 1, 0x200, false, 32, 2, Some(0));
            ack(s, 0, 1, 1, 2);
            assert_eq!(s.model.get("ready_mask"), 1);
            ack(s, 0, 2, 1, 0);
        });
    }
}

#[shoop_test]
fn epoch_flush_discards_inflight_captures_and_copied_reads_survive_matching_release_and_reuse() {
    run(1, false, |s| {
        begin(s, false, 0, 1);
        for _ in 0..20 {
            step(s, 0x100);
        }
        assert_eq!(s.model.get("ready_mask"), 0);
        s.model.set("epoch", 2);
        step(s, 0x100);
        assert_eq!(s.model.get("capture_ready"), 1);
        assert_eq!(s.model.get("free_mask"), 3);
        for _ in 0..20 {
            step(s, 0x100);
        }
        assert_eq!(
            s.model.get("last_sequence"),
            0,
            "old held request replayed into new epoch"
        );
        s.model.set("capture_valid", 0);
        step(s, 0);
        capture(s, 1, 0x1122_3344_5566_7788, false, 32, 2, Some(0));
        s.model.set_all(&[
            ("read_valid", 1),
            ("read_bank", 0),
            ("read_word", 16),
            ("ack_valid", 1),
            ("ack_bank", 0),
            ("ack_epoch", 2),
            ("ack_sequence", 1),
        ]);
        s.idle(1);
        assert_eq!(s.model.get("response_valid"), 1);
        assert_eq!(s.model.get("response_error"), 0);
        assert_eq!(s.model.get("read_data"), 0x5566_7788);
        assert_eq!(s.model.get("ack_receipt"), 0);
        s.model.set_all(&[("read_valid", 0), ("ack_valid", 0)]);
        begin(s, false, 64, 2);
        for _ in 0..100 {
            s.model.eval();
            if s.model.get("capture_ready") != 0 {
                break;
            }
            step(s, 0xaa);
            assert_eq!(
                s.model.get("read_data"),
                0x5566_7788,
                "reader retained packet RAM rather than a copy"
            );
        }
        assert_eq!(s.model.get("capture_ready"), 1);
        assert_eq!(s.model.get("ready_mask"), 1);
        assert_eq!(s.model.get("sequence0"), 2);
        s.model.set("capture_valid", 0);
        step(s, 0xaa);
        packet(
            s,
            1,
            0,
            &[
                0x1_0010_5346_5331,
                32u64 << 32 | 2,
                2,
                64,
                2,
                19u64 << 32 | 1,
                0,
                0,
            ],
            0xaa,
        );
        reset(s, 3);
        assert_eq!(s.model.get("free_mask"), 3);
        assert_eq!(s.model.get("ready_mask"), 0);
        read(s, 0, 0, true);
        read(s, 1, 0, true);
        ack(s, 0, 2, 2, 2);
        capture(s, 1, 0xbb, false, 96, 3, Some(0));
        ack(s, 0, 2, 1, 2);
        assert_eq!(s.model.get("ready_mask"), 1);
        capture(s, 1, 0xcc, false, 128, 3, Some(1));
        assert_eq!(s.model.get("ready_mask"), 3);
        s.model.set("epoch", 4);
        s.idle(1);
        assert_eq!(s.model.get("ready_mask"), 0);
        assert_eq!(s.model.get("free_mask"), 3);
        read(s, 0, 0, true);
        read(s, 1, 0, true);
        ack(s, 1, 3, 2, 2);
    });
}

#[shoop_test]
fn same_edge_full_bank_ack_is_conservative_and_held_ack_cannot_replay_capture() {
    run(1, false, |s| {
        capture(s, 1, 0x1122_3344_aabb_ccdd, false, 0, 1, Some(0));
        capture(s, 1, 0x5566, false, 32, 1, Some(1));
        begin(s, false, 64, 1);
        s.model.set_all(&[
            ("ack_valid", 1),
            ("ack_bank", 0),
            ("ack_epoch", 1),
            ("ack_sequence", 1),
            ("read_valid", 1),
            ("read_bank", 0),
            ("read_word", 16),
        ]);
        assert_eq!(step(s, 0), (false, false));
        assert_eq!(s.model.get("ack_response_valid"), 1);
        assert_eq!(s.model.get("ack_receipt"), 0);
        assert_eq!(s.model.get("response_valid"), 1);
        assert_eq!(s.model.get("response_error"), 0);
        assert_eq!(s.model.get("read_data"), 0xaabb_ccdd);
        assert_eq!(s.model.get("capture_ready"), 1);
        assert_eq!(s.model.get("last_sequence"), 3);
        assert_eq!(s.model.get("skipped"), 1);
        assert_eq!(s.model.get("ready_mask"), 2);
        s.model.set("read_valid", 0);
        s.idle(10);
        assert_eq!(s.model.get("ack_receipt"), 6);
        assert_eq!(s.model.get("last_sequence"), 3);
        assert_eq!(s.model.get("skipped"), 1);
        assert_eq!(s.model.get("read_data"), 0xaabb_ccdd);
        s.model.set_all(&[("ack_valid", 0), ("capture_valid", 0)]);
        step(s, 0);
        capture(s, 1, 0x7788, false, 96, 1, Some(0));
        packet(
            s,
            1,
            0,
            &[
                0x1_0010_5346_5331,
                32u64 << 32 | 1,
                4,
                96,
                1,
                19u64 << 32 | 1,
                1,
                0,
            ],
            0x7788,
        );
        packet(
            s,
            1,
            1,
            &[
                0x1_0010_5346_5331,
                32u64 << 32 | 1,
                2,
                32,
                1,
                19u64 << 32 | 1,
                0,
                0,
            ],
            0x5566,
        );
        ack(s, 0, 1, 0, 4);
        ack(s, 0, 1, 1, 6);
        assert_eq!(s.model.get("ready_mask"), 3);
    });
}

#[cfg(feature = "rt-interface")]
#[shoop_test]
fn host_snapshot_codec_reads_real_initial_and_settled_packets_without_releasing_them() {
    use rt_fpga_rtl::host::semantic::{Snapshot, SnapshotFormat, SnapshotPhase};
    run(128, false, |s| {
        let format = SnapshotFormat::new(1, 32, 128).unwrap();
        for (bank, initial, frame, revision, base) in [
            (0, true, 0, 0, 0xdead_beef_0000_0000),
            (
                1,
                false,
                0xfedc_ba98_7654_3200,
                0x1234_5678_9abc_def0,
                0x8000_0000_0000_0000,
            ),
        ] {
            capture(s, 128, base, initial, frame, revision, Some(bank));
            let words: Vec<_> = (0..format.packet_words())
                .map(|index| read(s, bank, index as u64, false) as u32)
                .collect();
            let snapshot = Snapshot::decode(&words, format).unwrap();
            let header = snapshot.header();
            assert_eq!(
                header.phase,
                if initial {
                    SnapshotPhase::Initial
                } else {
                    SnapshotPhase::Settled
                }
            );
            assert_eq!(header.sequence, bank + 1);
            assert_eq!(header.epoch, 1);
            assert_eq!(header.quantum, 32);
            assert_eq!(header.frame, frame);
            assert_eq!(header.revision, revision);
            assert_eq!(header.skipped, 0);
            for slot in 0..128 {
                let row = snapshot.physical_row(slot).unwrap();
                assert_eq!(
                    row.raw,
                    core::array::from_fn(|index| base + slot as u64 * 16 + index as u64)
                );
                assert_eq!(row.gain_bits, 0x3f00_0001 + slot as u32);
                assert_eq!(row.failed_plans, 0xf000_0000 + slot as u32);
                assert_eq!(row.last_pulse, 0xfedc_ba98_7654_0000 + slot as u64);
                assert_eq!(row.pulse_valid, slot & 1 != 0);
            }
        }
        assert_eq!(
            s.model.get("ready_mask"),
            3,
            "copying is not acknowledgement"
        );
        capture(s, 128, 0, false, 64, 1, None);
        assert_eq!(s.model.get("skipped"), 1);
        assert_eq!(s.model.get("ready_mask"), 3);
    });
}
