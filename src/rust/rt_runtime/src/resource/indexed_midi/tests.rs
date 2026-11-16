use super::*;
use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};

const Q: usize = 32;

#[shoop_test]
fn prepared_grab_pins_its_cut_while_live_history_advances_and_cancellation_returns_the_pin() {
    let mut resources = resources();
    let baseline = resources.usage();
    let pool = resources.prepare_indexed_midi_store(config()).unwrap();
    let source = resources.indexed_midi_source(pool, 0).unwrap();
    let mut input = MidiBuffer::new(8, 128).unwrap();
    let mut expected = [0; 60];
    for (index, (frame, packet)) in [
        (5, [0xb0, 7, 1]),
        (27, [0x90, 62, 90]),
        (37, [0xb0, 7, 2]),
        (59, [0x90, 63, 90]),
    ]
    .into_iter()
    .enumerate()
    {
        encode_midi_record(frame, &packet, &mut expected[index * 15..(index + 1) * 15]).unwrap();
    }
    assert_no_heap(|| {
        for tick in 0..4u64 {
            resources.begin_callback();
            input.clear();
            input.push(0, Q, &[0x90, 60 + tick as u8, 90]).unwrap();
            input.push(10, Q, &[0xb0, 7, tick as u8]).unwrap();
            resources
                .render_io()
                .append_indexed_midi_source(source, Frame(tick * Q as u64), &input)
                .unwrap();
        }
        let grab = resources
            .prepare_indexed_midi_grab(source, Frame(37)..Frame(100), 64)
            .unwrap();
        assert_eq!(resources.finish_indexed_midi_grab(grab), Err(Error::Busy));
        let mut tick = 4;
        loop {
            resources.begin_callback();
            if resources.progress_indexed_midi_grab(grab, 256).unwrap() == Status::Ready {
                break;
            }
            assert!(tick < 100);
            input.clear();
            input.push(0, Q, &[0xb0, 7, tick as u8]).unwrap();
            resources
                .render_io()
                .append_indexed_midi_source(source, Frame(tick * Q as u64), &input)
                .unwrap();
            tick += 1;
        }
        assert!(resources.indexed_midi_coverage(source).unwrap().start > Frame(128));
        let take = resources.finish_indexed_midi_grab(grab).unwrap();
        let snapshot = resources.snapshot(take, Frame(tick * Q as u64)).unwrap();
        assert_eq!(bytes::<60>(&mut resources, snapshot), expected);
        let index = resources.read_index(snapshot.id).unwrap();
        let initial = resources.render_io().midi_seek(index, 0).unwrap().state;
        assert_eq!(initial.channel(0).unwrap().controller(7), Some(0));
        assert!(initial.channel(0).unwrap().note(60).unwrap().held());
        assert!(initial.channel(0).unwrap().note(61).unwrap().held());
        assert!(!initial.channel(0).unwrap().note(62).unwrap().held());
        let end = resources.render_io().midi_seek(index, 64).unwrap().state;
        assert_eq!(end.channel(0).unwrap().controller(7), Some(2));
        assert!(end.channel(0).unwrap().note(63).unwrap().held());
        let coverage = resources.indexed_midi_coverage(source).unwrap();
        let cancelled = resources
            .prepare_indexed_midi_grab(source, coverage.clone(), coverage.end.0 - coverage.start.0)
            .unwrap();
        resources.progress_indexed_midi_grab(cancelled, 1).unwrap();
        resources.cancel_indexed_midi_grab(cancelled).unwrap();
        assert_eq!(
            resources.progress_indexed_midi_grab(cancelled, 1),
            Err(Error::InvalidId)
        );
        resources.release_snapshot(snapshot.id).unwrap();
        resources.remove(take).unwrap();
        drain(&mut resources);
        resources.cleanup_indexed_midi_store(pool).unwrap();
        drain(&mut resources);
        assert_eq!(resources.usage(), baseline);
    });
}
fn config() -> Config {
    Config {
        blocks: BlockLimits {
            event_credits: 2048,
            record_pages: 2048,
            import_bases: 4,
            media: 32,
            versions: (32) * 8,
            directories: 4096,
        },
        sources: 2,
        history_frames: 128,
        history_events: 32,
        history_bytes: 1024,
        views: 8,
        writers: 4,
        imports: 2,
        events_per_tick: 8,
        bytes_per_tick: 128,
        maintenance_steps_per_tick: 32,
    }
}

