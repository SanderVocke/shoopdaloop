#![cfg(feature = "rt-interface")]
use rt_fpga_rtl::host::semantic::{
    BoundTarget, CodecError, CommandRecord, GroupHeader, GroupIdentity, GroupPacket, GroupResult,
    HEADER_WORDS, MAX_COMMANDS, MAX_GROUP_WORDS, ObservationRow, ROW_WORDS, Snapshot,
    SnapshotFormat, SnapshotHeader, SnapshotPhase, SwitchOutcome,
};
use shoop_rt_protocol::immediate::{
    ClearPlanned, Code, IterationTransition, PlannedTransition, SwitchProgram, TransitionMode,
    TransitionPlan,
};
use shoop_wasm_test_support::shoop_test;

fn target() -> Option<BoundTarget> {
    Some(BoundTarget::new(127, u32::MAX, Some(511), true).unwrap())
}
fn header() -> GroupHeader {
    GroupHeader {
        epoch: 0xfedc_ba98,
        sequence: 0x1234_5678_9abc_def0,
        graph: 0x0123_4567_89ab_cdef,
        topology: 0xfedc_ba98_7654_3210,
        deadline: Some(0x8000_0000_0000_0020),
    }
}
fn put(words: &mut [u32], index: usize, value: u64) {
    words[index] = value as u32;
    words[index + 1] = (value >> 32) as u32;
}
fn result_words(identity: GroupIdentity, codes: &[Code]) -> Vec<u32> {
    assert_eq!(identity.count(), codes.len());
    let mut words = vec![0; HEADER_WORDS + codes.len()];
    words[..4].copy_from_slice(&[
        0x5346_5231,
        16 | ((codes.len() as u32) << 16),
        identity.epoch(),
        0,
    ]);
    put(&mut words, 4, identity.sequence());
    put(&mut words, 6, 0xfedc_ba98_7654_3220);
    put(&mut words, 8, 0x1234_5678_9abc_def0);
    put(&mut words, 10, 0x1234_5678_9abc_def0);
    words[12] = Code::Applied as u32;
    for (out, &code) in words[16..].iter_mut().zip(codes) {
        *out = code as u32;
    }
    words
}
fn snapshot_words(rows: usize, initial: bool) -> Vec<u32> {
    let mut words = vec![0; 16 + rows * 38];
    words[..4].copy_from_slice(&[
        0x5346_5331,
        16 | (u32::from(!initial) << 16),
        0xfedc_ba98,
        32,
    ]);
    put(&mut words, 4, if initial { 1 } else { u64::MAX });
    put(
        &mut words,
        6,
        if initial { 0 } else { 0xfedc_ba98_7654_3220 },
    );
    put(
        &mut words,
        8,
        if initial { 0 } else { 0x1234_5678_9abc_def0 },
    );
    words[10] = rows as u32;
    words[11] = 19;
    put(&mut words, 12, if initial { 0 } else { u64::MAX });
    for slot in 0..rows {
        let offset = 16 + slot * 38;
        for index in 0..16 {
            put(
                &mut words,
                offset + index * 2,
                0x8000_0000_0000_0000 | ((slot as u64) << 16) | index as u64,
            );
        }
        words[offset + 32] = 0x3f80_0001 + slot as u32;
        words[offset + 33] = u32::MAX - slot as u32;
        put(&mut words, offset + 34, 0xfedc_ba98_7654_0000 + slot as u64);
        words[offset + 36] = (slot & 1) as u32;
    }
    words
}

