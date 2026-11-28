use super::*;
use rt_fpga_rtl::host::semantic::{
    CodecError, MAX_SNAPSHOT_BUS_OPS, SNAPSHOT_COPY_WORDS, Snapshot, SnapshotBuffers,
    SnapshotDevice, SnapshotFormat, SnapshotPhase, SnapshotStep,
};
use std::collections::BTreeSet;

fn put(words: &mut [u32], index: usize, value: u64) {
    words[index] = value as u32;
    words[index + 1] = (value >> 32) as u32;
}
fn image(epoch: u32, sequence: u64, initial: bool) -> Vec<u32> {
    let mut words = vec![0; SNAPSHOT_COPY_WORDS];
    words[..4].copy_from_slice(&[0x5346_5331, 16 | (u32::from(!initial) << 16), epoch, 32]);
    put(&mut words, 4, sequence);
    if !initial {
        put(&mut words, 6, 0xfedc_ba98_7654_3220);
        put(&mut words, 8, 0x1234_5678_9abc_def0);
        put(&mut words, 12, sequence.saturating_sub(3));
    }
    words[10] = 128;
    words[11] = 19;
    for row in 0..128 {
        let offset = 16 + row * 38;
        for word in 0..16 {
            put(
                &mut words,
                offset + 2 * word,
                0x8000_0000_0000_0001 | ((row as u64) << 32) | ((word as u64) << 16),
            );
        }
        words[offset + 32] = if initial { 0xffc1_2345 } else { 0x3f80_0001 };
        words[offset + 33] = u32::MAX;
        put(&mut words, offset + 34, 0xfedc_ba98_7654_3210 + row as u64);
        words[offset + 36] = (row % 2) as u32;
    }
    words
}
fn step(device: &mut SnapshotDevice<'_, Bus>, bus: &Bus) -> Result<SnapshotStep, Error<u32>> {
    let before = bus.0.borrow().ops.len();
    let result = device.snapshot_step();
    assert!(bus.0.borrow().ops.len() - before <= MAX_SNAPSHOT_BUS_OPS);
    result
}
fn update(device: &mut SnapshotDevice<'_, Bus>, bus: &Bus) {
    for _ in 0..10_000 {
        if matches!(step(device, bus).unwrap(), SnapshotStep::Updated(_)) {
            return;
        }
    }
    panic!("bounded snapshot copy did not finish");
}

#[shoop_test]
fn newest_as_of_scan_is_coherent_during_refill_and_interleaved_group_result_pumping() {
    let bus = Bus::new();
    let mut device = bus.open();
    device.queue(packet(1, 32)).unwrap();
    publish(&mut device, &bus);
    results::ready(&bus, 0);
    let sequence = u64::MAX - 2;
    let selected = image(1, sequence, false);
    let newer = image(1, sequence + 1, false);
    {
        let mut s = bus.0.borrow_mut();
        s.snapshots = [Some(image(1, 1, true)), Some(selected.clone())];
        s.snapshot_refill = Some((0, newer.clone()));
    }
    let mut first = vec![0xa5a5_a5a5; SNAPSHOT_COPY_WORDS];
    let mut second = first.clone();
    let mut device = device.with_snapshots(SnapshotBuffers::new(&mut first, &mut second).unwrap());
    device.queue(packet(2, 16)).unwrap();
    let mut published = false;
    let mut result = false;
    let mut updated = false;
    for _ in 0..10_000 {
        if let SnapshotStep::Updated(header) = step(&mut device, &bus).unwrap() {
            assert_eq!(header.sequence, sequence);
            updated = true;
            break;
        }
        assert!(device.latest_snapshot().is_none());
        let before = bus.0.borrow().ops.len();
        if !published {
            published = matches!(
                device.publication_step().unwrap(),
                PublicationStep::Published { .. }
            );
        }
        assert!(bus.0.borrow().ops.len() - before <= MAX_PUBLICATION_BUS_OPS);
        let before = bus.0.borrow().ops.len();
        if !result {
            result =
                device.result_step().unwrap() == rt_fpga_rtl::host::semantic::ResultStep::Buffered;
        }
        assert!(bus.0.borrow().ops.len() - before <= 3);
    }
    assert!(updated && published && result);
    assert_eq!(
        device.take_result().unwrap().header().identity.sequence(),
        1
    );
    assert_eq!(bus.0.borrow().snapshot_acks, vec![(0, 1), (1, sequence)]);
    let expected = Snapshot::decode(&selected, SnapshotFormat::new(1, 32, 128).unwrap()).unwrap();
    {
        let latest = device.latest_snapshot().unwrap();
        assert_eq!(latest.header(), expected.header());
        for row in 0..128 {
            assert_eq!(
                latest.physical_row(row).unwrap(),
                expected.physical_row(row).unwrap()
            );
        }
    }
    for _ in 0..100 {
        step(&mut device, &bus).unwrap();
        assert_eq!(
            device.latest_snapshot().unwrap().header().sequence,
            sequence
        );
    }
    update(&mut device, &bus);
    assert_eq!(
        device.latest_snapshot().unwrap().header().sequence,
        sequence + 1
    );
    assert_eq!(
        bus.0.borrow().snapshot_acks,
        vec![(0, 1), (1, sequence), (0, sequence + 1)]
    );
}

