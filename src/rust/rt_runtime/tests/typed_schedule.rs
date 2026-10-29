mod support;
use rt_runtime::schedule::{typed_scheduler, Action};
use rt_runtime::transport::Lane;
use rt_runtime::{Budgets, Config, Error, Frame, PlannerId, Sequence};
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;

#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
fn config() -> rt_runtime::PreparedConfig {
    Config {
        scheduled_events: 2,
        midi_bytes_per_port: 8,
        page_bytes: 64,
        budgets: Budgets {
            events: 2,
            transfer_bytes: 64,
            subranges: 0,
            ..Budgets::default()
        },
        ..Config::default()
    }
    .prepare()
    .unwrap()
}
fn action(value: u32) -> Action<u32> {
    Action::Planner {
        planner: PlannerId::from_parts(0, 1).unwrap(),
        operation: value,
    }
}

#[shoop_test]
fn typed_payload_storage_does_not_expand_the_midi_packet_limit() {
    let (_results, mut scheduler) =
        typed_scheduler::<u32, u32, (), ()>(config(), 32, true).unwrap();
    assert_no_heap(|| {
        assert_eq!(scheduler.payload_limit(), 32);
        scheduler
            .admit(Sequence(1), Frame(0), Frame(0), action(1), &[0; 32])
            .unwrap();
        let midi = Action::Midi {
            node: rt_runtime::NodeId::from_parts(0, 1).unwrap(),
            port: 0,
        };
        assert_eq!(
            scheduler.admit(
                Sequence(2),
                Frame(0),
                Frame(0),
                midi,
                &[0xf0, 1, 2, 3, 4, 5, 6, 7, 0xf7]
            ),
            Err(Error::Capacity)
        );
        assert_eq!(scheduler.available(), 1);
    });
}

#[shoop_test]
fn pending_receipts_keep_credits_and_distinguish_delivery_from_completion() {
    let (mut results, mut scheduler) =
        typed_scheduler::<u32, u64, (), ()>(config(), 8, true).unwrap();
    assert_no_heap(|| {
        scheduler
            .admit(Sequence(1), Frame(2), Frame(5), action(7), &[1, 2, 3])
            .unwrap();
        let token = {
            let delivery = scheduler.deliver(Frame(5)).unwrap();
            assert_eq!(delivery.payload, [1, 2, 3]);
            delivery.token
        };
        assert_eq!(scheduler.pending(), 0);
        assert_eq!(scheduler.available(), 1);
        assert!(results.try_execution().is_none());
        assert_eq!(
            scheduler.receipts(Frame(4)).complete(token, Ok(8)),
            Err(Error::InvalidFrame)
        );
        scheduler
            .receipts(Frame(12))
            .complete(token, Ok(8))
            .unwrap();
        assert_eq!(
            scheduler.receipts(Frame(12)).complete(token, Ok(9)),
            Err(Error::InvalidState)
        );
        let execution = results.try_execution().unwrap();
        assert_eq!(execution.delivered_frame, Some(Frame(5)));
        assert_eq!(execution.actual_frame, Frame(12));
        assert_eq!(execution.late_by(), 3);
        assert_eq!(execution.operation_result, Some(8));
        assert_eq!(execution.result, Ok(()));
        assert_eq!(scheduler.available(), 1);
        scheduler.begin_callback();
        assert_eq!(scheduler.available(), 2);
        scheduler
            .admit(Sequence(2), Frame(12), Frame(12), action(9), &[])
            .unwrap();
        let new = scheduler.deliver(Frame(12)).unwrap().token;
        assert_ne!(token, new);
        assert_eq!(
            scheduler.receipts(Frame(12)).complete(token, Ok(10)),
            Err(Error::InvalidId)
        );
        scheduler
            .receipts(Frame(12))
            .complete(new, Err(Error::InvalidEvent))
            .unwrap();
        assert_eq!(
            results.try_execution().unwrap().result,
            Err(Error::InvalidEvent)
        );
    });
}