#[shoop_test]
fn ordinary_records_preserve_all_argument_bits_and_original_gain() {
    let plan = TransitionPlan {
        now: Some(TransitionMode::Recording),
        first: Some(PlannedTransition {
            mode: TransitionMode::Playing,
            skip: u32::MAX,
        }),
        second: None,
    };
    assert_eq!(
        CommandRecord::transition(target(), plan).words(),
        &[0, 127, u32::MAX, 1, 0, 0, 2, 1, u32::MAX, u32::MAX, 0, 0]
    );
    assert_eq!(
        CommandRecord::transition(target(), TransitionPlan::default()).words()[6..11],
        [u32::MAX, u32::MAX, 0, u32::MAX, 0]
    );
    for mask in [
        ClearPlanned::First,
        ClearPlanned::Second,
        ClearPlanned::Both,
    ] {
        assert_eq!(
            CommandRecord::clear(target(), mask).words(),
            &[1, 127, u32::MAX, 1, 0, 0, mask as u32, 0, 0, 0, 0, 0]
        );
    }
    for enabled in [false, true] {
        assert_eq!(
            CommandRecord::repeat(target(), enabled).words(),
            &[2, 127, u32::MAX, 1, 0, 0, u32::from(enabled), 0, 0, 0, 0, 0]
        );
    }
    for (bits, fixed) in [
        (0x0000_0000, 0),
        (0x8000_0000, 0),
        (1, 0),
        (0x8000_0001, 0),
        (0x3f80_0001, 0x0100_0002),
        (0xbf80_0001, 0xfeff_fffe),
        (0x4300_0000, 0x7fff_ffff),
        (0xc300_0000, 0x8000_0000),
        (0x7f7f_ffff, 0x7fff_ffff),
        (0xff7f_ffff, 0x8000_0000),
    ] {
        assert_eq!(
            CommandRecord::gain(target(), f32::from_bits(bits)).words(),
            &[
                3,
                127 | (511 << 16),
                u32::MAX,
                1,
                0,
                bits,
                fixed,
                0,
                0,
                0,
                0,
                0
            ]
        );
    }
    for retrigger in [false, true] {
        for seek in [false, true] {
            let request = IterationTransition {
                iteration: u64::MAX,
                retrigger,
                seek,
            };
            assert_eq!(
                CommandRecord::iteration(target(), request).words(),
                &[
                    4,
                    127,
                    u32::MAX,
                    1,
                    0,
                    0,
                    u32::MAX,
                    u32::MAX,
                    u32::from(retrigger) | (u32::from(seek) << 1),
                    0,
                    0,
                    0
                ]
            );
        }
    }
    let descriptor = SwitchProgram {
        revision: u64::MAX,
        region: u16::MAX,
        instructions: 1024,
        sync_count: 128,
        states: 512,
        buffers: 512,
    };
    for composites in [false, true] {
        assert_eq!(
            CommandRecord::switch(descriptor, composites).words(),
            &[
                5,
                0,
                0,
                0,
                0,
                0,
                u32::MAX,
                u32::MAX,
                0x0400_ffff,
                0x0200_0080,
                512 | (u32::from(composites) << 16),
                0
            ]
        );
    }
}

#[shoop_test]
fn refusal_and_binding_checks_do_not_alias_or_drop_records() {
    for slot in [128, 65536, u32::MAX] {
        assert_eq!(
            BoundTarget::new(slot, 1, None, false),
            Err(CodecError::Binding)
        );
    }
    assert_eq!(
        BoundTarget::new(0, 0, None, false),
        Err(CodecError::Binding)
    );
    for address in [65536, u32::MAX] {
        assert_eq!(
            BoundTarget::new(0, 1, Some(address), false),
            Err(CodecError::Binding)
        );
    }
    for raw in 0..=29 {
        let code = Code::from_raw(raw).unwrap();
        for (kind, record) in [
            (6, CommandRecord::refused(code)),
            (5, CommandRecord::refused_switch(code)),
        ] {
            if (4..=9).contains(&raw) || (11..=29).contains(&raw) {
                assert_eq!(
                    record.unwrap().words(),
                    &[kind, 0, 0, 0, raw, 0, 0, 0, 0, 0, 0, 0]
                );
            } else {
                assert_eq!(record, Err(CodecError::Refusal));
            }
        }
    }
    let missing_gain = Some(BoundTarget::new(0, 1, None, false).unwrap());
    for binding in [None, missing_gain, target()] {
        for bits in [0x7f80_0000, 0xff80_0000, 0x7fc0_0001, 0xff80_0001] {
            assert_eq!(
                CommandRecord::gain(binding, f32::from_bits(bits)),
                CommandRecord::refused(Code::InvalidValue).unwrap()
            );
        }
    }
    assert_eq!(
        CommandRecord::gain(None, 1.),
        CommandRecord::refused(Code::TargetUnavailable).unwrap()
    );
    assert_eq!(
        CommandRecord::gain(missing_gain, 1.),
        CommandRecord::refused(Code::Unsupported).unwrap()
    );
    assert_eq!(
        CommandRecord::repeat(None, true),
        CommandRecord::refused(Code::TargetUnavailable).unwrap()
    );
    assert_eq!(
        CommandRecord::clear(None, ClearPlanned::Both),
        CommandRecord::refused(Code::TargetUnavailable).unwrap()
    );
}

