use super::*;
use rt_fpga_compiler::{
    CompileContext, Identity, LogicalStateProgram, ProgramLimits, compile_with_chunk_frames,
    loading::{Allocator, LoadError},
};
use rt_fpga_rtl::host::semantic::{
    MAX_PREPARATION_BUS_OPS, PreparationProgress, ProgramSession, SelectionStep,
};
use shoop_rt_protocol::immediate::Code;
#[path = "../../../rt_fpga_compiler/tests/support/mono.rs"]
mod mono;

fn limits() -> ProgramLimits {
    ProgramLimits {
        instructions: 1024,
        state_values: 512,
        loop_slots: 128,
        jobs: 512,
        buffers: 512,
        history_slots: 512,
        input_ports: 1,
        output_ports: 2,
        quantum: 32,
    }
}
fn logical(entries: &[(u32, f32)]) -> LogicalStateProgram {
    let loops: Vec<_> = entries
        .iter()
        .map(|&(slot, gain)| mono::MonoLoop {
            id: Identity {
                slot,
                generation: 7,
            },
            source: None,
            input: true,
            output: true,
            gain,
            repeat_sync: false,
        })
        .collect();
    compile_with_chunk_frames(
        &mono::session(&loops),
        CompileContext {
            quantum: 32,
            limits: limits(),
        },
        32,
    )
    .unwrap()
}
fn session() -> (Bus, Allocator, ProgramSession<Bus>) {
    let bus = Bus::new();
    let mut recovery = bus.open().recovery();
    recovery.begin().unwrap();
    assert_eq!(recovery.poll(), Ok(RecoveryState::Ready(2)));
    let (allocator, session) = recovery
        .reopen()
        .unwrap()
        .program_session()
        .unwrap_or_else(|_| panic!("qualified reset refused"));
    (bus, allocator, session)
}
fn step(session: &mut ProgramSession<Bus>, bus: &Bus) -> Result<PreparationProgress, Error<u32>> {
    let before = bus.0.borrow().ops.len();
    let result = session.preparation_step();
    assert!(bus.0.borrow().ops.len() - before <= MAX_PREPARATION_BUS_OPS);
    result
}
fn prepare(session: &mut ProgramSession<Bus>, bus: &Bus) {
    for _ in 0..30_000 {
        if matches!(
            step(session, bus).unwrap(),
            PreparationProgress::Prepared(_)
        ) {
            return;
        }
    }
    panic!("preparation did not finish");
}
fn selection(session: &mut ProgramSession<Bus>, bus: &Bus) -> Result<SelectionStep, Error<u32>> {
    let before = bus.0.borrow().ops.len();
    let result = session.selection_step();
    assert!(bus.0.borrow().ops.len() - before <= 3);
    result
}
fn publish_selection(session: &mut ProgramSession<Bus>, bus: &Bus) {
    for _ in 0..200 {
        let before = bus.0.borrow().ops.len();
        assert_eq!(step(session, bus), Ok(PreparationProgress::Idle));
        assert_eq!(bus.0.borrow().ops.len(), before);
        if matches!(
            selection(session, bus).unwrap(),
            SelectionStep::Published(_)
        ) {
            return;
        }
    }
    panic!("selection was not published");
}
fn result(bus: &Bus, fenced: bool, wrong_revision: bool) {
    let mut s = bus.0.borrow_mut();
    let bank = s.states.iter().position(|&state| state == 2).unwrap();
    let group = &s.words[bank];
    let expected = u64::from(group[6]) | (u64::from(group[7]) << 32);
    let revision = if fenced {
        u64::from(group[22]) | (u64::from(group[23]) << 32)
    } else {
        expected
    };
    let revision = revision + u64::from(wrong_revision);
    let mut packet = vec![0; 17];
    packet[..6].copy_from_slice(&[0x5346_5231, 16 | (1 << 16), group[2], 0, group[4], group[5]]);
    packet[6] = 64;
    packet[8] = expected as u32;
    packet[9] = (expected >> 32) as u32;
    packet[10] = revision as u32;
    packet[11] = (revision >> 32) as u32;
    packet[12] = Code::Applied as u32;
    packet[13] = if fenced { 64 } else { 0 };
    packet[15] = if fenced { 2 } else { 1 };
    packet[16] = if fenced {
        Code::Applied
    } else {
        Code::InvalidValue
    } as u32;
    s.results[bank] = packet;
    s.states[bank] = 5;
}