#[shoop_test]
fn private_block_patches_wrap_atomically_and_keep_old_exports_and_boundary_state() {
    let mut resources = resources();
    let pool = resources.prepare_indexed_midi_store(config()).unwrap();
    let mut original = [0; 45];
    for (index, (frame, message)) in [
        (0, [0x90, 60, 100]),
        (32, [0xb0, 1, 12]),
        (96, [0x80, 60, 0]),
    ]
    .into_iter()
    .enumerate()
    {
        encode_midi_record(frame, &message, &mut original[index * 15..]).unwrap();
    }
    let loaded = upload(&mut resources, &original, 128);
    resources.begin_callback();
    let loaded_snapshot = resources.snapshot(loaded, Frame(0)).unwrap();
    let import = resources
        .prepare_indexed_midi_import(pool, loaded_snapshot.id, &MidiState::default())
        .unwrap();
    while resources.progress_indexed_midi_import(import, 8).unwrap() != Status::Ready {}
    let resource = resources.finish_indexed_midi_import(import).unwrap();
    let old = resources.snapshot(resource, Frame(0)).unwrap();
    let mut input = MidiBuffer::new(4, 12).unwrap();
    for (at, message) in [
        (0, [0xb0, 7, 93]),
        (47, [0x90, 67, 80]),
        (48, [0x80, 60, 0]),
        (63, [0x90, 68, 90]),
    ] {
        input.push(at, 64, &message).unwrap();
    }
    let mut before = MidiState::default();
    crate::midi::ShortMessage::managed(&[0x90, 60, 100])
        .unwrap()
        .apply(&mut before)
        .unwrap();
    assert_no_heap(|| {
        let edit = resources
            .prepare_indexed_midi_patch(resource, Revision(1), 80, 64, &input, &before)
            .unwrap();
        assert_eq!(resources.commit_indexed_midi_patch(edit), Err(Error::Busy));
        assert_eq!(
            resources.progress_indexed_midi_patch(edit, 1),
            Ok(Status::Building)
        );
        assert_eq!(bytes::<45>(&mut resources, old), original);
        assert_eq!(
            resources.progress_indexed_midi_patch(edit, 16),
            Ok(Status::Ready)
        );
        assert_eq!(resources.commit_indexed_midi_patch(edit), Ok(Revision(2)));
        assert_eq!(
            resources.progress_indexed_midi_patch(edit, 1),
            Err(Error::InvalidId)
        );
        let current = resources.snapshot(resource, Frame(1)).unwrap();
        let mut expected = [0; 75];
        for (index, (frame, message)) in [
            (0, [0x80, 60, 0]),
            (15, [0x90, 68, 90]),
            (32, [0xb0, 1, 12]),
            (80, [0xb0, 7, 93]),
            (127, [0x90, 67, 80]),
        ]
        .into_iter()
        .enumerate()
        {
            encode_midi_record(frame, &message, &mut expected[index * 15..]).unwrap();
        }
        assert_eq!(bytes::<75>(&mut resources, current), expected);
        assert_eq!(bytes::<45>(&mut resources, old), original);
        let index = resources.read_index(current.id).unwrap();
        let start = resources.render_io().midi_seek(index, 0).unwrap().state;
        assert_eq!(start.channel(0).unwrap().controller(7), Some(93));
        assert!(start.channel(0).unwrap().note(60).unwrap().held());
        assert!(start.channel(0).unwrap().note(67).unwrap().held());
        assert!(!start.channel(0).unwrap().note(68).unwrap().held());
        let edit = resources
            .prepare_indexed_midi_patch(resource, Revision(2), 80, 64, &input, &before)
            .unwrap();
        assert_eq!(
            resources.progress_indexed_midi_patch(edit, 1),
            Ok(Status::Building)
        );
        resources.cancel_indexed_midi_patch(edit).unwrap();
        let unchanged = resources.snapshot(resource, Frame(2)).unwrap();
        assert_eq!(bytes::<75>(&mut resources, unchanged), expected);
        for snapshot in [old, current, unchanged, loaded_snapshot] {
            resources.release_snapshot(snapshot.id).unwrap();
        }
        for resource in [resource, loaded] {
            resources.remove(resource).unwrap();
        }
        drain(&mut resources);
        resources.cleanup_indexed_midi_store(pool).unwrap();
        drain(&mut resources);
    });
}
fn resources() -> ResourceStore {
    ResourceStore::with_midi_indexing(
        crate::Config {
            max_frames: Q,
            page_bytes: 64,
            resource_pages: 256,
            budgets: crate::Budgets {
                transfer_bytes: 64,
                validation_steps: 256,
                ..crate::Budgets::default()
            },
            ..crate::Config::default()
        }
        .prepare()
        .unwrap(),
        Quantum::new(Q).unwrap(),
        MidiIndexConfig {
            versions: 32,
            checkpoints_per_version: 32,
            replay_events: 32,
        },
    )
    .unwrap()
}
fn drain(resources: &mut ResourceStore) {
    for _ in 0..2000 {
        resources.begin_callback();
        if resources.reclaim() == 0 {
            return;
        }
    }
    panic!("retirement must finish");
}
fn bytes<const N: usize>(resources: &mut ResourceStore, snapshot: Snapshot) -> [u8; N] {
    assert_eq!(snapshot.description.bytes, N);
    let mut data = [0; N];
    let mut cursor = resources.cursor(snapshot.id).unwrap();
    let mut copied = 0;
    while copied < N {
        let count = resources
            .render_io()
            .read(&mut cursor, &mut data[copied..])
            .unwrap();
        assert!(count > 0);
        copied += count;
    }
    data
}
fn admit(
    resources: &mut ResourceStore,
    resource: ResourceId,
    kind: WriteKind,
    position: u64,
) -> WriterId {
    let id = resources
        .prepare_indexed_midi_writer(resource, resources.revision(resource).unwrap(), kind)
        .unwrap();
    let mut steps = 0;
    while resources.progress_indexed_midi_writer(id, 1).unwrap() != Status::Ready {
        steps += 1;
        assert!(steps < 100);
    }
    for attempt in 0..1000 {
        if resources
            .prepare_indexed_midi_writer_position(id, position, 256)
            .unwrap()
        {
            break;
        }
        assert!(attempt < 999);
    }
    resources.start_indexed_midi_writer(id, position).unwrap();
    id
}
fn upload(resources: &mut ResourceStore, data: &[u8], frames: u64) -> ResourceId {
    let resource = resources.create().unwrap();
    let id = resources
        .begin_indexed_midi_upload(
            resource,
            Revision(0),
            Description {
                format: Format::Midi,
                frames,
                bytes: data.len(),
            },
            &MidiState::default(),
        )
        .unwrap();
    for (page, chunk) in data.chunks(64).enumerate() {
        resources.begin_callback();
        resources.write_upload(id, page * 64, chunk).unwrap();
    }
    resources.finish_upload(id).unwrap();
    resources.commit(id).unwrap();
    resource
}
fn import(
    resources: &mut ResourceStore,
    pool: StoreId,
    snapshot: Snapshot,
    initial: &MidiState,
) -> ResourceId {
    let job = resources
        .prepare_indexed_midi_import(pool, snapshot.id, initial)
        .unwrap();
    assert_eq!(resources.finish_indexed_midi_import(job), Err(Error::Busy));
    while resources.progress_indexed_midi_import(job, 1).unwrap() != Status::Ready {}
    resources.finish_indexed_midi_import(job).unwrap()
}

