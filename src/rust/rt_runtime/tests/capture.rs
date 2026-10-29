mod support;
use rt_runtime::buffer::MidiBuffer;
use rt_runtime::resource::*;
use rt_runtime::{Budgets, Config, Error, Frame, Revision};
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;

#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

fn store(pages: usize, bytes: usize) -> ResourceStore {
    ResourceStore::new(
        Config {
            resource_slots: 64,
            resource_pages: pages,
            page_bytes: 16,
            midi_bytes_per_port: 64,
            budgets: Budgets {
                transfer_bytes: bytes,
                validation_steps: 16,
                ..Budgets::default()
            },
            ..Config::default()
        }
        .prepare()
        .unwrap(),
    )
    .unwrap()
}
fn fetch(store: &mut ResourceStore, snapshot: Snapshot) -> Vec<u8> {
    let mut cursor = store.cursor(snapshot.id).unwrap();
    let mut result = Vec::with_capacity(snapshot.description.bytes);
    let mut bytes = [0; 16];
    loop {
        let count = assert_no_heap(|| {
            store.begin_callback();
            store.read(&mut cursor, &mut bytes)
        })
        .unwrap();
        if count == 0 {
            break;
        }
        result.extend_from_slice(&bytes[..count]);
    }
    result
}
fn floats(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}
fn reclaim_all(store: &mut ResourceStore) {
    for _ in 0..256 {
        assert_no_heap(|| {
            store.begin_callback();
            assert!(store.reclaim() <= 16);
        });
        if store.usage().retired_versions == 0 && store.usage().retired_pages == 0 {
            return;
        }
    }
    panic!("pool did not drain");
}

#[shoop_test]
fn audio_prefix_tail_is_immutable_while_the_original_tail_fills_and_capture_appends() {
    let mut store = store(16, 16);
    let resource = store.create().unwrap();
    let capture = assert_no_heap(|| {
        store.begin_capture(
            resource,
            Revision(0),
            Format::Audio { channels: 2 },
            Frame(100),
        )
    })
    .unwrap();
    let first = assert_no_heap(|| {
        store.begin_callback();
        assert_eq!(
            store.capture_audio(capture, Frame(100), &[1.0, 2.0]),
            Ok(Revision(2))
        );
        assert_eq!(
            store.snapshot(resource, Frame(100)),
            Err(Error::InvalidFrame)
        );
        let snapshot = store.snapshot(resource, Frame(101)).unwrap();
        assert_eq!(store.work().copied_bytes, 16); // 8 captured plus 8 tail copy.
        snapshot
    });
    let full_page = assert_no_heap(|| {
        store.begin_callback();
        store
            .capture_audio(capture, Frame(101), &[3.0, 4.0])
            .unwrap();
        let snapshot = store.snapshot(resource, Frame(102)).unwrap();
        assert_eq!(store.work().copied_bytes, 8); // Sealed full pages need no copy.
        snapshot
    });
    assert_no_heap(|| {
        store.begin_callback();
        store
            .capture_audio(capture, Frame(102), &[5.0, 6.0, 7.0, 8.0])
            .unwrap();
    });
    assert_eq!(store.currentness(first.id), Ok(Currentness::Stale));
    assert_eq!(first.cut_frame, Frame(101));
    assert_eq!(first.description.frames, 1);
    assert_eq!(fetch(&mut store, first), floats(&[1.0, 2.0]));
    assert_eq!(fetch(&mut store, full_page), floats(&[1.0, 2.0, 3.0, 4.0]));
    let live = store.snapshot(resource, Frame(1000)).unwrap();
    assert_eq!(live.cut_frame, Frame(104));
    assert_eq!(live.capture.unwrap().start_frame, Frame(100));
    assert_eq!(store.currentness(live.id), Ok(Currentness::Current));
    assert_no_heap(|| store.stop_capture(capture)).unwrap();
    assert_eq!(store.currentness(live.id), Ok(Currentness::Stale));
    let stopped = store.snapshot(resource, Frame(1000)).unwrap();
    assert!(!stopped.capture.unwrap().active);
    assert_eq!(stopped.capture.unwrap().incomplete, None);
    assert_eq!(
        fetch(&mut store, stopped),
        floats(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0])
    );
    assert_no_heap(|| {
        assert_eq!(
            store.capture_audio(capture, Frame(104), &[9.0, 10.0]),
            Err(Error::InvalidState)
        );
        store.remove(resource).unwrap();
        for snapshot in [first, full_page, live, stopped] {
            store.release_snapshot(snapshot.id).unwrap();
        }
    });
    reclaim_all(&mut store);
    assert_eq!(store.usage().free_pages, 16);
    assert_eq!(store.usage().free_versions, 64);
    assert_eq!(store.usage().free_snapshots, 64);
}

