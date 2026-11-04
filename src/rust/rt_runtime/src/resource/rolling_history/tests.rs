use super::*;
use crate::{
    resource::{Description, Format},
    Budgets, Config as RuntimeConfig, Revision,
};
use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};

fn store(pages: usize) -> ResourceStore {
    ResourceStore::new(
        RuntimeConfig {
            resource_pages: pages,
            page_bytes: 16,
            resource_slots: 8,
            budgets: Budgets {
                transfer_bytes: 16,
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

fn append(history: &mut RollingHistory, store: &mut ResourceStore, frame: u64) {
    history
        .append(
            store,
            Frame(frame),
            &core::array::from_fn::<_, 4, _>(|i| (frame + i as u64) as f32),
        )
        .unwrap();
}

fn read<const N: usize>(
    history: &RollingHistory,
    store: &mut ResourceStore,
    window: WindowId,
) -> [f32; N] {
    let mut output = [0.; N];
    let mut written = 0;
    while written < N {
        let count = history
            .read_audio(store, window, written as u64, &mut output[written..])
            .unwrap();
        assert!(count > 0);
        written += count;
    }
    output
}

fn cleanup(history: &mut RollingHistory, store: &mut ResourceStore) {
    history.begin_cleanup().unwrap();
    for _ in 0..history.ring.len() {
        store.begin_callback();
        assert!(history.maintain(store).unwrap() <= 1);
    }
    assert_eq!(history.reserved, 0);
    assert!(history.ring.iter().all(Option::is_none));
}

#[shoop_test]
fn overlapping_windows_survive_wraps_independent_release_and_complete_reclamation() {
    let mut store = store(6);
    let baseline = store.usage();
    let mut history = RollingHistory::prepare(
        &mut store,
        Config {
            history_pages: 2,
            retained_pages: 3,
            windows: 3,
        },
        Frame(0),
    )
    .unwrap();
    assert_eq!(store.usage().reserved_pages, 5);
    assert_no_heap(|| {
        for frame in [0, 4] {
            store.begin_callback();
            append(&mut history, &mut store, frame);
        }
        store.begin_callback();
        let a = history
            .lease_window(&mut store, Frame(0)..Frame(8))
            .unwrap();
        assert_eq!(store.work().copied_bytes, 0);
        assert_eq!(store.work().validation_steps, 1);
        store.begin_callback();
        let b = history
            .lease_window(&mut store, Frame(4)..Frame(8))
            .unwrap();
        assert_eq!(history.debt, 3);
        store.begin_callback();
        let before = store.usage();
        assert_eq!(
            history.lease_window(&mut store, Frame(0)..Frame(4)),
            Err(Error::Capacity)
        );
        assert_eq!(store.usage(), before);
        assert_eq!(store.work().validation_steps, 0);
        for frame in (8..64).step_by(4) {
            store.begin_callback();
            append(&mut history, &mut store, frame);
            assert_eq!(store.work().copied_bytes, 16);
            assert_eq!(
                read::<8>(&history, &mut store, a),
                [0., 1., 2., 3., 4., 5., 6., 7.]
            );
            assert_eq!(read::<4>(&history, &mut store, b), [4., 5., 6., 7.]);
            assert_eq!(store.copy_remaining(), 0);
        }
        assert_eq!(history.coverage(), Ok(Frame(56)..Frame(64)));
        assert_eq!(store.usage().free_pages, 2); // Two live and two retained physical pages.
        assert_eq!(store.usage().reserved_pages, 1);
        assert_eq!(history.begin_cleanup(), Err(Error::Busy));
        history.release_window(a).unwrap();
        assert_eq!(history.release_window(a), Err(Error::InvalidId));
        let mut untouched = [-1.; 4];
        assert_eq!(
            history.read_audio(&mut store, a, 0, &mut untouched),
            Err(Error::InvalidId)
        );
        assert_eq!(untouched, [-1.; 4]);
        for _ in 0..2 {
            store.begin_callback();
            assert_eq!(history.maintain(&mut store), Ok(1));
            assert_eq!(read::<4>(&history, &mut store, b), [4., 5., 6., 7.]);
        }
        assert_eq!(history.debt, 1);
        assert_eq!(store.usage().free_pages, 3);
        store.begin_callback();
        append(&mut history, &mut store, 64);
        let c = history
            .lease_window(&mut store, Frame(60)..Frame(68))
            .unwrap();
        assert_eq!(c.slot, a.slot);
        assert_ne!(c.generation, a.generation);
        assert_eq!(history.release_window(a), Err(Error::InvalidId));
        assert_eq!(
            read::<8>(&history, &mut store, c),
            [60., 61., 62., 63., 64., 65., 66., 67.]
        );
        assert_eq!(read::<4>(&history, &mut store, b), [4., 5., 6., 7.]);
        history.release_window(c).unwrap();
        history.release_window(b).unwrap();
        for _ in 0..3 {
            store.begin_callback();
            assert_eq!(history.maintain(&mut store), Ok(1));
        }
        assert_eq!(history.debt, 0);
        cleanup(&mut history, &mut store);
        assert_eq!(store.usage(), baseline);
        assert_eq!(history.pages.available_leases(), 9);
        assert_eq!(history.free_windows.len(), 3);
        store.begin_callback();
        assert_eq!(history.maintain(&mut store), Ok(0));
        assert_eq!(store.usage(), baseline);
        assert_eq!(
            history.append(&mut store, Frame(68), &[0.; 4]),
            Err(Error::InvalidState)
        );
    });
}

#[shoop_test]
fn window_acquisition_is_constant_work_and_full_retention_cannot_starve_the_writer() {
    for length in [2, 64] {
        let mut store = store(2 * length + 1);
        let baseline = store.usage();
        let mut history = RollingHistory::prepare(
            &mut store,
            Config {
                history_pages: length,
                retained_pages: length,
                windows: 1,
            },
            Frame(100),
        )
        .unwrap();
        assert_no_heap(|| {
            let resource = store.create().unwrap();
            let upload = store
                .begin_upload(
                    resource,
                    Revision(0),
                    Description {
                        format: Format::Audio { channels: 1 },
                        frames: 4,
                        bytes: 16,
                    },
                )
                .unwrap();
            assert_eq!(store.usage().reserved_pages, 2 * length + 1);
            for page in 0..length {
                store.begin_callback();
                append(&mut history, &mut store, 100 + page as u64 * 4);
            }
            store.begin_callback();
            let start = Frame(100);
            let end = Frame(100 + length as u64 * 4);
            let window = history.lease_window(&mut store, start..end).unwrap();
            assert_eq!(store.work().copied_bytes, 0);
            assert_eq!(store.work().validation_steps, 1);
            assert!(history.retained.iter().all(Option::is_none));
            for page in length..length * 5 {
                store.begin_callback();
                append(&mut history, &mut store, 100 + page as u64 * 4);
                assert_eq!(store.work().copied_bytes, 16);
                assert_eq!(store.work().validation_steps, 0);
            }
            assert_eq!(store.usage().free_pages, 1);
            assert_eq!(store.usage().reserved_pages, 1);
            assert_eq!(history.reserved, 0);
            let mut values = [0.; 5];
            for offset in 0..length as u64 * 4 {
                let count = history
                    .read_audio(&mut store, window, offset, &mut values)
                    .unwrap();
                assert_eq!(count, 4 - offset as usize % 4);
                for (index, value) in values[..count].iter().enumerate() {
                    assert_eq!(*value, (100 + offset + index as u64) as f32);
                }
            }
            history.release_window(window).unwrap();
            assert_eq!(history.begin_cleanup(), Err(Error::Busy));
            for remaining in (0..length).rev() {
                store.begin_callback();
                assert_eq!(history.maintain(&mut store), Ok(1));
                assert_eq!(store.work().validation_steps, 1);
                assert_eq!(store.work().reclaimed_pages, 1);
                if remaining > 0 {
                    assert_eq!(history.debt, length);
                }
            }
            assert_eq!(history.debt, 0);
            cleanup(&mut history, &mut store);
            store.cancel_upload(upload).unwrap();
            store.remove(resource).unwrap();
            store.begin_callback();
            assert_eq!(store.reclaim(), 1);
            assert_eq!(store.usage(), baseline);
        });
    }
}

#[shoop_test]
fn refusal_does_not_advance_history_publish_windows_or_spend_reservations() {
    let mut store = store(6);
    let mut history = RollingHistory::prepare(
        &mut store,
        Config {
            history_pages: 2,
            retained_pages: 3,
            windows: 1,
        },
        Frame(12),
    )
    .unwrap();
    assert_no_heap(|| {
        store.begin_callback();
        assert_eq!(
            history.append(&mut store, Frame(12), &[0.; 3]),
            Err(Error::InvalidShape)
        );
        assert_eq!(
            history.append(&mut store, Frame(16), &[0.; 4]),
            Err(Error::InvalidFrame)
        );
        assert_eq!(store.work().copied_bytes, 0);
        assert_eq!(store.usage().reserved_pages, 5);
        append(&mut history, &mut store, 12);
        let before = store.usage();
        assert_eq!(
            history.append(&mut store, Frame(16), &[0.; 4]),
            Err(Error::Budget)
        );
        assert_eq!(store.usage(), before);
        assert_eq!(history.reserved, 4);
        assert_eq!(history.coverage(), Ok(Frame(12)..Frame(16)));
        store.begin_callback();
        for range in [
            Frame(8)..Frame(16),
            Frame(12)..Frame(20),
            Frame(13)..Frame(16),
            Frame(12)..Frame(15),
            Frame(16)..Frame(16),
        ] {
            assert_eq!(
                history.lease_window(&mut store, range),
                Err(Error::InvalidFrame)
            );
            assert_eq!(history.debt, 0);
            assert_eq!(store.work().validation_steps, 0);
        }
        store.charge_external(0, 1).unwrap();
        assert_eq!(
            history.lease_window(&mut store, Frame(12)..Frame(16)),
            Err(Error::Budget)
        );
        assert_eq!(history.free_windows.len(), 1);
        store.begin_callback();
        let window = history
            .lease_window(&mut store, Frame(12)..Frame(16))
            .unwrap();
        store.begin_callback();
        assert_eq!(
            history.lease_window(&mut store, Frame(12)..Frame(16)),
            Err(Error::Capacity)
        );
        assert_eq!(history.debt, 1);
        assert_eq!(store.work().validation_steps, 0);
        append(&mut history, &mut store, 16);
        let before = store.usage();
        assert_eq!(
            history.append(&mut store, Frame(20), &[0.; 4]),
            Err(Error::Budget)
        );
        assert_eq!(history.coverage(), Ok(Frame(12)..Frame(20)));
        assert_eq!(store.usage(), before);
        assert!(history.retained.iter().all(Option::is_none));
        assert_eq!(
            read::<4>(&history, &mut store, window),
            [12., 13., 14., 15.]
        );
        store.begin_callback();
        append(&mut history, &mut store, 20);
        assert_eq!(
            read::<4>(&history, &mut store, window),
            [12., 13., 14., 15.]
        );
        let mut output = [-1.; 4];
        assert_eq!(
            history.read_audio(&mut store, window, 5, &mut output),
            Err(Error::InvalidOffset)
        );
        assert_eq!(output, [-1.; 4]);
        assert_eq!(
            history.read_audio(&mut store, window, 4, &mut output),
            Ok(0)
        );
        history.release_window(window).unwrap();
        store.begin_callback();
        history.maintain(&mut store).unwrap();
        cleanup(&mut history, &mut store);
    });
}

#[shoop_test]
fn scopes_generations_and_frame_exhaustion_cannot_alias_or_partially_publish() {
    let mut store = store(12);
    let mut foreign_store = self::store(6);
    let mut history = RollingHistory::prepare(
        &mut store,
        Config {
            history_pages: 1,
            retained_pages: 1,
            windows: 1,
        },
        Frame(0),
    )
    .unwrap();
    let mut other = RollingHistory::prepare(
        &mut store,
        Config {
            history_pages: 1,
            retained_pages: 1,
            windows: 1,
        },
        Frame(0),
    )
    .unwrap();
    let mut overflow = RollingHistory::prepare(
        &mut store,
        Config {
            history_pages: 1,
            retained_pages: 1,
            windows: 1,
        },
        Frame(u64::MAX - 3),
    )
    .unwrap();
    history.windows[0].generation = u32::MAX;
    assert_no_heap(|| {
        store.begin_callback();
        append(&mut history, &mut store, 0);
        let window = history
            .lease_window(&mut store, Frame(0)..Frame(4))
            .unwrap();
        assert_eq!(window.generation, u32::MAX);
        assert_eq!(other.release_window(window), Err(Error::InvalidId));
        let before = foreign_store.usage();
        foreign_store.begin_callback();
        assert_eq!(
            history.append(&mut foreign_store, Frame(4), &[0.; 4]),
            Err(Error::InvalidId)
        );
        assert_eq!(history.maintain(&mut foreign_store), Err(Error::InvalidId));
        assert_eq!(
            history.read_audio(&mut foreign_store, window, 0, &mut [0.; 4]),
            Err(Error::InvalidId)
        );
        assert_eq!(foreign_store.usage(), before);
        let before = store.usage();
        assert_eq!(
            overflow.append(&mut store, Frame(u64::MAX - 3), &[0.; 4]),
            Err(Error::Exhausted)
        );
        assert_eq!(store.usage(), before);
        assert_eq!(
            overflow.coverage(),
            Ok(Frame(u64::MAX - 3)..Frame(u64::MAX - 3))
        );
        assert_eq!(read::<4>(&history, &mut store, window), [0., 1., 2., 3.]);
        history.release_window(window).unwrap();
        store.begin_callback();
        assert_eq!(history.maintain(&mut store), Ok(1));
        assert!(history.free_windows.is_empty());
        assert_eq!(
            history.lease_window(&mut store, Frame(0)..Frame(4)),
            Err(Error::Capacity)
        );
        cleanup(&mut history, &mut store);
        cleanup(&mut other, &mut store);
        cleanup(&mut overflow, &mut store);
        assert_eq!(store.usage().reserved_pages, 0);
        assert_eq!(store.usage().free_pages, 12);
    });
}

#[shoop_test]
fn seeded_window_lifetimes_match_original_input_through_wraps_and_deferred_reuse() {
    let mut store = store(7);
    let baseline = store.usage();
    let mut history = RollingHistory::prepare(
        &mut store,
        Config {
            history_pages: 3,
            retained_pages: 4,
            windows: 3,
        },
        Frame(0),
    )
    .unwrap();
    let mut owners = [None::<(WindowId, u64, usize)>; 3];
    let mut random = 0x5271_04abu32;
    let mut next = 0;
    let mut actions = [0usize; 4];
    let mut refused = 0;
    assert_no_heap(|| {
        for _ in 0..512 {
            random ^= random << 13;
            random ^= random >> 17;
            random ^= random << 5;
            store.begin_callback();
            let slot = (random as usize / 4) % owners.len();
            match random % 4 {
                0 => {
                    append(&mut history, &mut store, next);
                    next += 4;
                    actions[0] += 1;
                }
                1 if next > 0 && owners[slot].is_none() => {
                    let available = (next / 4).min(3) as usize;
                    let pages = 1 + random as usize / 16 % available;
                    let start = next - pages as u64 * 4;
                    match history.lease_window(&mut store, Frame(start)..Frame(next)) {
                        Ok(window) => {
                            owners[slot] = Some((window, start, pages * 4));
                            actions[1] += 1;
                        }
                        Err(Error::Capacity) => refused += 1,
                        other => panic!("unexpected admission: {other:?}"),
                    }
                }
                2 => {
                    if let Some((window, _, _)) = owners[slot].take() {
                        history.release_window(window).unwrap();
                        actions[2] += 1;
                    }
                }
                _ => {
                    assert!(history.maintain(&mut store).unwrap() <= 1);
                    actions[3] += 1;
                }
            }
            assert_eq!(
                history.coverage(),
                Ok(Frame(next.saturating_sub(12))..Frame(next))
            );
            for &(window, start, frames) in owners.iter().flatten() {
                let mut output = [0.; 12];
                let mut offset = 0;
                while offset < frames {
                    let count = history
                        .read_audio(
                            &mut store,
                            window,
                            offset as u64,
                            &mut output[offset..frames],
                        )
                        .unwrap();
                    assert!(count > 0);
                    offset += count;
                }
                for (index, sample) in output[..frames].iter().enumerate() {
                    assert_eq!(*sample, (start + index as u64) as f32);
                }
            }
            assert!(store.work().copied_bytes <= 16);
            assert!(store.work().validation_steps <= 1);
            assert!(store.usage().free_pages >= store.usage().reserved_pages);
        }
        assert!(actions.iter().all(|&count| count > 10));
        assert!(refused > 0);
        for (window, _, _) in owners.into_iter().flatten() {
            history.release_window(window).unwrap();
        }
        for _ in 0..9 {
            store.begin_callback();
            assert!(history.maintain(&mut store).unwrap() <= 1);
        }
        assert_eq!(history.debt, 0);
        cleanup(&mut history, &mut store);
        assert_eq!(store.usage(), baseline);
    });
}
