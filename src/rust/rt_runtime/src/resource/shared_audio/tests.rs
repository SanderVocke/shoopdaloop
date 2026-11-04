use super::*;
use crate::{Budgets, Config as RuntimeConfig};
use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};

fn store(pages: usize, page_bytes: usize, max_frames: usize) -> ResourceStore {
    ResourceStore::new(
        RuntimeConfig {
            max_frames,
            resource_pages: pages,
            page_bytes,
            budgets: Budgets {
                transfer_bytes: page_bytes,
                validation_steps: 1,
                ..Budgets::default()
            },
            ..RuntimeConfig::default()
        }
        .prepare()
        .unwrap(),
        crate::Quantum::new(1).unwrap(),
    )
    .unwrap()
}

fn values<const N: usize>(
    media: &AudioStore,
    store: &mut ResourceStore,
    index: ReadIndex,
) -> [f32; N] {
    let mut bytes = [0u8; 256];
    media
        .read_bytes(&mut store.render_io(), index, 0, &mut bytes[..N * 4])
        .unwrap();
    core::array::from_fn(|i| f32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap()))
}

fn cleanup(media: &mut AudioStore, store: &mut ResourceStore) {
    for _ in 0..media.nodes.len() {
        store.begin_callback();
        assert!(media.maintain(store).unwrap() <= 1);
    }
    media.begin_cleanup().unwrap();
    for _ in 0..media.nodes.len() {
        store.begin_callback();
        assert!(media.maintain(store).unwrap() <= 1);
    }
    assert_eq!(media.free_nodes.len(), media.nodes.len());
    assert_eq!(media.reserved, 0);
}

#[shoop_test]
fn discontinuity_retains_exports_and_starts_truthful_coverage_without_replay() {
    let mut store = store(80, 16, 8);
    let mut media = AudioStore::prepare(
        &mut store,
        Config {
            channels: 1,
            history_frames: 8,
            views: 4,
            pages: 64,
        },
        Frame(0),
    )
    .unwrap();
    assert_no_heap(|| {
        media
            .append(&mut store.render_io(), Frame(0), &[1., 2., 3., 4.])
            .unwrap();
        let take = media.grab(Frame(0)..Frame(4)).unwrap();
        let old = media.export(take).unwrap();
        media
            .discontinuity(&mut store.render_io(), Frame(8))
            .unwrap();
        assert_eq!(media.coverage(), Ok(Frame(8)..Frame(8)));
        assert_eq!(
            media.append(&mut store.render_io(), Frame(4), &[5., 6.]),
            Err(Error::InvalidFrame)
        );
        media
            .append(&mut store.render_io(), Frame(8), &[9., 10., 11., 12.])
            .unwrap();
        assert_eq!(media.coverage(), Ok(Frame(8)..Frame(12)));
        assert_eq!(media.grab(Frame(4)..Frame(12)), Err(Error::InvalidFrame));
        assert_eq!(
            values::<4>(&media, &mut store, media.export_index(old).unwrap()),
            [1., 2., 3., 4.]
        );
        assert_eq!(
            media.discontinuity(&mut store.render_io(), Frame(11)),
            Err(Error::InvalidFrame)
        );
        let fresh = media.grab(Frame(8)..Frame(12)).unwrap();
        assert_eq!(
            values::<4>(&media, &mut store, media.take_index(fresh).unwrap()),
            [9., 10., 11., 12.]
        );
        media.release_take(fresh).unwrap();
        media.release_take(take).unwrap();
        media.release_export(old).unwrap();
        assert_eq!(media.export_index(old), Err(Error::InvalidId));
    });
    cleanup(&mut media, &mut store);
    assert_eq!(store.usage().free_pages, 80);
}

