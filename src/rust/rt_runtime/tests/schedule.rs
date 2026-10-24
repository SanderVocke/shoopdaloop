mod support;

use rt_runtime::schedule::{scheduler, Action};
use rt_runtime::{Budgets, Config, Error, Frame, NodeId, Sequence};
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;

#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

fn config(capacity: usize) -> rt_runtime::PreparedConfig {
    Config {
        scheduled_events: capacity,
        midi_bytes_per_port: 16,
        budgets: Budgets {
            events: capacity,
            subranges: capacity + 1,
            ..Budgets::default()
        },
        ..Config::default()
    }
    .prepare()
    .unwrap()
}
fn node(slot: u32) -> NodeId {
    NodeId::from_parts(slot, 1).unwrap()
}
fn parameter(slot: u32, key: u32) -> Action {
    Action::Parameter {
        node: node(slot),
        parameter: key,
        value: 0.5,
    }
}

#[shoop_test]
fn admission_reserves_event_split_and_reliable_result_credits() {
    let (mut results, mut schedule) = scheduler(config(2)).unwrap();
    assert_eq!(schedule.capacity(), 2);
    assert_no_heap(|| {
        schedule
            .admit(Sequence(1), Frame(100), Frame(0), parameter(0, 0), &[])
            .unwrap();
        schedule
            .admit(Sequence(2), Frame(200), Frame(0), parameter(0, 0), &[])
            .unwrap();
        assert_eq!(
            schedule.admit(Sequence(3), Frame(300), Frame(0), parameter(0, 0), &[]),
            Err(Error::Full)
        );
        assert!(schedule.due(Frame(99)).is_none());
        assert_eq!(schedule.due(Frame(100)).unwrap().sequence, Sequence(1));
        schedule.finish(Sequence(1), Frame(100), Ok(())).unwrap();
        schedule
            .finish(Sequence(2), Frame(200), Err(Error::InvalidId))
            .unwrap();
        assert_eq!(schedule.pending(), 0);
        assert_eq!(schedule.available(), 0);
        assert_eq!(
            schedule.admit(Sequence(3), Frame(300), Frame(0), parameter(0, 0), &[]),
            Err(Error::Full)
        );
    });
    let first = results.try_execution().unwrap();
    assert_eq!(first.sequence, Sequence(1));
    assert_eq!(first.actual_frame, Frame(100));
    assert_eq!(first.result, Ok(()));
    assert_eq!(first.late_by(), 0);
    assert_no_heap(|| {
        assert_eq!(schedule.begin_callback(), 1);
        schedule
            .admit(Sequence(3), Frame(300), Frame(200), parameter(0, 0), &[])
            .unwrap();
        schedule.finish(Sequence(3), Frame(300), Ok(())).unwrap();
    });
    assert_eq!(
        results.try_execution().unwrap().result,
        Err(Error::InvalidId)
    );
    assert_eq!(results.try_execution().unwrap().sequence, Sequence(3));
    assert!(results.try_execution().is_none());
    assert_no_heap(|| assert_eq!(schedule.begin_callback(), 2));
    assert_eq!(schedule.available(), 2);
}

#[shoop_test]
fn same_time_actions_order_by_node_kind_port_and_sequence() {
    let (mut results, mut schedule) = scheduler(config(5)).unwrap();
    assert_no_heap(|| {
        schedule
            .admit(
                Sequence(1),
                Frame(9),
                Frame(0),
                Action::Midi {
                    node: node(1),
                    port: 0,
                },
                &[0xf8],
            )
            .unwrap();
        schedule
            .admit(
                Sequence(2),
                Frame(9),
                Frame(0),
                Action::Midi {
                    node: node(0),
                    port: 0,
                },
                &[0xfa],
            )
            .unwrap();
        schedule
            .admit(Sequence(3), Frame(9), Frame(0), parameter(0, 1), &[])
            .unwrap();
        schedule
            .admit(Sequence(4), Frame(9), Frame(0), parameter(0, 0), &[])
            .unwrap();
        schedule
            .admit(
                Sequence(5),
                Frame(9),
                Frame(0),
                Action::Midi {
                    node: node(0),
                    port: 0,
                },
                &[0xfc],
            )
            .unwrap();
        for sequence in [4, 3, 2, 5, 1] {
            assert_eq!(schedule.due(Frame(9)).unwrap().sequence, Sequence(sequence));
            schedule
                .finish(Sequence(sequence), Frame(9), Ok(()))
                .unwrap();
        }
    });
    for sequence in [4, 3, 2, 5, 1] {
        assert_eq!(
            results.try_execution().unwrap().sequence,
            Sequence(sequence)
        );
    }
}

