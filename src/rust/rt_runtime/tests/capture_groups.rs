mod support;
use rt_runtime::{
    buffer::MidiBuffer, midi_state::MidiState, resource::*, Budgets, Config, Error, Frame, Revision,
};
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;
#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
fn config() -> rt_runtime::PreparedConfig {
    Config {
        max_frames: 8,
        resource_slots: 8,
        resource_pages: 64,
        page_bytes: 16,
        midi_events_per_port: 8,
        midi_bytes_per_port: 128,
        budgets: Budgets {
            transfer_bytes: 16,
            validation_steps: 1,
            ..Budgets::default()
        },
        ..Config::default()
    }
    .prepare()
    .unwrap()
}
fn store(midi_versions: usize) -> ResourceStore {
    ResourceStore::with_capture_groups(
        config(),
        Some(MidiIndexConfig {
            versions: midi_versions,
            checkpoints_per_version: 4,
            replay_events: 8,
        }),
        CaptureGroupConfig {
            groups: 2,
            members_per_group: 2,
            staging_bytes_per_group: 448,
        },
    )
    .unwrap()
}
fn audio(resource: rt_runtime::ResourceId) -> CaptureMember {
    CaptureMember {
        resource,
        format: Format::Audio { channels: 2 },
        midi_events: 0,
        midi_bytes: 0,
    }
}
fn midi(resource: rt_runtime::ResourceId) -> CaptureMember {
    CaptureMember {
        resource,
        format: Format::Midi,
        midi_events: 8,
        midi_bytes: 128,
    }
}
fn empty(store: &mut ResourceStore, resource: rt_runtime::ResourceId, format: Format) -> UploadId {
    let id = store
        .begin_upload(
            resource,
            store.revision(resource).unwrap(),
            Description {
                format,
                frames: 0,
                bytes: 0,
            },
        )
        .unwrap();
    store.finish_upload(id).unwrap();
    id
}
fn reclaim(store: &mut ResourceStore) {
    for _ in 0..128 {
        store.begin_callback();
        store.reclaim();
        if store.usage().retired_versions == 0 && store.usage().retired_pages == 0 {
            return;
        }
    }
    panic!("retirement did not finish");
}
#[shoop_test]
fn group_claims_are_atomic_and_reject_legacy_mutation_until_released() {
    let mut store = store(2);
    assert_no_heap(|| {
        let a = store.create().unwrap();
        let b = store.create().unwrap();
        let candidate = empty(&mut store, a, audio(a).format);
        let baseline = store.capture_group_usage();
        assert_eq!(
            store.bind_capture_group(&[audio(a), audio(a)]),
            Err(Error::InvalidShape)
        );
        assert_eq!(store.capture_group_usage(), baseline);
        let group = store.bind_capture_group(&[audio(a), midi(b)]).unwrap();
        assert_eq!(
            store.bind_capture_group(&[audio(a)]),
            Err(Error::InvalidState)
        );
        assert_eq!(store.remove(a), Err(Error::InvalidState));
        assert_eq!(store.commit(candidate), Err(Error::InvalidState));
        assert_eq!(
            store.begin_upload(
                a,
                Revision(0),
                Description {
                    format: audio(a).format,
                    frames: 0,
                    bytes: 0
                }
            ),
            Err(Error::InvalidState)
        );
        assert_eq!(
            store.begin_capture(a, Revision(0), audio(a).format, Frame(0)),
            Err(Error::InvalidState)
        );
        store.release_capture_group(group).unwrap();
        assert_eq!(store.capture_group_info(group), Err(Error::InvalidId));
        store.commit(candidate).unwrap();
        let capture = store
            .begin_capture(a, Revision(1), audio(a).format, Frame(0))
            .unwrap();
        assert_eq!(
            store.bind_capture_group(&[audio(a)]),
            Err(Error::InvalidState)
        );
        store.stop_capture(capture).unwrap();
        let next = store.bind_capture_group(&[audio(a)]).unwrap();
        assert_ne!(next, group);
        store.release_capture_group(next).unwrap();
        store.remove(a).unwrap();
        store.remove(b).unwrap();
        reclaim(&mut store);
        assert_eq!(store.usage().free_versions, 8);
        assert_eq!(store.capture_group_usage(), baseline);
    });
}
#[shoop_test]
fn promised_activation_and_empty_freeze_survive_all_unrelated_credit_saturation() {
    let mut store = store(1);
    let mut initial = MidiState::default();
    initial.apply_packet(&[0x90, 61, 77], 3).unwrap();
    let input = MidiBuffer::new(8, 128).unwrap();
    assert_no_heap(|| {
        let a = store.create().unwrap();
        let b = store.create().unwrap();
        let other = store.create().unwrap();
        let old_a = empty(&mut store, a, audio(a).format);
        let old_b = empty(&mut store, b, Format::Midi);
        store.commit(old_a).unwrap();
        store.commit(old_b).unwrap();
        let group = store.bind_capture_group(&[audio(a), midi(b)]).unwrap();
        let take = store.prepare_capture_take(group, &[initial]).unwrap();
        assert_eq!(store.info(a).unwrap().revision, Revision(1));
        let mut held = [None; 6];
        for slot in &mut held {
            *slot = Some(store.snapshot(a, Frame(0)).unwrap());
        }
        let mut uploads = [None; 4];
        for (i, slot) in uploads.iter_mut().enumerate() {
            *slot = Some(
                store
                    .begin_upload(
                        other,
                        Revision(0),
                        Description {
                            format: Format::Audio { channels: 1 },
                            frames: if i == 0 { 256 } else { 0 },
                            bytes: if i == 0 { 1024 } else { 0 },
                        },
                    )
                    .unwrap(),
            );
        }
        store.begin_callback();
        store
            .write_upload(uploads[0].unwrap(), 0, &[0; 16])
            .unwrap();
        assert_eq!(store.copy_remaining(), 0);
        assert_eq!(store.usage().free_versions, 0);
        assert_eq!(store.usage().free_snapshots, 0);
        assert_eq!(store.usage().free_pages, store.usage().reserved_pages);
        let work = store.work();
        store.activate_capture_take(take, Frame(17)).unwrap();
        assert_eq!(
            store.activate_capture_take(take, Frame(17)),
            Err(Error::InvalidState)
        );
        let raw_a = store.active_capture(a).unwrap();
        let raw_b = store.active_capture(b).unwrap();
        assert_eq!(store.stop_capture(raw_a), Err(Error::InvalidState));
        assert_eq!(
            store.render_io().capture_audio(raw_a, Frame(17), &[1., 2.]),
            Err(Error::InvalidState)
        );
        assert_eq!(
            store.render_io().capture_midi(raw_b, Frame(17), 1, &input),
            Err(Error::InvalidState)
        );
        assert_eq!(store.release_capture_group(group), Err(Error::InvalidState));
        let mut output = [None; 2];
        assert_eq!(
            store.freeze_capture_take(take, Frame(18), None, &mut output),
            Err(Error::InvalidFrame)
        );
        assert_eq!(output, [None; 2]);
        assert_eq!(
            store.freeze_capture_take(take, Frame(17), None, &mut output[..1]),
            Err(Error::InvalidShape)
        );
        store
            .freeze_capture_take(take, Frame(17), None, &mut output)
            .unwrap();
        assert_eq!(
            store.freeze_capture_take(take, Frame(17), None, &mut [None; 2]),
            Err(Error::InvalidState)
        );
        assert_eq!(store.work(), work);
        for snapshot in output.iter().flatten() {
            assert_eq!(snapshot.description.frames, 0);
            assert_eq!(snapshot.cut_frame, Frame(17));
            assert_eq!(snapshot.capture.unwrap().active, false);
        }
        let index = store.read_index(output[1].unwrap().id).unwrap();
        assert_eq!(
            store.render_io().midi_seek(index, 0).unwrap().state,
            initial
        );
        assert_eq!(
            store.currentness(held[0].unwrap().id),
            Ok(Currentness::Stale)
        );
        store.release_capture_group(group).unwrap();
        for snapshot in output.into_iter().chain(held).flatten() {
            store.release_snapshot(snapshot.id).unwrap();
        }
        for upload in uploads.into_iter().flatten() {
            store.cancel_upload(upload).unwrap();
        }
        for resource in [a, b, other] {
            store.remove(resource).unwrap();
        }
        reclaim(&mut store);
        assert_eq!(store.usage().free_versions, 8);
        assert_eq!(store.usage().free_snapshots, 8);
        assert_eq!(store.usage().free_pages, 64);
        assert_eq!(store.usage().reserved_pages, 0);
        assert_eq!(store.midi_index_usage().free_versions, 1);
    });
}
#[shoop_test]
fn group_preparation_refusal_and_cancellation_preserve_every_previous_member() {
    let mut store = store(1);
    assert_no_heap(|| {
        let a = store.create().unwrap();
        let b = store.create().unwrap();
        let group = store.bind_capture_group(&[midi(a), midi(b)]).unwrap();
        let before = store.usage();
        assert_eq!(
            store.prepare_capture_take(group, &[MidiState::default(); 2]),
            Err(Error::Capacity)
        );
        assert_eq!(store.usage(), before);
        assert_eq!(store.midi_index_usage().free_versions, 1);
        assert_eq!(store.revision(a), Ok(Revision(0)));
        assert_eq!(store.revision(b), Ok(Revision(0)));
        store.release_capture_group(group).unwrap();
        let group = store.bind_capture_group(&[audio(a), midi(b)]).unwrap();
        assert_eq!(
            store.prepare_capture_take(group, &[]),
            Err(Error::InvalidShape)
        );
        assert_eq!(store.usage(), before);
        for _ in 0..32 {
            let take = store
                .prepare_capture_take(group, &[MidiState::default()])
                .unwrap();
            assert_eq!(
                store.capture_group_info(group).unwrap().phase,
                CapturePhase::Prepared
            );
            assert_eq!(
                store.prepare_capture_take(group, &[MidiState::default()]),
                Err(Error::InvalidState)
            );
            assert_eq!(store.release_capture_group(group), Err(Error::InvalidState));
            store.cancel_capture_take(take).unwrap();
            assert_eq!(store.cancel_capture_take(take), Err(Error::InvalidState));
            reclaim(&mut store);
            assert_eq!(store.usage(), before);
            assert_eq!(store.midi_index_usage().free_versions, 1);
        }
        store.release_capture_group(group).unwrap();
    });
}
#[shoop_test]
fn scoped_groups_and_occupied_freeze_outputs_cannot_retarget_or_lose_owners() {
    let mut a = store(1);
    let mut b = store(1);
    assert_no_heap(|| {
        let ar = a.create().unwrap();
        let br = b.create().unwrap();
        let ga = a.bind_capture_group(&[audio(ar)]).unwrap();
        let gb = b.bind_capture_group(&[audio(br)]).unwrap();
        assert_ne!(ga, gb);
        let take = a.prepare_capture_take(ga, &[]).unwrap();
        assert_eq!(b.prepare_capture_take(ga, &[]), Err(Error::InvalidId));
        assert_eq!(b.cancel_capture_take(take), Err(Error::InvalidId));
        assert_eq!(
            b.activate_capture_take(take, Frame(0)),
            Err(Error::InvalidId)
        );
        assert_eq!(
            b.freeze_capture_take(take, Frame(0), None, &mut [None]),
            Err(Error::InvalidId)
        );
        a.activate_capture_take(take, Frame(u64::MAX)).unwrap();
        let mut first = [None];
        a.freeze_capture_take(take, Frame(u64::MAX), None, &mut first)
            .unwrap();
        let next = a.prepare_capture_take(ga, &[]).unwrap();
        assert_ne!(next, take);
        assert_eq!(a.cancel_capture_take(take), Err(Error::InvalidId));
        assert_eq!(
            a.activate_capture_take(take, Frame(u64::MAX)),
            Err(Error::InvalidId)
        );
        a.activate_capture_take(next, Frame(u64::MAX)).unwrap();
        assert_eq!(
            a.freeze_capture_take(take, Frame(u64::MAX), None, &mut [None]),
            Err(Error::InvalidId)
        );
        let saved = first;
        assert_eq!(
            a.freeze_capture_take(next, Frame(u64::MAX), None, &mut first),
            Err(Error::InvalidState)
        );
        assert_eq!(first, saved);
        let mut second = [None];
        a.freeze_capture_take(next, Frame(u64::MAX), Some(Error::Capacity), &mut second)
            .unwrap();
        assert_eq!(
            second[0].unwrap().capture.unwrap().incomplete,
            Some(Error::Capacity)
        );
        a.release_snapshot(first[0].unwrap().id).unwrap();
        a.release_snapshot(second[0].unwrap().id).unwrap();
        a.release_capture_group(ga).unwrap();
        b.release_capture_group(gb).unwrap();
        assert_eq!(a.release_capture_group(ga), Err(Error::InvalidId));
        a.remove(ar).unwrap();
        b.remove(br).unwrap();
        reclaim(&mut a);
        reclaim(&mut b);
        assert_eq!(a.usage().free_snapshots, 8);
        assert_eq!(a.usage().free_versions, 8);
    });
}
#[shoop_test]
fn group_storage_shape_and_legacy_opt_in_are_explicit() {
    let cfg = config();
    assert!(matches!(
        ResourceStore::with_capture_groups(
            cfg,
            None,
            CaptureGroupConfig {
                groups: 0,
                members_per_group: 1,
                staging_bytes_per_group: 0
            }
        ),
        Err(Error::InvalidConfig)
    ));
    assert!(matches!(
        ResourceStore::with_capture_groups(
            cfg,
            None,
            CaptureGroupConfig {
                groups: 1,
                members_per_group: 1,
                staging_bytes_per_group: usize::MAX
            }
        ),
        Err(Error::Capacity)
    ));
    let mut legacy = ResourceStore::new(cfg).unwrap();
    let mut no_staging = ResourceStore::with_capture_groups(
        cfg,
        None,
        CaptureGroupConfig {
            groups: 1,
            members_per_group: 1,
            staging_bytes_per_group: 0,
        },
    )
    .unwrap();
    assert_no_heap(|| {
        let resource = legacy.create().unwrap();
        assert_eq!(legacy.capture_group_usage(), CaptureGroupUsage::default());
        assert_eq!(
            legacy.bind_capture_group(&[audio(resource)]),
            Err(Error::InvalidState)
        );
        let resource = no_staging.create().unwrap();
        assert_eq!(
            no_staging.bind_capture_group(&[audio(resource)]),
            Err(Error::Capacity)
        );
        let group = no_staging
            .bind_capture_group(&[CaptureMember {
                resource,
                format: Format::Midi,
                midi_events: 0,
                midi_bytes: 0,
            }])
            .unwrap();
        assert_eq!(
            no_staging.prepare_capture_take(group, &[MidiState::default()]),
            Err(Error::InvalidState)
        );
        no_staging.release_capture_group(group).unwrap();
        no_staging.remove(resource).unwrap();
    });
}
