use super::*;
use shoop_wasm_test_support::heap as support;
use shoop_wasm_test_support::shoop_test;
#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
#[shoop_test]
fn exhausted_media_and_export_generations_retire_instead_of_retargeting() {
    let mut store = ResourceStore::new(
        rt_runtime::Config {
            resource_slots: 2,
            resource_pages: 2,
            ..rt_runtime::Config::default()
        }
        .prepare()
        .unwrap(),
        rt_runtime::Quantum::new(1).unwrap(),
    )
    .unwrap();
    let owner = LoopId::from_parts(0, 1).unwrap();
    let mut ledger = Ledger::prepare(1, 1, 1, 1).unwrap();
    ledger.bind(owner).unwrap();
    support::assert_no_heap(|| {
        let old = ledger
            .adopt(
                &mut store.io(),
                Input {
                    auxiliary: None,
                    description: Description {
                        auxiliary_channels: 0,
                        owner,
                        revision: Revision(0),
                        frames: 1,
                        channels: 0,
                        midi: false,
                    },
                    audio: None,
                    midi: None,
                },
            )
            .unwrap();
        ledger.media[0].generation = u32::MAX;
        let media = MediaId {
            generation: u32::MAX,
            ..old
        };
        assert_eq!(ledger.view(old), Err(Error::InvalidId));
        let old_export = ledger.export(media, Frame(0)).unwrap();
        ledger.exports[0].generation = u32::MAX;
        let export = ExportId {
            generation: u32::MAX,
            ..old_export
        };
        assert_eq!(
            ledger.release_export(&mut store.io(), old_export),
            Err(Error::InvalidId)
        );
        let old_pin = ledger.pin(media).unwrap();
        ledger.pins[0].generation = u32::MAX;
        let pin = PinId {
            generation: u32::MAX,
            ..old_pin
        };
        assert_eq!(
            ledger.release_pin(&mut store.io(), old_pin),
            Err(Error::InvalidId)
        );
        ledger.retire(&mut store.io(), media).unwrap();
        assert!(ledger.exported(export).is_ok());
        ledger.release_export(&mut store.io(), export).unwrap();
        assert_eq!(ledger.pinned(pin).unwrap(), media);
        ledger.release_pin(&mut store.io(), pin).unwrap();
        assert_eq!(
            ledger.usage(),
            Usage {
                free_media: 0,
                free_exports: 0,
                free_pins: 0,
                references: 0
            }
        );
        assert_eq!(ledger.media[0].generation, 0);
        assert_eq!(ledger.exports[0].generation, 0);
        assert_eq!(ledger.pins[0].generation, 0);
        assert_eq!(ledger.pinned(pin), Err(Error::InvalidId));
        assert_eq!(ledger.view(media), Err(Error::InvalidId));
        assert_eq!(ledger.exported(export), Err(Error::InvalidId));
        assert_eq!(
            ledger.reserve(Description {
                auxiliary_channels: 0,
                owner,
                revision: Revision(1),
                frames: 0,
                channels: 0,
                midi: false
            }),
            Err(Error::Capacity)
        );
    });
}

#[shoop_test]
fn two_audio_roles_require_independent_leases_and_release_each_exactly_once() {
    let mut store = ResourceStore::new(
        rt_runtime::Config {
            resource_slots: 4,
            ..rt_runtime::Config::default()
        }
        .prepare()
        .unwrap(),
        rt_runtime::Quantum::new(1).unwrap(),
    )
    .unwrap();
    let resource = store.create().unwrap();
    let upload = store
        .begin_upload(
            resource,
            Revision(0),
            rt_runtime::resource::Description {
                format: Format::Audio { channels: 1 },
                frames: 0,
                bytes: 0,
            },
        )
        .unwrap();
    store.finish_upload(upload).unwrap();
    store.commit(upload).unwrap();
    let snapshot = store.snapshot(resource, Frame(0)).unwrap();
    let source = Source {
        snapshot,
        index: store.read_index(snapshot.id).unwrap(),
    };
    let owner = LoopId::from_parts(0, 1).unwrap();
    let mut ledger = Ledger::prepare(1, 2, 1, 1).unwrap();
    ledger.bind(owner).unwrap();
    support::assert_no_heap(|| {
        let before = ledger.usage();
        let rejected = ledger
            .adopt(
                &mut store.io(),
                Input {
                    description: Description {
                        owner,
                        revision: Revision(0),
                        frames: 0,
                        channels: 1,
                        auxiliary_channels: 1,
                        midi: false,
                    },
                    audio: Some(source),
                    auxiliary: Some(source),
                    midi: None,
                },
            )
            .unwrap_err();
        assert_eq!(rejected.error, Error::InvalidState);
        assert_eq!(ledger.usage(), before);
        let retained = store.retain_snapshot(snapshot.id).unwrap();
        let auxiliary = Source {
            snapshot: retained,
            index: store.read_index(retained.id).unwrap(),
        };
        let media = ledger
            .adopt(
                &mut store.io(),
                Input {
                    auxiliary: Some(auxiliary),
                    ..rejected.input
                },
            )
            .unwrap();
        ledger.retire(&mut store.io(), media).unwrap();
        assert_eq!(store.currentness(snapshot.id), Err(Error::InvalidId));
        assert_eq!(store.currentness(retained.id), Err(Error::InvalidId));
        assert_eq!(store.usage().free_snapshots, 4);
        assert_eq!(ledger.usage(), before);
    });
}
