use super::*;
use crate::{
    resource::{Description, Format},
    Budgets, Config, Revision,
};
use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};

fn store() -> ResourceStore {
    ResourceStore::new(
        Config {
            resource_pages: 3,
            page_bytes: 16,
            resource_slots: 8,
            budgets: Budgets {
                transfer_bytes: 16,
                validation_steps: 1,
                ..Budgets::default()
            },
            ..Config::default()
        }
        .prepare()
        .unwrap(),
        crate::Quantum::new(1).unwrap(),
    )
    .unwrap()
}

fn samples(pages: &SharedPages, store: &ResourceStore, lease: LeaseId) -> [f32; 4] {
    let bytes = pages.bytes(store, lease).unwrap();
    core::array::from_fn(|index| {
        f32::from_le_bytes(bytes[index * 4..index * 4 + 4].try_into().unwrap())
    })
}

fn read(pages: &SharedPages, store: &ResourceStore, leases: [LeaseId; 2]) -> [f32; 8] {
    let first = samples(pages, store, leases[0]);
    let second = samples(pages, store, leases[1]);
    core::array::from_fn(|index| {
        if index < 4 {
            first[index]
        } else {
            second[index - 4]
        }
    })
}

#[shoop_test]
fn readers_share_pages_and_one_page_fork_preserves_all_other_owners() {
    for order in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        let mut store = store();
        let baseline = store.usage();
        let mut pages = SharedPages::prepare(&store, 8).unwrap();
        assert_no_heap(|| {
            store.begin_callback();
            let first = pages.insert_audio(&mut store, &[0., 1., 2., 3.]).unwrap();
            store.begin_callback();
            let second = pages.insert_audio(&mut store, &[4., 5., 6., 7.]).unwrap();
            let original = [first, second];
            store.begin_callback();
            let mut a = [pages.retain(first).unwrap(), pages.retain(second).unwrap()];
            let b = [pages.retain(first).unwrap(), pages.retain(second).unwrap()];
            assert_eq!(store.usage().free_pages, 1);
            assert_eq!(store.work().copied_bytes, 0);
            assert_eq!(pages.references[pages.page(first).unwrap()], 3);
            assert_eq!(pages.references[pages.page(second).unwrap()], 3);
            let fork = pages
                .fork_with_patch(&mut store, a[0], 2, &[20., 30.])
                .unwrap();
            assert_ne!(pages.page(fork).unwrap(), pages.page(a[0]).unwrap());
            pages.release(&mut store, a[0]).unwrap();
            a[0] = fork;
            assert_eq!(pages.page(a[1]), pages.page(b[1]));
            assert_eq!(store.work().copied_bytes, 16);
            assert_eq!(store.usage().free_pages, 0);
            store.begin_callback();
            assert_eq!(
                pages.fork_with_patch(&mut store, b[1], 0, &[40.]),
                Err(Error::Capacity)
            );
            assert_eq!(store.work().copied_bytes, 0);
            let expected = [
                [0., 1., 2., 3., 4., 5., 6., 7.],
                [0., 1., 20., 30., 4., 5., 6., 7.],
                [0., 1., 2., 3., 4., 5., 6., 7.],
            ];
            let mut owners = [Some(original), Some(a), Some(b)];
            for owner in order {
                for (index, leases) in owners.iter().enumerate() {
                    if let Some(leases) = leases {
                        assert_eq!(read(&pages, &store, *leases), expected[index]);
                    }
                }
                for lease in owners[owner].take().unwrap() {
                    pages.release(&mut store, lease).unwrap();
                }
                store.begin_callback();
                assert!(store.reclaim() <= 1);
            }
            for _ in 0..3 {
                store.begin_callback();
                assert!(store.reclaim() <= 1);
            }
            assert_eq!(store.usage(), baseline);
            assert_eq!(pages.free_leases.len(), 8);
            assert!(pages.references.iter().all(|&count| count == 0));
        });
    }
}