#[shoop_test]
fn independent_recovery_slot_survives_normal_saturation_and_lane_reordering() {
    let (mut results, mut scheduler) =
        typed_scheduler::<u32, u32, (), ()>(config(), 0, true).unwrap();
    assert_no_heap(|| {
        scheduler
            .admit_recovery(Sequence(3), Frame(0), Frame(0), action(0), &[])
            .unwrap();
        scheduler
            .admit(Sequence(1), Frame(0), Frame(0), action(1), &[])
            .unwrap();
        scheduler
            .admit(Sequence(2), Frame(0), Frame(0), action(2), &[])
            .unwrap();
        assert_eq!(
            scheduler.admit(Sequence(4), Frame(0), Frame(0), action(4), &[]),
            Err(Error::Full)
        );
        for expected in 1..=3 {
            let mut delivery = scheduler.deliver(Frame(0)).unwrap();
            assert_eq!(delivery.token.sequence(), Sequence(expected));
            delivery
                .receipts
                .complete(delivery.token, Ok(expected as u32))
                .unwrap();
        }
        assert!(!scheduler.recovery_pending());
        assert!(!scheduler.recovery_available());
        assert_eq!(
            scheduler.admit_recovery(Sequence(4), Frame(0), Frame(0), action(0), &[]),
            Err(Error::Full)
        );
        let mut lanes = [Lane::Normal; 3];
        for lane in &mut lanes {
            *lane = results.try_execution().unwrap().lane;
        }
        assert_eq!(lanes, [Lane::Normal, Lane::Normal, Lane::Recovery]);
        assert!(!scheduler.recovery_available());
        scheduler.begin_callback();
        assert!(scheduler.recovery_available());
        assert_eq!(scheduler.available(), 2);
    });
}

#[shoop_test]
fn queued_cancellation_has_no_delivery_and_retains_its_terminal_slot() {
    let (mut results, mut scheduler) =
        typed_scheduler::<u32, u32, (), ()>(config(), 0, true).unwrap();
    assert_no_heap(|| {
        scheduler
            .admit(Sequence(1), Frame(100), Frame(0), action(1), &[])
            .unwrap();
        scheduler
            .receipts(Frame(3))
            .cancel_queued(Sequence(1), 99)
            .unwrap();
        assert_eq!(scheduler.pending(), 0);
        assert_eq!(scheduler.available(), 1);
        assert!(scheduler.deliver(Frame(100)).is_none());
        assert_eq!(
            scheduler.receipts(Frame(4)).cancel_queued(Sequence(1), 99),
            Err(Error::InvalidState)
        );
        let execution = results.try_execution().unwrap();
        assert_eq!(execution.requested_frame, Frame(100));
        assert_eq!(execution.delivered_frame, None);
        assert_eq!(execution.actual_frame, Frame(3));
        assert_eq!(execution.operation_result, Some(99));
        assert_eq!(execution.late_by(), 0);
        scheduler.begin_callback();
        assert_eq!(scheduler.available(), 2);
    });
}

#[shoop_test]
fn foreign_tokens_payload_overflow_and_future_recovery_are_explicit() {
    let (_reader_a, mut a) = typed_scheduler::<u32, u32, (), ()>(config(), 8, true).unwrap();
    let (mut reader_b, mut b) = typed_scheduler::<u32, u32, (), ()>(config(), 8, true).unwrap();
    assert_no_heap(|| {
        for scheduler in [&mut a, &mut b] {
            scheduler
                .admit(Sequence(1), Frame(0), Frame(0), action(1), &[])
                .unwrap();
        }
        let foreign = a.deliver(Frame(0)).unwrap().token;
        let valid = b.deliver(Frame(0)).unwrap().token;
        assert_eq!(
            b.receipts(Frame(0)).complete(foreign, Ok(1)),
            Err(Error::InvalidId)
        );
        b.receipts(Frame(0)).complete(valid, Ok(2)).unwrap();
        assert_eq!(
            b.admit(Sequence(2), Frame(0), Frame(0), action(1), &[0; 9]),
            Err(Error::Capacity)
        );
        assert_eq!(
            b.admit_recovery(Sequence(3), Frame(1), Frame(0), action(1), &[]),
            Err(Error::InvalidEvent)
        );
        assert_eq!(
            b.admit_recovery(Sequence(3), Frame(0), Frame(0), action(1), &[0]),
            Err(Error::InvalidEvent)
        );
        assert!(b.recovery_available());
        assert_eq!(b.available(), 1);
        assert_eq!(reader_b.try_execution().unwrap().operation_result, Some(2));
    });
}

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Queued,
    Delivered,
    Terminal,
    Read,
}
#[derive(Clone, Copy)]
struct ModelEntry {
    sequence: Sequence,
    requested: Frame,
    lane: Lane,
    phase: Phase,
    delivered: Option<Frame>,
    token: Option<rt_runtime::schedule::ReceiptToken>,
}