#[shoop_test]
fn ordinary_resources_keep_uploaded_and_grabbed_versions_through_continuous_writes_and_exit() {
    let mut resources = resources();
    let baseline = resources.usage();
    let pool = resources.prepare_indexed_midi_store(config()).unwrap();
    let source = resources.indexed_midi_source(pool, 0).unwrap();
    let independent = resources.indexed_midi_source(pool, 1).unwrap();
    let mut input = MidiBuffer::new(8, 128).unwrap();
    let mut initial = MidiState::default();
    initial.apply_packet(&[0xb0, 7, 42], 64).unwrap();
    let mut encoded = [0; 30];
    encode_midi_record(96, &[0xb0, 1, 37], &mut encoded[..15]).unwrap();
    encode_midi_record(96, &[0x90, 60, 90], &mut encoded[15..]).unwrap();
    let loaded = upload(&mut resources, &encoded, 128);
    resources.begin_callback();
    let snapshot = resources.snapshot(loaded, Frame(0)).unwrap();
    let resource = import(&mut resources, pool, snapshot, &initial);
    resources.release_snapshot(snapshot.id).unwrap();
    resources.remove(loaded).unwrap();
    drain(&mut resources);
    assert_no_heap(|| {
        let old = resources.snapshot(resource, Frame(0)).unwrap();
        assert_eq!(bytes::<30>(&mut resources, old), encoded);
        let old_index = resources.read_index(old.id).unwrap();
        while !resources
            .io()
            .prepare_midi_position(old_index, 97, 256)
            .unwrap()
        {}
        let position = resources.render_io().midi_seek(old_index, 97).unwrap();
        let mut expected = initial;
        expected.apply_packet(&[0xb0, 1, 37], 64).unwrap();
        expected.apply_packet(&[0x90, 60, 90], 64).unwrap();
        assert_eq!(position.state, expected);
        assert_eq!(position.cursor.byte_position(), 30);
        let peer = resources.fork_indexed_midi(pool, old.id).unwrap();
        let writer = admit(&mut resources, resource, WriteKind::Fixed, 0);
        let second = admit(&mut resources, peer, WriteKind::Fixed, 0);
        assert_eq!(
            resources.prepare_indexed_midi_writer(resource, Revision(1), WriteKind::Fixed),
            Err(Error::Busy)
        );
        let prefix = resources
            .grab_indexed_midi(source, Frame(0)..Frame(0), 0)
            .unwrap();
        let growing = admit(
            &mut resources,
            prefix,
            WriteKind::Growing {
                maximum_frames: 2048,
            },
            0,
        );
        let mut latest = [0; 15];
        for tick in 0..48 {
            resources.begin_callback();
            input.clear();
            if tick % 4 != 1 {
                let packet = [0xb0, 7, tick as u8];
                input.push(7, Q, &packet).unwrap();
                encode_midi_record(7, &packet, &mut latest).unwrap();
            }
            let frame = Frame(tick * Q as u64);
            resources
                .render_io()
                .append_indexed_midi_source(source, frame, &input)
                .unwrap();
            assert_eq!(
                resources
                    .render_io()
                    .write_indexed_midi_tick(writer, source, frame)
                    .unwrap(),
                (tick + 1) % 4 * Q as u64
            );
            resources
                .render_io()
                .write_indexed_midi_tick(second, source, frame)
                .unwrap();
            resources
                .render_io()
                .write_indexed_midi_tick(growing, source, frame)
                .unwrap();
            assert_eq!(
                resources
                    .render_io()
                    .write_indexed_midi_tick(writer, source, frame),
                Err(Error::InvalidFrame)
            );
            assert_eq!(bytes::<30>(&mut resources, old), encoded);
            resources.reclaim();
        }
        assert_eq!(
            resources.indexed_midi_coverage(independent).unwrap(),
            Frame(0)..Frame(0)
        );
        // Exhaust view slots and optional transfer credits before mandatory exit.
        let mut held = [None; 6];
        for slot in &mut held {
            match resources.snapshot(resource, Frame(1536)) {
                Ok(snapshot) => *slot = Some(snapshot),
                Err(Error::Capacity) => break,
                Err(error) => panic!("unexpected snapshot refusal: {error:?}"),
            }
        }
        resources.copy_remaining = 0;
        resources.steps_remaining = 0;
        assert_eq!(
            resources.finish_indexed_midi_writer(writer),
            Ok(Revision(2))
        );
        assert_eq!(
            resources.finish_indexed_midi_writer(second),
            Ok(Revision(2))
        );
        assert_eq!(
            resources.finish_indexed_midi_writer(growing),
            Ok(Revision(2))
        );
        assert_eq!(
            resources.finish_indexed_midi_writer(writer),
            Err(Error::InvalidId)
        );
        for snapshot in held.into_iter().flatten() {
            resources.release_snapshot(snapshot.id).unwrap();
        }
        let current = resources.snapshot(resource, Frame(1536)).unwrap();
        assert_eq!(current.description.frames, 128);
        assert_eq!(current.description.bytes, 45);
        let mut expected = [0; 45];
        encode_midi_record(7, &[0xb0, 7, 44], &mut expected[..15]).unwrap();
        encode_midi_record(71, &[0xb0, 7, 46], &mut expected[15..30]).unwrap();
        encode_midi_record(103, &[0xb0, 7, 47], &mut expected[30..]).unwrap();
        assert_eq!(bytes::<45>(&mut resources, current), expected);
        assert_eq!(bytes::<30>(&mut resources, old), encoded);
        assert_eq!(resources.currentness(old.id), Ok(Currentness::Stale));
        let continued = resources.snapshot(prefix, Frame(1536)).unwrap();
        assert_eq!(continued.description.frames, 1536);
        assert_eq!(continued.description.bytes, 36 * 15);
        for id in [old.id, current.id, continued.id] {
            resources.release_snapshot(id).unwrap();
        }
        for id in [resource, peer, prefix] {
            resources.remove(id).unwrap();
        }
        drain(&mut resources);
        resources.cleanup_indexed_midi_store(pool).unwrap();
        drain(&mut resources);
        assert_eq!(resources.usage(), baseline);
    });
}

#[shoop_test]
fn source_reset_and_competing_upload_fence_private_writers_and_recover_owners() {
    let mut resources = resources();
    let baseline = resources.usage();
    let pool = resources.prepare_indexed_midi_store(config()).unwrap();
    let source = resources.indexed_midi_source(pool, 0).unwrap();
    let mut input = MidiBuffer::new(8, 128).unwrap();
    input.push(0, Q, &[0x90, 60, 90]).unwrap();
    assert_no_heap(|| {
        resources
            .render_io()
            .append_indexed_midi_source(source, Frame(0), &input)
            .unwrap();
        let take = resources
            .grab_indexed_midi(source, Frame(0)..Frame(32), 64)
            .unwrap();
        let old = resources.snapshot(take, Frame(32)).unwrap();
        let writer = admit(&mut resources, take, WriteKind::Fixed, 32);
        input.clear();
        resources
            .render_io()
            .append_indexed_midi_source(source, Frame(32), &input)
            .unwrap();
        resources
            .render_io()
            .write_indexed_midi_tick(writer, source, Frame(32))
            .unwrap();
        let reset = resources
            .render_io()
            .reset_indexed_midi_source(source, Frame(64), &MidiState::default())
            .unwrap();
        assert_eq!(
            resources.indexed_midi_coverage(source),
            Err(Error::InvalidId)
        );
        assert_eq!(
            resources
                .render_io()
                .write_indexed_midi_tick(writer, source, Frame(64)),
            Err(Error::InvalidId)
        );
        resources
            .render_io()
            .append_indexed_midi_source(reset, Frame(64), &input)
            .unwrap();
        assert_eq!(
            resources
                .render_io()
                .write_indexed_midi_tick(writer, reset, Frame(64)),
            Err(Error::InvalidFrame)
        );
        assert_eq!(
            resources.finish_indexed_midi_writer(writer),
            Ok(Revision(2))
        );
        let writer = admit(&mut resources, take, WriteKind::Fixed, 0);
        let upload = resources
            .begin_upload(
                take,
                Revision(2),
                Description {
                    format: Format::Midi,
                    frames: 64,
                    bytes: 0,
                },
            )
            .unwrap();
        resources.finish_upload(upload).unwrap();
        resources.commit(upload).unwrap();
        assert_eq!(
            resources.progress_indexed_midi_writer(writer, 1),
            Err(Error::StaleRevision)
        );
        assert_eq!(
            resources.finish_indexed_midi_writer(writer),
            Err(Error::StaleRevision)
        );
        resources.cancel_indexed_midi_writer(writer).unwrap();
        assert_eq!(old.description.frames, 64);
        let mut expected = [0; 15];
        encode_midi_record(0, &[0x90, 60, 90], &mut expected).unwrap();
        assert_eq!(bytes::<15>(&mut resources, old), expected);
        resources.remove(take).unwrap();
        assert_eq!(resources.cleanup_indexed_midi_store(pool), Err(Error::Busy));
        resources.release_snapshot(old.id).unwrap();
        drain(&mut resources);
        resources.cleanup_indexed_midi_store(pool).unwrap();
        drain(&mut resources);
        assert_eq!(resources.usage(), baseline);
    });
}