#[shoop_test]
fn partial_stereo_history_forks_and_cross_boundary_edits_preserve_exports() {
    let mut store = store(80, 24, 32);
    let baseline = store.usage();
    let mut media = AudioStore::prepare(
        &mut store,
        Config {
            channels: 2,
            history_frames: 11,
            views: 6,
            pages: 72,
        },
        Frame(0),
    )
    .unwrap();
    assert_no_heap(|| {
        let mut next = 0;
        for frames in [1, 3, 2, 5] {
            let input: [f32; 22] = core::array::from_fn(|i| {
                let value = (next + i / 2) as f32;
                if i % 2 == 0 {
                    value
                } else {
                    -value
                }
            });
            store.begin_callback();
            store.charge_external(24, 0).unwrap();
            media
                .append(
                    &mut store.render_io(),
                    Frame(next as u64),
                    &input[..frames * 2],
                )
                .unwrap();
            next += frames;
            assert_eq!(store.copy_remaining(), 0);
        }
        let a = media.grab(Frame(1)..Frame(8)).unwrap();
        let b = media.fork(a).unwrap();
        let export = media.export(a).unwrap();
        let a_index = media.take_index(a).unwrap();
        let b_index = media.take_index(b).unwrap();
        let old = media.export_index(export).unwrap();
        let original = [
            1., -1., 2., -2., 3., -3., 4., -4., 5., -5., 6., -6., 7., -7.,
        ];
        assert_eq!(values::<14>(&media, &mut store, a_index), original);
        let patch = [
            100., -100., 101., -101., 102., -102., 103., -103., 104., -104.,
        ];
        media.replace(&mut store.render_io(), a, 5, &patch).unwrap();
        assert_eq!(
            values::<14>(&media, &mut store, a_index),
            [102., -102., 103., -103., 104., -104., 4., -4., 5., -5., 100., -100., 101., -101.]
        );
        for frames in [7, 1, 13, 2, 9, 3, 21] {
            let input = [999.; 64];
            store.begin_callback();
            store.charge_external(24, 0).unwrap();
            media
                .append(
                    &mut store.render_io(),
                    Frame(next as u64),
                    &input[..frames * 2],
                )
                .unwrap();
            next += frames;
            assert_eq!(values::<14>(&media, &mut store, b_index), original);
            assert_eq!(values::<14>(&media, &mut store, old), original);
        }
        assert_eq!(media.info(a_index).unwrap().frames, 7);
        assert_eq!(media.info(a_index).unwrap().revision, Revision(1));
        assert_eq!(media.info(old).unwrap().revision, Revision(0));
        media.release_take(a).unwrap();
        assert_eq!(media.info(a_index), Err(Error::InvalidId));
        assert_eq!(values::<14>(&media, &mut store, old), original);
        media.release_take(b).unwrap();
        media.release_export(export).unwrap();
        cleanup(&mut media, &mut store);
        assert_eq!(store.usage(), baseline);
    });
}

#[shoop_test]
fn multichannel_write_preflight_refuses_without_publishing_or_consuming_credits() {
    let mut store = store(80, 24, 32);
    let mut media = AudioStore::prepare(
        &mut store,
        Config {
            channels: 2,
            history_frames: 11,
            views: 2,
            pages: 72,
        },
        Frame(0),
    )
    .unwrap();
    assert_no_heap(|| {
        store.begin_callback();
        media
            .append(&mut store.render_io(), Frame(0), &[1.; 22])
            .unwrap();
        let take = media.grab(Frame(1)..Frame(8)).unwrap();
        let index = media.take_index(take).unwrap();
        let before = media.info(index).unwrap();
        let quota = store.usage();
        let free = core::mem::take(&mut media.free_nodes);
        let coverage = media.coverage().unwrap();
        assert_eq!(
            media.append(&mut store.render_io(), Frame(11), &[9.; 4]),
            Err(Error::Capacity)
        );
        assert_eq!(media.coverage(), Ok(coverage));
        assert_eq!(
            media.replace(&mut store.render_io(), take, 5, &[9.; 10]),
            Err(Error::Capacity)
        );
        assert_eq!(media.info(index), Ok(before));
        assert_eq!(store.usage(), quota);
        assert_eq!(values::<14>(&media, &mut store, index), [1.; 14]);
        media.free_nodes = free;
        assert_eq!(
            media.replace(&mut store.render_io(), take, 5, &[9.; 9]),
            Err(Error::InvalidShape)
        );
        assert_eq!(
            media.replace(&mut store.render_io(), take, 7, &[9.; 2]),
            Err(Error::InvalidOffset)
        );
        assert_eq!(media.info(index), Ok(before));
        assert_eq!(store.usage(), quota);
        let export = media.export(take).unwrap();
        assert_eq!(media.export(take), Err(Error::Capacity));
        media
            .replace(&mut store.render_io(), take, 5, &[9.; 10])
            .unwrap();
        assert_eq!(
            values::<14>(&media, &mut store, media.export_index(export).unwrap()),
            [1.; 14]
        );
        media.release_export(export).unwrap();
        media.release_take(take).unwrap();
        cleanup(&mut media, &mut store);
        assert_eq!(store.usage().free_pages, 80);
    });
}