#[shoop_test]
fn complete_groups_keep_header_identity_order_and_attempt_time_checks() {
    let records = [
        CommandRecord::repeat(target(), false),
        CommandRecord::gain(None, f32::NAN),
        CommandRecord::clear(target(), ClearPlanned::Both),
        CommandRecord::refused_switch(Code::Cancelled).unwrap(),
    ];
    let packet = GroupPacket::new(header(), &records).unwrap();
    assert_eq!(
        &packet.words()[..16],
        &[
            0x5343_4631,
            0x0004_0010,
            0xfedc_ba98,
            1,
            0x9abc_def0,
            0x1234_5678,
            0x89ab_cdef,
            0x0123_4567,
            0x7654_3210,
            0xfedc_ba98,
            32,
            0x8000_0000,
            0,
            0,
            0,
            0
        ]
    );
    assert_eq!(
        packet.identity(),
        GroupIdentity::new(header().epoch, header().sequence, 4).unwrap()
    );
    for (actual, expected) in packet.words()[16..].chunks_exact(12).zip(records) {
        assert_eq!(actual, expected.words());
    }
    for count in 1..=MAX_COMMANDS {
        let records: Vec<_> = (0..count)
            .map(|index| CommandRecord::repeat(target(), index & 1 != 0))
            .collect();
        let mut absent = header();
        absent.deadline = None;
        let packet = GroupPacket::new(absent, &records).unwrap();
        assert_eq!(packet.words().len(), 16 + count * 12);
        assert_eq!(packet.words()[1], 16 | ((count as u32) << 16));
        assert_eq!(packet.words()[3], 0);
        assert_eq!(&packet.words()[10..16], &[0; 6]);
        for (actual, expected) in packet.words()[16..].chunks_exact(12).zip(records) {
            assert_eq!(actual, expected.words());
        }
    }
    assert_eq!(MAX_GROUP_WORDS, 400);
    assert_eq!(GroupPacket::new(header(), &[]), Err(CodecError::Length));
    assert_eq!(
        GroupPacket::new(header(), &[records[0]; 33]),
        Err(CodecError::Length)
    );
    let mut invalid = header();
    invalid.epoch = 0;
    assert_eq!(
        GroupPacket::new(invalid, &records),
        Err(CodecError::Identity)
    );
    invalid = header();
    invalid.sequence = 0;
    assert_eq!(
        GroupPacket::new(invalid, &records),
        Err(CodecError::Identity)
    );
    let misplaced = [records[3], records[0], records[3]];
    let packet = GroupPacket::new(header(), &misplaced).unwrap();
    assert_eq!(packet.words()[16], 5);
    assert_eq!(packet.words()[28], 2);
    assert_eq!(
        packet.words()[40],
        5,
        "the consumer must decide header refusal without reordering"
    );
}