#[shoop_test]
fn prepared_window_writer_resumes_after_a_source_gap_without_changing_old_exports() {
    let mut resources = resources();
    let baseline = resources.usage();
    let pool = resources.prepare_indexed_midi_store(config()).unwrap();
    let source = resources.indexed_midi_source(pool, 0).unwrap();
    let mut original = [0; 15];
    encode_midi_record(0, &[0x90, 60, 64], &mut original).unwrap();
    let loaded = upload(&mut resources, &original, Q as u64);
    resources.begin_callback();
    let snapshot = resources.snapshot(loaded, Frame(0)).unwrap();
    let take = import(&mut resources, pool, snapshot, &MidiState::default());
    resources.release_snapshot(snapshot.id).unwrap();
    resources.remove(loaded).unwrap();
    drain(&mut resources);
    let mut input = MidiBuffer::new(8, 128).unwrap();
    assert_no_heap(|| {
        let old = resources.snapshot(take, Frame(0)).unwrap();
        let writer = resources
            .prepare_indexed_midi_window_writer(take, resources.info(take).unwrap().revision)
            .unwrap();
        let ready = (0..100).any(|_| {
            resources.begin_callback();
            resources.progress_indexed_midi_writer(writer, 32).unwrap() == Status::Ready
        });
        assert!(ready);
        resources.start_indexed_midi_writer(writer, 0).unwrap();
        for tick in 0..3 {
            resources.begin_callback();
            input.clear();
            let packet = match tick {
                0 => [0x90, 69, 64],
                1 => [0x80, 69, 64],
                _ => [0x90, 70, 64],
            };
            input.push(0, Q, &packet).unwrap();
            let frame = Frame(tick * Q as u64);
            resources
                .render_io()
                .append_indexed_midi_source(source, frame, &input)
                .unwrap();
            if tick == 1 {
                continue;
            }
            if tick == 2 {
                resources.resume_indexed_midi_window_writer(writer).unwrap();
            }
            resources
                .render_io()
                .write_indexed_midi_window_tick(
                    writer,
                    source,
                    frame,
                    WriteSpan {
                        position: 0,
                        offset: 0,
                        frames: Q,
                    },
                )
                .unwrap();
        }
        resources.finish_indexed_midi_writer(writer).unwrap();
        let current = resources.snapshot(take, Frame(3 * Q as u64)).unwrap();
        let mut expected = [0; 15];
        encode_midi_record(0, &[0x90, 70, 64], &mut expected).unwrap();
        assert_eq!(bytes::<15>(&mut resources, current), expected);
        assert_eq!(bytes::<15>(&mut resources, old), original);
        resources.release_snapshot(current.id).unwrap();
        resources.release_snapshot(old.id).unwrap();
        resources.remove(take).unwrap();
        drain(&mut resources);
        resources.cleanup_indexed_midi_store(pool).unwrap();
        drain(&mut resources);
        assert_eq!(resources.usage(), baseline);
    });
}

#[shoop_test]
fn bounded_imports_and_exports_refuse_atomically_and_return_every_credit() {
    let mut resources = resources();
    let baseline = resources.usage();
    let mut limits = config();
    limits.views = 2;
    limits.imports = 1;
    let capacity = limits
        .capacity(resources.config(), resources.quantum())
        .unwrap();
    assert!(capacity.metadata_bytes < 8 * 1024 * 1024);
    assert_eq!(
        capacity.reserved_payload_bytes,
        limits.blocks.record_pages * 32 * 16
    );
    let pool = resources.prepare_indexed_midi_store(limits).unwrap();
    let mut encoded = [0; 15];
    encode_midi_record(0, &[0xb0, 7, 90], &mut encoded).unwrap();
    let loaded = upload(&mut resources, &encoded, 64);
    resources.begin_callback();
    let source = resources.snapshot(loaded, Frame(0)).unwrap();
    assert_no_heap(|| {
        let pending = resources
            .prepare_indexed_midi_import(pool, source.id, &MidiState::default())
            .unwrap();
        let pinned = resources.usage();
        assert_eq!(
            resources.prepare_indexed_midi_import(pool, source.id, &MidiState::default()),
            Err(Error::Capacity)
        );
        assert_eq!(resources.usage(), pinned);
        assert_eq!(
            resources.progress_indexed_midi_import(pending, 1),
            Ok(Status::Building)
        );
        resources.cancel_indexed_midi_import(pending).unwrap();
        assert_eq!(
            resources.progress_indexed_midi_import(pending, 1),
            Err(Error::InvalidId)
        );
        drain(&mut resources);
        let take = import(&mut resources, pool, source, &MidiState::default());
        let snapshot = resources.snapshot(take, Frame(0)).unwrap();
        let index = resources.read_index(snapshot.id).unwrap();
        assert_eq!(
            resources
                .render_io()
                .midi_parameter_history_untracked(index, 0),
            Ok(false)
        );
        assert_eq!(
            resources
                .render_io()
                .midi_parameter_history_untracked(index, 1),
            Ok(false)
        );

        let pinned = resources.usage();
        assert_eq!(resources.snapshot(take, Frame(0)), Err(Error::Capacity));
        assert_eq!(resources.retain_snapshot(snapshot.id), Err(Error::Capacity));
        assert_eq!(
            resources.fork_indexed_midi(pool, snapshot.id),
            Err(Error::Capacity)
        );
        assert_eq!(resources.usage(), pinned);
        // Background reads obey the transfer budget and cannot advance a refused cursor.
        let mut cursor = resources.cursor(snapshot.id).unwrap();
        let mut output = [0; 15];
        resources.copy_remaining = 0;
        assert_eq!(resources.read(&mut cursor, &mut output), Err(Error::Budget));
        assert_eq!(cursor.position(), 0);
        resources.begin_callback();
        assert_eq!(resources.read(&mut cursor, &mut output), Ok(15));
        assert_eq!(resources.work().copied_bytes, 15);
        assert_eq!(output, encoded);
        let retained = resources.retain_snapshot(source.id).unwrap();
        resources.release_snapshot(source.id).unwrap();
        resources.remove(loaded).unwrap();
        resources.release_snapshot(retained.id).unwrap();
        resources.release_snapshot(snapshot.id).unwrap();
        resources.remove(take).unwrap();
        drain(&mut resources);
        resources.cleanup_indexed_midi_store(pool).unwrap();
        drain(&mut resources);
        assert_eq!(resources.usage(), baseline);
    });
    let mut invalid = limits;
    invalid.sources = invalid.blocks.media + 1;
    assert_eq!(
        invalid.capacity(resources.config(), resources.quantum()),
        Err(Error::InvalidConfig)
    );
    let mut huge = limits;
    huge.writers = usize::MAX;
    assert_eq!(
        huge.capacity(resources.config(), resources.quantum()),
        Err(Error::Capacity)
    );
    assert!(matches!(
        resources.prepare_indexed_midi_store(huge),
        Err(Error::Capacity)
    ));
    assert_eq!(resources.usage(), baseline);
}

