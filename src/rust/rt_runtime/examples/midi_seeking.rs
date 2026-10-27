//! Lease-scoped event copies and strict-pre-target checkpoint positioning.

use rt_runtime::midi_state::MidiState;
use rt_runtime::resource::{
    encode_midi_record, Description, Format, MidiIndexConfig, ResourceStore, Snapshot, UploadId,
};
use rt_runtime::{Budgets, Config, Error, Frame, PreparedConfig, Revision};
use shoop_wasm_test_support::heap::{assert_no_heap, TrackingAllocator};

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

fn config(page: usize) -> Result<PreparedConfig, Error> {
    Config {
        max_frames: 8,
        resource_slots: 8,
        resource_pages: 16,
        page_bytes: page,
        midi_events_per_port: 8,
        midi_bytes_per_port: 64,
        budgets: Budgets {
            transfer_bytes: page,
            validation_steps: page,
            ..Budgets::default()
        },
        ..Config::default()
    }
    .prepare()
}

fn encode(records: &[(u64, &[u8])]) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    for &(frame, packet) in records {
        let start = bytes.len();
        bytes.resize(start + 12 + packet.len(), 0);
        encode_midi_record(frame, packet, &mut bytes[start..])?;
    }
    Ok(bytes)
}

fn write(store: &mut ResourceStore, upload: UploadId, bytes: &[u8]) -> Result<(), Error> {
    let page = store.config().limits().page_bytes;
    for (index, chunk) in bytes.chunks(page).enumerate() {
        assert_no_heap(|| {
            store.begin_callback();
            store.write_upload(upload, index * page, chunk)
        })?;
    }
    store.finish_upload(upload)
}

fn remove_all(
    store: &mut ResourceStore,
    owners: &[(rt_runtime::ResourceId, Snapshot)],
) -> Result<(), Error> {
    for &(resource, snapshot) in owners {
        store.release_snapshot(snapshot.id)?;
        store.remove(resource)?;
    }
    for _ in 0..32 {
        assert_no_heap(|| {
            store.begin_callback();
            store.reclaim();
        });
    }
    assert_eq!(store.usage().free_pages, 16);
    assert_eq!(store.usage().free_versions, 8);
    assert_eq!(store.usage().free_snapshots, 8);
    Ok(())
}

fn event_copies() -> Result<(), Error> {
    let config = config(16)?;
    let mut store = ResourceStore::new(config)?;
    let mut foreign = ResourceStore::new(config)?;
    let mut sysex = [0; 37];
    sysex[0] = 0xf0;
    sysex[1] = 0x7d;
    sysex[36] = 0xf7;
    let records: [(u64, &[u8]); 3] = [(0, &[0x90, 60, 100]), (2, &sysex), (2, &[0x80, 60, 0])];
    let bytes = encode(&records)?;
    let resource = store.create()?;
    let upload = store.begin_upload(
        resource,
        Revision(0),
        Description {
            format: Format::Midi,
            frames: 5,
            bytes: bytes.len(),
        },
    )?;
    write(&mut store, upload, &bytes)?;
    store.commit(upload)?;
    let old = store.snapshot(resource, Frame(0))?;
    let index = store.read_index(old.id)?;
    let mut cursor = store.render_io().midi_cursor(index)?;
    let mut scratch = [0xa5; 64];
    let mut retained = None;
    assert_no_heap(|| {
        for &(frame, packet) in &records {
            let event = store.render_io().midi_next(&mut cursor)?.unwrap();
            assert_eq!(event.frame(), frame);
            assert_eq!(event.byte_len(), packet.len());
            if packet.len() == 37 {
                let mut too_small = [0xa5; 8];
                assert_eq!(
                    store.render_io().midi_read(event, &mut too_small),
                    Err(Error::Capacity)
                );
                assert_eq!(too_small, [0xa5; 8]);
                retained = Some(event);
            }
            let count = store.render_io().midi_read(event, &mut scratch)?;
            assert_eq!(&scratch[..count], packet);
        }
        assert!(store.render_io().midi_next(&mut cursor)?.is_none());
        assert_eq!(cursor.byte_position(), bytes.len());
        assert!(matches!(
            store.render_io().midi_seek(index, 2),
            Err(Error::InvalidState)
        ));
        Ok::<_, Error>(())
    })?;
    let event = retained.unwrap();
    assert_eq!(
        foreign.render_io().midi_read(event, &mut scratch),
        Err(Error::InvalidId)
    );
    assert!(matches!(
        foreign.render_io().midi_cursor(index),
        Err(Error::InvalidId)
    ));
    let upload = store.begin_upload(
        resource,
        Revision(1),
        Description {
            format: Format::Midi,
            frames: 5,
            bytes: 0,
        },
    )?;
    store.finish_upload(upload)?;
    store.commit(upload)?;
    assert_eq!(store.render_io().midi_read(event, &mut scratch)?, 37);
    assert_eq!(scratch[..37], sysex);
    store.release_snapshot(old.id)?;
    assert_eq!(
        store.render_io().midi_read(event, &mut scratch),
        Err(Error::InvalidId)
    );
    assert!(matches!(
        store.render_io().midi_next(&mut cursor),
        Err(Error::InvalidId)
    ));
    store.remove(resource)?;
    for _ in 0..32 {
        assert_no_heap(|| {
            store.begin_callback();
            store.reclaim();
        });
    }
    assert_eq!(store.usage().free_pages, 16);
    assert_eq!(store.usage().free_versions, 8);
    assert_eq!(store.usage().free_snapshots, 8);
    drop(store);
    drop(foreign);
    Ok(())
}

