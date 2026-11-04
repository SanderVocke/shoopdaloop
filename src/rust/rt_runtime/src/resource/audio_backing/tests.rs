use super::*;
use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};

fn resources() -> ResourceStore {
    ResourceStore::new(
        crate::Config {
            max_frames: 8,
            resource_slots: 24,
            resource_pages: 128,
            page_bytes: 24,
            budgets: crate::Budgets {
                transfer_bytes: 24,
                validation_steps: 1,
                ..crate::Budgets::default()
            },
            ..crate::Config::default()
        }
        .prepare()
        .unwrap(),
        crate::Quantum::new(1).unwrap(),
    )
    .unwrap()
}

fn audio(store: &mut ResourceStore, channels: u32, history_frames: usize) -> AudioStoreId {
    store
        .prepare_audio_store(
            shared_audio::Config {
                channels,
                history_frames,
                views: 16,
                pages: 110,
            },
            Frame(0),
        )
        .unwrap()
}

fn read<const N: usize>(store: &mut ResourceStore, snapshot: Snapshot) -> [u8; N] {
    assert_eq!(N, snapshot.description.bytes);
    let mut cursor = store.cursor(snapshot.id).unwrap();
    let mut result = [0; N];
    let mut filled = 0;
    while filled < N {
        store.begin_callback();
        let count = store
            .read(&mut cursor, &mut result[filled..(filled + 7).min(N)])
            .unwrap();
        assert!(count > 0);
        filled += count;
    }
    result
}

fn drain(store: &mut ResourceStore) {
    for _ in 0..4096 {
        store.begin_callback();
        assert!(store.reclaim() <= 1);
    }
}

#[shoop_test]
fn generic_snapshots_pin_grabs_across_edits_upload_commits_and_resource_removal() {
    let mut store = resources();
    let baseline = store.usage();
    let audio = audio(&mut store, 5, 3);
    let input: [f32; 20] = core::array::from_fn(|i| i as f32 + 1.);
    let original: [u8; 60] = core::array::from_fn(|i| input[5 + i / 4].to_le_bytes()[i % 4]);
    assert_no_heap(|| {
        store.begin_callback();
        store
            .render_io()
            .append_audio_history(audio, Frame(0), &input)
            .unwrap();
        let resource = store.grab_audio(audio, Frame(1)..Frame(4)).unwrap();
        let old = store.snapshot(resource, Frame(4)).unwrap();
        let retained = store.retain_snapshot(old.id).unwrap();
        let peer = store.fork_audio(audio, old.id).unwrap();
        let peer_snapshot = store.snapshot(peer, Frame(4)).unwrap();
        store.charge_external(24, 0).unwrap();
        store
            .render_io()
            .replace_audio(resource, Revision(1), 2, &[99.; 10])
            .unwrap();
        assert_eq!(store.copy_remaining(), 0);
        assert_eq!(store.currentness(old.id), Ok(Currentness::Stale));
        let edited = store.snapshot(resource, Frame(4)).unwrap();
        let mut expected = original;
        for sample in (0..5).chain(10..15) {
            expected[sample * 4..sample * 4 + 4].copy_from_slice(&99f32.to_le_bytes());
        }
        assert_eq!(read::<60>(&mut store, edited), expected);
        assert_eq!(read::<60>(&mut store, peer_snapshot), original);
        assert_eq!(read::<60>(&mut store, retained), original);
        let upload = store
            .begin_upload(
                resource,
                Revision(2),
                Description {
                    format: Format::Audio { channels: 5 },
                    frames: 1,
                    bytes: 20,
                },
            )
            .unwrap();
        store.begin_callback();
        store.write_upload(upload, 0, &[0; 20]).unwrap();
        store.finish_upload(upload).unwrap();
        store.commit(upload).unwrap();
        assert_eq!(read::<60>(&mut store, edited), expected);
        store.remove(resource).unwrap();
        store.remove(peer).unwrap();
        assert_eq!(store.currentness(old.id), Ok(Currentness::ResourceRemoved));
        assert_eq!(read::<60>(&mut store, old), original);
        assert_eq!(store.cleanup_audio_store(audio), Err(Error::Busy));
        for snapshot in [old, retained, peer_snapshot, edited] {
            store.release_snapshot(snapshot.id).unwrap();
        }
        assert_eq!(store.cursor(old.id), Err(Error::InvalidId));
        drain(&mut store);
        store.cleanup_audio_store(audio).unwrap();
        drain(&mut store);
        assert_eq!(store.usage(), baseline);
    });
}