#[shoop_test]
fn seeded_receipt_lifecycle_matches_independent_slot_and_terminal_model() {
    use std::collections::VecDeque;
    let (mut results, mut scheduler) =
        typed_scheduler::<u32, u32, (), ()>(config(), 0, true).unwrap();
    let mut model: [Option<ModelEntry>; 3] = [None; 3];
    let mut expected = VecDeque::new();
    let mut random = 0xa36b_2047_2f58_81d9u64;
    let mut sequence = 0u64;
    let mut admitted = 0;
    let mut consumed = 0;
    let steps = if cfg!(miri) { 64 } else { 8192 };
    let check_result = |actual: rt_runtime::schedule::Execution<u32, u32>,
                        entry: ModelEntry,
                        at: Frame,
                        value: u32| {
        assert_eq!(actual.sequence, entry.sequence);
        assert_eq!(actual.action, action(entry.sequence.0 as u32));
        assert_eq!(actual.lane, entry.lane);
        assert_eq!(actual.requested_frame, entry.requested);
        assert_eq!(actual.delivered_frame, entry.delivered);
        assert_eq!(actual.actual_frame, at);
        assert_eq!(actual.result, Ok(()));
        assert_eq!(actual.operation_result, Some(value));
    };
    for step in 0..steps {
        random = random
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let at = Frame(step as u64);
        let index = (random >> 32) as usize % 3;
        match (random >> 16) % 8 {
            0 | 1 => {
                let recovery = (random >> 16) % 8 == 1;
                let slot = if recovery {
                    model[2].is_none().then_some(2)
                } else {
                    model[..2].iter().position(Option::is_none)
                };
                sequence += 1;
                let requested = Frame(at.0 + if recovery { 0 } else { (random >> 24) % 3 });
                let result = assert_no_heap(|| {
                    if recovery {
                        scheduler.admit_recovery(
                            Sequence(sequence),
                            requested,
                            at,
                            action(sequence as u32),
                            &[],
                        )
                    } else {
                        scheduler.admit(
                            Sequence(sequence),
                            requested,
                            at,
                            action(sequence as u32),
                            &[],
                        )
                    }
                });
                if let Some(slot) = slot {
                    result.unwrap();
                    admitted += 1;
                    model[slot] = Some(ModelEntry {
                        sequence: Sequence(sequence),
                        requested,
                        lane: if recovery {
                            Lane::Recovery
                        } else {
                            Lane::Normal
                        },
                        phase: Phase::Queued,
                        delivered: None,
                        token: None,
                    });
                } else {
                    assert_eq!(result, Err(Error::Full));
                }
            }
            2 => {
                let due = model
                    .iter()
                    .enumerate()
                    .filter_map(|(i, e)| {
                        e.filter(|e| e.phase == Phase::Queued && e.requested <= at)
                            .map(|e| (i, e))
                    })
                    .min_by_key(|(_, e)| (e.requested, e.sequence));
                let delivered = assert_no_heap(|| scheduler.deliver(at).map(|d| (d.token, d.lane)));
                if let Some((slot, entry)) = due {
                    let (token, lane) = delivered.unwrap();
                    assert_eq!(token.sequence(), entry.sequence);
                    assert_eq!(lane, entry.lane);
                    let entry = model[slot].as_mut().unwrap();
                    entry.token = Some(token);
                    entry.phase = Phase::Delivered;
                    entry.delivered = Some(at);
                } else {
                    assert!(delivered.is_none());
                }
            }
            3 | 4 => {
                if let Some(entry) = model[index] {
                    let cancel = (random >> 16) % 8 == 4;
                    if (!cancel && entry.phase == Phase::Delivered)
                        || (cancel && entry.phase == Phase::Queued && entry.lane == Lane::Normal)
                    {
                        let value = if cancel {
                            u32::MAX
                        } else {
                            entry.sequence.0 as u32
                        };
                        assert_no_heap(|| {
                            if cancel {
                                scheduler.receipts(at).cancel_queued(entry.sequence, value)
                            } else {
                                scheduler
                                    .receipts(at)
                                    .complete(entry.token.unwrap(), Ok(value))
                            }
                        })
                        .unwrap();
                        expected.push_back((entry, at, value));
                        model[index].as_mut().unwrap().phase = Phase::Terminal;
                    }
                }
            }
            5 => {
                let actual = assert_no_heap(|| results.try_execution());
                if let Some((entry, frame, value)) = expected.pop_front() {
                    check_result(actual.unwrap(), entry, frame, value);
                    consumed += 1;
                    model
                        .iter_mut()
                        .flatten()
                        .find(|e| e.sequence == entry.sequence)
                        .unwrap()
                        .phase = Phase::Read;
                } else {
                    assert!(actual.is_none());
                }
            }
            6 => {
                let count = model
                    .iter()
                    .flatten()
                    .filter(|e| e.phase == Phase::Read)
                    .count();
                assert_eq!(assert_no_heap(|| scheduler.begin_callback()), count);
                for entry in &mut model {
                    if entry.is_some_and(|e| e.phase == Phase::Read) {
                        *entry = None;
                    }
                }
            }
            _ => {}
        }
        assert_eq!(
            scheduler.available(),
            model[..2].iter().filter(|e| e.is_none()).count()
        );
        assert_eq!(scheduler.recovery_available(), model[2].is_none());
        assert_eq!(
            scheduler.pending(),
            model
                .iter()
                .flatten()
                .filter(|e| e.phase == Phase::Queued)
                .count()
        );
        assert_eq!(
            scheduler.next_frame(),
            model
                .iter()
                .flatten()
                .filter(|e| e.phase == Phase::Queued)
                .map(|e| e.requested)
                .min()
        );
    }
    let at = Frame(steps as u64 + 10);
    while let Some(token) = assert_no_heap(|| scheduler.deliver(at).map(|d| d.token)) {
        let entry = model
            .iter_mut()
            .flatten()
            .find(|e| e.sequence == token.sequence())
            .unwrap();
        entry.phase = Phase::Delivered;
        entry.delivered = Some(at);
        entry.token = Some(token);
    }
    for entry in model.iter_mut().flatten() {
        if entry.phase == Phase::Delivered {
            let value = entry.sequence.0 as u32;
            assert_no_heap(|| {
                scheduler
                    .receipts(at)
                    .complete(entry.token.unwrap(), Ok(value))
            })
            .unwrap();
            expected.push_back((*entry, at, value));
            entry.phase = Phase::Terminal;
        }
    }
    while let Some(actual) = assert_no_heap(|| results.try_execution()) {
        let (entry, frame, value) = expected.pop_front().unwrap();
        check_result(actual, entry, frame, value);
        consumed += 1;
    }
    assert!(expected.is_empty());
    assert!(admitted > 0);
    assert_eq!(admitted, consumed);
    assert_no_heap(|| scheduler.begin_callback());
    assert_eq!(scheduler.available(), 2);
    assert!(scheduler.recovery_available());
}

