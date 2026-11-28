use super::*;
use rt_fpga_rtl::host::semantic::{GroupResult, MAX_RESULT_BUS_OPS, ResultStep};
use shoop_rt_protocol::immediate::Code;

fn ready(bus: &Bus, bank: usize) {
    let mut state = bus.0.borrow_mut();
    assert_eq!(state.states[bank], 2);
    let group = &state.words[bank];
    let count = (group[1] >> 16) as usize;
    let mut words = vec![0; 16 + count];
    words[..6].copy_from_slice(&[
        0x5346_5231,
        16 | ((count as u32) << 16),
        group[2],
        0,
        group[4],
        group[5],
    ]);
    words[6..12].copy_from_slice(&[
        0x7654_3220,
        0xfedc_ba98,
        0x9abc_def0,
        0x1234_5678,
        0x9abc_def0,
        0x1234_5678,
    ]);
    words[12] = Code::Applied as u32;
    words[16..].fill(Code::InvalidValue as u32);
    state.results[bank] = words;
    state.states[bank] = 5;
}
fn step(device: &mut Device<Bus>, bus: &Bus) -> Result<ResultStep, Error<u32>> {
    let before = bus.0.borrow().ops.len();
    let result = device.result_step();
    assert!(bus.0.borrow().ops.len() - before <= MAX_RESULT_BUS_OPS);
    result
}
fn buffer(device: &mut Device<Bus>, bus: &Bus) {
    for _ in 0..120 {
        if step(device, bus).unwrap() == ResultStep::Buffered {
            return;
        }
        assert!(device.take_result().is_none());
    }
    panic!("bounded result copy did not finish");
}
fn assert_result(result: &GroupResult, sequence: u64, count: usize) {
    let header = result.header();
    assert_eq!(header.identity.sequence(), sequence);
    assert_eq!(header.identity.epoch(), 1);
    assert_eq!(header.frame, 0xfedc_ba98_7654_3220);
    assert_eq!(header.initial_revision, 0x1234_5678_9abc_def0);
    assert_eq!(header.current_revision, header.initial_revision);
    assert_eq!(header.code, Code::Applied);
    assert_eq!(result.codes(), vec![Code::InvalidValue; count]);
}

#[shoop_test]
fn complete_correlated_results_release_banks_and_keep_copies_independent_of_native_reuse() {
    let bus = Bus::new();
    let mut device = bus.open();
    for (index, count) in [1, 16, 32, 2, 32, 16, 1, 2].into_iter().enumerate() {
        let sequence = index as u64 + 1;
        device.queue(packet(sequence, count)).unwrap();
        let bank = publish(&mut device, &bus);
        assert_eq!(bank, 0);
        ready(&bus, bank);
        buffer(&mut device, &bus);
        assert!(device.published(bank).is_none());
        assert_eq!(bus.0.borrow().states[bank], 0);
        let before = bus.0.borrow().ops.len();
        for _ in 0..32 {
            assert_eq!(step(&mut device, &bus).unwrap(), ResultStep::Buffered);
        }
        assert_eq!(bus.0.borrow().ops.len(), before);
        assert_result(&device.take_result().unwrap(), sequence, count);
        assert!(device.take_result().is_none());
    }
    assert_eq!(bus.0.borrow().acknowledgements, 8);
    assert_eq!(bus.0.borrow().recovery_requests, 0);
}

#[shoop_test]
fn publication_and_result_receipt_windows_interleave_without_false_accounting_or_early_reuse() {
    let bus = Bus::new();
    let mut device = bus.open();
    device.queue(packet(1, 32)).unwrap();
    while bus.0.borrow().admitted.is_empty() {
        device.publication_step().unwrap();
    }
    ready(&bus, 0);
    assert!(device.published(0).is_none());
    assert_eq!(step(&mut device, &bus).unwrap(), ResultStep::Idle);
    assert_eq!(bus.0.borrow().acknowledgements, 0);
    assert_eq!(publish(&mut device, &bus), 0);
    while bus.0.borrow().acknowledgements == 0 {
        step(&mut device, &bus).unwrap();
    }
    assert!(device.published(0).is_some());
    assert!(device.take_result().is_none());
    device.queue(packet(2, 16)).unwrap();
    for _ in 0..3 {
        device.publication_step().unwrap();
    }
    assert_eq!(bus.0.borrow().bank, 1, "bank0 awaits confirmed ACK receipt");
    assert_eq!(step(&mut device, &bus).unwrap(), ResultStep::Buffered);
    assert_eq!(publish(&mut device, &bus), 1);
    ready(&bus, 1);
    let before = bus.0.borrow().ops.len();
    for _ in 0..32 {
        assert_eq!(step(&mut device, &bus).unwrap(), ResultStep::Buffered);
    }
    assert_eq!(bus.0.borrow().ops.len(), before);
    assert_result(&device.take_result().unwrap(), 1, 32);
    buffer(&mut device, &bus);
    assert_result(&device.take_result().unwrap(), 2, 16);
    assert_eq!(bus.0.borrow().acknowledgements, 2);
}

