//! Capture immutable audio/MIDI prefixes and handle auxiliary storage refusal.

use rt_runtime::buffer::MidiBuffer;
use rt_runtime::resource::{encode_midi_record, Currentness, Format, ResourceStore, Snapshot};
use rt_runtime::{Budgets, Config, Error, Frame, Revision};
use shoop_wasm_test_support::heap::{assert_no_heap, TrackingAllocator};

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

fn prepare(pages: usize) -> Result<ResourceStore, Error> {
    ResourceStore::new(
        Config {
            max_frames: 8,
            resource_slots: 6,
            resource_pages: pages,
            page_bytes: 16,
            midi_events_per_port: 4,
            midi_bytes_per_port: 16,
            budgets: Budgets {
                transfer_bytes: 16,
                ..Budgets::default()
            },
            ..Config::default()
        }
        .prepare()?,
    )
}

fn export(store: &mut ResourceStore, snapshot: Snapshot) -> Result<Vec<u8>, Error> {
    let mut bytes = vec![0; snapshot.description.bytes];
    let mut cursor = store.cursor(snapshot.id)?;
    assert_no_heap(|| {
        for _ in 0..bytes.len().div_ceil(16) + 1 {
            let position = cursor.position();
            if position == bytes.len() {
                break;
            }
            assert!(
                store
                    .render_io()
                    .read(&mut cursor, &mut bytes[position..])?
                    > 0
            );
        }
        assert_eq!(cursor.position(), bytes.len());
        Ok::<_, Error>(())
    })?;
    Ok(bytes)
}

fn immutable_prefixes() -> Result<(), Error> {
    let mut store = prepare(8)?;
    store.begin_callback();
    let audio = store.create()?;
    // Capture appends contiguous intervals at absolute frames; a snapshot pins the actual prefix.
    let capture = store.begin_capture(
        audio,
        Revision(0),
        Format::Audio { channels: 1 },
        Frame(100),
    )?;
    assert_no_heap(|| {
        store
            .render_io()
            .capture_audio(capture, Frame(100), &[1.0, 2.0, 3.0])
    })?;
    let prefix = store.snapshot(audio, Frame(110))?;
    assert_eq!(prefix.cut_frame, Frame(103));
    assert_eq!(prefix.description.frames, 3);
    assert_eq!(prefix.revision, Revision(2));
    assert_no_heap(|| {
        store
            .render_io()
            .capture_audio(capture, Frame(103), &[4.0, 5.0, 6.0])
    })?;
    let expected: Vec<u8> = [1.0f32, 2.0, 3.0]
        .iter()
        .flat_map(|sample| sample.to_le_bytes())
        .collect();
    assert_eq!(export(&mut store, prefix)?, expected);
    assert_eq!(store.currentness(prefix.id)?, Currentness::Stale);
    store.begin_callback();
    let before_stop = store.snapshot(audio, Frame(106))?;
    assert_eq!(store.stop_capture(capture)?, Revision(3));
    assert_eq!(store.currentness(before_stop.id)?, Currentness::Stale);
    let frozen = store.snapshot(audio, Frame(120))?;
    assert_eq!(frozen.cut_frame, Frame(106));
    assert_eq!(frozen.description.frames, 6);
    assert_eq!(frozen.revision, before_stop.revision);
    assert!(!frozen.capture.unwrap().active);
    let expected: Vec<u8> = [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0]
        .iter()
        .flat_map(|sample| sample.to_le_bytes())
        .collect();
    assert_eq!(export(&mut store, frozen)?, expected);

    let midi = store.create()?;
    let capture = store.begin_capture(midi, Revision(0), Format::Midi, Frame(200))?;
    let mut input = MidiBuffer::new(2, 6)?;
    input.push(1, 4, &[0x90, 60, 100])?;
    input.push(1, 4, &[0x80, 60, 0])?;
    assert_no_heap(|| {
        store
            .render_io()
            .capture_midi(capture, Frame(200), 4, &input)
    })?;
    store.begin_callback();
    let midi_prefix = store.snapshot(midi, Frame(204))?;
    input.clear();
    assert_no_heap(|| {
        store
            .render_io()
            .capture_midi(capture, Frame(204), 2, &input)
    })?;
    assert_eq!(store.info(midi)?.description.frames, 6);
    assert_eq!(store.info(midi)?.description.bytes, 30);
    assert_eq!(store.stop_capture(capture)?, Revision(3));
    let midi_frozen = store.snapshot(midi, Frame(206))?;
    assert_eq!(midi_prefix.description.frames, 4);
    assert_eq!(midi_frozen.description.frames, 6);
    assert_eq!(midi_frozen.cut_frame, Frame(206));
    let mut expected = [0; 30];
    assert_eq!(
        encode_midi_record(1, &[0x90, 60, 100], &mut expected[..15])?,
        15
    );
    assert_eq!(
        encode_midi_record(1, &[0x80, 60, 0], &mut expected[15..])?,
        15
    );
    assert_eq!(export(&mut store, midi_prefix)?, expected);
    assert_eq!(export(&mut store, midi_frozen)?, expected);
    for snapshot in [prefix, before_stop, frozen, midi_prefix, midi_frozen] {
        store.release_snapshot(snapshot.id)?;
    }
    store.remove(audio)?;
    store.remove(midi)?;
    for _ in 0..16 {
        assert_no_heap(|| {
            store.begin_callback();
            store.reclaim();
        });
    }
    assert_eq!(store.usage().free_pages, 8);
    assert_eq!(store.usage().free_versions, 6);
    assert_eq!(store.usage().free_snapshots, 6);
    drop(store);
    Ok(())
}

