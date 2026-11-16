use super::*;
use crate::buffer::MidiBuffer;
use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};

const Q: usize = 32;
fn resources() -> ResourceStore {
    ResourceStore::new(
        crate::Config {
            max_frames: Q,
            resource_slots: 12,
            resource_pages: 512,
            page_bytes: 64,
            ..crate::Config::default()
        }
        .prepare()
        .unwrap(),
        Quantum::new(Q).unwrap(),
    )
    .unwrap()
}
fn midi_config() -> indexed_midi::Config {
    indexed_midi::Config {
        blocks: indexed_midi::BlockLimits {
            event_credits: 2048,
            record_pages: 2048,
            import_bases: 4,
            media: 32,
            versions: (32) * 8,
            directories: 4096,
        },
        sources: 1,
        history_frames: 128,
        history_events: 32,
        history_bytes: 4096,
        views: 8,
        writers: 2,
        imports: 1,
        events_per_tick: 8,
        bytes_per_tick: 128,
        maintenance_steps_per_tick: 32,
    }
}
fn read<const N: usize>(resources: &mut ResourceStore, snapshot: Snapshot) -> [u8; N] {
    assert_eq!(snapshot.description.bytes, N);
    let mut cursor = resources.cursor(snapshot.id).unwrap();
    let mut bytes = [0; N];
    let mut offset = 0;
    while offset < N {
        let copied = resources
            .render_io()
            .read(&mut cursor, &mut bytes[offset..])
            .unwrap();
        assert!(copied > 0);
        offset += copied;
    }
    bytes
}
fn drain(resources: &mut ResourceStore) {
    for _ in 0..4096 {
        resources.begin_callback();
        if resources.reclaim() == 0 {
            return;
        }
    }
    panic!("retirement did not finish");
}

#[shoop_test]
fn paired_publication_uses_reserved_owners_after_every_generic_and_media_slot_is_full() {
    let mut resources = resources();
    let baseline = resources.usage();
    let audio_store = resources
        .prepare_audio_store(
            shared_audio::Config {
                channels: 2,
                history_frames: 64,
                views: 12,
                pages: 256,
            },
            Frame(0),
        )
        .unwrap();
    let midi_store = resources.prepare_indexed_midi_store(midi_config()).unwrap();
    let source = resources.indexed_midi_source(midi_store, 0).unwrap();
    let mut input = MidiBuffer::new(8, 128).unwrap();
    let mut samples: [f32; 64] = core::array::from_fn(|i| i as f32);
    let initial_audio: [u8; 256] = core::array::from_fn(|i| samples[i / 4].to_le_bytes()[i % 4]);
    let mut initial_midi = [0; 15];
    encode_midi_record(0, &[0xb0, 7, 11], &mut initial_midi).unwrap();
    assert_no_heap(|| {
        resources.begin_callback();
        resources
            .render_io()
            .append_audio_history(audio_store, Frame(0), &samples)
            .unwrap();
        input.push(0, Q, &[0xb0, 7, 11]).unwrap();
        resources
            .render_io()
            .append_indexed_midi_source(source, Frame(0), &input)
            .unwrap();
        let audio = resources
            .grab_audio(audio_store, Frame(0)..Frame(32))
            .unwrap();
        let midi = resources
            .grab_indexed_midi(source, Frame(0)..Frame(32), 32)
            .unwrap();
        let old_audio = resources.snapshot(audio, Frame(32)).unwrap();
        let old_midi = resources.snapshot(midi, Frame(32)).unwrap();
        resources
            .prepare_audio_growth(audio, Revision(1), 128)
            .unwrap();
        let audio_exit = resources
            .reserve_writer_snapshot(audio, Frame(32), 128)
            .unwrap();
        let midi_exit = resources
            .reserve_writer_snapshot(midi, Frame(32), 128)
            .unwrap();
        let writer = resources
            .prepare_indexed_midi_writer(
                midi,
                Revision(1),
                indexed_midi::WriteKind::Growing {
                    maximum_frames: 128,
                },
            )
            .unwrap();
        while resources.progress_indexed_midi_writer(writer, 1).unwrap()
            != indexed_midi::Status::Ready
        {}
        resources.start_indexed_midi_writer(writer, 32).unwrap();
        let mut held = [None; 8];
        for slot in &mut held {
            *slot = Some(resources.snapshot(audio, Frame(32)).unwrap());
        }
        let mut peers = [None; 4];
        for slot in &mut peers {
            *slot = Some(
                resources
                    .fork_indexed_midi(midi_store, old_midi.id)
                    .unwrap(),
            );
        }
        let mut unrelated = [None; 6];
        for slot in &mut unrelated {
            let resource = resources.create().unwrap();
            let upload = resources
                .begin_upload(
                    resource,
                    Revision(0),
                    Description {
                        format: Format::Midi,
                        frames: 128,
                        bytes: 0,
                    },
                )
                .unwrap();
            resources.finish_upload(upload).unwrap();
            resources.commit(upload).unwrap();
            *slot = Some(resource);
        }
        assert_eq!(resources.usage().free_snapshots, 0);
        assert_eq!(resources.usage().free_versions, 0);
        resources.copy_remaining = 0;
        resources.steps_remaining = 0;
        let mut audio_revision = Revision(1);
        for tick in 1..=3 {
            for (i, value) in samples.iter_mut().enumerate() {
                *value = (tick * 64 + i) as f32;
            }
            input.clear();
            input
                .push(tick as u32, Q, &[0xb0, 7, tick as u8 + 30])
                .unwrap();
            let frame = Frame(tick as u64 * Q as u64);
            resources
                .render_io()
                .append_indexed_midi_source(source, frame, &input)
                .unwrap();
            resources
                .render_io()
                .write_indexed_midi_tick(writer, source, frame)
                .unwrap();
            audio_revision = resources
                .render_io()
                .append_audio_take(audio, audio_revision, &samples)
                .unwrap();
        }
        resources
            .finish_audio_growth(audio, audio_revision)
            .unwrap();
        resources.finish_indexed_midi_writer(writer).unwrap();
        assert_eq!(resources.snapshot(audio, Frame(128)), Err(Error::Capacity));
        let audio_now = resources
            .publish_reserved_snapshot(audio_exit, Frame(128))
            .unwrap();
        let midi_now = resources
            .publish_reserved_snapshot(midi_exit, Frame(128))
            .unwrap();
        assert_eq!(resources.copy_remaining(), 0);
        assert_eq!(resources.steps_remaining(), 0);
        assert_eq!(
            resources.publish_reserved_snapshot(audio_exit, Frame(128)),
            Err(Error::InvalidState)
        );
        assert_eq!(
            resources.cancel_reserved_snapshot(midi_exit),
            Err(Error::InvalidState)
        );
        let expected_audio: [u8; 1024] =
            core::array::from_fn(|i| ((i / 4) as f32).to_le_bytes()[i % 4]);
        assert_eq!(read::<1024>(&mut resources, audio_now), expected_audio);
        let mut expected_midi = [0; 60];
        expected_midi[..15].copy_from_slice(&initial_midi);
        for tick in 1..=3 {
            encode_midi_record(
                tick as u64 * 33,
                &[0xb0, 7, tick as u8 + 30],
                &mut expected_midi[tick * 15..(tick + 1) * 15],
            )
            .unwrap();
        }
        assert_eq!(read::<60>(&mut resources, midi_now), expected_midi);
        assert_eq!(read::<256>(&mut resources, old_audio), initial_audio);
        assert_eq!(read::<15>(&mut resources, old_midi), initial_midi);
        assert_eq!(
            resources.currentness(audio_now.id),
            Ok(Currentness::Current)
        );
        assert_eq!(resources.currentness(old_audio.id), Ok(Currentness::Stale));
        for snapshot in [old_audio, old_midi, audio_now, midi_now]
            .into_iter()
            .chain(held.into_iter().flatten())
        {
            resources.release_snapshot(snapshot.id).unwrap();
        }
        for resource in [Some(audio), Some(midi)]
            .into_iter()
            .chain(peers)
            .chain(unrelated)
            .flatten()
        {
            resources.remove(resource).unwrap();
        }
        drain(&mut resources);
        resources.cleanup_audio_store(audio_store).unwrap();
        resources.cleanup_indexed_midi_store(midi_store).unwrap();
        drain(&mut resources);
        assert_eq!(resources.usage(), baseline);
    });
}