#[shoop_test]
fn invalid_patches_and_exhausted_copy_page_or_lease_credits_do_not_publish() {
    let mut store = store();
    let baseline = store.usage();
    let mut pages = SharedPages::prepare(&store, 2).unwrap();
    assert_no_heap(|| {
        store.begin_callback();
        let source = pages.insert_audio(&mut store, &[0., 1., 2., 3.]).unwrap();
        let before = store.usage();
        assert_eq!(
            pages.fork_with_patch(&mut store, source, 1, &[9.]),
            Err(Error::Budget)
        );
        assert_eq!(store.usage(), before);
        assert_eq!(pages.free_leases.len(), 1);
        store.begin_callback();
        for (offset, patch, error) in [
            (0, &[][..], Error::InvalidShape),
            (4, &[9.][..], Error::InvalidOffset),
            (3, &[9., 8.][..], Error::InvalidOffset),
            (usize::MAX, &[9.][..], Error::InvalidOffset),
        ] {
            assert_eq!(
                pages.fork_with_patch(&mut store, source, offset, patch),
                Err(error)
            );
            assert_eq!(store.usage(), before);
            assert_eq!(pages.free_leases.len(), 1);
            assert_eq!(store.work().copied_bytes, 0);
        }
        let other = pages.retain(source).unwrap();
        assert_eq!(pages.retain(source), Err(Error::Capacity));
        assert_eq!(
            pages.fork_with_patch(&mut store, source, 1, &[9.]),
            Err(Error::Capacity)
        );
        assert_eq!(store.usage(), before);
        assert_eq!(store.work().copied_bytes, 0);
        assert_eq!(samples(&pages, &store, source), [0., 1., 2., 3.]);
        assert_eq!(samples(&pages, &store, other), [0., 1., 2., 3.]);
        pages.release(&mut store, other).unwrap();

        let resource = store.create().unwrap();
        let reservation = store
            .begin_upload(
                resource,
                Revision(0),
                Description {
                    format: Format::Audio { channels: 1 },
                    frames: 8,
                    bytes: 32,
                },
            )
            .unwrap();
        let reserved = store.usage();
        assert_eq!(reserved.reserved_pages, 2);
        assert_eq!(
            pages.fork_with_patch(&mut store, source, 1, &[9.]),
            Err(Error::Capacity)
        );
        assert_eq!(store.usage(), reserved);
        assert_eq!(store.work().copied_bytes, 0);
        store.cancel_upload(reservation).unwrap();
        let fork = pages.fork_with_patch(&mut store, source, 1, &[9.]).unwrap();
        assert_eq!(samples(&pages, &store, fork), [0., 9., 2., 3.]);
        assert_eq!(samples(&pages, &store, source), [0., 1., 2., 3.]);
        assert_eq!(store.work().copied_bytes, 16);
        pages.release(&mut store, fork).unwrap();
        pages.release(&mut store, source).unwrap();
        store.remove(resource).unwrap();
        for _ in 0..3 {
            store.begin_callback();
            assert!(store.reclaim() <= 1);
        }
        assert_eq!(store.usage(), baseline);
        assert_eq!(pages.free_leases.len(), 2);
    });
}

#[shoop_test]
fn stale_or_foreign_leases_cannot_release_a_peer_or_read_reused_storage() {
    let mut store = store();
    let mut foreign_store = self::store();
    let mut pages = SharedPages::prepare(&store, 2).unwrap();
    let mut foreign_pool = SharedPages::prepare(&store, 2).unwrap();
    assert_no_heap(|| {
        store.begin_callback();
        let original = pages.insert_audio(&mut store, &[0., 1., 2., 3.]).unwrap();
        let physical = pages.page(original).unwrap();
        let peer = pages.retain(original).unwrap();
        pages.release(&mut store, original).unwrap();
        assert_eq!(pages.bytes(&store, original), Err(Error::InvalidId));
        assert_eq!(pages.retain(original), Err(Error::InvalidId));
        assert_eq!(pages.release(&mut store, original), Err(Error::InvalidId));
        assert_eq!(pages.references[physical], 1);
        let newer = pages.retain(peer).unwrap();
        assert_eq!(newer.slot, original.slot);
        assert_ne!(newer.generation, original.generation);
        assert_eq!(pages.bytes(&store, original), Err(Error::InvalidId));
        assert_eq!(pages.bytes(&foreign_store, peer), Err(Error::InvalidId));
        assert_eq!(
            pages.release(&mut foreign_store, peer),
            Err(Error::InvalidId)
        );
        assert_eq!(foreign_pool.retain(peer), Err(Error::InvalidId));
        assert_eq!(
            foreign_pool.release(&mut store, peer),
            Err(Error::InvalidId)
        );
        assert_eq!(pages.references[physical], 2);
        pages.release(&mut store, peer).unwrap();
        assert_eq!(samples(&pages, &store, newer), [0., 1., 2., 3.]);
        pages.release(&mut store, newer).unwrap();
        store.begin_callback();
        assert_eq!(store.reclaim(), 1);
        let reused = pages
            .insert_audio(&mut store, &[10., 11., 12., 13.])
            .unwrap();
        assert_eq!(pages.page(reused).unwrap(), physical);
        assert_eq!(pages.bytes(&store, original), Err(Error::InvalidId));
        assert_eq!(pages.bytes(&store, peer), Err(Error::InvalidId));
        assert_eq!(pages.bytes(&store, newer), Err(Error::InvalidId));
        assert_eq!(samples(&pages, &store, reused), [10., 11., 12., 13.]);
        pages.release(&mut store, reused).unwrap();
    });
}

