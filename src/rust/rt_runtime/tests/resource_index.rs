mod support;
use rt_runtime::resource::*;
use rt_runtime::{Budgets, Config, Error, Frame, Revision};
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;
#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
fn store() -> ResourceStore {
    ResourceStore::new(
        Config {
            resource_slots: 8,
            resource_pages: 600,
            page_bytes: 16,
            max_frames: 64,
            budgets: Budgets {
                transfer_bytes: 16,
                validation_steps: 1,
                ..Budgets::default()
            },
            ..Config::default()
        }
        .prepare()
        .unwrap(),
    )
    .unwrap()
}
fn upload(store: &mut ResourceStore, resource: rt_runtime::ResourceId, data: &[u8]) -> Snapshot {
    let upload = assert_no_heap(|| {
        store.begin_upload(
            resource,
            store.revision(resource).unwrap(),
            Description {
                format: Format::Audio { channels: 1 },
                frames: data.len() as u64 / 4,
                bytes: data.len(),
            },
        )
    })
    .unwrap();
    for (ordinal, chunk) in data.chunks(16).enumerate() {
        assert_no_heap(|| {
            store.begin_callback();
            store.write_upload(upload, ordinal * 16, chunk).unwrap();
        });
    }
    assert_no_heap(|| {
        store.finish_upload(upload).unwrap();
        store.commit(upload).unwrap();
        store.snapshot(resource, Frame(0))
    })
    .unwrap()
}
fn check_bytes(store: &mut ResourceStore, index: ReadIndex, offset: usize, expected: &[u8]) {
    let before = store.index_work().render_steps;
    let mut cursor = store.io().render().cursor_at(index, offset).unwrap();
    assert!(store.index_work().render_steps - before <= 21);
    assert_eq!(cursor.position(), offset);
    let mut output = [0; 7];
    let count = store.render_io().read(&mut cursor, &mut output).unwrap();
    let count_expected = 7.min(16 - offset % 16).min(expected.len() - offset);
    assert_eq!(count, count_expected);
    assert_eq!(&output[..count], &expected[offset..offset + count]);
}
#[shoop_test]
fn indexed_reads_match_flat_bytes_for_all_offsets_and_seeded_wraps_without_budget() {
    let mut store = store();
    let data: Vec<u8> = (0..1024).flat_map(|n| (n as f32).to_le_bytes()).collect();
    let resource = store.create().unwrap();
    let snapshot = upload(&mut store, resource, &data);
    let index = assert_no_heap(|| store.read_index(snapshot.id)).unwrap();
    assert_no_heap(|| {
        store.begin_callback();
        let mut old_cursor = store.cursor(snapshot.id).unwrap();
        store.read(&mut old_cursor, &mut [0; 16]).unwrap();
        for offset in 0..=data.len() {
            check_bytes(&mut store, index, offset, &data);
        }
        let mut seed = 0x0b29778f3a19fbe5u64;
        for _ in 0..4096 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            check_bytes(
                &mut store,
                index,
                (seed % (data.len() as u64 + 1)) as usize,
                &data,
            );
        }
        assert_eq!(store.copy_remaining(), 0);
        assert_eq!(store.steps_remaining(), 1);
        assert_eq!(store.work().copied_bytes, 16);
        assert_eq!(store.index_work().background_steps, 0);
        assert_eq!(
            store.render_io().cursor_at(index, usize::MAX),
            Err(Error::InvalidOffset)
        );
    });
}
#[shoop_test]
fn old_prefix_index_survives_tail_changes_and_tree_rotations_without_freeze_scan() {
    let mut store = store();
    let resource = store.create().unwrap();
    let capture = store
        .begin_capture(
            resource,
            Revision(0),
            Format::Audio { channels: 1 },
            Frame(0),
        )
        .unwrap();
    let samples: Vec<_> = (0..15).map(|n| n as f32).collect();
    let expected: Vec<_> = samples.iter().flat_map(|n| n.to_le_bytes()).collect();
    assert_no_heap(|| {
        store.begin_callback();
        store
            .render_io()
            .capture_audio(capture, Frame(0), &samples)
            .unwrap();
        let snapshot = store.snapshot(resource, Frame(15)).unwrap();
        let old_index = store.read_index(snapshot.id).unwrap();
        store
            .render_io()
            .capture_audio(capture, Frame(15), &[9.0; 61])
            .unwrap();
        let before = store.index_work();
        store.stop_capture(capture).unwrap();
        let frozen = store.snapshot(resource, Frame(76)).unwrap();
        let frozen_index = store.read_index(frozen.id).unwrap();
        assert_eq!(store.index_work(), before);
        for offset in 0..=expected.len() {
            check_bytes(&mut store, old_index, offset, &expected);
        }
        let mut cursor = store.render_io().cursor_at(frozen_index, 75 * 4).unwrap();
        let mut sample = [0; 4];
        store.render_io().read(&mut cursor, &mut sample).unwrap();
        assert_eq!(sample, 9.0f32.to_le_bytes());
        store.remove(resource).unwrap();
        check_bytes(&mut store, old_index, 48, &expected);
        store.release_snapshot(snapshot.id).unwrap();
        store.release_snapshot(frozen.id).unwrap();
        assert_eq!(
            store.render_io().cursor_at(old_index, 0),
            Err(Error::InvalidId)
        );
    });
}
#[shoop_test]
fn foreign_and_released_indexes_and_cursors_cannot_alias_reused_slots() {
    let mut a = store();
    let mut b = store();
    let ra = a.create().unwrap();
    let rb = b.create().unwrap();
    let sa = upload(&mut a, ra, &[1; 16]);
    let sb = upload(&mut b, rb, &[2; 16]);
    assert_eq!(sa.id, sb.id);
    let ia = a.read_index(sa.id).unwrap();
    let ib = b.read_index(sb.id).unwrap();
    let mut ca = a.cursor(sa.id).unwrap();
    let mut cb = b.cursor(sb.id).unwrap();
    assert_no_heap(|| {
        assert_eq!(a.render_io().cursor_at(ib, 0), Err(Error::InvalidId));
        assert_eq!(b.cursor_at(ia, 0), Err(Error::InvalidId));
        assert_eq!(
            a.render_io().read(&mut cb, &mut [0; 4]),
            Err(Error::InvalidId)
        );
        assert_eq!(b.read(&mut ca, &mut [0; 4]), Err(Error::InvalidId));
        a.release_snapshot(sa.id).unwrap();
        let replacement = a.snapshot(ra, Frame(1)).unwrap();
        assert_eq!(replacement.id.slot, sa.id.slot);
        assert_ne!(replacement.id.generation, sa.id.generation);
        assert_eq!(a.render_io().cursor_at(ia, 0), Err(Error::InvalidId));
        assert_eq!(
            a.render_io().read(&mut ca, &mut [0; 4]),
            Err(Error::InvalidId)
        );
        let next = a.read_index(replacement.id).unwrap();
        check_bytes(&mut a, next, 0, &[1; 16]);
    });
}
#[shoop_test]
fn index_retirement_and_cancelled_partial_uploads_recycle_every_page_and_owner() {
    let mut store = store();
    let baseline = store.usage();
    let resource = store.create().unwrap();
    let old = upload(&mut store, resource, &[1; 64]);
    let old_index = store.read_index(old.id).unwrap();
    let current = upload(&mut store, resource, &[2; 96]);
    let current_index = store.read_index(current.id).unwrap();
    let pending = store
        .begin_upload(
            resource,
            store.revision(resource).unwrap(),
            Description {
                format: Format::Audio { channels: 1 },
                frames: 20,
                bytes: 80,
            },
        )
        .unwrap();
    assert_no_heap(|| {
        for offset in [0, 16, 32] {
            store.begin_callback();
            store.write_upload(pending, offset, &[3; 16]).unwrap();
        }
        store.cancel_upload(pending).unwrap();
        store.remove(resource).unwrap();
        check_bytes(&mut store, old_index, 60, &[1; 64]);
        check_bytes(&mut store, current_index, 92, &[2; 96]);
        store.release_snapshot(old.id).unwrap();
        store.release_snapshot(current.id).unwrap();
        for _ in 0..32 {
            store.begin_callback();
            assert!(store.reclaim() <= 1);
        }
        assert_eq!(store.usage(), baseline);
    });
    let resource = store.create().unwrap();
    let reused = upload(&mut store, resource, &[4; 80]);
    let index = store.read_index(reused.id).unwrap();
    assert_no_heap(|| {
        for offset in 0..=80 {
            check_bytes(&mut store, index, offset, &[4; 80]);
        }
    });
}
#[shoop_test]
fn empty_and_midi_byte_indexes_preserve_eof_and_exact_serialized_bytes() {
    let mut store = store();
    let resource = store.create().unwrap();
    let empty = upload(&mut store, resource, &[]);
    let empty_index = store.read_index(empty.id).unwrap();
    let midi = store.create().unwrap();
    let mut bytes = [0; 32];
    let count = encode_midi_record(3, &[0x90, 60, 100], &mut bytes).unwrap();
    let upload = store
        .begin_upload(
            midi,
            Revision(0),
            Description {
                format: Format::Midi,
                frames: 8,
                bytes: count,
            },
        )
        .unwrap();
    assert_no_heap(|| {
        check_bytes(&mut store, empty_index, 0, &[]);
        for (offset, byte) in bytes[..count].iter().enumerate() {
            store.begin_callback();
            store
                .write_upload(upload, offset, std::slice::from_ref(byte))
                .unwrap();
        }
        store.finish_upload(upload).unwrap();
        store.commit(upload).unwrap();
        let snapshot = store.snapshot(midi, Frame(8)).unwrap();
        let index = store.read_index(snapshot.id).unwrap();
        for offset in 0..=count {
            check_bytes(&mut store, index, offset, &bytes[..count]);
        }
    });
}