#[shoop_test]
fn shared_source_pool_keeps_histories_independent_and_live_under_global_owner_pressure() {
    let mut store = resources();
    let baseline = store.usage();
    let pool = store
        .prepare_audio_pool(
            shared_audio::Config {
                channels: 2,
                history_frames: 3,
                views: 8,
                pages: 10,
            },
            Frame(0),
            2,
        )
        .unwrap();
    let a = store.audio_history(pool, 0).unwrap();
    let b = store.audio_history(pool, 1).unwrap();
    assert_eq!(store.audio_history(pool, 2), Err(Error::InvalidId));
    let capacity = store.audio_store_capacity(pool).unwrap();
    assert_eq!((capacity.histories, capacity.history_pages), (2, 4));
    let mut foreign = resources();
    let foreign_pool = audio(&mut foreign, 2, 3);
    let foreign_source = foreign.audio_history(foreign_pool, 0).unwrap();
    let first = [1f32, -1., 2., -2., 3., -3.];
    let second = [10001f32, -10001., 10002., -10002., 10003., -10003.];
    let first_bytes: [u8; 24] = core::array::from_fn(|i| first[i / 4].to_le_bytes()[i % 4]);
    let second_bytes: [u8; 24] = core::array::from_fn(|i| second[i / 4].to_le_bytes()[i % 4]);
    assert_no_heap(|| {
        store.begin_callback();
        assert_eq!(
            store
                .render_io()
                .append_audio_source(foreign_source, Frame(0), &first),
            Err(Error::InvalidId)
        );
        store
            .render_io()
            .append_audio_source(a, Frame(0), &first)
            .unwrap();
        store
            .render_io()
            .append_audio_source(b, Frame(0), &second)
            .unwrap();
        let take_a = store.grab_audio_source(a, Frame(0)..Frame(3)).unwrap();
        let take_b = store.grab_audio_source(b, Frame(0)..Frame(3)).unwrap();
        let old_a = store.snapshot(take_a, Frame(3)).unwrap();
        let old_b = store.snapshot(take_b, Frame(3)).unwrap();
        let before = store.usage();
        assert_eq!(
            store.grab_audio_source(b, Frame(0)..Frame(3)),
            Err(Error::Capacity)
        );
        assert_eq!(store.usage(), before);

        let unrelated = store.create().unwrap();
        let free = store.usage().free_pages - store.usage().reserved_pages;
        assert_eq!(free, baseline.free_pages - 10);
        let upload = store
            .begin_upload(
                unrelated,
                Revision(0),
                Description {
                    format: Format::Audio { channels: 2 },
                    frames: (free * 3) as u64,
                    bytes: free * 24,
                },
            )
            .unwrap();
        assert_eq!(store.usage().free_pages, store.usage().reserved_pages);
        store
            .render_io()
            .replace_audio(take_a, Revision(1), 1, &[99., -99.])
            .unwrap();
        for tick in 1..64 {
            let frame = Frame(tick * 3);
            store.begin_callback();
            store.charge_external(24, 0).unwrap();
            if tick == 10 {
                let unchanged = store.audio_source_coverage(b).unwrap();
                store
                    .render_io()
                    .audio_source_discontinuity(a, frame)
                    .unwrap();
                assert_eq!(store.audio_source_coverage(a), Ok(frame..frame));
                assert_eq!(store.audio_source_coverage(b), Ok(unchanged));
            }
            let base = tick as f32 * 100.;
            let input_a = [
                base + 1.,
                -base - 1.,
                base + 2.,
                -base - 2.,
                base + 3.,
                -base - 3.,
            ];
            let input_b = input_a.map(|sample| {
                if sample > 0. {
                    sample + 10000.
                } else {
                    sample - 10000.
                }
            });
            store
                .render_io()
                .append_audio_source(a, frame, &input_a)
                .unwrap();
            store
                .render_io()
                .append_audio_source(b, frame, &input_b)
                .unwrap();
            assert_eq!(store.copy_remaining(), 0);
        }
        assert_eq!(read::<24>(&mut store, old_a), first_bytes);
        assert_eq!(read::<24>(&mut store, old_b), second_bytes);
        store.release_snapshot(old_a.id).unwrap();
        store.release_snapshot(old_b.id).unwrap();
        drain(&mut store);
        let changed = store.snapshot(take_a, Frame(192)).unwrap();
        let mut expected = first_bytes;
        expected[8..12].copy_from_slice(&99f32.to_le_bytes());
        expected[12..16].copy_from_slice(&(-99f32).to_le_bytes());
        assert_eq!(read::<24>(&mut store, changed), expected);
        store.release_snapshot(changed.id).unwrap();
        drain(&mut store);
        for (source, offset) in [(a, 0f32), (b, 10000f32)] {
            let resource = store
                .grab_audio_source(source, Frame(189)..Frame(192))
                .unwrap();
            let snapshot = store.snapshot(resource, Frame(192)).unwrap();
            let samples = [
                6301. + offset,
                -6301. - offset,
                6302. + offset,
                -6302. - offset,
                6303. + offset,
                -6303. - offset,
            ];
            let bytes: [u8; 24] = core::array::from_fn(|i| samples[i / 4].to_le_bytes()[i % 4]);
            assert_eq!(read::<24>(&mut store, snapshot), bytes);
            store.release_snapshot(snapshot.id).unwrap();
            store.remove(resource).unwrap();
            drain(&mut store);
        }
        store.cancel_upload(upload).unwrap();
        store.remove(unrelated).unwrap();
        store.remove(take_a).unwrap();
        store.remove(take_b).unwrap();
        drain(&mut store);
        store.cleanup_audio_store(pool).unwrap();
        drain(&mut store);
        assert_eq!(store.audio_source_coverage(a), Err(Error::InvalidState));
        assert_eq!(store.usage(), baseline);
        foreign.cleanup_audio_store(foreign_pool).unwrap();
        drain(&mut foreign);
        assert_eq!(foreign.usage().free_pages, 128);
    });
}