#[shoop_test]
fn snapshot_storage_is_exact_initial_rows_are_nonmembers_and_exhausted_publication_is_idle() {
    let mut first = vec![0; SNAPSHOT_COPY_WORDS];
    for length in [0, SNAPSHOT_COPY_WORDS - 1, SNAPSHOT_COPY_WORDS + 1] {
        let mut second = vec![0; length];
        assert!(matches!(
            SnapshotBuffers::new(&mut first, &mut second),
            Err(CodecError::Length)
        ));
    }
    let bus = Bus::new();
    let device = bus.open();
    bus.0.borrow_mut().snapshots[0] = Some(image(1, 1, true));
    let mut second = first.clone();
    let mut device = device.with_snapshots(SnapshotBuffers::new(&mut first, &mut second).unwrap());
    update(&mut device, &bus);
    let view = device.latest_snapshot().unwrap();
    assert_eq!(view.header().phase, SnapshotPhase::Initial);
    assert_eq!(view.header().revision, 0);
    assert_eq!(view.physical_row(127).unwrap().gain_bits, 0xffc1_2345);
    assert_eq!(view.physical_row(127).unwrap().raw[0] & 1, 1);
    bus.0.borrow_mut().snapshots[1] = Some(image(1, u64::MAX, false));
    update(&mut device, &bus);
    assert_eq!(
        device.latest_snapshot().unwrap().header().sequence,
        u64::MAX
    );
    for _ in 0..32 {
        assert_eq!(step(&mut device, &bus).unwrap(), SnapshotStep::Progress);
        assert_eq!(step(&mut device, &bus).unwrap(), SnapshotStep::Idle);
    }
    assert_eq!(bus.0.borrow().recovery_requests, 0);
}

