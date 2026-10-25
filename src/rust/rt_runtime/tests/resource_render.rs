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
fn store() -> ResourceStore {
    ResourceStore::new(
        Config {
            max_frames: 64,
            max_nodes: 4,
            max_ports: 8,
            resource_slots: 8,
            resource_pages: 64,
            page_bytes: 16,
            midi_events_per_port: 8,
            midi_bytes_per_port: 128,
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
fn upload(store: &mut ResourceStore, data: &[u8]) -> Snapshot {
    let resource = store.create().unwrap();
    let upload = store
        .begin_upload(
            resource,
            Revision(0),
            Description {
                format: Format::Audio { channels: 1 },
                frames: data.len() as u64 / 4,
                bytes: data.len(),
            },
        )
        .unwrap();
    for (index, chunk) in data.chunks(16).enumerate() {
        store.begin_callback();
        store.write_upload(upload, index * 16, chunk).unwrap();
    }
    store.finish_upload(upload).unwrap();
    store.commit(upload).unwrap();
    store.snapshot(resource, Frame(0)).unwrap()
}

#[shoop_test]
fn required_audio_access_exceeds_exhausted_background_allowance_without_refreshing_it() {
    let mut store = store();
    let samples = [1.0f32; 64];
    let data: Vec<_> = samples.iter().flat_map(|v| v.to_le_bytes()).collect();
    let snapshot = upload(&mut store, &data);
    let mut cursor = store.cursor(snapshot.id).unwrap();
    let resource = store.create().unwrap();
    let capture = store
        .begin_capture(
            resource,
            Revision(0),
            Format::Audio { channels: 1 },
            Frame(0),
        )
        .unwrap();
    let legacy_resource = store.create().unwrap();
    let legacy = store
        .begin_capture(
            legacy_resource,
            Revision(0),
            Format::Audio { channels: 1 },
            Frame(0),
        )
        .unwrap();
    let mut output = [0; 256];
    assert_no_heap(|| {
        store.begin_callback();
        assert_eq!(store.read(&mut cursor, &mut output[..16]), Ok(16));
        assert_eq!(store.copy_remaining(), 0);
        assert_eq!(
            store.read(&mut cursor, &mut output[16..]),
            Err(Error::Budget)
        );
        let mut offset = 16;
        while offset < output.len() {
            let count = store
                .io()
                .render()
                .read(&mut cursor, &mut output[offset..])
                .unwrap();
            assert!(count > 0);
            offset += count;
        }
        store
            .render_io()
            .capture_audio(capture, Frame(0), &samples)
            .unwrap();
        assert_eq!(store.copy_remaining(), 0);
        assert_eq!(store.steps_remaining(), 1);
        assert_eq!(store.work().copied_bytes, 16);
        assert_eq!(store.render_work().copied_bytes, 496);
        assert_eq!(store.render_work().operations, 16);
        assert_eq!(
            store.info(resource).unwrap().capture.unwrap().next_frame,
            Frame(64)
        );
        assert_eq!(
            store.capture_audio(legacy, Frame(0), &[1.0]),
            Err(Error::Budget)
        );
        assert_eq!(
            store
                .info(legacy_resource)
                .unwrap()
                .capture
                .unwrap()
                .incomplete,
            Some(Error::Budget)
        );
        assert_eq!(store.render_io().read(&mut cursor, &mut [0; 4]), Ok(0));
        assert_eq!(store.render_work().operations, 17);
    });
    assert_eq!(output.as_slice(), data);
}

#[shoop_test]
fn required_midi_validation_and_copy_are_separate_from_background_steps() {
    let mut store = store();
    let source = upload(&mut store, &[0; 16]);
    let mut source_cursor = store.cursor(source.id).unwrap();
    let resource = store.create().unwrap();
    let capture = store
        .begin_capture(resource, Revision(0), Format::Midi, Frame(0))
        .unwrap();
    let legacy_resource = store.create().unwrap();
    let legacy = store
        .begin_capture(legacy_resource, Revision(0), Format::Midi, Frame(0))
        .unwrap();
    let mut input = MidiBuffer::new(4, 12).unwrap();
    for offset in [0, 2, 4, 6] {
        input.push(offset, 8, &[0x90, 60, 100]).unwrap();
    }
    let mut bytes = [0; 60];
    assert_no_heap(|| {
        store.begin_callback();
        store.read(&mut source_cursor, &mut [0; 16]).unwrap();
        assert_eq!(
            store.capture_midi(legacy, Frame(0), 8, &input),
            Err(Error::Budget)
        );
        store
            .io()
            .render()
            .capture_midi(capture, Frame(0), 8, &input)
            .unwrap();
        assert_eq!(store.render_work().validation_steps, 4);
        assert_eq!(store.render_work().copied_bytes, 60);
        assert_eq!(store.steps_remaining(), 1);
        assert_eq!(store.copy_remaining(), 0);
        store.stop_capture(capture).unwrap();
        let snapshot = store.snapshot(resource, Frame(8)).unwrap();
        let mut cursor = store.cursor(snapshot.id).unwrap();
        let mut offset = 0;
        while offset < bytes.len() {
            let count = store
                .render_io()
                .read(&mut cursor, &mut bytes[offset..])
                .unwrap();
            assert!(count > 0);
            offset += count;
        }
        store.release_snapshot(snapshot.id).unwrap();
        assert_eq!(store.render_work().copied_bytes, 120);
        assert_eq!(store.work().validation_steps, 0);
    });
    for (record, offset) in bytes.chunks_exact(15).zip([0u64, 2, 4, 6]) {
        assert_eq!(&record[..8], &offset.to_le_bytes());
        assert_eq!(&record[8..12], &3u32.to_le_bytes());
        assert_eq!(&record[12..], &[0x90, 60, 100]);
    }
}

#[shoop_test]
fn required_access_preserves_snapshot_tails_and_budgeted_reclamation() {
    let mut store = store();
    let baseline = store.usage();
    let resource = store.create().unwrap();
    let capture = store
        .begin_capture(
            resource,
            Revision(0),
            Format::Audio { channels: 1 },
            Frame(0),
        )
        .unwrap();
    let mut data = [0; 8];
    assert_no_heap(|| {
        store.begin_callback();
        store
            .render_io()
            .capture_audio(capture, Frame(0), &[1.0, 2.0])
            .unwrap();
        let snapshot = store.snapshot(resource, Frame(2)).unwrap();
        store
            .render_io()
            .capture_audio(capture, Frame(2), &[3.0, 4.0])
            .unwrap();
        let mut cursor = store.cursor(snapshot.id).unwrap();
        store.read(&mut cursor, &mut data).unwrap();
        assert_eq!(store.copy_remaining(), 0);
        let mut cursor = store.cursor(snapshot.id).unwrap();
        store.remove(resource).unwrap();
        assert_eq!(
            store.currentness(snapshot.id),
            Ok(Currentness::ResourceRemoved)
        );
        store.render_io().read(&mut cursor, &mut data).unwrap();
        assert_eq!(store.render_work().copied_bytes, 24);
        store.release_snapshot(snapshot.id).unwrap();
        assert_eq!(
            store.render_io().read(&mut cursor, &mut data),
            Err(Error::InvalidId)
        );
        for _ in 0..4 {
            store.begin_callback();
            assert_eq!(store.render_work(), RenderWork::default());
            assert!(store.reclaim() <= 1);
        }
        assert_eq!(store.usage(), baseline);
    });
    assert_eq!(&data[..4], &1.0f32.to_le_bytes());
    assert_eq!(&data[4..], &2.0f32.to_le_bytes());
}

#[shoop_test]
fn required_access_still_refuses_bad_shapes_frames_and_physical_exhaustion() {
    let mut store = store();
    let resource = store.create().unwrap();
    let capture = store
        .begin_capture(
            resource,
            Revision(0),
            Format::Audio { channels: 2 },
            Frame(0),
        )
        .unwrap();
    assert_no_heap(|| {
        store.begin_callback();
        assert_eq!(
            store
                .render_io()
                .capture_audio(capture, Frame(1), &[1.0, 2.0]),
            Err(Error::InvalidFrame)
        );
        assert_eq!(
            store.render_io().capture_audio(capture, Frame(0), &[1.0]),
            Err(Error::InvalidShape)
        );
        for frame in [0, 64] {
            store
                .render_io()
                .capture_audio(capture, Frame(frame), &[1.0; 128])
                .unwrap();
        }
        assert_eq!(
            store
                .render_io()
                .capture_audio(capture, Frame(128), &[1.0, 2.0]),
            Err(Error::Capacity)
        );
        let info = store.info(resource).unwrap();
        assert_eq!(info.description.frames, 128);
        assert_eq!(info.capture.unwrap().next_frame, Frame(128));
        assert_eq!(info.capture.unwrap().incomplete, Some(Error::Capacity));
        assert_eq!(store.work(), Work::default());
        assert_eq!(store.render_work().copied_bytes, 1024);
        assert_eq!(store.copy_remaining(), 16);
    });
}