#[shoop_test]
fn source_pool_preparation_refuses_without_reserving_an_incomplete_pool() {
    let mut store = resources();
    let before = store.usage();
    let config = shared_audio::Config {
        channels: 2,
        history_frames: 3,
        views: 8,
        pages: 5,
    };
    assert_eq!(
        store.prepare_audio_pool(config, Frame(0), 2),
        Err(Error::Capacity)
    );
    assert_eq!(store.usage(), before);
    assert_eq!(
        store.prepare_audio_pool(config, Frame(0), 0),
        Err(Error::InvalidConfig)
    );
    assert_eq!(store.usage(), before);
}

#[shoop_test]
fn loaded_pcm_longer_than_history_is_a_zero_copy_base_with_isolated_wrapping_edits() {
    let mut store = resources();
    let baseline = store.usage();
    let audio = audio(&mut store, 2, 3);
    let input: [u8; 136] = core::array::from_fn(|i| (i / 4 + 1) as u8);
    assert_no_heap(|| {
        let loaded = store.create().unwrap();
        let upload = store
            .begin_upload(
                loaded,
                Revision(0),
                Description {
                    format: Format::Audio { channels: 2 },
                    frames: 17,
                    bytes: input.len(),
                },
            )
            .unwrap();
        for (page, chunk) in input.chunks(24).enumerate() {
            store.begin_callback();
            store.write_upload(upload, page * 24, chunk).unwrap();
        }
        store.finish_upload(upload).unwrap();
        store.commit(upload).unwrap();
        let source = store.snapshot(loaded, Frame(0)).unwrap();
        store.begin_callback();
        let before = store.usage();
        let editable = store.fork_audio(audio, source.id).unwrap();
        assert_eq!(store.usage().free_pages, before.free_pages);
        assert_eq!(store.work().copied_bytes, 0);
        let original = store.snapshot(editable, Frame(0)).unwrap();
        store.release_snapshot(source.id).unwrap();
        store.remove(loaded).unwrap();
        store.charge_external(24, 0).unwrap();
        let patch = [10., 20., 30., 40., 50., 60.];
        store
            .render_io()
            .replace_audio(editable, Revision(1), 16, &patch)
            .unwrap();
        assert_eq!(store.copy_remaining(), 0);
        assert_eq!(
            store
                .render_io()
                .replace_audio(editable, Revision(1), 0, &patch),
            Err(Error::StaleRevision)
        );
        let changed = store.snapshot(editable, Frame(0)).unwrap();
        let mut expected = input;
        for frame in 0..3 {
            let position = (16 + frame) % 17;
            for channel in 0..2 {
                expected[position * 8 + channel * 4..position * 8 + channel * 4 + 4]
                    .copy_from_slice(&patch[frame * 2 + channel].to_le_bytes());
            }
        }
        assert_eq!(read::<136>(&mut store, original), input);
        assert_eq!(read::<136>(&mut store, changed), expected);
        // Full-loop set supersedes the editable resource without changing a get
        // already in flight. The same byte stream can be uploaded again.
        let restored = store.create().unwrap();
        let upload = store
            .begin_upload(restored, Revision(0), changed.description)
            .unwrap();
        for (page, chunk) in expected.chunks(24).enumerate() {
            store.begin_callback();
            store.write_upload(upload, page * 24, chunk).unwrap();
        }
        store.finish_upload(upload).unwrap();
        store.commit(upload).unwrap();
        let round_trip = store.snapshot(restored, Frame(0)).unwrap();
        assert_eq!(read::<136>(&mut store, round_trip), expected);
        for snapshot in [original, changed, round_trip] {
            store.release_snapshot(snapshot.id).unwrap();
        }
        store.remove(editable).unwrap();
        store.remove(restored).unwrap();
        drain(&mut store);
        store.cleanup_audio_store(audio).unwrap();
        drain(&mut store);
        assert_eq!(store.usage(), baseline);
    });
}