#[shoop_test]
fn byte_pressure_evicts_whole_quanta_and_keeps_state_and_old_exports() {
    let mut resources = resources();
    let baseline = resources.usage();
    let mut limits = config();
    limits.history_frames = 4096;
    limits.history_events = 64;
    limits.history_bytes = 512;
    let pool = resources.prepare_indexed_midi_store(limits).unwrap();
    let source = resources.indexed_midi_source(pool, 0).unwrap();
    let mut input = MidiBuffer::new(8, 128).unwrap();
    let mut rows = Vec::<(u64, [u8; 3], usize)>::with_capacity(128);
    let mut encoded = Vec::with_capacity(512);
    let mut old_bytes = Vec::with_capacity(512);
    let mut old = None;
    let mut origin = 0;
    let mut initial = MidiState::default();
    assert_no_heap(|| {
        for tick in 0..64u64 {
            resources.begin_callback();
            resources.copy_remaining = 0;
            resources.steps_remaining = 0;
            input.clear();
            if tick == 0 {
                for packet in [
                    [0xb0, 7, 33],
                    [0x90, 60, 90],
                    [0xb0, 66, 127],
                    [0xb0, 64, 127],
                    [0x80, 60, 0],
                ] {
                    input.push(0, Q, &packet).unwrap();
                }
            }
            if tick < 40 {
                input.push(0, Q, &[0xc0, tick as u8]).unwrap();
            } else {
                for key in 0..8 {
                    input.push(key, Q, &[0x91, key as u8, tick as u8]).unwrap();
                }
            }
            for index in 0..input.len() {
                let (offset, bytes) = input.get(index).unwrap();
                let mut packet = [0; 3];
                packet[..bytes.len()].copy_from_slice(bytes);
                rows.push((tick * Q as u64 + offset as u64, packet, bytes.len()));
            }
            let end = (tick + 1) * Q as u64;
            origin = origin.max(end.saturating_sub(limits.history_frames));
            while rows.first().is_some_and(|row| row.0 < origin)
                || rows.len() > limits.history_events
                || rows.iter().map(|row| 12 + row.2).sum::<usize>() > limits.history_bytes
            {
                let (frame, packet, len) = rows.remove(0);
                origin = origin.max((frame + 1).div_ceil(Q as u64) * Q as u64);
                initial.apply_packet(&packet[..len], 4).unwrap();
            }
            resources
                .render_io()
                .append_indexed_midi_source(source, Frame(tick * Q as u64), &input)
                .unwrap();
            assert_eq!(
                resources.indexed_midi_coverage(source),
                Ok(Frame(origin)..Frame(end))
            );
            if tick >= 39 && tick <= 42 || tick == 63 {
                let take = resources
                    .grab_indexed_midi(source, Frame(origin)..Frame(end), end - origin)
                    .unwrap();
                let snapshot = resources.snapshot(take, Frame(end)).unwrap();
                resources.remove(take).unwrap();
                encoded.clear();
                for &(frame, packet, len) in &rows {
                    let mut bytes = [0; 16];
                    let n = encode_midi_record(frame - origin, &packet[..len], &mut bytes).unwrap();
                    encoded.extend_from_slice(&bytes[..n]);
                }
                let index = resources.read_index(snapshot.id).unwrap();
                assert_eq!(
                    resources.render_io().midi_seek(index, 0).unwrap().state,
                    initial.basic_projection()
                );
                assert_eq!(snapshot.description.bytes, encoded.len());
                let mut cursor = resources.cursor(snapshot.id).unwrap();
                let mut actual = [0; 512];
                let mut count = 0;
                while count < encoded.len() {
                    count += resources
                        .render_io()
                        .read(&mut cursor, &mut actual[count..encoded.len()])
                        .unwrap();
                }
                assert_eq!(&actual[..count], &encoded);
                if old.is_none() {
                    old = Some(snapshot);
                    old_bytes.extend_from_slice(&encoded);
                } else {
                    resources.release_snapshot(snapshot.id).unwrap();
                }
            }
            if let Some(snapshot) = old {
                let mut cursor = resources.cursor(snapshot.id).unwrap();
                let mut actual = [0; 512];
                let mut count = 0;
                while count < old_bytes.len() {
                    count += resources
                        .render_io()
                        .read(&mut cursor, &mut actual[count..old_bytes.len()])
                        .unwrap();
                }
                assert_eq!(&actual[..count], &old_bytes);
            }
        }
        resources.release_snapshot(old.unwrap().id).unwrap();
        drain(&mut resources);
        resources.cleanup_indexed_midi_store(pool).unwrap();
        drain(&mut resources);
        assert_eq!(resources.usage(), baseline);
    });
}