#[shoop_test]
fn reference_and_lease_generations_exhaust_without_wrapping() {
    let mut store = store();
    let mut pages = SharedPages::prepare(&store, 1).unwrap();
    pages.leases[0].generation = u32::MAX;
    assert_no_heap(|| {
        store.begin_callback();
        let source = pages.insert_audio(&mut store, &[0., 1., 2., 3.]).unwrap();
        let physical = pages.page(source).unwrap();
        pages.references[physical] = usize::MAX;
        assert_eq!(pages.retain(source), Err(Error::Exhausted));
        assert_eq!(pages.references[physical], usize::MAX);
        pages.references[physical] = 1;
        pages.release(&mut store, source).unwrap();
        assert_eq!(pages.bytes(&store, source), Err(Error::InvalidId));
        assert!(pages.free_leases.is_empty());
        store.begin_callback();
        assert_eq!(store.reclaim(), 1);
        assert_eq!(
            pages.insert_audio(&mut store, &[4., 5., 6., 7.]),
            Err(Error::Capacity)
        );
        assert_eq!(store.work().copied_bytes, 0);
        assert_eq!(store.usage().free_pages, 3);
    });
}

#[shoop_test]
fn exclusive_reuse_invalidates_old_handles_and_reclamation_obeys_the_step_budget() {
    let mut store = store();
    let baseline = store.usage();
    let mut pages = SharedPages::prepare(&store, 2).unwrap();
    assert_no_heap(|| {
        store.begin_callback();
        let original = pages.insert_audio(&mut store, &[0., 1., 2., 3.]).unwrap();
        let peer = pages.retain(original).unwrap();
        let physical = pages.page(original).unwrap();
        assert_eq!(
            pages.replace_audio(&mut store, original, &[4.; 4]),
            Err(Error::Busy)
        );
        assert_eq!(samples(&pages, &store, peer), [0., 1., 2., 3.]);
        pages.release(&mut store, peer).unwrap();
        assert_eq!(
            pages.replace_audio(&mut store, original, &[4.; 4]),
            Err(Error::Budget)
        );
        assert_eq!(samples(&pages, &store, original), [0., 1., 2., 3.]);
        store.begin_callback();
        let reused = pages
            .replace_audio(&mut store, original, &[4., 5., 6., 7.])
            .unwrap();
        assert_eq!(pages.page(reused).unwrap(), physical);
        assert_eq!(pages.bytes(&store, original), Err(Error::InvalidId));
        assert_eq!(samples(&pages, &store, reused), [4., 5., 6., 7.]);
        assert_eq!(store.work().copied_bytes, 16);
        pages.leases[reused.slot as usize].generation = u32::MAX;
        let last = LeaseId {
            generation: u32::MAX,
            ..reused
        };
        store.begin_callback();
        assert_eq!(
            pages.replace_audio(&mut store, last, &[8.; 4]),
            Err(Error::Exhausted)
        );
        assert_eq!(store.work().copied_bytes, 0);
        assert_eq!(samples(&pages, &store, last), [4., 5., 6., 7.]);
        store.charge_external(0, 1).unwrap();
        assert_eq!(pages.reclaim_lease(&mut store, last), Err(Error::Budget));
        assert_eq!(samples(&pages, &store, last), [4., 5., 6., 7.]);
        store.begin_callback();
        assert_eq!(pages.reclaim_lease(&mut store, last), Ok(true));
        assert_eq!(store.work().validation_steps, 1);
        assert_eq!(store.work().reclaimed_pages, 1);
        assert_eq!(pages.bytes(&store, last), Err(Error::InvalidId));
        assert_eq!(store.usage(), baseline);
    });
}
