use rt_fpga_reference::midi::{ContentStore, Error, Message, MidiState, StoreLimits};
use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};
#[global_allocator]
static ALLOCATOR: shoop_wasm_test_support::heap::TrackingAllocator =
    shoop_wasm_test_support::heap::TrackingAllocator;

fn limits() -> StoreLimits {
    StoreLimits {
        block_events: 3,
        blocks: 8,
        max_events: 12,
        views: 8,
        snapshots: 4,
        retirements: 4,
    }
}
fn msg(v: u8) -> Message {
    Message::managed(&[0xb0, 1, v]).unwrap()
}

#[shoop_test]
fn retained_nested_ranges_survive_append_source_release_and_reuse() {
    let mut store = ContentStore::prepare(limits()).unwrap();
    assert_no_heap(|| {
        let state = store.snapshot(&MidiState::default()).unwrap();
        let source = store.writer(state).unwrap();
        for i in 0..8 {
            store.append(source, 10 + i * 2, msg(i as u8)).unwrap();
        }
        store.advance(source, 40).unwrap();
        let range = store.range(source, 13, 30, state).unwrap();
        let nested = store.range(range, 2, 12, state).unwrap();
        store.append(source, 40, msg(99)).unwrap();
        store.advance(source, 50).unwrap();
        store.release(source).unwrap();
        store.release(range).unwrap();
        store.release_snapshot(state).unwrap();
        assert_eq!(store.describe(nested).unwrap().frames, 10);
        let expected = [(1, 3), (3, 4), (5, 5), (7, 6), (9, 7)];
        for (i, (frame, value)) in expected.into_iter().enumerate() {
            let event = store.read(nested, i as u32).unwrap().unwrap();
            assert_eq!((event.frame, event.message), (frame, msg(value)));
        }
        while store.maintenance(1) != 0 {}
        assert_eq!(store.usage().views, 1);
        let retained = store.retain(nested).unwrap();
        store.release(nested).unwrap();
        assert!(store.read(retained, 0).unwrap().is_some());
        store.release(retained).unwrap();
        while store.maintenance(1) != 0 {}
        assert_eq!(store.usage(), Default::default());
        assert_eq!(store.describe(nested), Err(Error::InvalidState));
        let next_state = store.snapshot(&MidiState::default()).unwrap();
        assert_ne!(next_state, state);
        assert_eq!(store.state(state), Err(Error::InvalidState));
    });
}

#[shoop_test]
fn retirement_queue_refusal_is_atomic_and_release_does_not_walk_blocks() {
    let mut l = limits();
    l.retirements = 1;
    let mut store = ContentStore::prepare(l).unwrap();
    let state = store.snapshot(&MidiState::default()).unwrap();
    let a = store.writer(state).unwrap();
    let b = store.writer(state).unwrap();
    for i in 0..12 {
        store.append(a, i, msg(i as u8)).unwrap();
    }
    store.finish(a, 12).unwrap();
    store.finish(b, 12).unwrap();
    let work = store.work();
    store.release(a).unwrap();
    assert_eq!(store.work() - work, 1);
    assert_eq!(store.release(b), Err(Error::Capacity));
    assert_eq!(store.describe(b).unwrap().frames, 12);
    assert_eq!(store.maintenance(1), 1);
    assert_eq!(store.usage().retirements, 1);
    while store.maintenance(1) != 0 {}
    store.release(b).unwrap();
}

#[shoop_test]
fn history_prefix_discard_preserves_pinned_data_and_stable_equal_time_order() {
    let mut store = ContentStore::prepare(limits()).unwrap();
    let state = store.snapshot(&MidiState::default()).unwrap();
    let writer = store.writer(state).unwrap();
    for i in 0..9 {
        store.append(writer, i / 2, msg(i as u8)).unwrap();
    }
    store.advance(writer, 10).unwrap();
    let frozen = store.range(writer, 1, 4, state).unwrap();
    assert_eq!(store.discard_before(writer, 3).unwrap(), 6);
    for i in 0..6 {
        store.append(writer, 11 + i, msg(30 + i as u8)).unwrap();
    }
    assert_eq!(store.describe(frozen).unwrap().events, 6);
    for i in 0..6 {
        assert_eq!(
            store.read(frozen, i).unwrap().unwrap().message,
            msg(i as u8 + 2)
        );
    }
    let e = store.read(frozen, 0).unwrap().unwrap();
    assert_eq!((e.frame, e.order), (0, 2));
}

#[shoop_test]
fn silent_ranges_and_capacity_errors_keep_explicit_duration_and_accepted_prefix() {
    let mut l = limits();
    l.blocks = 1;
    let mut store = ContentStore::prepare(l).unwrap();
    let state = store.snapshot(&MidiState::default()).unwrap();
    let source = store.writer(state).unwrap();
    store.advance(source, 100).unwrap();
    let silent = store.range(source, 20, 80, state).unwrap();
    assert_eq!(store.describe(silent).unwrap().frames, 60);
    assert_eq!(store.read(silent, 0).unwrap(), None);
    for i in 0..3 {
        store.append(source, 100 + i, msg(i as u8)).unwrap();
    }
    assert_eq!(store.append(source, 103, msg(3)), Err(Error::Capacity));
    assert_eq!(store.describe(source).unwrap().events, 3);
    assert_eq!(store.finish(source, 104), Err(Error::Incomplete));
    assert_eq!(store.describe(silent).unwrap().frames, 60);
}