#[shoop_test]
fn late_timestamps_and_long_clocks_are_explicit_without_wrapping() {
    let (mut results, mut schedule) = scheduler(config(2)).unwrap();
    assert_no_heap(|| {
        let admission = schedule
            .admit(Sequence(1), Frame(3), Frame(10), parameter(0, 0), &[])
            .unwrap();
        assert_eq!(admission.effective_frame, Frame(10));
        assert_eq!(admission.late_by, 7);
        assert_eq!(
            schedule.finish(Sequence(1), Frame(9), Ok(())),
            Err(Error::InvalidFrame)
        );
        assert_eq!(
            schedule.finish(Sequence(999), Frame(10), Ok(())),
            Err(Error::InvalidId)
        );
        schedule.finish(Sequence(1), Frame(12), Ok(())).unwrap();
        let admission = schedule
            .admit(
                Sequence(2),
                Frame(u64::MAX - 1),
                Frame(u64::MAX - 2),
                parameter(0, 0),
                &[],
            )
            .unwrap();
        assert_eq!(admission.effective_frame, Frame(u64::MAX - 1));
        assert!(schedule.due(Frame(u64::MAX - 2)).is_none());
        schedule
            .finish(Sequence(2), Frame(u64::MAX - 1), Ok(()))
            .unwrap();
    });
    assert_eq!(results.try_execution().unwrap().late_by(), 9);
    assert_eq!(results.try_execution().unwrap().late_by(), 0);
}

#[shoop_test]
fn malformed_payloads_and_duplicate_sequences_cannot_consume_credits() {
    let (_results, mut schedule) = scheduler(config(2)).unwrap();
    assert_no_heap(|| {
        assert_eq!(
            schedule.admit(Sequence(0), Frame(0), Frame(0), parameter(0, 0), &[]),
            Err(Error::InvalidId)
        );
        assert_eq!(
            schedule.admit(Sequence(1), Frame(0), Frame(0), parameter(0, 0), &[1]),
            Err(Error::InvalidEvent)
        );
        assert_eq!(
            schedule.admit(
                Sequence(1),
                Frame(0),
                Frame(0),
                Action::Parameter {
                    node: node(0),
                    parameter: 0,
                    value: f32::NAN
                },
                &[]
            ),
            Err(Error::InvalidEvent)
        );
        let midi = Action::Midi {
            node: node(0),
            port: 0,
        };
        assert_eq!(
            schedule.admit(Sequence(1), Frame(0), Frame(0), midi, &[0x90, 60]),
            Err(Error::InvalidEvent)
        );
        assert_eq!(
            schedule.admit(Sequence(1), Frame(0), Frame(0), midi, &[0; 17]),
            Err(Error::Capacity)
        );
        assert_eq!(schedule.available(), 2);
        schedule
            .admit(
                Sequence(1),
                Frame(0),
                Frame(0),
                midi,
                &[0xf0, 0x7d, 1, 0xf7],
            )
            .unwrap();
        assert_eq!(
            schedule.admit(Sequence(1), Frame(1), Frame(0), midi, &[0xf8]),
            Err(Error::InvalidId)
        );
        assert_eq!(schedule.available(), 1);
        let due = schedule.due(Frame(0)).unwrap();
        assert_eq!(due.payload, &[0xf0, 0x7d, 1, 0xf7]);
    });
}

#[shoop_test]
fn conservative_reservations_guarantee_work_for_arbitrary_block_partitions() {
    for event_budget in 1..=8 {
        let limits = Config {
            scheduled_events: 64,
            budgets: Budgets {
                events: event_budget,
                subranges: 4,
                ..Budgets::default()
            },
            ..Config::default()
        }
        .prepare()
        .unwrap();
        let (mut results, mut schedule) = scheduler(limits).unwrap();
        let capacity = event_budget.min(3);
        assert_eq!(schedule.capacity(), capacity);
        assert_no_heap(|| {
            for index in 0..capacity {
                schedule
                    .admit(
                        Sequence(index as u64 + 1),
                        Frame(index as u64 * 3),
                        Frame(0),
                        parameter(0, 0),
                        &[],
                    )
                    .unwrap();
            }
            let mut start = 0;
            for length in [1, 2, 3, 8] {
                let end = start + length;
                let mut applied = 0;
                let mut splits = 1;
                while let Some(at) = schedule.next_frame() {
                    if at.0 >= end {
                        break;
                    }
                    let due = schedule.due(at).unwrap();
                    let sequence = due.sequence;
                    if at.0 > start {
                        splits += 1;
                    }
                    schedule.finish(sequence, at, Ok(())).unwrap();
                    applied += 1;
                }
                assert!(applied <= event_budget);
                assert!(splits <= 4);
                start = end;
            }
        });
        let mut completed = 0;
        while results.try_execution().is_some() {
            completed += 1;
        }
        assert_eq!(completed, capacity);
    }
}

#[shoop_test]
fn disconnected_result_reader_saturates_but_never_loses_or_reexecutes_events() {
    let (results, mut schedule) = scheduler(config(1)).unwrap();
    drop(results);
    assert_no_heap(|| {
        schedule
            .admit(Sequence(1), Frame(0), Frame(0), parameter(0, 0), &[])
            .unwrap();
        schedule.finish(Sequence(1), Frame(0), Ok(())).unwrap();
        for _ in 0..100 {
            assert_eq!(schedule.begin_callback(), 0);
            assert!(schedule.due(Frame(100)).is_none());
            assert_eq!(
                schedule.admit(Sequence(2), Frame(100), Frame(100), parameter(0, 0), &[]),
                Err(Error::Full)
            );
        }
    });
    drop(schedule);
}
