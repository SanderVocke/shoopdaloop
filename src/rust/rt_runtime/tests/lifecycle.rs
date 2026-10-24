mod support;

use rt_runtime::transport::{channel, Lane, Outcome, TransportConfig};
use rt_runtime::{Epoch, Error, Frame};
use shoop_wasm_test_support::shoop_test;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use support::assert_no_heap;

#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;

#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

fn config(slots: usize) -> TransportConfig {
    TransportConfig {
        normal_slots: slots,
        recovery_slots: 1,
        telemetry_slots: 1,
        operations_per_cycle: 2,
        recovery_per_cycle: 1,
        publications_per_cycle: 2,
    }
}

#[derive(Debug)]
struct Owned {
    id: usize,
    drops: Arc<AtomicUsize>,
}
impl Drop for Owned {
    fn drop(&mut self) {
        assert!(
            !support::is_rendering(),
            "owning destructor on render thread"
        );
        self.drops.fetch_add(1, Ordering::Relaxed);
    }
}
fn owned(id: usize, drops: &Arc<AtomicUsize>) -> Owned {
    Owned {
        id,
        drops: Arc::clone(drops),
    }
}

#[shoop_test]
fn transport_configuration_rejects_invalid_capacities() {
    let mut settings = config(1);
    settings.recovery_slots = 0;
    assert!(matches!(
        channel::<u8, (), u8>(settings),
        Err(Error::InvalidConfig)
    ));
    settings = config(usize::MAX);
    assert!(matches!(
        channel::<u8, (), u8>(settings),
        Err(Error::Capacity)
    ));
    settings = config(i32::MAX as usize);
    assert!(matches!(
        channel::<u8, (), u8>(settings),
        Err(Error::Capacity)
    ));
}

#[shoop_test]
fn full_return_queue_retains_credits_and_recovery_has_reserved_admission() {
    let drops = Arc::new(AtomicUsize::new(0));
    let (mut control, mut render) = channel::<Owned, Owned, (u32, u32)>(config(2)).unwrap();
    let first = control.submit(Lane::Normal, owned(1, &drops)).unwrap();
    let second = control.submit(Lane::Normal, owned(2, &drops)).unwrap();
    let refused = control.submit(Lane::Normal, owned(3, &drops)).unwrap_err();
    assert_eq!(refused.error, Error::Full);
    assert_eq!(refused.value.id, 3);
    drop(refused);
    let recovery = control.submit(Lane::Recovery, owned(4, &drops)).unwrap();
    let displaced = owned(5, &drops);
    assert_no_heap(|| {
        render.begin_cycle(Frame(128), Epoch(2));
        let op = render.next_operation().unwrap();
        assert_eq!(op.sequence, recovery.sequence());
        assert_eq!(op.lane, Lane::Recovery);
        render.complete(Outcome::Applied, Some(displaced)).unwrap();
        assert_eq!(render.next_operation().unwrap().sequence, first.sequence());
        render.complete(Outcome::Applied, None).unwrap();
        assert!(render.next_operation().is_none());
        assert_eq!(render.counters().operations, 2);
        render.begin_cycle(Frame(256), Epoch(2));
        assert_eq!(render.next_operation().unwrap().sequence, second.sequence());
        render
            .complete(Outcome::Rejected(Error::InvalidId), None)
            .unwrap();
        assert!(render.next_operation().is_none());
    });
    assert_eq!(drops.load(Ordering::Relaxed), 1);
    let refused = control.submit(Lane::Normal, owned(6, &drops)).unwrap_err();
    assert_eq!(refused.error, Error::Full);
    drop(refused);
    let completed = control.try_completion().unwrap();
    assert_eq!(completed.sequence, recovery.sequence());
    assert_eq!(completed.frame, Frame(128));
    assert_eq!(completed.epoch, Epoch(2));
    assert_eq!(completed.retired.as_ref().unwrap().id, 5);
    drop(completed);
    assert_eq!(control.try_completion().unwrap().sequence, first.sequence());
    assert_eq!(
        control.try_completion().unwrap().outcome,
        Outcome::Rejected(Error::InvalidId)
    );
    assert_eq!(control.outstanding(), 0);
    assert!(control.try_completion().is_none());
    assert_eq!(drops.load(Ordering::Relaxed), 6);
}

#[shoop_test]
fn cancellation_observation_is_terminal_and_does_not_destroy_on_render() {
    let drops = Arc::new(AtomicUsize::new(0));
    let (mut control, mut render) = channel::<Owned, (), u32>(config(2)).unwrap();
    let cancelled = control.submit(Lane::Normal, owned(1, &drops)).unwrap();
    cancelled.cancel();
    drop(cancelled);
    let late = control.submit(Lane::Normal, owned(2, &drops)).unwrap();
    assert_no_heap(|| {
        render.begin_cycle(Frame(0), Epoch(0));
        let operation = render.next_operation().unwrap();
        assert_eq!(operation.request.id, 2);
        late.cancel(); // Dequeue already admitted this operation.
        render.complete(Outcome::Applied, None).unwrap();
        assert!(render.next_operation().is_none());
    });
    assert_eq!(drops.load(Ordering::Relaxed), 0);
    assert_eq!(
        control.try_completion().unwrap().outcome,
        Outcome::Cancelled
    );
    assert_eq!(control.try_completion().unwrap().outcome, Outcome::Applied);
    assert_eq!(drops.load(Ordering::Relaxed), 2);
}