#[shoop_test]
fn silent_source_ticks_avoid_projection_copies_and_writer_zero_uses_the_pre_input_state() {
    let mut resources = resources();
    let baseline = resources.usage();
    let pool = resources.prepare_indexed_midi_store(config()).unwrap();
    let source = resources.indexed_midi_source(pool, 0).unwrap();
    let mut input = MidiBuffer::new(8, 128).unwrap();
    assert_no_heap(|| {
        input.push(0, Q, &[0xb0, 7, 55]).unwrap();
        resources.begin_callback();
        resources
            .render_io()
            .append_indexed_midi_source(source, Frame(0), &input)
            .unwrap();
        input.clear();
        resources.begin_callback();
        resources
            .render_io()
            .append_indexed_midi_source(source, Frame(32), &input)
            .unwrap();
        assert_eq!(resources.render_work().copied_bytes, size_of::<MidiState>());
        for frame in [64, 96] {
            resources.begin_callback();
            resources.copy_remaining = 0;
            resources.steps_remaining = 0;
            resources
                .render_io()
                .append_indexed_midi_source(source, Frame(frame), &input)
                .unwrap();
            assert_eq!(resources.render_work().copied_bytes, 0);
        }
        let take = resources
            .grab_indexed_midi(source, Frame(64)..Frame(96), 32)
            .unwrap();
        let writer = admit(&mut resources, take, WriteKind::Fixed, 0);
        input.push(0, Q, &[0xb0, 7, 100]).unwrap();
        resources
            .render_io()
            .append_indexed_midi_source(source, Frame(128), &input)
            .unwrap();
        resources
            .render_io()
            .write_indexed_midi_tick(writer, source, Frame(128))
            .unwrap();
        resources.finish_indexed_midi_writer(writer).unwrap();
        let snapshot = resources.snapshot(take, Frame(160)).unwrap();
        let index = resources.read_index(snapshot.id).unwrap();
        let before = resources.render_io().midi_seek(index, 0).unwrap().state;
        let after = resources.render_io().midi_seek(index, 1).unwrap().state;
        assert_eq!(before.channel(0).unwrap().controller(7), Some(55));
        assert_eq!(after.channel(0).unwrap().controller(7), Some(100));
        resources.release_snapshot(snapshot.id).unwrap();
        resources.remove(take).unwrap();
        drain(&mut resources);
        resources.cleanup_indexed_midi_store(pool).unwrap();
        drain(&mut resources);
        assert_eq!(resources.usage(), baseline);
    });
}

#[shoop_test]
fn required_cleanup_recovers_hidden_packets_while_capture_runs_without_optional_credits() {
    let mut resources = resources();
    let baseline = resources.usage();
    let mut limits = config();
    limits.history_frames = 512;
    limits.history_events = 128;
    limits.maintenance_steps_per_tick = 8;
    let pool = resources.prepare_indexed_midi_store(limits).unwrap();
    let source = resources.indexed_midi_source(pool, 0).unwrap();
    let mut input = MidiBuffer::new(8, 128).unwrap();
    assert_no_heap(|| {
        for tick in 0..8 {
            input.clear();
            for event in 0..8 {
                input
                    .push(event as u32, Q, &[0xb0, (event + 10) as u8, 1])
                    .unwrap();
            }
            resources
                .render_io()
                .append_indexed_midi_source(source, Frame(tick as u64 * Q as u64), &input)
                .unwrap();
        }
        let take = resources
            .grab_indexed_midi(source, Frame(0)..Frame(1), 32)
            .unwrap();
        let old = resources.snapshot(take, Frame(256)).unwrap();
        let peer = resources.retain_snapshot(old.id).unwrap();
        let source = resources
            .render_io()
            .reset_indexed_midi_source(source, Frame(256), &MidiState::default())
            .unwrap();
        input.clear();
        resources.copy_remaining = 0;
        resources.steps_remaining = 0;
        for tick in 8..72 {
            resources
                .render_io()
                .append_indexed_midi_source(source, Frame(tick * Q as u64), &input)
                .unwrap();
        }
        let usage = resources.indexed_midi_usage(pool).unwrap();
        assert!(usage.available_records >= limits.blocks.record_pages - 2);
        assert_eq!(usage.available_bodies, usage.available_records);
        assert_eq!(usage.views, 3);
        let mut expected = [0; 15];
        encode_midi_record(0, &[0xb0, 10, 1], &mut expected).unwrap();
        assert_eq!(bytes::<15>(&mut resources, old), expected);
        assert_eq!(bytes::<15>(&mut resources, peer), expected);
        assert_eq!(resources.copy_remaining(), 0);
        assert_eq!(resources.steps_remaining(), 0);
        resources.release_snapshot(old.id).unwrap();
        resources.release_snapshot(peer.id).unwrap();
        resources.remove(take).unwrap();
        drain(&mut resources);
        resources.cleanup_indexed_midi_store(pool).unwrap();
        drain(&mut resources);
        assert_eq!(resources.usage(), baseline);
    });
}