#[shoop_test]
fn capture_exhaustion_preserves_a_complete_prefix_and_cannot_silently_resume_after_a_gap() {
    let mut store = store(2, 16);
    let resource = store.create().unwrap();
    let capture = store
        .begin_capture(
            resource,
            Revision(0),
            Format::Audio { channels: 1 },
            Frame(0),
        )
        .unwrap();
    let snapshot = assert_no_heap(|| {
        store.begin_callback();
        store.capture_audio(capture, Frame(0), &[1.0, 2.0]).unwrap();
        store.snapshot(resource, Frame(2)).unwrap()
    });
    assert_eq!(store.usage().free_pages, 0);
    assert_no_heap(|| {
        store.begin_callback();
        store.capture_audio(capture, Frame(2), &[3.0, 4.0]).unwrap();
        store.begin_callback();
        assert_eq!(
            store.capture_audio(capture, Frame(4), &[5.0]),
            Err(Error::Capacity)
        );
        assert_eq!(store.work().copied_bytes, 0);
    });
    let info = store.info(resource).unwrap();
    assert_eq!(info.description.frames, 4);
    assert_eq!(info.description.bytes, 16);
    assert_eq!(info.capture.unwrap().incomplete, Some(Error::Capacity));
    assert!(!info.capture.unwrap().active);
    assert_eq!(fetch(&mut store, snapshot), floats(&[1.0, 2.0]));
    store.release_snapshot(snapshot.id).unwrap();
    reclaim_all(&mut store);
    assert_eq!(store.usage().free_pages, 1);
    assert_eq!(
        store.capture_audio(capture, Frame(4), &[5.0]),
        Err(Error::Incomplete)
    );
    let prefix = store.snapshot(resource, Frame(500)).unwrap();
    assert_eq!(prefix.cut_frame, Frame(4));
    assert_eq!(prefix.capture.unwrap().incomplete, Some(Error::Capacity));
    assert_eq!(fetch(&mut store, prefix), floats(&[1.0, 2.0, 3.0, 4.0]));
}

#[shoop_test]
fn tail_snapshots_refuse_before_acquiring_credits_when_copy_or_unreserved_pages_are_unavailable() {
    let mut store = store(4, 16);
    let resource = store.create().unwrap();
    let capture = store
        .begin_capture(
            resource,
            Revision(0),
            Format::Audio { channels: 1 },
            Frame(0),
        )
        .unwrap();
    let other = store.create().unwrap();
    let upload = store
        .begin_upload(
            other,
            Revision(0),
            Description {
                format: Format::Audio { channels: 1 },
                frames: 2,
                bytes: 8,
            },
        )
        .unwrap();
    assert_no_heap(|| {
        store.begin_callback();
        store.capture_audio(capture, Frame(0), &[1.0, 2.0]).unwrap();
        store.write_upload(upload, 0, &[0; 8]).unwrap();
        let before = store.usage();
        assert_eq!(store.snapshot(resource, Frame(2)), Err(Error::Budget));
        assert_eq!(store.usage(), before);
        store.begin_callback();
    });
    let reservation = store
        .begin_upload(
            other,
            Revision(0),
            Description {
                format: Format::Audio { channels: 1 },
                frames: 8,
                bytes: 32,
            },
        )
        .unwrap();
    let before = store.usage();
    assert_no_heap(|| assert_eq!(store.snapshot(resource, Frame(2)), Err(Error::Capacity)));
    assert_eq!(store.usage(), before);
    assert_eq!(
        store.info(resource).unwrap().capture.unwrap().incomplete,
        None
    );
    store.cancel_upload(reservation).unwrap();
    let snapshot = assert_no_heap(|| store.snapshot(resource, Frame(2))).unwrap();
    assert_eq!(fetch(&mut store, snapshot), floats(&[1.0, 2.0]));
}