#[shoop_test]
fn result_prefixes_identities_versions_and_codes_fail_closed() {
    let identity = GroupIdentity::new(u32::MAX, u64::MAX, MAX_COMMANDS).unwrap();
    let mut codes = [Code::Applied; MAX_COMMANDS];
    codes[1] = Code::InvalidValue;
    codes[31] = Code::Scheduled;
    let words = result_words(identity, &codes);
    let result = GroupResult::decode(&words, identity).unwrap();
    assert_eq!(result.codes(), codes);
    let header = result.header();
    assert_eq!(header.identity, identity);
    assert_eq!(header.frame, 0xfedc_ba98_7654_3220);
    assert_eq!(header.initial_revision, 0x1234_5678_9abc_def0);
    assert_eq!(header.current_revision, header.initial_revision);
    for length in 0..words.len() {
        assert_eq!(
            GroupResult::decode(&words[..length], identity),
            Err(CodecError::Length)
        );
    }
    let mut extra = words.clone();
    extra.push(0);
    assert_eq!(
        GroupResult::decode(&extra, identity),
        Err(CodecError::Length)
    );
    for (index, value) in [
        (0, 0x5346_5230),
        (1, 15 | (32 << 16)),
        (1, 16 | (31 << 16)),
        (12, 0),
        (12, 10),
        (12, u32::MAX),
        (16, 0),
        (16, 30),
        (16, u32::MAX),
        (15, 4),
    ] {
        let mut bad = words.clone();
        bad[index] = value;
        assert!(
            GroupResult::decode(&bad, identity).is_err(),
            "index={index}, value={value}"
        );
    }
    for bit in 1..32 {
        let mut bad = words.clone();
        bad[3] = 1 << bit;
        assert_eq!(GroupResult::decode(&bad, identity), Err(CodecError::Format));
    }
    for index in [2, 4, 5] {
        for bit in 0..32 {
            let mut bad = words.clone();
            bad[index] ^= 1 << bit;
            assert_eq!(
                GroupResult::decode(&bad, identity),
                Err(CodecError::Identity)
            );
        }
    }
    for code in [
        Code::StaleConfiguration,
        Code::DeadlineMissed,
        Code::CannotApply,
    ] {
        let mut refused = result_words(identity, &[code; MAX_COMMANDS]);
        refused[12] = code as u32;
        assert_eq!(
            GroupResult::decode(&refused, identity).unwrap().codes(),
            &[code; MAX_COMMANDS]
        );
        refused[16] = Code::Applied as u32;
        assert_eq!(
            GroupResult::decode(&refused, identity),
            Err(CodecError::Format)
        );
    }
}

#[shoop_test]
fn fenced_and_uncertain_results_remain_distinct_without_rewriting_prefix_effects() {
    let identity = GroupIdentity::new(7, 0x1234_0000_0001, 3).unwrap();
    let mut words = result_words(
        identity,
        &[Code::Applied, Code::InvalidValue, Code::Applied],
    );
    words[15] = 2;
    put(&mut words, 10, 0x1234_5678_9abc_def1);
    put(&mut words, 13, 0xfedc_ba98_7654_3220);
    let result = GroupResult::decode(&words, identity).unwrap();
    assert_eq!(result.header().switch, SwitchOutcome::Fenced);
    assert!(!result.header().faulted);
    assert_eq!(
        result.codes(),
        &[Code::Applied, Code::InvalidValue, Code::Applied]
    );
    let mut not_fenced = words.clone();
    not_fenced[3] = 1;
    assert_eq!(
        GroupResult::decode(&not_fenced, identity),
        Err(CodecError::Format)
    );
    not_fenced[15] = 3;
    not_fenced[18] = 0;
    let uncertain = GroupResult::decode(&not_fenced, identity).unwrap();
    assert!(uncertain.header().faulted);
    assert_eq!(uncertain.header().switch, SwitchOutcome::Uncertain);
    assert_eq!(
        uncertain.codes(),
        &[Code::Applied, Code::InvalidValue, Code::Unprocessed]
    );
    not_fenced[3] = 0;
    assert_eq!(
        GroupResult::decode(&not_fenced, identity),
        Err(CodecError::Format)
    );
    for (index, value) in [(10, words[8]), (18, Code::CannotApply as u32)] {
        let mut bad = words.clone();
        bad[index] = value;
        assert_eq!(GroupResult::decode(&bad, identity), Err(CodecError::Format));
    }
    for outcome in [0, 1, 2] {
        let mut bad = words.clone();
        bad[15] = outcome;
        put(&mut bad, 13, 5);
        assert_eq!(GroupResult::decode(&bad, identity), Err(CodecError::Format));
    }
}