#[shoop_test]
fn mapped_writer_keeps_nonquantized_raw_tails_zero_projection_and_the_complete_cut_on_density_refusal(
) {
    let mut resources = resources();
    let baseline = resources.usage();
    let pool = resources.prepare_indexed_midi_store(config()).unwrap();
    let source = resources.indexed_midi_source(pool, 0).unwrap();
    let mut encoded = [0; 45];
    for (index, (frame, packet)) in [
        (1, [0xb0, 2, 11]),
        (32, [0x90, 61, 80]),
        (38, [0xb0, 2, 22]),
    ]
    .into_iter()
    .enumerate()
    {
        encode_midi_record(frame, &packet, &mut encoded[index * 15..(index + 1) * 15]).unwrap();
    }
    let loaded = upload(&mut resources, &encoded, 39);
    let old_file = resources.snapshot(loaded, Frame(0)).unwrap();
    let mut initial = MidiState::default();
    initial.apply_packet(&[0xb0, 1, 13], 64).unwrap();
    let take = import(&mut resources, pool, old_file, &initial);
    let old_view = resources.snapshot(take, Frame(0)).unwrap();
    let writer = resources
        .prepare_indexed_midi_window_writer(take, Revision(1))
        .unwrap();
    while resources.progress_indexed_midi_writer(writer, 1).unwrap() != Status::Ready {}
    resources.start_indexed_midi_writer(writer, 0).unwrap();
    let publication = resources
        .reserve_writer_snapshot(take, Frame(0), 39)
        .unwrap();
    let mut input = MidiBuffer::new(8, 128).unwrap();
    let mut expected = [0; 90];
    let mut at = 0;
    for (frame, packet) in [
        (0, &[0x90, 60, 90][..]),
        (1, &[0xb0, 1, 41][..]),
        (26, &[0x80, 60, 0][..]),
        (32, &[0x90, 61, 80][..]),
        (35, &[0xb0, 2, 88][..]),
        (38, &[0xb0, 2, 99][..]),
    ] {
        let end = at + 12 + packet.len();
        encode_midi_record(frame, packet, &mut expected[at..end]).unwrap();
        at = end;
    }
    let mut final_expected = [0; 195];
    let mut at = 0;
    for (frame, packet) in core::iter::once((0, [0x90, 60, 90]))
        .chain((0..8).map(|value| (1, [0xb0, 9, value])))
        .chain([
            (26, [0x80, 60, 0]),
            (32, [0x90, 61, 80]),
            (35, [0xb0, 2, 88]),
            (38, [0xb0, 2, 99]),
        ])
    {
        encode_midi_record(frame, &packet, &mut final_expected[at..at + 15]).unwrap();
        at += 15;
    }
    assert_no_heap(|| {
        resources.copy_remaining = 0;
        resources.steps_remaining = 0;
        for (offset, packet) in [
            (1, &[0xb0, 7, 44][..]),
            (5, &[0x90, 60, 90][..]),
            (6, &[0xb0, 1, 41][..]),
            (31, &[0x80, 60, 0][..]),
        ] {
            input.push(offset, Q, packet).unwrap();
        }
        resources
            .render_io()
            .append_indexed_midi_source(source, Frame(0), &input)
            .unwrap();
        resources
            .render_io()
            .write_indexed_midi_window_tick(
                writer,
                source,
                Frame(0),
                WriteSpan {
                    position: 0,
                    offset: 5,
                    frames: 27,
                },
            )
            .unwrap();
        input.clear();
        for (offset, value) in [(0, 77), (1, 88), (4, 99), (5, 100)] {
            input.push(offset, Q, &[0xb0, 2, value]).unwrap();
        }
        resources
            .render_io()
            .append_indexed_midi_source(source, Frame(32), &input)
            .unwrap();
        resources
            .render_io()
            .write_indexed_midi_window_tick(
                writer,
                source,
                Frame(32),
                WriteSpan {
                    position: 35,
                    offset: 1,
                    frames: 4,
                },
            )
            .unwrap();
        let resource = resources.snapshot_indexed_midi_writer(writer).unwrap();
        let intermediate = resources.snapshot(resource, Frame(64)).unwrap();
        resources.remove(resource).unwrap();
        assert_eq!(bytes::<90>(&mut resources, intermediate), expected);
        for tick in 2..4 {
            input.clear();
            for value in 0..8 {
                input
                    .push(0, Q, &[0xb0, 9, value + 8 * (tick - 2) as u8])
                    .unwrap();
            }
            let frame = Frame(tick * Q as u64);
            resources
                .render_io()
                .append_indexed_midi_source(source, frame, &input)
                .unwrap();
            while !resources
                .prepare_indexed_midi_writer_position(writer, tick - 1, 256)
                .unwrap()
            {}
            let result = resources.render_io().write_indexed_midi_window_tick(
                writer,
                source,
                frame,
                WriteSpan {
                    position: tick - 1,
                    offset: 0,
                    frames: 1,
                },
            );
            assert_eq!(
                result,
                if tick == 2 {
                    Ok(())
                } else {
                    Err(Error::Capacity)
                }
            );
        }
        assert_eq!(
            (resources.copy_remaining(), resources.steps_remaining()),
            (0, 0)
        );
        resources.finish_indexed_midi_writer(writer).unwrap();
        let complete = resources
            .publish_reserved_snapshot(publication, Frame(96))
            .unwrap();
        assert_eq!(complete.description.frames, 39);
        assert_eq!(bytes::<195>(&mut resources, complete), final_expected);
        assert_eq!(bytes::<90>(&mut resources, intermediate), expected);
        assert_eq!(bytes::<45>(&mut resources, old_file), encoded);
        let index = resources.read_index(complete.id).unwrap();
        let zero = resources.render_io().midi_seek(index, 0).unwrap().state;
        assert_eq!(zero.channel(0).unwrap().controller(1), None);
        assert_eq!(zero.channel(0).unwrap().controller(7), Some(44));
        while !resources
            .io()
            .prepare_midi_position(index, 39, 256)
            .unwrap()
        {}
        let end = resources.render_io().midi_seek(index, 39).unwrap().state;
        assert_eq!(end.channel(0).unwrap().controller(2), Some(99));
        assert_eq!(end.channel(0).unwrap().controller(9), Some(7));
        let index = resources.read_index(old_view.id).unwrap();
        assert_eq!(
            resources.render_io().midi_seek(index, 0).unwrap().state,
            initial
        );
        for snapshot in [complete, intermediate, old_file, old_view] {
            resources.release_snapshot(snapshot.id).unwrap();
        }
        for resource in [take, loaded] {
            resources.remove(resource).unwrap();
        }
        drain(&mut resources);
        resources.cleanup_indexed_midi_store(pool).unwrap();
        drain(&mut resources);
        assert_eq!(resources.usage(), baseline);
    });
}

#[shoop_test]
fn recording_prefix_import_captures_live_tail_and_cannot_publish_holes() {
    let mut resources = resources();
    let baseline = resources.usage();
    let pool = resources.prepare_indexed_midi_store(config()).unwrap();
    let source = resources.indexed_midi_source(pool, 0).unwrap();
    let mut input = MidiBuffer::new(8, 128).unwrap();
    let mut encoded = [0; 360];
    for tick in 0..12 {
        encode_midi_record(
            tick * Q as u64 + 3,
            &[0xb0, 7, tick as u8],
            &mut encoded[tick as usize * 30..tick as usize * 30 + 15],
        )
        .unwrap();
        encode_midi_record(
            tick * Q as u64 + 5,
            &[0x90, 60, 80],
            &mut encoded[tick as usize * 30 + 15..tick as usize * 30 + 30],
        )
        .unwrap();
    }
    let loaded = upload(&mut resources, &encoded, 12 * Q as u64);
    resources.begin_callback();
    let snapshot = resources.snapshot(loaded, Frame(0)).unwrap();
    assert_no_heap(|| {
        let (take, writer, import) = resources
            .prepare_indexed_midi_recording(pool, snapshot.id, &MidiState::default())
            .unwrap();
        let mut expected = [0; 840];
        expected[..360].copy_from_slice(&encoded);
        for tick in 0..32 {
            resources.begin_callback();
            input.clear();
            input.push(7, Q, &[0xb0, 1, tick as u8]).unwrap();
            let frame = Frame(tick * Q as u64);
            resources
                .render_io()
                .append_indexed_midi_source(source, frame, &input)
                .unwrap();
            resources
                .render_io()
                .write_indexed_midi_tick(writer, source, frame)
                .unwrap();
            encode_midi_record(
                (12 + tick) * Q as u64 + 7,
                &[0xb0, 1, tick as u8],
                &mut expected[360 + tick as usize * 15..375 + tick as usize * 15],
            )
            .unwrap();
            if tick < 24 {
                assert_eq!(
                    resources.snapshot_indexed_midi_writer(writer),
                    Err(Error::Busy)
                );
                assert_eq!(
                    resources.finish_indexed_midi_writer(writer),
                    Err(Error::Busy)
                );
                assert_eq!(
                    resources.fix_indexed_midi_writer_window(writer),
                    Err(Error::Busy)
                );
            }
            let status = resources.progress_indexed_midi_import(import, 1).unwrap();
            assert_eq!(
                status,
                if tick < 24 {
                    Status::Building
                } else {
                    Status::Ready
                }
            );
            resources.reclaim();
        }
        resources
            .finish_indexed_midi_recording_import(import)
            .unwrap();
        let retained = resources.snapshot_indexed_midi_writer(writer).unwrap();
        let complete = resources.snapshot(retained, Frame(1024)).unwrap();
        resources.remove(retained).unwrap();
        assert_eq!(complete.description.frames, 44 * Q as u64);
        assert_eq!(bytes::<840>(&mut resources, complete), expected);
        resources.fix_indexed_midi_writer_window(writer).unwrap();
        input.clear();
        resources
            .render_io()
            .append_indexed_midi_source(source, Frame(1024), &input)
            .unwrap();
        resources
            .render_io()
            .write_indexed_midi_window_tick(
                writer,
                source,
                Frame(1024),
                WriteSpan {
                    position: 0,
                    offset: 0,
                    frames: Q,
                },
            )
            .unwrap();
        resources.finish_indexed_midi_writer(writer).unwrap();
        let final_view = resources.snapshot(take, Frame(1056)).unwrap();
        let mut final_expected = [0; 810];
        final_expected.copy_from_slice(&expected[30..]);
        assert_eq!(bytes::<810>(&mut resources, final_view), final_expected);
        assert_eq!(bytes::<840>(&mut resources, complete), expected);
        let (cancelled, writer, import) = resources
            .prepare_indexed_midi_recording(pool, snapshot.id, &MidiState::default())
            .unwrap();
        resources.progress_indexed_midi_import(import, 1).unwrap();
        resources.cancel_indexed_midi_writer(writer).unwrap();
        assert_eq!(
            resources.progress_indexed_midi_import(import, 1),
            Err(Error::InvalidId)
        );
        for snapshot in [snapshot, complete, final_view] {
            resources.release_snapshot(snapshot.id).unwrap();
        }
        for resource in [take, loaded, cancelled] {
            resources.remove(resource).unwrap();
        }
        drain(&mut resources);
        resources.cleanup_indexed_midi_store(pool).unwrap();
        drain(&mut resources);
        assert_eq!(resources.usage(), baseline);
    });
}

