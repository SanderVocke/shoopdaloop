mod support;
use rt_runtime::midi::EventRecord;
use rt_runtime::midi_runtime::{
    InputStamp, MediaImage, PositionStatus, Profile, Store, WriteMode, WriteSpan, WriterSet,
};
use rt_runtime::midi_state::MidiState;
use rt_runtime::{Error, Frame};
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;

#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

fn store() -> Store {
    Store::prepare(Profile {
        pages: 128,
        tiles: 128,
        directories: 4096,
        versions: 64,
        tile_shift: 6,
        pages_per_tile: 8,
        touched_tiles: 4,
    })
    .unwrap()
}
fn event(frame: u64, order: u64, bytes: &[u8]) -> EventRecord {
    EventRecord::new(Frame(frame), order, bytes).unwrap()
}
fn image(store: &mut Store, events: &[EventRecord]) -> MediaImage {
    let empty = store.empty().unwrap();
    let version = store.append(empty, events).unwrap();
    store.release(empty).unwrap();
    MediaImage {
        version,
        origin: Frame(0),
        frames: 1024,
        revision: 0,
        cut: Frame(0),
        initial: MidiState::default(),
        frontier: MidiState::default(),
        frontier_position: 0,
    }
}

#[shoop_test]
fn replacement_commits_one_complete_cut_and_preserves_retained_events() {
    let mut store = store();
    let originals = [
        event(0, 0, &[0xc0, 3]),
        event(12, 0, &[0xb0, 7, 9]),
        event(1000, 0, &[0x90, 60, 77]),
    ];
    let source = image(&mut store, &originals);
    let mut writers = WriterSet::prepare(2, 32, 32).unwrap();
    assert_no_heap(|| {
        let id = writers
            .begin(&mut store, &source, WriteMode::Fixed, 0)
            .unwrap();
        let entry = writers.snapshot(&mut store, id).unwrap();
        assert_eq!(entry.frontier_position, 0);
        store.release(entry.version).unwrap();
        assert_eq!(
            writers.progress(&mut store, id, 5000),
            Ok(PositionStatus::Ready)
        );
        writers.activate(id, Frame(64)).unwrap();
        let stamp = InputStamp {
            sequence: 1,
            frame: Frame(64),
        };
        writers.begin_interval(&mut store, id, stamp, 1).unwrap();
        let incoming = [
            event(68, 0, &[0x90, 62, 100]),
            event(70, 0, &[0xb0, 64, 127]),
            event(72, 0, &[0x80, 62, 0]),
        ];
        writers
            .stage_span(
                &mut store,
                id,
                stamp,
                &MidiState::default(),
                &incoming,
                WriteSpan {
                    position: 0,
                    offset: 0,
                    frames: 32,
                },
            )
            .unwrap();
        let prior = writers.snapshot(&mut store, id).unwrap();
        assert_eq!(prior.revision, 0);
        assert_eq!(store.first(prior.version, None), Ok(Some(originals[0])));
        assert_eq!(writers.commit_interval(&mut store, id), Ok(Frame(96)));
        let current = writers.snapshot(&mut store, id).unwrap();
        assert_eq!(current.cut, Frame(96));
        assert_eq!(current.revision, 1);
        assert_eq!(current.frontier_position, 32);
        let mut expected = MidiState::default();
        for row in incoming {
            expected.apply_packet(row.message().as_bytes(), 4).unwrap();
        }
        assert_eq!(current.frontier, expected.basic_projection());
        let mut after = None;
        for row in [
            event(4, 0, &[0x90, 62, 100]),
            event(6, 0, &[0xb0, 64, 127]),
            event(8, 0, &[0x80, 62, 0]),
            originals[2],
        ] {
            assert_eq!(store.first(current.version, after), Ok(Some(row)));
            after = Some((row.frame, row.order));
        }
        assert_eq!(store.first(current.version, after), Ok(None));
        assert_eq!(store.first(source.version, None), Ok(Some(originals[0])));
        store.release(prior.version).unwrap();
        store.release(current.version).unwrap();
        writers.release(&mut store, id).unwrap();
        store.release(source.version).unwrap();
        while store.maintain(1) != 0 {}
        assert_eq!(store.usage().leases, 0);
        assert_eq!(store.usage().pages, 0);
    });
}

#[shoop_test]
fn wrapped_interval_failure_keeps_the_complete_version_and_freezes_the_writer() {
    let mut store = store();
    let source = image(&mut store, &[event(0, 0, &[0xc0, 2])]);
    let mut writers = WriterSet::prepare(1, 32, 8).unwrap();
    assert_no_heap(|| {
        let id = writers
            .begin(&mut store, &source, WriteMode::Windowed, 1008)
            .unwrap();
        assert_eq!(
            writers.progress(&mut store, id, 5000),
            Ok(PositionStatus::Ready)
        );
        writers.activate(id, Frame(0)).unwrap();
        let input = InputStamp {
            sequence: 1,
            frame: Frame(0),
        };
        writers.begin_interval(&mut store, id, input, 2).unwrap();
        let incoming = [event(0, 0, &[0x90, 60, 1]), event(20, 0, &[0x90, 62, 2])];
        writers
            .stage_span(
                &mut store,
                id,
                input,
                &MidiState::default(),
                &incoming,
                WriteSpan {
                    position: 1008,
                    offset: 0,
                    frames: 16,
                },
            )
            .unwrap();
        assert_eq!(
            writers.commit_interval(&mut store, id),
            Err(Error::Incomplete)
        );
        assert_eq!(
            writers.stage_span(
                &mut store,
                id,
                input,
                &MidiState::default(),
                &incoming,
                WriteSpan {
                    position: 0,
                    offset: 16,
                    frames: 17
                }
            ),
            Err(Error::InvalidEvent)
        );
        writers.abort_interval(&mut store, id).unwrap();
        let complete = writers.snapshot(&mut store, id).unwrap();
        assert_eq!(complete.cut, source.cut);
        assert_eq!(complete.revision, source.revision);
        assert_eq!(store.totals(complete.version).unwrap().events, 1);
        assert_eq!(
            writers.begin_interval(&mut store, id, input, 0),
            Err(Error::InvalidFrame)
        );
        store.release(complete.version).unwrap();
        writers.release(&mut store, id).unwrap();
        store.release(source.version).unwrap();
        while store.maintain(1) != 0 {}
        assert_eq!(store.usage().leases, 0);
    });
}