#[shoop_test]
fn snapshot_protocol_errors_never_expose_partial_images_or_release_unvalidated_selected_banks() {
    for (index, value) in [
        (0, 0),
        (1, 15),
        (2, 2),
        (3, 31),
        (4, 0),
        (10, 127),
        (11, 18),
        (14, 1),
    ] {
        let bus = Bus::new();
        let device = bus.open();
        let mut words = image(1, 2, false);
        words[index] = value;
        bus.0.borrow_mut().snapshots[0] = Some(words);
        let mut a = vec![0; SNAPSHOT_COPY_WORDS];
        let mut b = a.clone();
        let mut device = device.with_snapshots(SnapshotBuffers::new(&mut a, &mut b).unwrap());
        let mut failed = false;
        for _ in 0..100 {
            if step(&mut device, &bus).is_err() {
                failed = true;
                break;
            }
        }
        assert!(failed);
        assert!(device.latest_snapshot().is_none());
        assert!(bus.0.borrow().snapshot_acks.is_empty());
    }
    let bus = Bus::new();
    let device = bus.open();
    bus.0.borrow_mut().snapshots[0] = Some(image(1, 1, true));
    let mut a = vec![0; SNAPSHOT_COPY_WORDS];
    let mut b = a.clone();
    let mut device = device.with_snapshots(SnapshotBuffers::new(&mut a, &mut b).unwrap());
    update(&mut device, &bus);
    let mut words = image(1, 2, false);
    words[SNAPSHOT_COPY_WORDS - 1] = 1;
    bus.0.borrow_mut().snapshots[1] = Some(words);
    let mut failed = false;
    for _ in 0..10_000 {
        if let Err(error) = step(&mut device, &bus) {
            assert_eq!(error, Error::UnexpectedResponse);
            failed = true;
            break;
        }
        assert_eq!(device.latest_snapshot().unwrap().header().sequence, 1);
    }
    assert!(failed);
    assert_eq!(device.latest_snapshot().unwrap().header().sequence, 1);
    assert_eq!(bus.0.borrow().snapshot_acks.len(), 1);
    for length in [0, 15, 16, 17, 4096, SNAPSHOT_COPY_WORDS - 1] {
        let bus = Bus::new();
        let device = bus.open();
        let mut words = image(1, 2, false);
        words.truncate(length);
        bus.0.borrow_mut().snapshots[0] = Some(words);
        let mut a = vec![0; SNAPSHOT_COPY_WORDS];
        let mut b = a.clone();
        let mut device = device.with_snapshots(SnapshotBuffers::new(&mut a, &mut b).unwrap());
        let mut failed = false;
        for _ in 0..10_000 {
            if let Err(error) = step(&mut device, &bus) {
                assert_eq!(error, Error::Rejected(RECEIPT_UNAVAILABLE));
                failed = true;
                break;
            }
        }
        assert!(failed);
        assert!(device.latest_snapshot().is_none());
        assert!(bus.0.borrow().snapshot_acks.is_empty());
    }
    for receipt in 1..=9 {
        let bus = Bus::new();
        let device = bus.open();
        {
            let mut s = bus.0.borrow_mut();
            s.snapshots[0] = Some(image(1, 2, false));
            s.forced_snapshot_receipt = Some(receipt);
        }
        let mut a = vec![0; SNAPSHOT_COPY_WORDS];
        let mut b = a.clone();
        let mut device = device.with_snapshots(SnapshotBuffers::new(&mut a, &mut b).unwrap());
        assert!((0..100).any(|_| step(&mut device, &bus).is_err()));
        assert!(device.latest_snapshot().is_none());
        assert!(bus.0.borrow().snapshot_acks.is_empty());
    }
    for mask in [4, u32::MAX] {
        let bus = Bus::new();
        let device = bus.open();
        bus.set(REG_SNAPSHOT_READY, mask);
        let mut a = vec![0; SNAPSHOT_COPY_WORDS];
        let mut b = a.clone();
        let mut device = device.with_snapshots(SnapshotBuffers::new(&mut a, &mut b).unwrap());
        assert_eq!(step(&mut device, &bus), Err(Error::UnexpectedResponse));
    }
}

