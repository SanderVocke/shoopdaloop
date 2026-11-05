use super::*;
use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};

const Q: usize = 32;
fn config() -> Config {
    Config {
        packets: PacketLimits {
            pages: 128,
            bodies: 512,
            records: 2048,
            max_packet: 64,
            bases: 4,
        },
        indexes: IndexLimits {
            leases: 32,
            versions: 16,
            category_pages: 128,
            packet_nodes: 4096,
            scalar_nodes: 4096,
            hold_nodes: 1024,
            sostenuto_nodes: 1024,
        },
        sources: 2,
        history_frames: 128,
        history_events: 32,
        history_bytes: 4096,
        views: 8,
        writers: 4,
        imports: 2,
        events_per_tick: 8,
        bytes_per_tick: 128,
        maintenance_steps_per_tick: 32,
    }
}
fn resources() -> ResourceStore {
    ResourceStore::new(
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
    resources.start_indexed_midi_writer(id, position).unwrap();
    id
}
fn upload(resources: &mut ResourceStore, data: &[u8], frames: u64) -> ResourceId {
    let resource = resources.create().unwrap();
    let id = resources
        .begin_upload(
            resource,
            Revision(0),
            Description {
                format: Format::Midi,
                frames,
                bytes: data.len(),
            },
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
    let mut original = [0; 35];
    let mut sysex = [37; 20];
    sysex[0] = 0xf0;
    sysex[19] = 0xf7;
    encode_midi_record(96, &sysex, &mut original[..32]).unwrap();
    // One complete note-on at the same frame follows the full SysEx body.
    let mut encoded = [0; 47];
    encoded[..32].copy_from_slice(&original[..32]);
    encode_midi_record(96, &[0x90, 60, 90], &mut encoded[32..]).unwrap();
    let loaded = upload(&mut resources, &encoded, 128);
    resources.begin_callback();
    let snapshot = resources.snapshot(loaded, Frame(0)).unwrap();
    let resource = import(&mut resources, pool, snapshot, &initial);
    resources.release_snapshot(snapshot.id).unwrap();
    resources.remove(loaded).unwrap();
    drain(&mut resources);
    assert_no_heap(|| {
        let old = resources.snapshot(resource, Frame(0)).unwrap();
        assert_eq!(bytes::<47>(&mut resources, old), encoded);
        let old_index = resources.read_index(old.id).unwrap();
        let position = resources.render_io().midi_seek(old_index, 97).unwrap();
        let mut expected = initial;
        expected.apply_packet(&[0x90, 60, 90], 64).unwrap();
        assert_eq!(position.state, expected);
        assert_eq!(position.cursor.byte_position(), 47);
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
            assert_eq!(bytes::<47>(&mut resources, old), encoded);
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
        assert_eq!(bytes::<47>(&mut resources, old), encoded);
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
            Err(Error::InvalidId)
        );
        resources.cancel_indexed_midi_writer(writer).unwrap();
        let writer = admit(&mut resources, take, WriteKind::Fixed, 0);
        let upload = resources
            .begin_upload(
                take,
                Revision(1),
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
    assert_eq!(capacity.reserved_payload_bytes, 128 * 64);
    let pool = resources.prepare_indexed_midi_store(limits).unwrap();
    let mut encoded = [0; 15];
    encode_midi_record(0, &[0xb0, 96, 90], &mut encoded).unwrap();
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
            Ok(true)
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
        assert_eq!(resources.read(&mut cursor, &mut output), Ok(12));
        assert_eq!(resources.work().copied_bytes, 12);
        assert_eq!(resources.read(&mut cursor, &mut output[12..]), Ok(3));
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
    invalid.sources = invalid.indexes.versions + 1;
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
fn byte_pressure_evicts_a_large_prefix_at_one_complete_cut_and_preserves_old_exports() {
    let mut resources = resources();
    let baseline = resources.usage();
    let mut limits = config();
    limits.history_frames = 4096;
    limits.history_events = 64;
    limits.history_bytes = 512;
    let pool = resources.prepare_indexed_midi_store(limits).unwrap();
    let source = resources.indexed_midi_source(pool, 0).unwrap();
    let mut input = MidiBuffer::new(8, 128).unwrap();
    let mut before = [0; 507];
    for tick in 1..40 {
        encode_midi_record(
            (tick - 1) as u64 * Q as u64,
            &[0xf8],
            &mut before[(tick - 1) * 13..tick * 13],
        )
        .unwrap();
    }
    let mut expected = [0; 510];
    for tick in 18..40 {
        encode_midi_record(
            (tick - 18) as u64 * Q as u64,
            &[0xf8],
            &mut expected[(tick - 18) * 13..(tick - 17) * 13],
        )
        .unwrap();
    }
    let mut packet = [1; 16];
    packet[0] = 0xf0;
    packet[15] = 0xf7;
    for event in 0..8 {
        encode_midi_record(
            (40 - 18) * Q as u64 + event as u64,
            &packet,
            &mut expected[286 + event * 28..286 + (event + 1) * 28],
        )
        .unwrap();
    }
    assert_no_heap(|| {
        for tick in 0..40 {
            resources.begin_callback();
            input.clear();
            input.push(0, Q, &[0xf8]).unwrap();
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
            resources
                .render_io()
                .append_indexed_midi_source(source, Frame(tick * Q as u64), &input)
                .unwrap();
            resources.reclaim();
        }
        assert_eq!(
            resources.indexed_midi_coverage(source),
            Ok(Frame(32)..Frame(1280))
        );
        let take = resources
            .grab_indexed_midi(source, Frame(32)..Frame(1280), 1248)
            .unwrap();
        let old = resources.snapshot(take, Frame(1280)).unwrap();
        assert_eq!(bytes::<507>(&mut resources, old), before);
        resources.begin_callback();
        resources.copy_remaining = 0;
        resources.steps_remaining = 0;
        input.clear();
        for event in 0..8 {
            input.push(event, Q, &packet).unwrap();
        }
        resources
            .render_io()
            .append_indexed_midi_source(source, Frame(1280), &input)
            .unwrap();
        assert_eq!(
            resources.indexed_midi_coverage(source),
            Ok(Frame(576)..Frame(1312))
        );
        let current = resources
            .grab_indexed_midi(source, Frame(576)..Frame(1312), 736)
            .unwrap();
        let snapshot = resources.snapshot(current, Frame(1312)).unwrap();
        assert_eq!(bytes::<510>(&mut resources, snapshot), expected);
        assert_eq!(bytes::<507>(&mut resources, old), before);
        let index = resources.read_index(snapshot.id).unwrap();
        let state = resources.render_io().midi_seek(index, 0).unwrap().state;
        let mut projected = MidiState::default();
        for packet in [
            [0xb0, 7, 33],
            [0x90, 60, 90],
            [0xb0, 66, 127],
            [0xb0, 64, 127],
            [0x80, 60, 0],
        ] {
            projected.apply_packet(&packet, 64).unwrap();
        }
        assert_eq!(state, projected);
        input.clear();
        for event in 0..8 {
            input.push(event, Q, &packet).unwrap();
        }
        resources
            .render_io()
            .append_indexed_midi_source(source, Frame(1312), &input)
            .unwrap();
        assert_eq!(
            resources.indexed_midi_coverage(source),
            Ok(Frame(1152)..Frame(1344))
        );
        let second = resources
            .grab_indexed_midi(source, Frame(1152)..Frame(1344), 192)
            .unwrap();
        let second_snapshot = resources.snapshot(second, Frame(1344)).unwrap();
        let mut second_bytes = [0; 500];
        for tick in 36..40 {
            encode_midi_record(
                (tick - 36) * Q as u64,
                &[0xf8],
                &mut second_bytes[(tick as usize - 36) * 13..(tick as usize - 35) * 13],
            )
            .unwrap();
        }
        for tick in 40..42 {
            for event in 0..8 {
                let offset = 52 + (tick - 40) * 8 * 28 + event * 28;
                encode_midi_record(
                    (tick - 36) as u64 * Q as u64 + event as u64,
                    &packet,
                    &mut second_bytes[offset..offset + 28],
                )
                .unwrap();
            }
        }
        assert_eq!(bytes::<500>(&mut resources, second_snapshot), second_bytes);
        assert_eq!(bytes::<510>(&mut resources, snapshot), expected);
        assert_eq!(bytes::<507>(&mut resources, old), before);
        resources
            .render_io()
            .append_indexed_midi_source(source, Frame(1344), &input)
            .unwrap();
        assert_eq!(
            resources.indexed_midi_coverage(source),
            Ok(Frame(1312)..Frame(1376))
        );
        resources.release_snapshot(second_snapshot.id).unwrap();
        resources.remove(second).unwrap();
        resources.release_snapshot(snapshot.id).unwrap();
        resources.release_snapshot(old.id).unwrap();
        resources.remove(take).unwrap();
        resources.remove(current).unwrap();
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
                    .push(event as u32, Q, &[0xb0, (tick * 8 + event + 2) as u8, 1])
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
        assert_eq!(usage.available_records, limits.packets.records - 1);
        assert_eq!(usage.available_bodies, limits.packets.bodies - 1);
        assert_eq!(usage.views, 3);
        let mut expected = [0; 15];
        encode_midi_record(0, &[0xb0, 2, 1], &mut expected).unwrap();
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