#[shoop_test]
fn discovery_and_unrequested_reopen_never_grant_preparation_authority() {
    let bus = Bus::new();
    assert!(bus.open().program_session().is_err());
    let mut recovery = bus.open().recovery();
    assert!(recovery.reopen().unwrap().program_session().is_err());
    let mut recovery = bus.open().recovery();
    recovery.begin().unwrap();
    assert_eq!(recovery.poll(), Ok(RecoveryState::Ready(2)));
    let mut device = recovery.reopen().unwrap();
    device
        .queue(
            GroupPacket::new(
                GroupHeader {
                    epoch: 2,
                    sequence: 1,
                    graph: 0,
                    topology: 0,
                    deadline: None,
                },
                &[CommandRecord::gain(None, f32::NAN)],
            )
            .unwrap(),
        )
        .unwrap();
    assert!(device.program_session().is_err());
    assert_eq!(bus.0.borrow().recovery_requests, 1);
}

#[shoop_test]
fn trusted_staging_uses_original_source_and_retains_leases_until_the_correlated_fence() {
    let (bus, mut allocator, mut session) = session();
    let mut program = logical(&[(11, 0.12345679)]);
    let layout = allocator.stage(&program).unwrap();
    let mut foreign = Allocator::new(limits());
    assert_eq!(
        session.stage(foreign.stage(&program).unwrap()),
        Err(LoadError::IncompatibleState)
    );
    let mut changed = layout.clone();
    changed.loops[0].fresh = false;
    assert_eq!(session.stage(changed), Err(LoadError::IncompatibleState));
    program.bindings[0].initial_gain_bits = Some(f32::NAN.to_bits());
    session.stage(layout.clone()).unwrap();
    assert_eq!(
        session.staged().unwrap().compiled.bindings[0].initial_gain_bits,
        Some(0.12345679_f32.to_bits())
    );
    assert_eq!(allocator.cancel(layout.revision), Err(LoadError::Busy));
    prepare(&mut session, &bus);
    assert!(
        bus.0
            .borrow()
            .services
            .iter()
            .all(|(_, words)| words[1] != SEMANTIC_SERVICE_ENABLE_AUDIO)
    );
    session.select(None).unwrap();
    allocator.publish(layout.revision).unwrap();
    assert_eq!(allocator.complete(layout.revision), Err(LoadError::Busy));
    publish_selection(&mut session, &bus);
    assert!(
        bus.0
            .borrow()
            .services
            .iter()
            .all(|(_, words)| words[1] != SEMANTIC_SERVICE_ENABLE_AUDIO)
    );
    assert!(session.cancel_preparation(layout.revision).is_err());
    result(&bus, true, false);
    let mut fenced = false;
    for _ in 0..200 {
        if let SelectionStep::Fenced { selected, retired } = selection(&mut session, &bus).unwrap()
        {
            assert_eq!((selected.revision, selected.frame), (layout.revision, 64));
            assert!(retired.is_none());
            fenced = true;
            break;
        }
        assert_eq!(allocator.complete(layout.revision), Err(LoadError::Busy));
    }
    assert!(fenced);
    allocator.complete(layout.revision).unwrap();
    assert_eq!(session.active().unwrap().layout.revision, layout.revision);
    assert_eq!(
        bus.0
            .borrow()
            .services
            .iter()
            .filter(|(_, w)| w[1] == 9)
            .count(),
        1
    );
    let next = allocator
        .stage(&logical(&[(11, 0.75), (27, -0.0)]))
        .unwrap();
    let retained = session.active().unwrap().compiled.bindings[0].gain.unwrap() as u32;
    let start = bus.0.borrow().services.len();
    session.stage(next.clone()).unwrap();
    prepare(&mut session, &bus);
    for (_, w) in &bus.0.borrow().services[start..] {
        assert!(!(w[1] == 1 && w[2] == 0x101 && w[3] == retained));
        if w[1] == 1 && w[2] == 0x103 {
            assert!(w[3] >= 3);
        }
    }
    session.select(Some(0)).unwrap();
    allocator.publish(next.revision).unwrap();
    publish_selection(&mut session, &bus);
    result(&bus, false, false);
    let mut refused = false;
    for _ in 0..100 {
        if let SelectionStep::Refused { code, prepared } = selection(&mut session, &bus).unwrap() {
            assert_eq!(code, Code::InvalidValue);
            assert_eq!(prepared.image().layout.revision, next.revision);
            refused = true;
            break;
        }
    }
    assert!(refused);
    allocator.refused(next.revision).unwrap();
    assert_eq!(session.active().unwrap().layout.revision, layout.revision);
    assert_eq!(
        bus.0
            .borrow()
            .services
            .iter()
            .filter(|(_, w)| w[1] == 9)
            .count(),
        1
    );
}