#[shoop_test]
fn midi_capture_keeps_complete_messages_relative_frames_silence_duration_and_old_prefixes() {
    let mut store = store(16, 64);
    let resource = store.create().unwrap();
    let capture = store
        .begin_capture(resource, Revision(0), Format::Midi, Frame(10))
        .unwrap();
    let mut input = MidiBuffer::new(8, 64).unwrap();
    input.push(2, 8, &[0x90, 60, 100]).unwrap();
    input.push(7, 8, &[0xf0, 0x7d, 1, 2, 0xf7]).unwrap();
    let first = assert_no_heap(|| {
        store.begin_callback();
        store.capture_midi(capture, Frame(10), 8, &input).unwrap();
        assert_eq!(store.work().copied_bytes, 32);
        assert_eq!(store.work().validation_steps, 2);
        store.snapshot(resource, Frame(18)).unwrap()
    });
    let mut expected = [0; 32];
    encode_midi_record(2, &[0x90, 60, 100], &mut expected[..15]).unwrap();
    encode_midi_record(7, &[0xf0, 0x7d, 1, 2, 0xf7], &mut expected[15..]).unwrap();
    input.clear();
    assert_no_heap(|| {
        store.begin_callback();
        store.capture_midi(capture, Frame(18), 8, &input).unwrap();
        assert_eq!(store.work().copied_bytes, 0);
    });
    let silent_tail = store.snapshot(resource, Frame(26)).unwrap();
    assert_eq!(silent_tail.description.frames, 16);
    assert_eq!(first.description.frames, 8);
    assert_eq!(store.currentness(first.id), Ok(Currentness::Stale));
    input.push(1, 8, &[0xf8]).unwrap();
    let last = assert_no_heap(|| {
        store.begin_callback();
        store.capture_midi(capture, Frame(26), 8, &input).unwrap();
        store.snapshot(resource, Frame(34)).unwrap()
    });
    assert_eq!(last.description.frames, 24);
    assert_eq!(last.cut_frame, Frame(34));
    assert_eq!(fetch(&mut store, first), expected);
    assert_eq!(fetch(&mut store, silent_tail), expected);
    let mut total = expected.to_vec();
    let mut record = [0; 13];
    encode_midi_record(17, &[0xf8], &mut record).unwrap();
    total.extend_from_slice(&record);
    assert_eq!(fetch(&mut store, last), total);
}

#[shoop_test]
fn invalid_capture_calls_do_not_advance_content_and_clock_or_budget_exhaustion_is_explicit() {
    let mut store = store(8, 16);
    let resource = store.create().unwrap();
    let capture = store
        .begin_capture(
            resource,
            Revision(0),
            Format::Audio { channels: 2 },
            Frame(u64::MAX - 1),
        )
        .unwrap();
    assert_no_heap(|| {
        store.begin_callback();
        assert_eq!(
            store.capture_audio(capture, Frame(0), &[1.0, 2.0]),
            Err(Error::InvalidFrame)
        );
        assert_eq!(
            store.capture_audio(capture, Frame(u64::MAX - 1), &[1.0]),
            Err(Error::InvalidShape)
        );
        assert_eq!(
            store.capture_audio(capture, Frame(u64::MAX - 1), &[1.0; 4]),
            Err(Error::Exhausted)
        );
    });
    assert_eq!(store.info(resource).unwrap().description.frames, 0);
    assert_eq!(
        store.info(resource).unwrap().capture.unwrap().incomplete,
        Some(Error::Exhausted)
    );
    let next = store
        .begin_capture(
            resource,
            Revision(1),
            Format::Audio { channels: 1 },
            Frame(0),
        )
        .unwrap();
    assert_no_heap(|| {
        store.begin_callback();
        assert_eq!(
            store.capture_audio(next, Frame(0), &[1.0; 5]),
            Err(Error::Budget)
        );
        assert_eq!(
            store.capture_audio(capture, Frame(u64::MAX - 1), &[1.0; 2]),
            Err(Error::InvalidId)
        );
    });
    assert_eq!(store.info(resource).unwrap().description.bytes, 0);
    assert_eq!(
        store.info(resource).unwrap().capture.unwrap().incomplete,
        Some(Error::Budget)
    );
    assert_eq!(store.usage().reserved_pages, 0);
}