#[shoop_test]
fn sampled_snapshot_phase_and_row_boundary_bus_errors_and_epoch_crossings_never_replay_acks() {
    let baseline = Bus::new();
    let device = baseline.open();
    baseline.0.borrow_mut().snapshots[0] = Some(image(1, 2, false));
    let mut a = vec![0; SNAPSHOT_COPY_WORDS];
    let mut b = a.clone();
    let mut device = device.with_snapshots(SnapshotBuffers::new(&mut a, &mut b).unwrap());
    baseline.0.borrow_mut().ops.clear();
    update(&mut device, &baseline);
    let trace = baseline.0.borrow().ops.clone();
    let mut sites = BTreeSet::new();
    let mut data = 0;
    for (index, op) in trace.iter().enumerate() {
        let chosen = match op {
            Op::Read(REG_SNAPSHOT_DATA) => {
                let word = data;
                data += 1;
                word < 16
                    || word >= 16 + SNAPSHOT_COPY_WORDS - 2
                    || (word >= 16 && (word - 16) % (38 * 16) == 0)
            }
            Op::Read(REG_EPOCH | REG_SNAPSHOT_RECEIPT) => false,
            _ => true,
        };
        if chosen {
            for offset in -1..=4 {
                let at = index as isize + 1 + offset;
                if at > 0 && (at as usize) <= trace.len() {
                    sites.insert(at as usize);
                }
            }
        }
    }
    sites.insert(trace.len());
    for at in sites {
        for mode in 0..3 {
            let bus = Bus::new();
            let device = bus.open();
            bus.0.borrow_mut().snapshots[0] = Some(image(1, 2, false));
            let mut a = vec![0; SNAPSHOT_COPY_WORDS];
            let mut b = a.clone();
            let mut device = device.with_snapshots(SnapshotBuffers::new(&mut a, &mut b).unwrap());
            {
                let mut s = bus.0.borrow_mut();
                s.ops.clear();
                if mode == 2 {
                    s.reset_at = Some(at);
                } else {
                    s.fail = Some((at, mode == 1));
                }
            }
            let mut failed = false;
            for _ in 0..10_000 {
                match step(&mut device, &bus) {
                    Err(Error::Bus(99)) if mode != 2 => {
                        failed = true;
                        break;
                    }
                    Err(Error::EpochChanged) if mode == 2 => {
                        failed = true;
                        break;
                    }
                    Ok(SnapshotStep::Progress) => {}
                    other => panic!("site{at}/{mode}: {other:?}"),
                }
            }
            assert!(failed);
            assert!(device.latest_snapshot().is_none());
            let before = bus.0.borrow().ops.len();
            assert_eq!(step(&mut device, &bus), Err(Error::Fault));
            assert_eq!(device.result_step(), Err(Error::Fault));
            assert_eq!(device.publication_step(), Err(Error::Fault));
            let s = bus.0.borrow();
            assert_eq!(s.ops.len(), before);
            assert!(s.snapshot_acks.len() <= 1);
            assert_eq!(s.recovery_requests, 0);
        }
    }
}

#[shoop_test]
fn consuming_snapshot_recovery_cancels_partial_copy_and_fresh_storage_starts_empty_in_the_new_epoch()
 {
    let bus = Bus::new();
    let device = bus.open();
    bus.0.borrow_mut().snapshots[0] = Some(image(1, 1, true));
    let mut a = vec![0; SNAPSHOT_COPY_WORDS];
    let mut b = a.clone();
    let mut device = device.with_snapshots(SnapshotBuffers::new(&mut a, &mut b).unwrap());
    update(&mut device, &bus);
    bus.0.borrow_mut().snapshots[1] = Some(image(1, 2, false));
    for _ in 0..100 {
        step(&mut device, &bus).unwrap();
    }
    assert_eq!(device.latest_snapshot().unwrap().header().epoch, 1);
    let mut recovery = device.recovery();
    recovery.begin().unwrap();
    assert_eq!(recovery.poll().unwrap(), RecoveryState::Ready(2));
    let fresh = recovery.reopen().unwrap();
    let mut fresh = fresh.with_snapshots(SnapshotBuffers::new(&mut a, &mut b).unwrap());
    assert!(fresh.latest_snapshot().is_none());
    bus.0.borrow_mut().snapshots[0] = Some(image(2, 1, true));
    update(&mut fresh, &bus);
    let header = fresh.latest_snapshot().unwrap().header();
    assert_eq!(header.epoch, 2);
    assert_eq!(header.sequence, 1);
    assert_eq!(header.frame, 0);
    assert_eq!(header.revision, 0);
    assert_eq!(bus.0.borrow().recovery_requests, 1);
}
