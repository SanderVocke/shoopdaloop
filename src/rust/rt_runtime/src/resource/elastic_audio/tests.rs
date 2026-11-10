use super::*;
use crate::{Budgets, Config};
use shoop_wasm_test_support::shoop_test;

fn resources() -> ResourceStore {
    let config = Config {
        max_frames: 16,
        resource_pages: 1,
        page_bytes: 64,
        budgets: Budgets {
            transfer_bytes: 4096,
            validation_steps: 1,
            ..Budgets::default()
        },
        ..Config::default()
    }
    .prepare()
    .unwrap();
    let mut store = ResourceStore::with_capture_groups(
        config,
        Quantum::new(16).unwrap(),
        None,
        CaptureGroupConfig {
            groups: 2,
            members_per_group: 1,
            staging_bytes_per_group: 128,
        },
    )
    .unwrap();
    store.set_audio_render_mode(crate::AudioRenderMode::FpgaReference);
    store.begin_callback();
    store
}

fn bytes(store: &mut ResourceStore, snapshot: Snapshot) -> Vec<u8> {
    let index = store.read_index(snapshot.id).unwrap();
    let mut cursor = store.cursor_at(index, 0).unwrap();
    let mut output = vec![0; snapshot.description.bytes];
    let mut copied = 0;
    while copied < output.len() {
        store.begin_callback();
        let count = store.read(&mut cursor, &mut output[copied..]).unwrap();
        assert!(count > 0);
        copied += count;
    }
    output
}

#[shoop_test]
fn media_ranges_outlive_native_take_reclamation_until_their_own_release() {
    let mut store = resources();
    let resource = store.create().unwrap();
    let capture = store
        .begin_capture(
            resource,
            Revision(0),
            Format::Audio { channels: 1 },
            Frame(0),
        )
        .unwrap();
    for frame in [0, 16, 32] {
        store.begin_callback();
        store
            .render_io()
            .capture_audio(capture, Frame(frame), &[0.25; 16])
            .unwrap();
    }
    let take = store.versions[capture.slot as usize]
        .value
        .unwrap()
        .elastic
        .unwrap();
    let range = store
        .elastic_audio
        .as_mut()
        .unwrap()
        .media
        .slice(take.first, 15, 2)
        .unwrap();
    store.stop_capture(capture).unwrap();
    store.remove(resource).unwrap();
    for _ in 0..20 {
        store.begin_callback();
        assert!(store.reclaim() <= 1);
    }
    assert_eq!(store.elastic_audio_work().slots, 3);
    assert_eq!(store.elastic_audio_work().free_slots, 1);
    let mut output = [Fixed::ZERO; 2];
    store
        .elastic_audio
        .as_mut()
        .unwrap()
        .media
        .read_loop(range, 0, 2, &mut output)
        .unwrap();
    assert_eq!(output, [Fixed::from_f32(0.25); 2]);
    let resource = store.create().unwrap();
    let capture = store
        .begin_capture(
            resource,
            Revision(0),
            Format::Audio { channels: 1 },
            Frame(48),
        )
        .unwrap();
    store
        .render_io()
        .capture_audio(capture, Frame(48), &[-0.5; 16])
        .unwrap();
    assert_eq!(store.elastic_audio_work().slots, 3);
    store
        .elastic_audio
        .as_mut()
        .unwrap()
        .media
        .clear(range)
        .unwrap();
    assert_eq!(store.elastic_audio_work().free_slots, 2);
    for frame in [64, 80] {
        store.begin_callback();
        store
            .render_io()
            .capture_audio(capture, Frame(frame), &[0.5; 16])
            .unwrap();
    }
    assert_eq!(store.elastic_audio_work().slots, 3);
    assert_eq!(store.elastic_audio_work().free_slots, 0);
}

#[shoop_test]
fn freed_slots_supply_capacity_without_growing_the_arena_again() {
    let mut audio = ElasticAudio::new(Quantum::new(16).unwrap(), 16);
    let mut old = audio.create(1).unwrap();
    audio.prepare(old, 16, 0).unwrap();
    audio.append_staging(old, &[0; 64]);
    let capacity = audio.work().capacity_slots;
    audio.reclaim(&mut old).unwrap();
    assert_eq!(old.channels, 0);
    let next = audio.create(1).unwrap();
    audio.reset_work();
    audio.prepare(next, 16, 0).unwrap();
    assert_eq!(audio.work().capacity_slots, capacity);
    assert_eq!(audio.work().arena_growths, 0);
    audio.append_staging(next, &[0; 64]);
    assert_eq!(audio.work().slots, 1);
    assert_eq!(audio.work().free_slots, 0);
}