#[shoop_test]
fn realistic_history_sizing_keeps_small_write_work_logarithmic() {
    for keep in [17, 48_000 * 30] {
        let ring = (keep as usize)
            .div_ceil(512)
            .checked_add(1)
            .unwrap()
            .next_power_of_two();
        let pages = 5 * ring + 3;
        let mut store = store(pages, 4096, 32);
        let mut media = AudioStore::prepare(
            &mut store,
            Config {
                channels: 2,
                history_frames: keep,
                views: 4,
                pages,
            },
            Frame(0),
        )
        .unwrap();
        let capacity = media.capacity();
        std::println!(
            "history_frames={keep} capacity={capacity:?} reserved_audio_bytes={}",
            pages * 4096
        );
        assert_eq!(capacity.history_pages, ring);
        assert!(capacity.metadata_bytes < 16 * 1024 * 1024);
        assert_no_heap(|| {
            store.begin_callback();
            media
                .append(&mut store.render_io(), Frame(0), &[1., 2.])
                .unwrap();
            let take = media.grab(Frame(0)..Frame(1)).unwrap();
            store.begin_callback();
            media
                .replace(&mut store.render_io(), take, 0, &[3., 4.])
                .unwrap();
            assert!(store.render_work().copied_bytes <= 2 * 4096 + 8);
            assert!(store.render_work().validation_steps <= 4 * (media.depth + 1));
            assert_eq!(store.work().copied_bytes, 0);
            media.release_take(take).unwrap();
            cleanup(&mut media, &mut store);
            assert_eq!(store.usage().free_pages, pages);
        });
    }
}

