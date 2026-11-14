use super::*;
use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};

fn setup() -> (ResourceStore, MidiStoreId, MidiBuffer) {
    let mut store = ResourceStore::with_midi_indexing(
        crate::Config {
            resource_pages: 4096,
            page_bytes: 16,
            budgets: crate::Budgets {
                transfer_bytes: 16,
                validation_steps: 1,
                ..crate::Budgets::default()
            },
            ..crate::Config::default()
        }
        .prepare()
        .unwrap(),
        crate::Quantum::new(1).unwrap(),
        MidiIndexConfig {
            versions: 8,
            checkpoints_per_version: 16,
            replay_events: 4,
        },
    )
    .unwrap();
    let midi = store
        .prepare_midi_store(
            shared_midi::Config {
                history_frames: 64,
                chunks: 8,
                views: 16,
                pages: 4000,
                max_events: 32,
                max_bytes: 256,
                max_packet: 48,
                transactions: 2,
                edit_steps: 3,
            },
            Frame(0),
            &MidiState::default(),
        )
        .unwrap();
    (store, midi, MidiBuffer::new(32, 256).unwrap())
}
fn check(
    store: &mut ResourceStore,
    snapshot: Snapshot,
    expected: &[(u64, &[u8])],
    initial: &MidiState,
) {
    let index = store.read_index(snapshot.id).unwrap();
    let mut cursor = store.render_io().midi_cursor(index).unwrap();
    let mut packet = [0; 48];
    let mut bytes = 0;
    for &(at, data) in expected {
        let event = store.render_io().midi_next(&mut cursor).unwrap().unwrap();
        assert_eq!(event.frame(), at);
        assert_eq!(
            store.render_io().midi_read(event, &mut packet),
            Ok(data.len())
        );
        assert_eq!(&packet[..data.len()], data);
        bytes += 12 + data.len();
    }
    assert_eq!(store.render_io().midi_next(&mut cursor), Ok(None));
    assert_eq!(snapshot.description.bytes, bytes);
    for frame in 0..=snapshot.description.frames {
        let mut state = *initial;
        for &(_, data) in expected.iter().filter(|(at, _)| *at < frame) {
            state.apply_packet(data, 48).unwrap();
        }
        let position = store.render_io().midi_seek(index, frame).unwrap();
        assert_eq!(position.state, state.basic_projection());
        assert!(position.replayed_events <= 8);
    }
    let mut wire = [0; 512];
    let mut size = 0;
    for &(at, data) in expected {
        wire[size..size + 8].copy_from_slice(&at.to_le_bytes());
        wire[size + 8..size + 12].copy_from_slice(&(data.len() as u32).to_le_bytes());
        wire[size + 12..size + 12 + data.len()].copy_from_slice(data);
        size += 12 + data.len();
    }
    for start in 0..=size {
        let mut cursor = store.cursor_at(index, start).unwrap();
        let mut position = start;
        while position < size {
            store.begin_callback();
            let mut out = [0; 7];
            let count = store.read(&mut cursor, &mut out).unwrap();
            assert!(count > 0);
            assert_eq!(&out[..count], &wire[position..position + count]);
            position += count;
        }
        assert_eq!(store.read(&mut cursor, &mut [0; 7]), Ok(0));
    }
}
fn drain(store: &mut ResourceStore) {
    for _ in 0..10000 {
        store.begin_callback();
        assert!(store.reclaim() <= 1);
    }
}

#[shoop_test]
fn ordinary_snapshots_byte_cursors_and_midi_seeks_pin_complete_revisions() {
    let (mut store, midi, mut input) = setup();
    let baseline = {
        let mut usage = store.usage();
        usage.reserved_pages = 0;
        usage
    };
    let control = [0xb0, 1, 35];
    assert_no_heap(|| {
        input.push(0, 12, &[0x90, 60, 80]).unwrap();
        input.push(3, 12, &control).unwrap();
        input.push(8, 12, &[0x80, 60, 0]).unwrap();
        store
            .render_io()
            .append_midi_history(midi, Frame(0), 12, &input)
            .unwrap();
        let resource = store.grab_midi(midi, Frame(2)..Frame(12)).unwrap();
        let old = store.snapshot(resource, Frame(12)).unwrap();
        let retained = store.retain_snapshot(old.id).unwrap();
        let fork = store.fork_midi(midi, old.id).unwrap();
        let peer = store.snapshot(fork, Frame(12)).unwrap();
        let mut initial = MidiState::default();
        initial.apply_packet(&[0x90, 60, 80], 48).unwrap();
        let originals = [(1, &control[..]), (6, &[0x80, 60, 0][..])];
        check(&mut store, old, &originals, &initial);
        input.clear();
        input.push(0, 2, &[0xb0, 1, 99]).unwrap();
        let edit = store
            .render_io()
            .prepare_midi_replace(resource, Revision(1), 5, 2, &input, &initial)
            .unwrap();
        input.clear();
        while store.render_io().progress_midi_edit(edit, 3) == Ok(shared_midi::EditStatus::Building)
        {
        }
        assert_eq!(store.render_io().commit_midi_edit(edit), Ok(Revision(2)));
        let edited = store.snapshot(resource, Frame(12)).unwrap();
        check(
            &mut store,
            edited,
            &[(1, &control), (5, &[0xb0, 1, 99])],
            &initial,
        );
        assert_eq!(store.currentness(old.id), Ok(Currentness::Stale));
        let upload = store
            .begin_upload(
                resource,
                Revision(2),
                Description {
                    format: Format::Midi,
                    frames: 1,
                    bytes: 0,
                },
            )
            .unwrap();
        store.finish_upload(upload).unwrap();
        store.commit(upload).unwrap();
        store.remove(resource).unwrap();
        store.remove(fork).unwrap();
        for source in [old, retained, peer] {
            check(&mut store, source, &originals, &initial);
        }
        check(
            &mut store,
            edited,
            &[(1, &control), (5, &[0xb0, 1, 99])],
            &initial,
        );
        assert_eq!(store.cleanup_midi_store(midi), Err(Error::Busy));
        for source in [old, retained, peer, edited] {
            store.release_snapshot(source.id).unwrap();
        }
        drain(&mut store);
        store.cleanup_midi_store(midi).unwrap();
        drain(&mut store);
        assert_eq!(store.usage(), baseline);
    });
}