fn checkpoint_states() -> Result<(), Error> {
    let mut store = ResourceStore::with_midi_indexing(
        config(128)?,
        MidiIndexConfig {
            versions: 4,
            checkpoints_per_version: 3,
            replay_events: 2,
        },
    )?;
    let mut initial = MidiState::default();
    initial.apply_packet(&[0x90, 62, 40], 3)?;
    let records: [(u64, &[u8]); 5] = [
        (0, &[0x90, 60, 100]),
        (1, &[0xb0, 64, 127]),
        (1, &[0x80, 60, 45]),
        (3, &[0xb0, 64, 0]),
        (4, &[0x90, 61, 80]),
    ];
    let bytes = encode(&records)?;
    let resource = store.create()?;
    let upload = store.begin_indexed_midi_upload(
        resource,
        Revision(0),
        Description {
            format: Format::Midi,
            frames: 6,
            bytes: bytes.len(),
        },
        &initial,
    )?;
    write(&mut store, upload, &bytes)?;
    store.commit(upload)?;
    let snapshot = store.snapshot(resource, Frame(0))?;
    let index = store.read_index(snapshot.id)?;
    assert_eq!(store.midi_index_info(resource)?.events, 5);
    assert_eq!(store.midi_index_info(resource)?.checkpoints, 3);
    let mut packet = [0; 3];
    assert_no_heap(|| {
        for target in [0, 1, 2, 3, 4, 6] {
            let mut position = store.render_io().midi_seek(index, target)?;
            assert!(position.replayed_events < 2);
            let mut linear = initial;
            for &(frame, bytes) in &records {
                if frame < target {
                    linear.apply_packet(bytes, 3)?;
                }
            }
            assert_eq!(position.state, linear);
            let channel = position.state.channel(0).unwrap();
            for key in 0..128 {
                let note = channel.note(key).unwrap();
                let (velocity, held) = match key {
                    62 => (40, true),
                    60 if target == 1 => (100, true),
                    60 if target == 2 || target == 3 => (100, false),
                    61 if target == 6 => (80, true),
                    _ => (0, false),
                };
                assert_eq!((note.velocity(), note.held()), (velocity, held));
            }
            assert_eq!(
                channel.controller(64),
                Some(if target == 2 || target == 3 { 127 } else { 0 })
            );
            let next = store.render_io().midi_next(&mut position.cursor)?;
            assert_eq!(
                next.map(|event| event.frame()),
                records
                    .iter()
                    .find(|(frame, _)| *frame >= target)
                    .map(|(frame, _)| *frame)
            );
            if target == 1 {
                store.render_io().midi_read(next.unwrap(), &mut packet)?;
                assert_eq!(packet, [0xb0, 64, 127]);
                let note_off = store.render_io().midi_next(&mut position.cursor)?.unwrap();
                store.render_io().midi_read(note_off, &mut packet)?;
                assert_eq!(packet, [0x80, 60, 45]);
            }
        }
        assert!(matches!(
            store.render_io().midi_seek(index, 7),
            Err(Error::InvalidFrame)
        ));
        Ok::<_, Error>(())
    })?;

    let overflow = store.create()?;
    let mut six = records.to_vec();
    six.push((5, &[0xf8]));
    let six_bytes = encode(&six)?;
    let upload = store.begin_indexed_midi_upload(
        overflow,
        Revision(0),
        Description {
            format: Format::Midi,
            frames: 6,
            bytes: six_bytes.len(),
        },
        &initial,
    )?;
    store.begin_callback();
    store.write_upload(upload, 0, &bytes)?;
    assert_eq!(
        store.write_upload(upload, bytes.len(), &six_bytes[bytes.len()..]),
        Err(Error::Capacity)
    );
    assert_eq!(store.finish_upload(upload), Err(Error::Incomplete));
    store.cancel_upload(upload)?;
    store.remove(overflow)?;

    let malformed = store.create()?;
    let valid = encode(&records[..2])?;
    let mut invalid = valid.clone();
    *invalid.last_mut().unwrap() = 0xff;
    let upload = store.begin_indexed_midi_upload(
        malformed,
        Revision(0),
        Description {
            format: Format::Midi,
            frames: 6,
            bytes: valid.len(),
        },
        &initial,
    )?;
    store.begin_callback();
    assert_eq!(
        store.write_upload(upload, 0, &invalid),
        Err(Error::InvalidEvent)
    );
    assert_eq!(store.work().validation_steps, invalid.len());
    assert_eq!(store.work().copied_bytes, 0);
    store.write_upload(upload, 0, &valid)?;
    store.finish_upload(upload)?;
    store.cancel_upload(upload)?;
    store.remove(malformed)?;

    let empty = store.create()?;
    let upload = store.begin_indexed_midi_upload(
        empty,
        Revision(0),
        Description {
            format: Format::Midi,
            frames: 0,
            bytes: 0,
        },
        &initial,
    )?;
    store.finish_upload(upload)?;
    store.commit(upload)?;
    let empty_snapshot = store.snapshot(empty, Frame(0))?;
    let empty_index = store.read_index(empty_snapshot.id)?;
    let mut empty_position = store.render_io().midi_seek(empty_index, 0)?;
    assert_eq!(empty_position.state, initial);
    assert!(store
        .render_io()
        .midi_next(&mut empty_position.cursor)?
        .is_none());
    remove_all(&mut store, &[(resource, snapshot), (empty, empty_snapshot)])?;
    assert_eq!(store.midi_index_usage().free_versions, 4);
    drop(store);
    Ok(())
}

fn main() -> Result<(), Error> {
    event_copies()?;
    checkpoint_states()?;
    println!("MIDI seeking: cross-page packets, retained leases, strict pre-target states, five-event bound and atomic refusal verified");
    Ok(())
}