#[shoop_test]
fn every_result_bus_error_and_epoch_crossing_prefix_quarantines_without_ack_replay() {
    let baseline = Bus::new();
    let mut device = baseline.open();
    device.queue(packet(1, 32)).unwrap();
    publish(&mut device, &baseline);
    ready(&baseline, 0);
    baseline.0.borrow_mut().ops.clear();
    buffer(&mut device, &baseline);
    let count = baseline.0.borrow().ops.len();
    for at in 1..=count {
        for mode in 0..3 {
            let bus = Bus::new();
            let mut device = bus.open();
            device.queue(packet(1, 32)).unwrap();
            publish(&mut device, &bus);
            ready(&bus, 0);
            {
                let mut state = bus.0.borrow_mut();
                state.ops.clear();
                if mode == 2 {
                    state.reset_at = Some(at);
                } else {
                    state.fail = Some((at, mode == 1));
                }
            }
            let mut failed = false;
            for _ in 0..120 {
                match step(&mut device, &bus) {
                    Err(Error::Bus(99)) if mode != 2 => {
                        failed = true;
                        break;
                    }
                    Err(Error::EpochChanged) if mode == 2 => {
                        failed = true;
                        break;
                    }
                    Ok(ResultStep::Progress) => {}
                    other => panic!("prefix{at}/{mode}: {other:?}"),
                }
            }
            assert!(failed, "prefix{at}/{mode}");
            assert!(device.take_result().is_none());
            assert!(device.published(0).is_some());
            let before = bus.0.borrow().ops.len();
            assert_eq!(step(&mut device, &bus), Err(Error::Fault));
            assert_eq!(device.publication_step(), Err(Error::Fault));
            assert_eq!(device.queue(packet(2, 1)), Err(Error::Fault));
            let state = bus.0.borrow();
            assert_eq!(state.ops.len(), before);
            assert_eq!(state.admitted.len(), 1);
            assert!(state.acknowledgements <= 1);
            assert_eq!(state.recovery_requests, 0);
        }
    }
}

#[shoop_test]
fn malformed_foreign_partial_and_refused_packets_never_expose_results_or_acknowledge() {
    let patches = [
        (0, 0),
        (1, 16),
        (2, 2),
        (3, 2),
        (4, 2),
        (12, u32::MAX),
        (13, 1),
        (15, 2),
        (16, 0),
        (16, u32::MAX),
        (10, 0),
    ];
    for patch in patches {
        let bus = Bus::new();
        let mut device = bus.open();
        device.queue(packet(1, 32)).unwrap();
        publish(&mut device, &bus);
        ready(&bus, 0);
        bus.0.borrow_mut().results[0][patch.0] = patch.1;
        let mut failed = false;
        for _ in 0..120 {
            if let Err(error) = step(&mut device, &bus) {
                assert_eq!(error, Error::UnexpectedResponse);
                failed = true;
                break;
            }
        }
        assert!(failed, "patch {patch:?}");
        assert!(device.take_result().is_none());
        assert_eq!(bus.0.borrow().acknowledgements, 0);
    }
    for length in 0..48 {
        let bus = Bus::new();
        let mut device = bus.open();
        device.queue(packet(1, 32)).unwrap();
        publish(&mut device, &bus);
        ready(&bus, 0);
        bus.0.borrow_mut().results[0].truncate(length);
        for _ in 0..120 {
            if let Err(error) = step(&mut device, &bus) {
                assert_eq!(error, Error::Rejected(RECEIPT_UNAVAILABLE));
                break;
            }
        }
        assert!(device.take_result().is_none());
        assert_eq!(step(&mut device, &bus), Err(Error::Fault));
        assert_eq!(bus.0.borrow().acknowledgements, 0);
    }
    for mask in [2, 3, 16, u32::MAX] {
        let bus = Bus::new();
        let mut device = bus.open();
        device.queue(packet(1, 1)).unwrap();
        publish(&mut device, &bus);
        device.queue(packet(2, 1)).unwrap();
        publish(&mut device, &bus);
        ready(&bus, 0);
        ready(&bus, 1);
        bus.set(REG_RESULT_READY, mask);
        assert_eq!(step(&mut device, &bus), Err(Error::UnexpectedResponse));
        assert_eq!(bus.0.borrow().acknowledgements, 0);
    }
    for receipt in 1..=9 {
        let bus = Bus::new();
        let mut device = bus.open();
        device.queue(packet(1, 1)).unwrap();
        publish(&mut device, &bus);
        ready(&bus, 0);
        bus.set(REG_RESULT_RECEIPT, receipt);
        let mut failed = false;
        for _ in 0..120 {
            if step(&mut device, &bus).is_err() {
                failed = true;
                break;
            }
        }
        assert!(failed);
        assert_eq!(bus.0.borrow().acknowledgements, 0);
    }
    let bus = Bus::new();
    let mut device = bus.open();
    bus.set(REG_STATUS, 4);
    assert_eq!(step(&mut device, &bus).unwrap(), ResultStep::Progress);
    assert_eq!(step(&mut device, &bus), Err(Error::Fault));
}