fn upload(
    store: &mut ResourceStore,
    duration: u64,
    events: &[(u64, &[u8])],
    initial: &MidiState,
) -> ResourceId {
    let mut wire = [0; 512];
    let mut used = 0;
    for &(frame, packet) in events {
        used += encode_midi_record(frame, packet, &mut wire[used..]).unwrap();
    }
    let resource = store.create().unwrap();
    let upload = store
        .begin_indexed_midi_upload(
            resource,
            Revision(0),
            Description {
                format: Format::Midi,
                frames: duration,
                bytes: used,
            },
            initial,
        )
        .unwrap();
    for (offset, byte) in wire[..used].iter().enumerate() {
        store.begin_callback();
        store
            .write_upload(upload, offset, core::slice::from_ref(byte))
            .unwrap();
    }
    store.begin_callback();
    store.finish_upload(upload).unwrap();
    store.commit(upload).unwrap();
    resource
}

#[shoop_test]
fn indexed_uploads_longer_than_history_import_without_packet_copy_and_survive_removal() {
    let (mut store, midi, mut input) = setup();
    let control = [0xb0, 1, 35];
    let original = [
        (4, &[0x90, 64, 77][..]),
        (100, &control[..]),
        (100, &[0xb0, 7, 21][..]),
        (256, &[0x80, 64, 0][..]),
    ];
    let mut initial = MidiState::default();
    initial.apply_packet(&[0x90, 60, 91], 48).unwrap();
    assert_no_heap(|| {
        let source = upload(&mut store, 257, &original, &initial);
        let source_snapshot = store.snapshot(source, Frame(0)).unwrap();
        let import = store.prepare_midi_import(midi, source_snapshot.id).unwrap();
        let pages = store.usage().free_pages;
        assert_eq!(store.finish_midi_import(import), Err(Error::Busy));
        store.remove(source).unwrap();
        store.release_snapshot(source_snapshot.id).unwrap();
        let mut frame = 0;
        while store.midi_import_status(import) == Ok(shared_midi::EditStatus::Building) {
            store.begin_callback();
            store.charge_external(16, 1).unwrap();
            store.render_io().progress_midi_import(import, 1).unwrap();
            assert_eq!(store.usage().free_pages, pages - frame.min(1));
            input.clear();
            input.push(0, 1, &[0xb2, 1, frame as u8]).unwrap();
            store
                .render_io()
                .append_midi_history(midi, Frame(frame as u64), 1, &input)
                .unwrap();
            frame += 1;
        }
        let resource = store.finish_midi_import(import).unwrap();
        let old = store.snapshot(resource, Frame(0)).unwrap();
        let fork = store.fork_midi(midi, old.id).unwrap();
        check(&mut store, old, &original, &initial);
        input.clear();
        input.push(0, 20, &[0xb0, 7, 99]).unwrap();
        let edit = store
            .render_io()
            .prepare_midi_replace(resource, Revision(1), 90, 20, &input, &initial)
            .unwrap();
        while store.render_io().progress_midi_edit(edit, 2) == Ok(shared_midi::EditStatus::Building)
        {
        }
        store.render_io().commit_midi_edit(edit).unwrap();
        let edited = store.snapshot(resource, Frame(0)).unwrap();
        check(
            &mut store,
            edited,
            &[
                (4, original[0].1),
                (90, &[0xb0, 7, 99]),
                (256, original[3].1),
            ],
            &initial,
        );
        check(&mut store, old, &original, &initial);
        let peer = store.snapshot(fork, Frame(0)).unwrap();
        check(&mut store, peer, &original, &initial);
        for resource in [resource, fork] {
            store.remove(resource).unwrap();
        }
        for snapshot in [old, edited, peer] {
            store.release_snapshot(snapshot.id).unwrap();
        }
        drain(&mut store);
        store.cleanup_midi_store(midi).unwrap();
        drain(&mut store);
        assert_eq!(store.usage().free_pages, 4096);
        assert_eq!(store.usage().reserved_pages, 0);
        assert_eq!(store.midi_index_usage().free_versions, 8);
    });
}