#[shoop_test]
fn public_audio_handles_and_invalid_boundaries_preserve_sources_and_output() {
    let mut store = store(80, 24, 32);
    let baseline = store.usage();
    let mut foreign_store = self::store(80, 24, 32);
    let mut media = AudioStore::prepare(
        &mut store,
        Config {
            channels: 2,
            history_frames: 11,
            views: 4,
            pages: 72,
        },
        Frame(17),
    )
    .unwrap();
    assert_no_heap(|| {
        store.begin_callback();
        let before = store.usage();
        assert_eq!(
            media.append(&mut store.render_io(), Frame(18), &[0.; 2]),
            Err(Error::InvalidFrame)
        );
        assert_eq!(
            media.append(&mut store.render_io(), Frame(17), &[0.; 3]),
            Err(Error::InvalidShape)
        );
        media
            .append(&mut store.render_io(), Frame(17), &[])
            .unwrap();
        assert_eq!(media.coverage(), Ok(Frame(17)..Frame(17)));
        assert_eq!(store.usage(), before);
        assert_eq!(store.render_work().copied_bytes, 0);
        media
            .append(&mut store.render_io(), Frame(17), &[1.; 22])
            .unwrap();
        let take = media.grab(Frame(18)..Frame(25)).unwrap();
        let index = media.take_index(take).unwrap();
        let export = media.export(take).unwrap();
        let mut output = [0xa5; 16];
        assert_eq!(
            media.read_bytes(&mut foreign_store.render_io(), index, 0, &mut output),
            Err(Error::InvalidId)
        );
        assert_eq!(
            media.read_bytes(&mut store.render_io(), index, 6, &mut output),
            Err(Error::InvalidOffset)
        );
        assert_eq!(output, [0xa5; 16]);
        assert_eq!(
            media.replace(&mut foreign_store.render_io(), take, 0, &[9.; 4]),
            Err(Error::InvalidId)
        );
        assert_eq!(values::<14>(&media, &mut store, index), [1.; 14]);
        let stale = media.take_index(take).unwrap();
        media.release_take(take).unwrap();
        assert_eq!(media.take_index(take), Err(Error::InvalidId));
        assert_eq!(
            media.replace(&mut store.render_io(), take, 0, &[9.; 4]),
            Err(Error::InvalidId)
        );
        assert_eq!(media.info(stale), Err(Error::InvalidId));
        let newer = media.grab(Frame(19)..Frame(26)).unwrap();
        assert_eq!(media.info(stale), Err(Error::InvalidId));
        assert_eq!(
            values::<14>(&media, &mut store, media.export_index(export).unwrap()),
            [1.; 14]
        );
        media.release_take(newer).unwrap();
        media.release_export(export).unwrap();
        cleanup(&mut media, &mut store);
        assert_eq!(store.usage(), baseline);
    });
}

#[shoop_test]
fn non_divisible_multichannel_pages_keep_frames_coherent_through_large_callbacks() {
    let mut store = store(80, 24, 16);
    let baseline = store.usage();
    let mut media = AudioStore::prepare(
        &mut store,
        Config {
            channels: 5,
            history_frames: 7,
            views: 4,
            pages: 72,
        },
        Frame(0),
    )
    .unwrap();
    assert_no_heap(|| {
        store.begin_callback();
        let input: [f32; 35] = core::array::from_fn(|i| (10 * (i / 5) + i % 5) as f32);
        media
            .append(&mut store.render_io(), Frame(0), &input)
            .unwrap();
        let take = media.grab(Frame(1)..Frame(6)).unwrap();
        let saved = media.export(take).unwrap();
        let original: [f32; 25] = core::array::from_fn(|i| (10 * (1 + i / 5) + i % 5) as f32);
        let patch: [f32; 15] = core::array::from_fn(|i| 100. + i as f32);
        media
            .replace(&mut store.render_io(), take, 4, &patch)
            .unwrap();
        let expected: [f32; 25] = core::array::from_fn(|i| match i / 5 {
            0 => 105. + (i % 5) as f32,
            1 => 110. + (i % 5) as f32,
            4 => 100. + (i % 5) as f32,
            _ => original[i],
        });
        assert_eq!(
            values::<25>(&media, &mut store, media.take_index(take).unwrap()),
            expected
        );
        let later: [f32; 80] = core::array::from_fn(|i| (10 * (7 + i / 5) + i % 5) as f32);
        media
            .append(&mut store.render_io(), Frame(7), &later)
            .unwrap();
        let newest = media.grab(Frame(16)..Frame(23)).unwrap();
        let expected_history: [f32; 35] =
            core::array::from_fn(|i| (10 * (16 + i / 5) + i % 5) as f32);
        assert_eq!(
            values::<35>(&media, &mut store, media.take_index(newest).unwrap()),
            expected_history
        );
        assert_eq!(
            values::<25>(&media, &mut store, media.export_index(saved).unwrap()),
            original
        );
        media.release_take(newest).unwrap();
        media.release_take(take).unwrap();
        media.release_export(saved).unwrap();
        cleanup(&mut media, &mut store);
        assert_eq!(store.usage(), baseline);
    });
}