#[shoop_test]
fn disconnected_results_retain_pending_and_completed_receipts_without_reexecution() {
    let (reader, mut scheduler) = typed_scheduler::<u32, u32, (), ()>(config(), 0, true).unwrap();
    drop(reader);
    assert_no_heap(|| {
        scheduler
            .admit(Sequence(1), Frame(0), Frame(0), action(1), &[])
            .unwrap();
        scheduler
            .admit(Sequence(2), Frame(0), Frame(0), action(2), &[])
            .unwrap();
        let first = scheduler.deliver(Frame(0)).unwrap().token;
        let second = scheduler.deliver(Frame(0)).unwrap().token;
        scheduler
            .receipts(Frame(5))
            .complete(second, Ok(2))
            .unwrap();
        for _ in 0..100 {
            assert_eq!(scheduler.begin_callback(), 0);
            assert_eq!(scheduler.available(), 0);
            assert!(scheduler.deliver(Frame(99)).is_none());
        }
        scheduler
            .admit_recovery(Sequence(3), Frame(5), Frame(5), action(0), &[])
            .unwrap();
        let mut recovery = scheduler.deliver(Frame(5)).unwrap();
        recovery.receipts.complete(first, Ok(99)).unwrap();
        recovery.receipts.complete(recovery.token, Ok(0)).unwrap();
        assert!(!scheduler.recovery_pending());
        assert_eq!(scheduler.begin_callback(), 0);
    });
}