fn incomplete_prefix() -> Result<(), Error> {
    let mut store = prepare(1)?;
    store.begin_callback();
    let resource = store.create()?;
    let capture = store.begin_capture(
        resource,
        Revision(0),
        Format::Audio { channels: 1 },
        Frame(100),
    )?;
    assert_no_heap(|| {
        store
            .render_io()
            .capture_audio(capture, Frame(100), &[1.0, 2.0, 3.0])
    })?;
    assert_eq!(store.snapshot(resource, Frame(103)), Err(Error::Capacity));
    assert!(store.info(resource)?.capture.unwrap().active);
    let mut output = [0.0; 2];
    let refused = assert_no_heap(|| {
        output.copy_from_slice(&[0.25, 0.5]);
        store
            .render_io()
            .capture_audio(capture, Frame(103), &[4.0, 5.0])
    });
    assert_eq!(refused, Err(Error::Capacity));
    assert_eq!(output, [0.25, 0.5]);
    let info = store.info(resource)?;
    assert_eq!(info.description.frames, 3);
    assert_eq!(info.description.bytes, 12);
    assert_eq!(info.revision, Revision(2));
    assert!(!info.capture.unwrap().active);
    assert_eq!(info.capture.unwrap().incomplete, Some(Error::Capacity));
    store.begin_callback();
    assert_eq!(
        store.render_io().capture_audio(capture, Frame(103), &[4.0]),
        Err(Error::Incomplete)
    );
    let frozen = store.snapshot(resource, Frame(120))?;
    assert_eq!(frozen.cut_frame, Frame(103));
    let expected: Vec<u8> = [1.0f32, 2.0, 3.0]
        .iter()
        .flat_map(|sample| sample.to_le_bytes())
        .collect();
    assert_eq!(export(&mut store, frozen)?, expected);
    store.release_snapshot(frozen.id)?;
    store.remove(resource)?;
    for _ in 0..8 {
        assert_no_heap(|| {
            store.begin_callback();
            store.reclaim();
        });
    }
    assert_eq!(store.usage().free_pages, 1);
    assert_eq!(store.usage().free_versions, 6);
    assert_eq!(store.usage().free_snapshots, 6);
    drop(store);
    Ok(())
}

fn main() -> Result<(), Error> {
    immutable_prefixes()?;
    incomplete_prefix()?;
    println!("recording prefixes: immutable tails, truthful cuts, silent MIDI duration and complete-or-refused capture verified");
    Ok(())
}