#[shoop_test]
fn scoped_audio_and_export_capacity_fail_without_consuming_generic_credits() {
    let mut store = resources();
    let mut foreign = resources();
    let foreign_audio = audio(&mut foreign, 2, 3);
    let audio = store
        .prepare_audio_store(
            shared_audio::Config {
                channels: 2,
                history_frames: 3,
                views: 2,
                pages: 32,
            },
            Frame(0),
        )
        .unwrap();
    assert_no_heap(|| {
        store.begin_callback();
        let before = store.usage();
        assert_eq!(
            store.grab_audio(foreign_audio, Frame(0)..Frame(1)),
            Err(Error::InvalidId)
        );
        assert_eq!(
            store
                .render_io()
                .append_audio_history(foreign_audio, Frame(0), &[1., 2.]),
            Err(Error::InvalidId)
        );
        assert_eq!(store.usage(), before);
        store
            .render_io()
            .append_audio_history(audio, Frame(0), &[1., 2., 3., 4.])
            .unwrap();
        let resource = store.grab_audio(audio, Frame(0)..Frame(2)).unwrap();
        let snapshot = store.snapshot(resource, Frame(2)).unwrap();
        let before = store.usage();
        for _ in 0..4 {
            assert_eq!(store.retain_snapshot(snapshot.id), Err(Error::Capacity));
            assert_eq!(store.snapshot(resource, Frame(2)), Err(Error::Capacity));
            assert_eq!(store.fork_audio(audio, snapshot.id), Err(Error::Capacity));
            assert_eq!(store.usage(), before);
        }
        let index = store.read_index(snapshot.id).unwrap();
        let mut cursor = store.cursor_at(index, 3).unwrap();
        let untouched = cursor;
        store.charge_external(24, 0).unwrap();
        let mut bytes = [77; 5];
        assert_eq!(store.read(&mut cursor, &mut bytes), Err(Error::Budget));
        assert_eq!(cursor, untouched);
        assert_eq!(bytes, [77; 5]);
        let revision = store.revision(resource).unwrap();
        assert_eq!(
            store
                .render_io()
                .replace_audio(resource, revision, 0, &[1.]),
            Err(Error::InvalidShape)
        );
        assert_eq!(store.revision(resource), Ok(revision));
        assert_eq!(store.currentness(snapshot.id), Ok(Currentness::Current));
        store.release_snapshot(snapshot.id).unwrap();
        assert_eq!(store.cursor_at(index, 0), Err(Error::InvalidId));
        drain(&mut store);
        let new = store.snapshot(resource, Frame(2)).unwrap();
        assert_ne!(new.id, snapshot.id);
        store.release_snapshot(new.id).unwrap();
        store.remove(resource).unwrap();
        drain(&mut store);
        store.cleanup_audio_store(audio).unwrap();
        drain(&mut store);
        assert_eq!(store.usage().free_pages, 128);
    });
}