#[shoop_test]
fn retained_cuts_cannot_spend_live_capture_credits_without_optional_maintenance() {
    for (fragmented, finish_writers) in [(false, false), (true, false), (false, true)] {
        let mut resources = resources();
        let baseline = resources.usage();
        let mut limits = config();
        limits.blocks.record_pages = if fragmented { 100 } else { 112 };
        limits.blocks.event_credits = limits.blocks.record_pages;
        limits.history_bytes = 512;
        if fragmented {
            limits.history_frames = 1024;
        }
        let pool = resources.prepare_indexed_midi_store(limits).unwrap();
        let sources = [
            resources.indexed_midi_source(pool, 0).unwrap(),
            resources.indexed_midi_source(pool, 1).unwrap(),
        ];
        let mut input = MidiBuffer::new(8, 128).unwrap();
        let mut denser_input = MidiBuffer::new(8, 128).unwrap();
        for key in 0..8 {
            denser_input.push(key, Q, &[0x91, key as u8, 7]).unwrap();
        }
        let mut held = [None; 7];
        let mut admitted = 0;
        let mut refused = false;
        let mut old = None;
        let mut expected = [0; 120];
        let mut expected_bytes = 0;
        assert_no_heap(|| {
            for tick in 0..200u64 {
                resources.begin_callback();
                resources.copy_remaining = 0;
                resources.steps_remaining = 0;
                input.clear();
                for event in 0..if fragmented || finish_writers { 1 } else { 8 } {
                    input
                        .push(event, Q, &[0x90, event as u8, (tick % 127 + 1) as u8])
                        .unwrap();
                }
                for (index, source) in sources.into_iter().enumerate() {
                    let begin = Frame(tick * Q as u64);
                    let packets = if fragmented && index == 1 {
                        &denser_input
                    } else {
                        &input
                    };
                    resources
                        .render_io()
                        .append_indexed_midi_source(source, begin, packets)
                        .unwrap_or_else(|error| {
                            panic!("live source at tick {tick}, fragmented={fragmented}: {error:?}")
                        });
                    assert_eq!(
                        resources.indexed_midi_coverage(source).unwrap().end,
                        Frame((tick + 1) * Q as u64)
                    );
                }
                if tick > 3 && tick % 4 == 0 && admitted < held.len() {
                    match resources.grab_indexed_midi(
                        sources[0],
                        Frame(tick * Q as u64)..Frame((tick + 1) * Q as u64),
                        Q as u64,
                    ) {
                        Ok(resource) => {
                            if finish_writers {
                                let writer = admit(&mut resources, resource, WriteKind::Fixed, 0);
                                resources
                                    .render_io()
                                    .write_indexed_midi_tick(
                                        writer,
                                        sources[0],
                                        Frame(tick * Q as u64),
                                    )
                                    .unwrap();
                                resources.finish_indexed_midi_writer(writer).unwrap();
                            }
                            held[admitted] = Some(resource);
                            admitted += 1;
                            if old.is_none() {
                                old = Some(
                                    resources
                                        .snapshot(resource, Frame((tick + 1) * Q as u64))
                                        .unwrap(),
                                );
                                expected_bytes = input.len() * 15;
                                for i in 0..input.len() {
                                    let (frame, packet) = input.get(i).unwrap();
                                    encode_midi_record(
                                        u64::from(frame),
                                        packet,
                                        &mut expected[i * 15..(i + 1) * 15],
                                    )
                                    .unwrap();
                                }
                            }
                        }
                        Err(Error::Capacity) => refused = true,
                        Err(error) => panic!("unexpected admission: {error:?}"),
                    }
                }
            }
            assert!(admitted > 0);
            if finish_writers {
                assert_eq!(
                    admitted,
                    held.len(),
                    "immutable cuts must return future writer workspace"
                );
            } else {
                assert!(refused, "owner admission must reserve live source storage");
            }
            if fragmented {
                assert!(
                    resources.indexed_midi_coverage(sources[0]).unwrap().start.0
                        >= (200 * Q - 1024) as u64
                );
            }
            let old = old.unwrap();
            if fragmented || finish_writers {
                assert_eq!(bytes::<15>(&mut resources, old), expected[..expected_bytes]);
            } else {
                assert_eq!(bytes::<120>(&mut resources, old), expected);
            }
            resources.release_snapshot(old.id).unwrap();
            for resource in held.into_iter().flatten() {
                resources.remove(resource).unwrap();
            }
            drain(&mut resources);
            resources.cleanup_indexed_midi_store(pool).unwrap();
            drain(&mut resources);
            assert_eq!(resources.usage(), baseline);
        });
    }
}
