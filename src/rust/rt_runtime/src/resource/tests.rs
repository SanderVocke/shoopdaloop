use super::*;
use crate::{Budgets, Config};

#[shoop_wasm_test_support::shoop_test]
fn exhausted_resource_version_and_lease_generations_are_never_reused() {
    let config = Config {
        resource_slots: 2,
        resource_pages: 2,
        page_bytes: 4,
        budgets: Budgets {
            transfer_bytes: 4,
            validation_steps: 1,
            ..Budgets::default()
        },
        ..Config::default()
    }
    .prepare()
    .unwrap();
    let mut store = ResourceStore::new(config, crate::Quantum::new(1).unwrap()).unwrap();
    store.resources[0].generation = u32::MAX;
    store.versions[0].generation = u32::MAX;
    store.snapshots[0].generation = u32::MAX;
    let resource = store.create().unwrap();
    assert_eq!(resource.generation(), u32::MAX);
    let description = Description {
        format: Format::Audio { channels: 1 },
        frames: 0,
        bytes: 0,
    };
    let version = store
        .begin_upload(resource, Revision(0), description)
        .unwrap();
    store.finish_upload(version).unwrap();
    store.commit(version).unwrap();
    let snapshot = store.snapshot(resource, Frame(0)).unwrap();
    store.remove(resource).unwrap();
    store.release_snapshot(snapshot.id).unwrap();
    store.begin_callback();
    assert_eq!(store.reclaim(), 1);
    assert_eq!(store.free_resources.len(), 1);
    assert_eq!(store.usage().free_versions, 1);
    assert_eq!(store.usage().free_snapshots, 1);
    assert_eq!(store.revision(resource), Err(Error::InvalidId));
    assert_eq!(store.finish_upload(version), Err(Error::InvalidId));
    assert_eq!(store.currentness(snapshot.id), Err(Error::InvalidId));
    let next = store.create().unwrap();
    assert_eq!(next.slot(), 1);
    assert_eq!(store.create(), Err(Error::Capacity));
    let version = store.begin_upload(next, Revision(0), description).unwrap();
    assert_eq!(version.slot, 1);
    store.finish_upload(version).unwrap();
    store.commit(version).unwrap();
    assert_eq!(store.snapshot(next, Frame(0)).unwrap().id.slot, 1);
}

#[shoop_wasm_test_support::shoop_test]
fn reserved_snapshot_credit_survives_saturation_and_fences_reuse() {
    let mut store = ResourceStore::new(
        Config {
            resource_slots: 3,
            ..Config::default()
        }
        .prepare()
        .unwrap(),
        crate::Quantum::new(1).unwrap(),
    )
    .unwrap();
    let resource = store.create().unwrap();
    let upload = store
        .begin_upload(
            resource,
            Revision(0),
            Description {
                format: Format::Audio { channels: 1 },
                frames: 0,
                bytes: 0,
            },
        )
        .unwrap();
    store.finish_upload(upload).unwrap();
    store.commit(upload).unwrap();
    shoop_wasm_test_support::heap::assert_no_heap(|| {
        let first = store.reserve_snapshot().unwrap();
        let second = store.reserve_snapshot().unwrap();
        let ordinary = store.snapshot(resource, Frame(0)).unwrap();
        assert_eq!(store.snapshot(resource, Frame(0)), Err(Error::Capacity));
        assert_eq!(store.reserve_snapshot(), Err(Error::Capacity));
        let snapshot = store.snapshot_reserved(first, resource, Frame(16)).unwrap();
        assert_eq!(snapshot.cut_frame, Frame(16));
        assert_eq!(
            store.cancel_snapshot_reservation(first),
            Err(Error::InvalidId)
        );
        store.cancel_snapshot_reservation(second).unwrap();
        let next = store.reserve_snapshot().unwrap();
        assert_ne!(second, next);
        assert_eq!(
            store.snapshot_reserved(second, resource, Frame(32)),
            Err(Error::InvalidId)
        );
        store.cancel_snapshot_reservation(next).unwrap();
        store.release_snapshot(snapshot.id).unwrap();
        store.release_snapshot(ordinary.id).unwrap();
        assert_eq!(store.usage().free_snapshots, 3);
    });
}