#[shoop_test]
fn cancellation_waits_for_the_accepted_service_and_never_reissues_a_cancelled_recipe() {
    let (bus, mut allocator, mut session) = session();
    let layout = allocator.stage(&logical(&[(11, 0.5)])).unwrap();
    session.stage(layout.clone()).unwrap();
    for _ in 0..100 {
        step(&mut session, &bus).unwrap();
        if bus.0.borrow().services.len() == 1 {
            break;
        }
    }
    assert_eq!(bus.0.borrow().services.len(), 1);
    session.cancel_preparation(layout.revision).unwrap();
    assert_eq!(allocator.cancel(layout.revision), Err(LoadError::Busy));
    let mut cancelled = false;
    for _ in 0..100 {
        if step(&mut session, &bus).unwrap() == PreparationProgress::Cancelled(layout.revision) {
            cancelled = true;
            break;
        }
        assert_eq!(allocator.cancel(layout.revision), Err(LoadError::Busy));
    }
    assert!(cancelled);
    assert_eq!(bus.0.borrow().service_pops, 1);
    assert_eq!(bus.0.borrow().services.len(), 1);
    allocator.cancel(layout.revision).unwrap();
    assert_eq!(session.stage(layout), Err(LoadError::Busy));
    assert_eq!(step(&mut session, &bus), Ok(PreparationProgress::Idle));
}

#[shoop_test]
fn every_first_service_error_and_epoch_prefix_quarantines_without_delivery_replay_or_lease_release()
{
    let (bus, mut allocator, mut baseline) = session();
    let layout = allocator.stage(&logical(&[(11, 0.5)])).unwrap();
    baseline.stage(layout).unwrap();
    let start = bus.0.borrow().ops.len();
    for _ in 0..100 {
        step(&mut baseline, &bus).unwrap();
        if bus.0.borrow().service_pops == 1 {
            break;
        }
    }
    step(&mut baseline, &bus).unwrap();
    let count = bus.0.borrow().ops.len() - start;
    assert!(count > 100);
    for prefix in 1..=count {
        for mode in 0..3 {
            let (bus, mut allocator, mut session) = session();
            let layout = allocator.stage(&logical(&[(11, 0.5)])).unwrap();
            session.stage(layout.clone()).unwrap();
            let start = bus.0.borrow().ops.len();
            let expected = if mode == 2 {
                bus.0.borrow_mut().reset_at = Some(start + prefix);
                Error::EpochChanged
            } else {
                bus.0.borrow_mut().fail = Some((start + prefix, mode == 1));
                Error::Bus(99)
            };
            let mut failed = false;
            for _ in 0..100 {
                if step(&mut session, &bus).as_ref().err() == Some(&expected) {
                    failed = true;
                    break;
                }
            }
            assert!(failed, "prefix {prefix}/{count}, mode={mode}");
            assert_eq!(allocator.cancel(layout.revision), Err(LoadError::Busy));
            let before = bus.0.borrow().ops.len();
            assert_eq!(step(&mut session, &bus), Err(Error::Fault));
            assert_eq!(bus.0.borrow().ops.len(), before);
            assert!(bus.0.borrow().services.len() <= 1);
            assert!(bus.0.borrow().service_pops <= 1);
            assert_eq!(bus.0.borrow().recovery_requests, 1);
        }
    }
}