#[shoop_test]
fn settled_snapshots_preserve_raw_render_cursors_and_loss_metadata() {
    let words = snapshot_words(128, false);
    assert_eq!(words.len() * 4, 19_520);
    assert_eq!(ROW_WORDS, 38);
    let format = SnapshotFormat::new(0xfedc_ba98, 32, 128).unwrap();
    let snapshot = Snapshot::decode(&words, format).unwrap();
    let header = snapshot.header();
    assert_eq!(header.phase, SnapshotPhase::Settled);
    assert_eq!(header.sequence, u64::MAX);
    assert_eq!(header.skipped, u64::MAX);
    assert_eq!(header.frame, 0xfedc_ba98_7654_3220);
    assert_eq!(header.revision, 0x1234_5678_9abc_def0);
    for slot in 0..128 {
        let row = snapshot.physical_row(slot).unwrap();
        assert_eq!(
            row,
            ObservationRow {
                raw: core::array::from_fn(|index| 0x8000_0000_0000_0000
                    | ((slot as u64) << 16)
                    | index as u64),
                gain_bits: 0x3f80_0001 + slot as u32,
                failed_plans: u32::MAX - slot as u32,
                last_pulse: 0xfedc_ba98_7654_0000 + slot as u64,
                pulse_valid: slot & 1 != 0,
            }
        );
        assert_ne!(
            row.raw[2], row.raw[8],
            "render and next cursors are not interchangeable"
        );
    }
    for slot in [128, 65536, usize::MAX] {
        assert_eq!(snapshot.physical_row(slot), Err(CodecError::Binding));
    }
}

#[shoop_test]
fn initial_snapshot_rows_are_raw_nonmembers_not_restoration_data() {
    let mut words = snapshot_words(128, true);
    words[16..].fill(u32::MAX);
    for row in words[16..].chunks_exact_mut(38) {
        row[36] = 1;
        row[37] = 0;
    }
    let snapshot =
        Snapshot::decode(&words, SnapshotFormat::new(0xfedc_ba98, 32, 128).unwrap()).unwrap();
    assert_eq!(snapshot.header().phase, SnapshotPhase::Initial);
    assert_eq!(snapshot.header().revision, 0);
    assert_eq!(snapshot.header().frame, 0);
    assert_eq!(snapshot.header().sequence, 1);
    let raw = snapshot.physical_row(127).unwrap();
    assert_eq!(raw.raw, [u64::MAX; 16]);
    assert_eq!(
        raw.gain_bits,
        u32::MAX,
        "unused retained rows need not contain finite gain metadata"
    );
    assert!(f32::from_bits(raw.gain_bits).is_nan());
    assert_eq!(raw.failed_plans, u32::MAX);
    assert_eq!(raw.last_pulse, u64::MAX);
    assert!(raw.pulse_valid);
}

#[shoop_test]
fn snapshot_prefixes_profiles_and_reserved_fields_never_alias() {
    let words = snapshot_words(1, false);
    let format = SnapshotFormat::new(0xfedc_ba98, 32, 1).unwrap();
    for length in 0..words.len() {
        assert!(Snapshot::decode(&words[..length], format).is_err());
    }
    let mut extra = words.clone();
    extra.push(0);
    assert!(Snapshot::decode(&extra, format).is_err());
    for (index, value) in [
        (0, 0),
        (1, 0x1_000f),
        (1, 0x2_0010),
        (2, 0),
        (3, 0),
        (3, 0x1_0020),
        (10, 0),
        (10, 129),
        (10, 0x1_0001),
        (11, 18),
        (11, 0x1_0013),
    ] {
        let mut bad = words.clone();
        bad[index] = value;
        assert!(Snapshot::decode(&bad, format).is_err(), "index={index}");
    }
    for index in [14, 15] {
        for bit in 0..32 {
            let mut bad = words.clone();
            bad[index] = 1 << bit;
            assert!(Snapshot::decode(&bad, format).is_err());
        }
    }
    for (index, first) in [(52, 1), (53, 0)] {
        for bit in first..32 {
            let mut bad = words.clone();
            bad[index] = 1 << bit;
            assert!(Snapshot::decode(&bad, format).is_err());
        }
    }
    for (epoch, quantum, rows) in [
        (0, 32, 1),
        (1, 0, 1),
        (1, 129, 1),
        (1, 32, 0),
        (1, 32, 129),
        (1, 32, usize::MAX),
    ] {
        assert!(SnapshotFormat::new(epoch, quantum, rows).is_err());
    }
    let initial = snapshot_words(1, true);
    for index in [4, 6, 7, 8, 9, 12, 13] {
        let mut bad = initial.clone();
        bad[index] = 5;
        assert!(Snapshot::decode(&bad, format).is_err());
    }
    for length in 0..16 {
        assert_eq!(
            SnapshotHeader::decode(&words[..length], format),
            Err(CodecError::Length)
        );
    }
    let mut no_sequence = words.clone();
    put(&mut no_sequence, 4, 0);
    assert!(Snapshot::decode(&no_sequence, format).is_err());
}
