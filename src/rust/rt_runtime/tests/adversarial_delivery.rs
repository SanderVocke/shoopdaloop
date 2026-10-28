mod support;

use rt_runtime::schedule::{typed_scheduler, Action, Execution, ReceiptToken};
use rt_runtime::transport::Lane;
use rt_runtime::{Budgets, Config, Error, Frame, PlannerId, Sequence};
use shoop_wasm_test_support::shoop_test;
use std::collections::VecDeque;
use support::assert_no_heap;

#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    Queued,
    Delivered,
    Terminal,
    Read,
}
#[derive(Clone)]
struct Entry {
    sequence: Sequence,
    requested: Frame,
    effective: Frame,
    lane: Lane,
    phase: Phase,
    delivered: Option<Frame>,
    token: Option<ReceiptToken>,
    payload: Vec<u8>,
}
fn action(sequence: Sequence) -> Action<u32> {
    Action::Planner {
        planner: PlannerId::from_parts(0, 1).unwrap(),
        operation: sequence.0 as u32,
    }
}
fn random(seed: &mut u64) -> u64 {
    *seed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    *seed >> 16
}
fn check_result(actual: Execution<u32, u32>, entry: &Entry, at: Frame, result: Result<u32, Error>) {
    assert_eq!(actual.sequence, entry.sequence);
    assert_eq!(actual.action, action(entry.sequence));
    assert_eq!(actual.lane, entry.lane);
    assert_eq!(actual.requested_frame, entry.requested);
    assert_eq!(actual.delivered_frame, entry.delivered);
    assert_eq!(actual.actual_frame, at);
    assert_eq!(actual.result, result.map(|_| ()));
    assert_eq!(actual.operation_result, result.ok());
    assert_eq!(
        actual.late_by(),
        entry
            .delivered
            .map_or(0, |frame| frame.0.saturating_sub(entry.requested.0))
    );
}