#[shoop_test]
fn elastic_pcm_upload_export_and_fixed_reads_bypass_the_native_payload_pool() {
    let mut store = resources();
    let initial = store.usage();
    let resource = store.create().unwrap();
    let input: Vec<_> = (0..400).map(|i| ((i % 17) as f32 - 8.) / 16.).collect();
    let encoded: Vec<_> = input.iter().flat_map(|x| x.to_le_bytes()).collect();
    let upload = store
        .begin_upload(
            resource,
            Revision(0),
            Description {
                format: Format::Audio { channels: 2 },
                frames: 200,
                bytes: encoded.len(),
            },
        )
        .unwrap();
    // Arbitrary byte splits can end inside a float or between stereo channels.
    for (offset, byte) in encoded.iter().enumerate() {
        store.begin_callback();
        store.write_upload(upload, offset, &[*byte]).unwrap();
    }
    store.finish_upload(upload).unwrap();
    store.commit(upload).unwrap();
    let snapshot = store.snapshot(resource, Frame(0)).unwrap();
    assert_eq!(bytes(&mut store, snapshot), encoded);
    let index = store.read_index(snapshot.id).unwrap();
    for channel in 0..2 {
        let mut samples = [Fixed::ZERO; 9];
        store
            .read_elastic_audio(index, channel, 198, 200, &mut samples)
            .unwrap();
        for (offset, sample) in samples.iter().enumerate() {
            assert_eq!(
                *sample,
                Fixed::from_f32(input[((198 + offset) % 200) * 2 + channel])
            );
        }
    }
    assert_eq!(store.usage().free_pages, initial.free_pages);
    assert_eq!(store.usage().reserved_pages, 0);
    assert_eq!(store.elastic_audio_work().slots, 26);
}

#[shoop_test]
fn pinned_prefixes_survive_growth_clear_and_reuse_until_their_last_lease_releases() {
    let mut store = resources();
    let resource = store.create().unwrap();
    let capture = store
        .begin_capture(
            resource,
            Revision(0),
            Format::Audio { channels: 1 },
            Frame(0),
        )
        .unwrap();
    store
        .render_io()
        .capture_audio(capture, Frame(0), &[0.25; 4])
        .unwrap();
    let prefix = store.snapshot(resource, Frame(4)).unwrap();
    store
        .render_io()
        .capture_audio(capture, Frame(4), &[0.5; 4])
        .unwrap();
    assert_eq!(
        bytes(&mut store, prefix),
        [0.25f32.to_le_bytes(); 4].concat()
    );
    store.stop_capture(capture).unwrap();
    let revision = store.revision(resource).unwrap();
    let newer = store
        .begin_capture(resource, revision, Format::Audio { channels: 1 }, Frame(8))
        .unwrap();
    store
        .render_io()
        .capture_audio(newer, Frame(8), &[0.75; 4])
        .unwrap();
    store.stop_capture(newer).unwrap();
    store.remove(resource).unwrap();
    for _ in 0..10 {
        store.begin_callback();
        store.reclaim();
    }
    assert_eq!(
        bytes(&mut store, prefix),
        [0.25f32.to_le_bytes(); 4].concat()
    );
    store.release_snapshot(prefix.id).unwrap();
    let slots = store.elastic_audio_work().slots;
    for _ in 0..20 {
        store.begin_callback();
        assert!(store.reclaim() <= 1);
    }
    assert_eq!(store.elastic_audio_work().free_slots, slots);
    let resource = store.create().unwrap();
    let capture = store
        .begin_capture(
            resource,
            Revision(0),
            Format::Audio { channels: 1 },
            Frame(12),
        )
        .unwrap();
    store
        .render_io()
        .capture_audio(capture, Frame(12), &[-0.5; 4])
        .unwrap();
    assert_eq!(store.elastic_audio_work().slots, slots);
}

#[shoop_test]
fn paired_fixed_capture_preserves_low_bits_and_cancel_does_not_publish_staging() {
    let mut store = resources();
    let resource = store.create().unwrap();
    let group = store
        .bind_capture_group(&[CaptureMember {
            resource,
            format: Format::Audio { channels: 2 },
            midi_events: 0,
            midi_bytes: 0,
        }])
        .unwrap();
    let take = store.prepare_capture_take(group, &[]).unwrap();
    store.activate_capture_take(take, Frame(0)).unwrap();
    let tick = store.reserve_capture_tick(take, Frame(0)).unwrap();
    let left_pattern = [
        Fixed(i32::MAX),
        Fixed(16_777_217),
        Fixed(i32::MIN),
        Fixed(-3),
    ];
    let right_pattern = [Fixed(17), Fixed(-16_777_217), Fixed(3), Fixed(i32::MAX - 1)];
    let left: [Fixed; 16] = core::array::from_fn(|index| left_pattern[index % 4]);
    let right: [Fixed; 16] = core::array::from_fn(|index| right_pattern[index % 4]);
    assert_eq!(
        store.stage_elastic_audio(tick, 0, 1, 2, &right),
        Err(Error::InvalidState)
    );
    store.stage_elastic_audio(tick, 0, 0, 2, &left).unwrap();
    assert_eq!(store.commit_capture_tick(tick), Err(Error::Incomplete));
    store.stage_elastic_audio(tick, 0, 1, 2, &right).unwrap();
    store.commit_capture_tick(tick).unwrap();
    assert_eq!(store.elastic_audio_work().reserved_chunks, 0);
    let snapshot = store.snapshot(resource, Frame(16)).unwrap();
    let index = store.read_index(snapshot.id).unwrap();
    for (channel, expected) in [left, right].into_iter().enumerate() {
        let mut output = [Fixed::ZERO; 16];
        store
            .read_elastic_audio(index, channel, 0, 16, &mut output)
            .unwrap();
        assert_eq!(output, expected);
    }
    let cancelled = store.reserve_capture_tick(take, Frame(16)).unwrap();
    store
        .stage_elastic_audio(cancelled, 0, 0, 2, &[Fixed(99); 16])
        .unwrap();
    store.cancel_capture_tick(cancelled).unwrap();
    assert_eq!(store.info(resource).unwrap().description.frames, 16);
    assert_eq!(store.elastic_audio_work().reserved_chunks, 0);
    assert_eq!(store.usage().reserved_pages, 0);
}
