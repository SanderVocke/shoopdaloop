mod support;
use rt_runtime::midi::EventRecord;
use rt_runtime::midi_runtime::{PositionJobs, PositionStatus, Profile, Store};
use rt_runtime::midi_state::{MidiState, BASIC_STATE_CELLS};
use rt_runtime::{Error, Frame};
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;

#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

fn store() -> Store {
    Store::prepare(Profile {
        pages: 32,
        tiles: 32,
        directories: 2048,
        versions: 32,
        tile_shift: 6,
        pages_per_tile: 4,
        touched_tiles: 4,
    })
    .unwrap()
}

#[shoop_test]
fn prepared_positions_pin_the_exact_cut_and_preserve_boundary_originals() {
    let mut store = store();
    let mut jobs = PositionJobs::prepare(2).unwrap();
    let empty = store.empty().unwrap();
    let events = [
        EventRecord::new(Frame(0), 0, &[0xb0, 7, 91]).unwrap(),
        EventRecord::new(Frame(4), 0, &[0x90, 60, 100]).unwrap(),
        EventRecord::new(Frame(8), 0, &[0x80, 60, 0]).unwrap(),
        EventRecord::new(Frame(8), 1, &[0x90, 62, 87]).unwrap(),
    ];
    let version = store.edit(empty, Frame(0)..Frame(64), &events).unwrap();
    let seed = MidiState::default();
    let job = assert_no_heap(|| jobs.begin(&mut store, version, &seed, Frame(8))).unwrap();
    assert!(matches!(jobs.finish(job), Err(Error::Busy)));
    assert_no_heap(|| store.release(version)).unwrap();
    assert_eq!(store.release(version), Err(Error::InvalidId));
    let mut ready = false;
    assert_no_heap(|| {
        for step in 0..BASIC_STATE_CELLS + 1000 {
            let before = jobs.work();
            let status = jobs.progress(&mut store, job, 1).unwrap();
            let after = jobs.work();
            assert_eq!(
                after.state_cells + after.lookups + after.update_steps
                    - before.state_cells
                    - before.lookups
                    - before.update_steps,
                1
            );
            if status == PositionStatus::Ready {
                ready = true;
                break;
            }
            assert!(step + 1 < BASIC_STATE_CELLS + 1000);
        }
        let position = jobs.finish(job).unwrap();
        let mut expected = seed;
        for event in events.iter().filter(|event| event.frame < Frame(8)) {
            expected
                .apply_packet(event.message().as_bytes(), 4)
                .unwrap();
        }
        assert_eq!(position.state, expected.basic_projection());
        assert_eq!(
            store.first(position.version, position.after),
            Ok(Some(events[2]))
        );
        assert_eq!(position.expected, version);
        store.release(position.version).unwrap();
        assert!(matches!(jobs.finish(job), Err(Error::InvalidId)));
    });
    assert!(ready);
}

#[shoop_test]
fn cancelled_position_jobs_return_their_pin_and_policy_refusal_allocates_nothing() {
    let mut store = store();
    let mut jobs = PositionJobs::prepare(1).unwrap();
    let empty = store.empty().unwrap();
    let before = store.usage();
    let mut invalid = MidiState::default();
    invalid.apply_packet(&[0xb0, 6, 1], 3).unwrap();
    assert_eq!(
        jobs.begin(&mut store, empty, &invalid, Frame(0)),
        Err(Error::InvalidParameter)
    );
    assert_eq!(store.usage(), before);
    let job = jobs
        .begin(&mut store, empty, &MidiState::default(), Frame(0))
        .unwrap();
    assert_no_heap(|| {
        jobs.progress(&mut store, job, 3).unwrap();
        jobs.cancel(&mut store, job).unwrap();
        assert!(matches!(jobs.finish(job), Err(Error::InvalidId)));
        while store.maintain(1) != 0 {}
    });
    assert_eq!(store.usage(), before);
}