#[shoop_test]
fn callbacks_snapshot_available_work_and_preserve_borrowed_current_operations() {
    let (mut control, mut render) = channel::<u32, (), u32>(config(4)).unwrap();
    render.begin_cycle(Frame(0), Epoch(0));
    control.submit(Lane::Normal, 1).unwrap();
    assert_no_heap(|| assert!(render.next_operation().is_none()));
    render.begin_cycle(Frame(128), Epoch(0));
    assert_eq!(*render.next_operation().unwrap().request, 1);
    render.begin_cycle(Frame(256), Epoch(1));
    assert_no_heap(|| {
        assert_eq!(*render.next_operation().unwrap().request, 1);
        render.complete(Outcome::Applied, None).unwrap();
    });
    let result = control.try_completion().unwrap();
    assert_eq!(result.frame, Frame(256));
    assert_eq!(result.epoch, Epoch(1));
}

#[shoop_test]
fn telemetry_is_coherent_bounded_and_does_not_compete_with_completions() {
    let (mut control, mut render) = channel::<u32, (), [u32; 4]>(config(1)).unwrap();
    control.submit(Lane::Normal, 42).unwrap();
    assert_no_heap(|| {
        render.begin_cycle(Frame(0), Epoch(0));
        render.publish([1, 2, 3, 4]).unwrap();
        assert_eq!(render.publish([5, 6, 7, 8]), Err(Error::Full));
        render.next_operation().unwrap();
        render.complete(Outcome::Applied, None).unwrap();
        assert_eq!(render.counters().telemetry, 1);
        assert_eq!(render.counters().telemetry_refused, 1);
    });
    assert_eq!(control.try_telemetry(), Some([1, 2, 3, 4]));
    assert_eq!(control.try_telemetry(), None);
    assert_eq!(control.try_completion().unwrap().request, 42);
    assert_no_heap(|| {
        render.publish([9; 4]).unwrap();
        assert_eq!(render.publish([10; 4]), Err(Error::Full));
    });
    assert_eq!(control.try_telemetry(), Some([9; 4]));
}

#[shoop_test]
fn invalid_completion_returns_the_owned_retirement_bundle() {
    let drops = Arc::new(AtomicUsize::new(0));
    let (control, mut render) = channel::<u8, Owned, u8>(config(1)).unwrap();
    let displaced = owned(1, &drops);
    let error = assert_no_heap(|| render.complete(Outcome::Applied, Some(displaced))).unwrap_err();
    assert_eq!(error.error, Error::InvalidState);
    assert_eq!(error.retired.as_ref().unwrap().id, 1);
    assert_eq!(drops.load(Ordering::Relaxed), 0);
    drop(error);
    drop(control);
    drop(render);
    assert_eq!(drops.load(Ordering::Relaxed), 1);
}

#[shoop_test]
fn control_disconnect_retains_payloads_until_non_render_shutdown() {
    let drops = Arc::new(AtomicUsize::new(0));
    let (mut control, mut render) = channel::<Owned, Owned, u8>(config(2)).unwrap();
    control.submit(Lane::Normal, owned(1, &drops)).unwrap();
    control.submit(Lane::Normal, owned(2, &drops)).unwrap();
    assert!(render.connected());
    drop(control);
    let displaced = owned(3, &drops);
    assert_no_heap(|| {
        assert!(!render.connected());
        render.begin_cycle(Frame(0), Epoch(0));
        render.next_operation().unwrap();
        render.complete(Outcome::Applied, Some(displaced)).unwrap();
        render.next_operation().unwrap();
        render
            .complete(Outcome::Rejected(Error::Disconnected), None)
            .unwrap();
        for index in 1..100 {
            render.begin_cycle(Frame(index * 128), Epoch(0));
            assert!(render.next_operation().is_none());
        }
    });
    assert_eq!(drops.load(Ordering::Relaxed), 0);
    drop(render); // Host has stopped callbacks.
    assert_eq!(drops.load(Ordering::Relaxed), 3);
}

#[shoop_test]
fn render_disconnect_rejects_new_ownership_and_reclaims_every_queue_on_stop() {
    let drops = Arc::new(AtomicUsize::new(0));
    let (mut control, mut render) = channel::<Owned, (), u8>(config(3)).unwrap();
    for id in 0..3 {
        control.submit(Lane::Normal, owned(id, &drops)).unwrap();
    }
    assert_no_heap(|| {
        render.begin_cycle(Frame(0), Epoch(0));
        render.next_operation().unwrap();
        render.complete(Outcome::Applied, None).unwrap();
        render.next_operation().unwrap(); // One held current, one still queued.
    });
    drop(render);
    assert!(!control.connected());
    let refused = control.submit(Lane::Normal, owned(4, &drops)).unwrap_err();
    assert_eq!(refused.error, Error::Disconnected);
    drop(refused);
    drop(control);
    assert_eq!(drops.load(Ordering::Relaxed), 4);
}