#[shoop_test]
fn seeded_typed_payloads_receipts_and_recycled_tokens_match_a_full_length_independent_model() {
    let (mut witness, counts) =
        support::measure(|| Vec::<u8>::with_capacity(std::hint::black_box(1)));
    assert!(counts.allocations > 0);
    let (_, counts) = support::measure(|| witness.reserve_exact(std::hint::black_box(129)));
    assert!(counts.reallocations > 0);
    std::hint::black_box(&witness);
    let (_, counts) = support::measure(|| drop(witness));
    assert!(counts.deallocations > 0);
    let config = Config {
        scheduled_events: 2,
        midi_bytes_per_port: 4,
        page_bytes: 64,
        budgets: Budgets {
            events: 2,
            subranges: 0,
            ..Budgets::default()
        },
        ..Config::default()
    }
    .prepare()
    .unwrap();
    let mut coverage = [0usize; 12];
    let mut outcomes = [0usize; 18];
    for mut seed in [0x91a3_4218_u64, 0x725c_8931, 0xf163_8a97, 0xa831_7905] {
        let (mut reader, mut scheduler) = typed_scheduler::<u32, u32>(config, 4, true).unwrap();
        let (_foreign_reader, mut foreign) = typed_scheduler::<u32, u32>(config, 4, true).unwrap();
        let foreign_token = assert_no_heap(|| {
            foreign
                .admit(Sequence(1), Frame(0), Frame(0), action(Sequence(1)), &[])
                .unwrap();
            foreign.deliver(Frame(0)).unwrap().token
        });
        let mut entries: [Option<Entry>; 3] = std::array::from_fn(|_| None);
        let mut expected = VecDeque::new();
        let mut stale = Vec::new();
        let mut admitted = 0;
        let mut consumed = 0;
        for step in 0..512_u64 {
            let bits = random(&mut seed);
            let choice = bits as usize % 12;
            let index = (bits >> 16) as usize % 3;
            let at = Frame((1_u64 << 63) + step);
            coverage[choice] += 1;
            match choice {
                0 | 1 | 10 => {
                    let recovery = choice == 1;
                    let slot = if recovery {
                        entries[2].is_none().then_some(2)
                    } else {
                        entries[..2].iter().position(Option::is_none)
                    };
                    let sequence = Sequence((1_u64 << 63) + step * 4 + 2);
                    let requested = if recovery {
                        Frame(at.0 - bits % 3)
                    } else if bits & 0x100 != 0 {
                        Frame(at.0 - bits % 4)
                    } else {
                        Frame(at.0 + bits % 8)
                    };
                    let mut payload = if recovery {
                        vec![]
                    } else {
                        sequence.0.to_le_bytes()[..(bits as usize % 5)].to_vec()
                    };
                    if choice == 10 {
                        let before = (
                            scheduler.available(),
                            scheduler.recovery_available(),
                            scheduler.pending(),
                            scheduler.next_frame(),
                        );
                        let bad = assert_no_heap(|| {
                            scheduler.admit(
                                Sequence(sequence.0 + 1),
                                requested,
                                at,
                                action(sequence),
                                &[0; 5],
                            )
                        });
                        assert_eq!(
                            bad,
                            Err(if slot.is_some() {
                                Error::Capacity
                            } else {
                                Error::Full
                            })
                        );
                        assert_eq!(
                            (
                                scheduler.available(),
                                scheduler.recovery_available(),
                                scheduler.pending(),
                                scheduler.next_frame()
                            ),
                            before
                        );
                        outcomes[10] += usize::from(slot.is_some());
                    }
                    let result = assert_no_heap(|| {
                        if recovery {
                            scheduler.admit_recovery(
                                sequence,
                                requested,
                                at,
                                action(sequence),
                                &payload,
                            )
                        } else {
                            scheduler.admit(sequence, requested, at, action(sequence), &payload)
                        }
                    });
                    if let Some(slot) = slot {
                        let admission = result.unwrap();
                        assert_eq!(admission.sequence, sequence);
                        assert_eq!(admission.effective_frame, requested.max(at));
                        assert_eq!(admission.late_by, at.0.saturating_sub(requested.0));
                        entries[slot] = Some(Entry {
                            sequence,
                            requested,
                            effective: requested.max(at),
                            lane: if recovery {
                                Lane::Recovery
                            } else {
                                Lane::Normal
                            },
                            phase: Phase::Queued,
                            delivered: None,
                            token: None,
                            payload: payload.clone(),
                        });
                        admitted += 1;
                        outcomes[usize::from(recovery)] += 1;
                    } else {
                        assert_eq!(result, Err(Error::Full));
                        outcomes[12 + usize::from(recovery)] += 1;
                    }
                    payload.fill(0xe7);
                }
                2 => {
                    let due = entries
                        .iter()
                        .enumerate()
                        .filter_map(|(index, entry)| {
                            entry
                                .as_ref()
                                .filter(|entry| {
                                    entry.phase == Phase::Queued && entry.effective <= at
                                })
                                .map(|entry| (index, entry.effective, entry.sequence))
                        })
                        .min_by_key(|(_, frame, sequence)| (*frame, *sequence));
                    let actual = assert_no_heap(|| {
                        scheduler.deliver(at).map(|delivery| {
                            let mut payload = [0; 4];
                            payload[..delivery.payload.len()].copy_from_slice(delivery.payload);
                            (
                                delivery.token,
                                delivery.action,
                                delivery.lane,
                                delivery.requested_frame,
                                delivery.frame,
                                payload,
                                delivery.payload.len(),
                            )
                        })
                    });
                    if let Some((slot, _, _)) = due {
                        let (token, operation, lane, requested, delivered, payload, count) =
                            actual.unwrap();
                        let entry = entries[slot].as_mut().unwrap();
                        assert_eq!(token.sequence(), entry.sequence);
                        assert_eq!(operation, action(entry.sequence));
                        assert_eq!(lane, entry.lane);
                        assert_eq!(requested, entry.requested);
                        assert_eq!(delivered, at);
                        assert_eq!(&payload[..count], entry.payload);
                        outcomes[2] += 1;
                        outcomes[16] += usize::from(count > 0);
                        entry.phase = Phase::Delivered;
                        entry.delivered = Some(at);
                        entry.token = Some(token);
                    } else {
                        assert!(actual.is_none());
                    }
                }
                3 | 4 => {
                    if let Some(entry) = entries[index].as_mut() {
                        let cancel = choice == 4;
                        if (!cancel && entry.phase == Phase::Delivered)
                            || (cancel
                                && entry.phase == Phase::Queued
                                && entry.lane == Lane::Normal)
                        {
                            let result = if cancel {
                                Ok(u32::MAX)
                            } else if bits & 0x1000 != 0 {
                                Err(Error::InvalidEvent)
                            } else {
                                Ok(entry.sequence.0 as u32)
                            };
                            assert_no_heap(|| {
                                if cancel {
                                    scheduler
                                        .receipts(at)
                                        .cancel_queued(entry.sequence, result.unwrap())
                                } else {
                                    scheduler
                                        .receipts(at)
                                        .complete(entry.token.unwrap(), result)
                                }
                            })
                            .unwrap();
                            expected.push_back((entry.clone(), at, result));
                            if cancel {
                                outcomes[4] += 1;
                            } else {
                                outcomes[if result.is_ok() { 3 } else { 14 }] += 1;
                                outcomes[17] += usize::from(entry.delivered != Some(at));
                            }
                            entry.phase = Phase::Terminal;
                        }
                    }
                }
                5 => {
                    let actual = assert_no_heap(|| reader.try_execution());
                    if let Some((entry, frame, result)) = expected.pop_front() {
                        check_result(actual.unwrap(), &entry, frame, result);
                        consumed += 1;
                        outcomes[5] += 1;
                        entries
                            .iter_mut()
                            .flatten()
                            .find(|current| current.sequence == entry.sequence)
                            .unwrap()
                            .phase = Phase::Read;
                    } else {
                        assert!(actual.is_none());
                    }
                }
                6 => {
                    let count = entries
                        .iter()
                        .flatten()
                        .filter(|entry| entry.phase == Phase::Read)
                        .count();
                    assert_eq!(assert_no_heap(|| scheduler.begin_callback()), count);
                    outcomes[6] += usize::from(count > 0);
                    for entry in &mut entries {
                        if entry
                            .as_ref()
                            .is_some_and(|entry| entry.phase == Phase::Read)
                        {
                            if let Some(token) = entry.as_ref().unwrap().token {
                                stale.push(token);
                            }
                            *entry = None;
                        }
                    }
                }
                7 => {
                    assert_eq!(
                        assert_no_heap(|| scheduler.receipts(at).complete(foreign_token, Ok(99))),
                        Err(Error::InvalidId)
                    );
                    outcomes[7] += 1;
                }
                8 => {
                    if let Some(entry) = &entries[index] {
                        if entry.phase == Phase::Delivered {
                            assert_eq!(
                                assert_no_heap(|| scheduler
                                    .receipts(Frame(entry.delivered.unwrap().0 - 1))
                                    .complete(entry.token.unwrap(), Ok(99))),
                                Err(Error::InvalidFrame)
                            );
                            outcomes[8] += 1;
                        }
                    }
                }
                9 => {
                    if !stale.is_empty() {
                        let token = stale[(bits >> 24) as usize % stale.len()];
                        assert_eq!(
                            assert_no_heap(|| scheduler.receipts(at).complete(token, Ok(99))),
                            Err(Error::InvalidId)
                        );
                        outcomes[9] += 1;
                    }
                }
                11 => {
                    if let Some(entry) = &entries[index] {
                        if matches!(entry.phase, Phase::Terminal | Phase::Read) {
                            if let Some(token) = entry.token {
                                assert_eq!(
                                    assert_no_heap(|| scheduler
                                        .receipts(at)
                                        .complete(token, Ok(99))),
                                    Err(Error::InvalidState)
                                );
                                outcomes[11] += 1;
                            }
                        }
                    }
                }
                _ => unreachable!(),
            }
            outcomes[15] += usize::from(entries.iter().all(Option::is_some));
            assert_eq!(
                scheduler.available(),
                entries[..2].iter().filter(|entry| entry.is_none()).count()
            );
            assert_eq!(scheduler.recovery_available(), entries[2].is_none());
            assert_eq!(
                scheduler.pending(),
                entries
                    .iter()
                    .flatten()
                    .filter(|entry| entry.phase == Phase::Queued)
                    .count()
            );
            assert_eq!(
                scheduler.next_frame(),
                entries
                    .iter()
                    .flatten()
                    .filter(|entry| entry.phase == Phase::Queued)
                    .map(|entry| entry.effective)
                    .min()
            );
        }
        let at = Frame(u64::MAX);
        while let Some(token) =
            assert_no_heap(|| scheduler.deliver(at).map(|delivery| delivery.token))
        {
            let entry = entries
                .iter_mut()
                .flatten()
                .find(|entry| entry.sequence == token.sequence())
                .unwrap();
            entry.phase = Phase::Delivered;
            entry.delivered = Some(at);
            entry.token = Some(token);
        }
        for entry in entries.iter_mut().flatten() {
            if entry.phase == Phase::Delivered {
                let result = Ok(entry.sequence.0 as u32);
                assert_no_heap(|| {
                    scheduler
                        .receipts(at)
                        .complete(entry.token.unwrap(), result)
                })
                .unwrap();
                expected.push_back((entry.clone(), at, result));
            }
        }
        while let Some(actual) = assert_no_heap(|| reader.try_execution()) {
            let (entry, frame, result) = expected.pop_front().unwrap();
            check_result(actual, &entry, frame, result);
            consumed += 1;
        }
        assert!(expected.is_empty());
        assert_eq!(admitted, consumed);
        assert!(admitted > 0 && !stale.is_empty());
        assert_no_heap(|| scheduler.begin_callback());
        assert_eq!(scheduler.available(), 2);
        assert!(scheduler.recovery_available());
        for lane in [Lane::Normal, Lane::Recovery] {
            let sequence = Sequence(u64::MAX);
            assert_no_heap(|| {
                if lane == Lane::Normal {
                    scheduler.admit(sequence, at, at, action(sequence), &[])
                } else {
                    scheduler.admit_recovery(sequence, at, at, action(sequence), &[])
                }
            })
            .unwrap();
            let token = assert_no_heap(|| scheduler.deliver(at).unwrap().token);
            assert_no_heap(|| scheduler.receipts(at).complete(token, Ok(7))).unwrap();
            assert_eq!(
                assert_no_heap(|| reader.try_execution())
                    .unwrap()
                    .operation_result,
                Some(7)
            );
            assert_no_heap(|| scheduler.begin_callback());
            let result = assert_no_heap(|| {
                if lane == Lane::Normal {
                    scheduler.admit(sequence, at, at, action(sequence), &[])
                } else {
                    scheduler.admit_recovery(sequence, at, at, action(sequence), &[])
                }
            });
            assert_eq!(result, Err(Error::InvalidId));
        }
        assert_eq!(scheduler.available(), 2);
        assert!(scheduler.recovery_available());
        assert_eq!(scheduler.pending(), 0);
    }
    assert!(coverage.iter().all(|count| *count > 0));
    assert!(
        outcomes.iter().all(|count| *count > 0),
        "outcomes: {outcomes:?}"
    );
}