#[shoop_test]
fn malformed_service_status_receipt_tag_code_and_reserved_words_never_pop_or_release() {
    for (register, value) in [
        (REG_STATUS, 8),
        (REG_RESPONSE_TAG, 0),
        (REG_RESPONSE_TAG, 99),
        (REG_RESPONSE_CODE, 1),
        (REG_RESPONSE_CODE, 2),
        (REG_RESPONSE_CODE, 3),
        (REG_RESPONSE_CODE, u32::MAX),
        (REG_RESPONSE_DATA + 16, 2),
        (REG_RESPONSE_DATA + 44, 1),
    ] {
        let (bus, mut allocator, mut session) = session();
        let layout = allocator.stage(&logical(&[(11, 0.5)])).unwrap();
        session.stage(layout.clone()).unwrap();
        for _ in 0..100 {
            step(&mut session, &bus).unwrap();
            if bus.0.borrow().services.len() == 1 {
                break;
            }
        }
        bus.set(register, value);
        let mut failed = false;
        for _ in 0..100 {
            if step(&mut session, &bus).is_err() {
                failed = true;
                break;
            }
        }
        assert!(failed, "register {register:x}, value {value}");
        assert_eq!(bus.0.borrow().service_pops, 0);
        assert_eq!(allocator.cancel(layout.revision), Err(LoadError::Busy));
    }
    let (bus, mut allocator, mut session) = session();
    let layout = allocator.stage(&logical(&[(11, 0.5)])).unwrap();
    session.stage(layout.clone()).unwrap();
    for _ in 0..3 {
        step(&mut session, &bus).unwrap();
    }
    bus.set(REG_SUBMISSION_RESULT, 7);
    assert_eq!(step(&mut session, &bus), Err(Error::Rejected(7)));
    assert!(bus.0.borrow().services.is_empty());
    assert_eq!(allocator.cancel(layout.revision), Err(LoadError::Busy));
}

#[shoop_test]
fn foreign_fenced_revision_never_releases_resources_or_restores_old_authority() {
    let (bus, mut allocator, mut session) = session();
    let layout = allocator.stage(&logical(&[(11, 0.5)])).unwrap();
    session.stage(layout.clone()).unwrap();
    prepare(&mut session, &bus);
    session.select(None).unwrap();
    allocator.publish(layout.revision).unwrap();
    publish_selection(&mut session, &bus);
    result(&bus, true, true);
    let mut failed = false;
    for _ in 0..200 {
        if matches!(
            selection(&mut session, &bus),
            Err(Error::UnexpectedResponse)
        ) {
            failed = true;
            break;
        }
    }
    assert!(failed);
    assert!(session.active().is_none());
    assert_eq!(allocator.complete(layout.revision), Err(LoadError::Busy));
    let mut recovery = session.recovery();
    assert!(matches!(recovery.reopen(), Err(Error::Full)));
    recovery.begin().unwrap();
    assert_eq!(recovery.poll(), Ok(RecoveryState::Ready(3)));
    let (mut fresh_allocator, mut fresh) = recovery.reopen().unwrap();
    assert_eq!(fresh.epoch(), 3);
    assert!(fresh.active().is_none());
    assert_eq!(fresh.stage(layout), Err(LoadError::IncompatibleState));
    let next = fresh_allocator.stage(&logical(&[(11, 0.25)])).unwrap();
    fresh.stage(next).unwrap();
    assert!(matches!(recovery.reopen(), Err(Error::Full)));
}