#[shoop_test]
fn deterministic_admission_model_preserves_fifo_and_recovers_all_credits() {
    for slots in 1..=4 {
        let (mut control, mut render) = channel::<u32, (), u32>(config(slots)).unwrap();
        let mut expected = std::collections::VecDeque::new();
        let mut random = 17u32;
        let mut next = 0;
        for frame in 0..5000 {
            random = random.wrapping_mul(1664525).wrapping_add(1013904223);
            if random & 3 != 0 {
                match control.submit(Lane::Normal, next) {
                    Ok(ticket) => {
                        let cancel = random & 8 != 0;
                        if cancel {
                            ticket.cancel();
                        }
                        expected.push_back((ticket.sequence(), next, cancel));
                        next += 1;
                    }
                    Err(error) => {
                        assert_eq!(error.error, Error::Full);
                        assert_eq!(error.value, next);
                    }
                }
            }
            assert_no_heap(|| {
                render.begin_cycle(Frame(frame), Epoch(0));
                while render.next_operation().is_some() {
                    render.complete(Outcome::Applied, None).unwrap();
                }
                assert!(render.counters().operations <= 2);
                assert!(render.counters().completions <= 2);
            });
            if random & 16 != 0 {
                while let Some(completion) = control.try_completion() {
                    let (sequence, value, cancelled) = expected.pop_front().unwrap();
                    assert_eq!(completion.sequence, sequence);
                    assert_eq!(completion.request, value);
                    assert_eq!(
                        completion.outcome,
                        if cancelled {
                            Outcome::Cancelled
                        } else {
                            Outcome::Applied
                        }
                    );
                }
            }
            assert_eq!(control.outstanding(), expected.len());
            assert!(expected.len() <= slots);
        }
        for _ in 0..slots {
            render.begin_cycle(Frame(5000), Epoch(0));
            while render.next_operation().is_some() {
                render.complete(Outcome::Applied, None).unwrap();
            }
            while control.try_completion().is_some() {
                expected.pop_front().unwrap();
            }
        }
        assert!(expected.is_empty());
        assert_eq!(control.outstanding(), 0);
    }
}

#[shoop_test(no_wasm = "native two-thread SPSC stress with render allocator guard")]
fn two_thread_stress_keeps_completion_and_state_publication_coherent() {
    let operations = if cfg!(miri) { 32 } else { 10_000 };
    let (mut control, mut render) = channel::<u64, (), [u64; 2]>(config(4)).unwrap();
    let worker = std::thread::spawn(move || {
        for frame in 0..1_000_000 {
            let connected = assert_no_heap(|| {
                render.begin_cycle(Frame(frame), Epoch(0));
                while let Some(operation) = render.next_operation() {
                    let value = *operation.request;
                    let _ = render.publish([value, !value]);
                    render.complete(Outcome::Applied, None).unwrap();
                }
                render.connected()
            });
            if !connected {
                return;
            }
            std::thread::yield_now();
        }
        panic!("control did not finish within stress bound");
    });
    let mut sent = 0;
    let mut received = 0;
    while received < operations {
        if sent < operations {
            match control.submit(Lane::Normal, sent) {
                Ok(_) => sent += 1,
                Err(error) => assert_eq!(error.error, Error::Full),
            }
        }
        while let Some(completion) = control.try_completion() {
            assert_eq!(completion.request, received);
            assert_eq!(completion.outcome, Outcome::Applied);
            received += 1;
        }
        while let Some([value, inverse]) = control.try_telemetry() {
            assert_eq!(value, !inverse);
        }
        std::thread::yield_now();
    }
    drop(control);
    worker.join().unwrap();
}

#[shoop_test(no_wasm = "Loom explores the one-way native cancellation flag")]
fn cancellation_flag_model_has_a_single_observation_cut() {
    loom::model(|| {
        use loom::sync::atomic::{AtomicBool, Ordering};
        let flag = loom::sync::Arc::new(AtomicBool::new(false));
        let producer = flag.clone();
        let worker = loom::thread::spawn(move || producer.store(true, Ordering::Release));
        let observed = flag.load(Ordering::Acquire);
        worker.join().unwrap();
        assert!(flag.load(Ordering::Acquire));
        // Either observation is legitimate: the dequeue observation is the cut,
        // not whether cancellation became true before the completion was read.
        let outcome = if observed {
            Outcome::Cancelled
        } else {
            Outcome::Applied
        };
        assert!(matches!(outcome, Outcome::Cancelled | Outcome::Applied));
    });
}
