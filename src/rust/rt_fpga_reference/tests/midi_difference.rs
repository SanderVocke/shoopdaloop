use rt_fpga_reference::midi::{
    ComparisonId, ContentStore, DifferenceService, Error, Interpreter, Message, MidiState,
    StoreLimits, STATE_CELLS,
};
use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};
#[global_allocator]
static ALLOCATOR: shoop_wasm_test_support::heap::TrackingAllocator =
    shoop_wasm_test_support::heap::TrackingAllocator;

fn store() -> ContentStore {
    ContentStore::prepare(StoreLimits {
        block_events: 4,
        blocks: 4,
        max_events: 8,
        views: 4,
        snapshots: 4,
        retirements: 4,
    })
    .unwrap()
}
fn message(data: &[u8]) -> Message {
    Message::managed(data).unwrap()
}

#[shoop_test]
fn incremental_maps_match_fresh_comparisons_after_modes_pedals_and_resets() {
    let mut store = store();
    let mut target = MidiState::default();
    for data in [
        [0xb3, 1, 55],
        [0xb3, 64, 127],
        [0x93, 60, 90],
        [0xb3, 66, 127],
        [0x83, 60, 0],
        [0xc3, 7, 0],
    ] {
        let bytes = if data[0] == 0xc3 {
            &data[..2]
        } else {
            &data[..]
        };
        target.apply_packet(bytes, 3).unwrap();
    }
    let snapshot = store.snapshot(&target).unwrap();
    let id = ComparisonId {
        target: snapshot,
        destination: 0,
    };
    let mut live = Interpreter::default();
    let mut service = DifferenceService::prepare(4, 2).unwrap();
    service
        .initialize(&store, id, live.state().unwrap(), STATE_CELLS as u32)
        .unwrap();
    let mut oracle = DifferenceService::prepare(4, 1).unwrap();
    let mut seed = 7u32;
    assert_no_heap(|| {
        for i in 0..700 {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let channel = (seed >> 20) as u8 & 15;
            let value = (seed >> 8) as u8 & 127;
            let data = match i % 13 {
                0 => [0xff, 0, 0],
                1 => [0xb0 | channel, 121, 0],
                2 => [0xb0 | channel, 66, value],
                3 => [0xb0 | channel, 64, value],
                4 => [0xb0 | channel, 69, value],
                5 => [0xb0 | channel, 124 + value % 4, 0],
                6 => [0xb0 | channel, 120, 0],
                7 => [0xb0 | channel, 123, 0],
                8 => [0xc0 | channel, value, 0],
                9 => [0xa0 | channel, value, value],
                10 => [0xe0 | channel, value, value],
                11 => [0x90 | channel, value, value],
                _ => [0xb0 | channel, 1, value],
            };
            let len = if data[0] == 0xff {
                1
            } else if data[0] & 0xf0 == 0xc0 {
                2
            } else {
                3
            };
            live.begin(message(&data[..len])).unwrap();
            while !live.step_observed(|s, cell| service.changed(&store, 0, s, cell)) {}
            let temp = store.snapshot(&target).unwrap();
            let oid = ComparisonId {
                target: temp,
                destination: 0,
            };
            oracle
                .initialize(&store, oid, live.state().unwrap(), STATE_CELLS as u32)
                .unwrap();
            assert_eq!(
                service.get(&store, id).unwrap(),
                oracle.get(&store, oid).unwrap(),
                "iteration {i}"
            );
            store.release_snapshot(temp).unwrap();
        }
    });
}

#[shoop_test]
fn drift_and_return_clear_bits_and_staged_setup_tracks_intervening_changes() {
    let mut store = store();
    let mut target = MidiState::default();
    target.apply_packet(&[0xb0, 1, 40], 3).unwrap();
    let snapshot = store.snapshot(&target).unwrap();
    let id = ComparisonId {
        target: snapshot,
        destination: 0,
    };
    let mut live = Interpreter::new(target);
    let mut service = DifferenceService::prepare(4, 1).unwrap();
    assert!(!service
        .initialize(&store, id, live.state().unwrap(), 200)
        .unwrap());
    assert_eq!(service.get(&store, id), Err(Error::Busy));
    for value in [90, 40] {
        live.begin(message(&[0xb0, 1, value])).unwrap();
        while !live.step_observed(|s, cell| service.changed(&store, 0, s, cell)) {}
        service
            .initialize(&store, id, live.state().unwrap(), STATE_CELLS as u32)
            .unwrap();
        assert_eq!(service.get(&store, id).unwrap().contains(129), value != 40);
    }
    assert!(service.get(&store, id).unwrap().is_empty());
    store.release_snapshot(snapshot).unwrap();
    assert_eq!(service.get(&store, id), Err(Error::InvalidState));
    let next = store.snapshot(&target).unwrap();
    assert_ne!(next, snapshot);
    assert_eq!(
        service.get(
            &store,
            ComparisonId {
                target: next,
                destination: 0
            }
        ),
        Err(Error::Busy)
    );
}

#[shoop_test]
fn ordinary_message_notifies_only_its_changed_fields() {
    let mut live = Interpreter::default();
    for (bytes, expected) in [
        (&[0xb0, 1, 40][..], 1),
        (&[0xb0, 1, 40][..], 0),
        (&[0xc0, 5][..], 1),
        (&[0x90, 60, 70][..], 1),
    ] {
        let mut visits = 0;
        live.begin(message(bytes)).unwrap();
        while !live.step_observed(|_, _| visits += 1) {}
        assert_eq!(visits, expected);
    }
}