#[shoop_test]
fn invalid_midi_offsets_are_atomic_and_validation_exhaustion_stops_capture_visibly() {
    let mut store = store(16, 512);
    let resource = store.create().unwrap();
    let capture = store
        .begin_capture(resource, Revision(0), Format::Midi, Frame(0))
        .unwrap();
    let mut input = MidiBuffer::new(32, 64).unwrap();
    input.push(5, 8, &[0xf8]).unwrap();
    assert_no_heap(|| {
        store.begin_callback();
        assert_eq!(
            store.capture_midi(capture, Frame(0), 4, &input),
            Err(Error::InvalidEvent)
        );
        assert_eq!(store.work().copied_bytes, 0);
        assert_eq!(store.work().validation_steps, 1);
        store.capture_midi(capture, Frame(0), 8, &input).unwrap();
    });
    input.clear();
    for _ in 0..17 {
        input.push(0, 8, &[0xf8]).unwrap();
    }
    assert_no_heap(|| {
        store.begin_callback();
        assert_eq!(
            store.capture_midi(capture, Frame(8), 8, &input),
            Err(Error::Budget)
        );
    });
    let info = store.info(resource).unwrap();
    assert_eq!(info.description.frames, 8);
    assert_eq!(info.capture.unwrap().incomplete, Some(Error::Budget));
}

#[shoop_test]
fn seeded_capture_prefixes_survive_many_tail_mutations_and_recover_every_pool_credit() {
    let mut store = store(128, 64);
    let resource = store.create().unwrap();
    let capture = store
        .begin_capture(
            resource,
            Revision(0),
            Format::Audio { channels: 1 },
            Frame(0),
        )
        .unwrap();
    let mut seed = 0x1234_5678u32;
    let mut values = vec![];
    let mut snapshots = vec![];
    for _ in 0..32 {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let count = 1 + (seed as usize % 4);
        let frame = Frame(values.len() as u64);
        let samples = [
            values.len() as f32,
            values.len() as f32 + 1.0,
            values.len() as f32 + 2.0,
            values.len() as f32 + 3.0,
        ];
        let snapshot = assert_no_heap(|| {
            store.begin_callback();
            store
                .capture_audio(capture, frame, &samples[..count])
                .unwrap();
            store
                .snapshot(resource, frame.checked_add(count).unwrap())
                .unwrap()
        });
        assert!(store.work().copied_bytes <= 64);
        values.extend_from_slice(&samples[..count]);
        snapshots.push(snapshot);
    }
    assert_no_heap(|| store.remove(resource)).unwrap();
    for snapshot in snapshots {
        assert_eq!(
            store.currentness(snapshot.id),
            Ok(Currentness::ResourceRemoved)
        );
        assert_eq!(
            fetch(&mut store, snapshot),
            floats(&values[..snapshot.description.frames as usize])
        );
        assert_no_heap(|| store.release_snapshot(snapshot.id)).unwrap();
    }
    reclaim_all(&mut store);
    assert_eq!(
        store.usage(),
        PoolUsage {
            free_pages: 128,
            reserved_pages: 0,
            free_versions: 64,
            free_snapshots: 64,
            retired_versions: 0,
            retired_pages: 0
        }
    );
}