#[shoop_test]
fn cancelled_stale_and_abandoned_publications_return_their_future_owners() {
    let mut resources = resources();
    let baseline = resources.usage();
    let midi_store = resources.prepare_indexed_midi_store(midi_config()).unwrap();
    let source = resources.indexed_midi_source(midi_store, 0).unwrap();
    let mut foreign = self::resources();
    assert_no_heap(|| {
        let midi = resources
            .grab_indexed_midi(source, Frame(0)..Frame(0), 32)
            .unwrap();
        let first = resources
            .reserve_writer_snapshot(midi, Frame(0), 64)
            .unwrap();
        assert_eq!(
            foreign.cancel_reserved_snapshot(first),
            Err(Error::InvalidId)
        );
        resources.cancel_reserved_snapshot(first).unwrap();
        assert_eq!(
            resources.cancel_reserved_snapshot(first),
            Err(Error::InvalidId)
        );
        drain(&mut resources);
        let second = resources
            .reserve_writer_snapshot(midi, Frame(0), 64)
            .unwrap();
        let upload = resources
            .begin_upload(
                midi,
                Revision(1),
                Description {
                    format: Format::Midi,
                    frames: 32,
                    bytes: 0,
                },
            )
            .unwrap();
        resources.finish_upload(upload).unwrap();
        resources.commit(upload).unwrap();
        assert_eq!(
            resources.publish_reserved_snapshot(second, Frame(32)),
            Err(Error::StaleRevision)
        );
        resources.cancel_reserved_snapshot(second).unwrap();
        resources.remove(midi).unwrap();
        drain(&mut resources);
        let midi = resources
            .grab_indexed_midi(source, Frame(0)..Frame(0), 32)
            .unwrap();
        let abandoned = resources
            .reserve_writer_snapshot(midi, Frame(0), 64)
            .unwrap();
        resources.remove(midi).unwrap();
        assert_eq!(
            resources.publish_reserved_snapshot(abandoned, Frame(32)),
            Err(Error::InvalidId)
        );
        resources.cancel_reserved_snapshot(abandoned).unwrap();
        drain(&mut resources);
        resources.cleanup_indexed_midi_store(midi_store).unwrap();
        drain(&mut resources);
        assert_eq!(resources.usage(), baseline);
    });
}