#[shoop_test]
fn cancelled_import_and_superseded_pending_edit_return_all_owned_credits() {
    let (mut store, midi, mut input) = setup();
    let packet = [0xb0, 2, 49];
    assert_no_heap(|| {
        let source = upload(
            &mut store,
            32,
            &[(0, &[0x90, 60, 80]), (1, &[0xb0, 1, 2]), (2, &packet)],
            &MidiState::default(),
        );
        let old = store.snapshot(source, Frame(0)).unwrap();
        let baseline = store.usage();
        let import = store.prepare_midi_import(midi, old.id).unwrap();
        assert_eq!(
            store.render_io().progress_midi_import(import, 1),
            Ok(shared_midi::EditStatus::Building)
        );
        assert_eq!(store.finish_midi_import(import), Err(Error::Busy));
        store.cancel_midi_import(import).unwrap();
        assert_eq!(store.midi_import_status(import), Err(Error::InvalidId));
        drain(&mut store);
        assert_eq!(store.usage(), baseline);
        input.push(0, 8, &[0x90, 60, 80]).unwrap();
        store
            .render_io()
            .append_midi_history(midi, Frame(0), 8, &input)
            .unwrap();
        let resource = store.grab_midi(midi, Frame(0)..Frame(8)).unwrap();
        let snapshot = store.snapshot(resource, Frame(8)).unwrap();
        input.clear();
        input.push(0, 1, &[0xb0, 1, 99]).unwrap();
        let edit = store
            .render_io()
            .prepare_midi_replace(resource, Revision(1), 2, 1, &input, &MidiState::default())
            .unwrap();
        while store.render_io().progress_midi_edit(edit, 3) == Ok(shared_midi::EditStatus::Building)
        {
        }
        let replacement = store
            .begin_upload(
                resource,
                Revision(1),
                Description {
                    format: Format::Midi,
                    frames: 8,
                    bytes: 0,
                },
            )
            .unwrap();
        store.begin_callback();
        store.finish_upload(replacement).unwrap();
        store.commit(replacement).unwrap();
        assert_eq!(
            store.render_io().commit_midi_edit(edit),
            Err(Error::StaleRevision)
        );
        store.cancel_midi_edit(edit).unwrap();
        check(
            &mut store,
            snapshot,
            &[(0, &[0x90, 60, 80])],
            &MidiState::default(),
        );
        for resource in [resource, source] {
            store.remove(resource).unwrap();
        }
        for snapshot in [snapshot, old] {
            store.release_snapshot(snapshot.id).unwrap();
        }
        drain(&mut store);
        store.cleanup_midi_store(midi).unwrap();
        drain(&mut store);
        assert_eq!(store.usage().free_pages, 4096);
        assert_eq!(store.usage().reserved_pages, 0);
        assert_eq!(store.midi_index_usage().free_versions, 8);
    });
}

#[shoop_test]
fn one_step_reclamation_rotates_between_linear_audio_and_midi_owners() {
    let (mut store, midi, mut input) = setup();
    let audio = store
        .prepare_audio_store(
            shared_audio::Config {
                channels: 1,
                history_frames: 8,
                views: 4,
                pages: 32,
            },
            Frame(0),
        )
        .unwrap();
    assert_no_heap(|| {
        store
            .render_io()
            .append_audio_history(audio, Frame(0), &[1.; 8])
            .unwrap();
        let linear = upload(
            &mut store,
            8,
            &[(0, &[0x90, 60, 80])],
            &MidiState::default(),
        );
        store.remove(linear).unwrap();
        for event in 0..32 {
            input.push(event, 32, &[0xb0, 1, event as u8]).unwrap();
        }
        store
            .render_io()
            .append_midi_history(midi, Frame(0), 32, &input)
            .unwrap();
        store.cleanup_audio_store(audio).unwrap();
        store.cleanup_midi_store(midi).unwrap();
        let before = store.usage().free_pages;
        let initial_midi = store.midi_stores[midi.slot].as_ref().unwrap().work();
        for _ in 0..12 {
            store.begin_callback();
            assert_eq!(store.reclaim(), 1);
        }

        assert_eq!(
            store.usage().free_versions,
            store.config.limits().resource_slots
        );
        assert!(
            store.midi_stores[midi.slot]
                .as_ref()
                .unwrap()
                .work()
                .reclaimed
                > initial_midi.reclaimed
        );
        for _ in 0..36 {
            store.begin_callback();
            assert!(store.reclaim() <= 1);
        }
        assert!(store.usage().reserved_pages <= 4000);
        assert!(store.usage().free_pages >= before + 2);
        drain(&mut store);
        assert_eq!(store.usage().free_pages, 4096);
        assert_eq!(store.usage().reserved_pages, 0);
    });
}
