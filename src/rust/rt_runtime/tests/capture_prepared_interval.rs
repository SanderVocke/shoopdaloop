mod support;
use rt_runtime::{
    buffer::MidiBuffer, midi_state::MidiState, resource::*, Budgets, Config, Error, Frame,
    ResourceId, Revision,
};
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;
#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
fn store(pages: usize) -> ResourceStore {
    indexed_store(pages, 4, 4)
}
fn indexed_store(pages: usize, checkpoints: usize, span: usize) -> ResourceStore {
    let cfg = Config {
        max_frames: 4,
        resource_slots: 8,
        resource_pages: pages,
        page_bytes: 16,
        midi_events_per_port: 2,
        midi_bytes_per_port: 6,
        budgets: Budgets {
            transfer_bytes: 16,
            validation_steps: 128,
            ..Budgets::default()
        },
        ..Config::default()
    }
    .prepare()
    .unwrap();
    ResourceStore::with_capture_groups(
        cfg,
        Some(MidiIndexConfig {
            versions: 4,
            checkpoints_per_version: checkpoints,
            replay_events: span,
        }),
        CaptureGroupConfig {
            groups: 2,
            members_per_group: 2,
            staging_bytes_per_group: 64,
        },
    )
    .unwrap()
}
fn bind(store: &mut ResourceStore, resources: [ResourceId; 2]) -> CaptureGroupId {
    store
        .bind_capture_group(&[
            CaptureMember {
                resource: resources[0],
                format: Format::Audio { channels: 1 },
                midi_events: 0,
                midi_bytes: 0,
            },
            CaptureMember {
                resource: resources[1],
                format: Format::Midi,
                midi_events: 2,
                midi_bytes: 6,
            },
        ])
        .unwrap()
}
fn resources(store: &mut ResourceStore) -> [ResourceId; 2] {
    [store.create().unwrap(), store.create().unwrap()]
}
fn teardown(store: &mut ResourceStore, resources: &[ResourceId], pages: usize) {
    for &resource in resources {
        store.remove(resource).unwrap();
    }
    for _ in 0..64 {
        store.begin_callback();
        store.reclaim();
    }
    assert_eq!(store.usage().free_pages, pages);
    assert_eq!(store.usage().reserved_pages, 0);
    assert_eq!(store.usage().free_versions, 8);
    assert_eq!(store.usage().free_snapshots, 8);
    assert_eq!(store.midi_index_usage().free_versions, 4);
    assert_eq!(store.capture_group_usage().free_groups, 2);
}
#[shoop_test]
fn first_reservation_preserves_active_revisions_until_exact_activation_and_freeze() {
    let mut store = store(32);
    let ids = resources(&mut store);
    let mut packets = MidiBuffer::new(2, 6).unwrap();
    let mut frozen = [None; 2];
    assert_no_heap(|| {
        let audio = store
            .begin_upload(
                ids[0],
                Revision(0),
                Description {
                    format: Format::Audio { channels: 1 },
                    frames: 1,
                    bytes: 4,
                },
            )
            .unwrap();
        store.begin_callback();
        store.write_upload(audio, 0, &0.5f32.to_le_bytes()).unwrap();
        store.finish_upload(audio).unwrap();
        store.commit(audio).unwrap();
        let midi = store
            .begin_indexed_midi_upload(
                ids[1],
                Revision(0),
                Description {
                    format: Format::Midi,
                    frames: 1,
                    bytes: 15,
                },
                &MidiState::default(),
            )
            .unwrap();
        let mut record = [0; 15];
        encode_midi_record(0, &[0x90, 55, 70], &mut record).unwrap();
        store.begin_callback();
        store.write_upload(midi, 0, &record).unwrap();
        store.finish_upload(midi).unwrap();
        store.commit(midi).unwrap();
        let old = [
            store.snapshot(ids[0], Frame(1)).unwrap(),
            store.snapshot(ids[1], Frame(1)).unwrap(),
        ];
        let before = [store.info(ids[0]).unwrap(), store.info(ids[1]).unwrap()];
        let group = bind(&mut store, ids);
        let mut initial = MidiState::default();
        initial.apply_packet(&[0x90, 70, 99], 3).unwrap();
        let take = store.prepare_capture_take(group, &[initial]).unwrap();
        let reserved = store
            .reserve_prepared_capture_interval(take, Frame(10), 2)
            .unwrap();
        assert_eq!(
            store.capture_group_info(group).unwrap().phase,
            CapturePhase::PreparedReserved
        );
        assert_eq!(store.usage().reserved_pages, 3);
        assert_eq!(
            store.stage_capture_audio(reserved, 0, &[-0.25, 0.75]),
            Err(Error::InvalidState)
        );
        assert_eq!(
            store.stage_capture_midi(reserved, 1, &packets),
            Err(Error::InvalidState)
        );
        assert_eq!(
            store.commit_capture_interval(reserved),
            Err(Error::InvalidState)
        );
        assert_eq!(store.cancel_capture_take(take), Err(Error::InvalidState));
        assert_eq!(
            store.activate_capture_take(take, Frame(11)),
            Err(Error::InvalidFrame)
        );
        assert_eq!(
            [store.info(ids[0]).unwrap(), store.info(ids[1]).unwrap()],
            before
        );
        store.activate_capture_take(take, Frame(10)).unwrap();
        assert_eq!(
            store.capture_group_info(group).unwrap().phase,
            CapturePhase::Reserved
        );
        assert_eq!(store.usage().reserved_pages, 3);
        packets.push(1, 2, &[0x90, 61, 88]).unwrap();
        store
            .stage_capture_audio(reserved, 0, &[-0.25, 0.75])
            .unwrap();
        store.stage_capture_midi(reserved, 1, &packets).unwrap();
        store.commit_capture_interval(reserved).unwrap();
        assert_eq!(store.usage().reserved_pages, 0);
        assert_eq!(store.usage().free_pages, 28);
        store
            .freeze_capture_take(take, Frame(12), None, &mut frozen)
            .unwrap();
        let audio = frozen[0].unwrap();
        let midi = frozen[1].unwrap();
        assert_eq!(audio.description.frames, 2);
        assert_eq!(audio.description.bytes, 8);
        assert_eq!(midi.description.frames, 2);
        assert_eq!(midi.description.bytes, 15);
        assert_eq!(audio.revision, Revision(3));
        assert_eq!(midi.revision, Revision(3));
        let audio_index = store.read_index(audio.id).unwrap();
        let mut cursor = store.render_io().cursor_at(audio_index, 0).unwrap();
        let mut bytes = [0; 8];
        assert_eq!(store.render_io().read(&mut cursor, &mut bytes), Ok(8));
        assert_eq!(&bytes[..4], &(-0.25f32).to_le_bytes());
        assert_eq!(&bytes[4..], &0.75f32.to_le_bytes());
        let midi_index = store.read_index(midi.id).unwrap();
        assert_eq!(
            store.render_io().midi_seek(midi_index, 0).unwrap().state,
            initial
        );
        assert_eq!(
            store
                .render_io()
                .midi_seek(midi_index, 2)
                .unwrap()
                .state
                .channel(0)
                .unwrap()
                .note(61)
                .unwrap()
                .velocity(),
            88
        );
        let old_audio = store.read_index(old[0].id).unwrap();
        let mut cursor = store.render_io().cursor_at(old_audio, 0).unwrap();
        assert_eq!(store.render_io().read(&mut cursor, &mut bytes), Ok(4));
        assert_eq!(&bytes[..4], &0.5f32.to_le_bytes());
        let old_midi = store.read_index(old[1].id).unwrap();
        assert_eq!(
            store
                .render_io()
                .midi_seek(old_midi, 1)
                .unwrap()
                .state
                .channel(0)
                .unwrap()
                .note(55)
                .unwrap()
                .velocity(),
            70
        );
        for snapshot in [old[0], old[1], audio, midi] {
            store.release_snapshot(snapshot.id).unwrap();
        }
        store.release_capture_group(group).unwrap();
        teardown(&mut store, &ids, 32);
    });
}
#[shoop_test]
fn competing_prepared_groups_reserve_all_or_none_and_cancel_without_publishing() {
    let mut store = store(5);
    let a = resources(&mut store);
    let b = resources(&mut store);
    let groups = [bind(&mut store, a), bind(&mut store, b)];
    assert_no_heap(|| {
        for round in 0..32 {
            let takes = [
                store
                    .prepare_capture_take(groups[0], &[MidiState::default()])
                    .unwrap(),
                store
                    .prepare_capture_take(groups[1], &[MidiState::default()])
                    .unwrap(),
            ];
            let first = round % 2;
            let second = 1 - first;
            let frames = 1 + round % 4;
            let at = Frame(4 * round as u64);
            // Independent byte ledger: ceil(4*frames/16) + ceil((2*12+6)/16) = 3.
            let interval = store
                .reserve_prepared_capture_interval(takes[first], at, frames)
                .unwrap();
            assert_eq!(store.usage().reserved_pages, 3);
            let before = store.usage();
            assert_eq!(
                store.reserve_prepared_capture_interval(takes[second], at, frames),
                Err(Error::Capacity)
            );
            assert_eq!(store.usage(), before);
            assert_eq!(
                store.capture_group_info(groups[second]).unwrap().phase,
                CapturePhase::Prepared
            );
            store.cancel_capture_interval(interval).unwrap();
            assert_eq!(store.usage().reserved_pages, 0);
            assert_eq!(
                store.capture_group_info(groups[first]).unwrap().next_frame,
                None
            );
            let other = store
                .reserve_prepared_capture_interval(takes[second], at, frames)
                .unwrap();
            store.cancel_capture_interval(other).unwrap();
            let again = store
                .reserve_prepared_capture_interval(takes[first], at, frames)
                .unwrap();
            assert_ne!(interval, again);
            assert_eq!(
                store.cancel_capture_interval(interval),
                Err(Error::InvalidId)
            );
            store.cancel_capture_interval(again).unwrap();
            for take in takes {
                store.cancel_capture_take(take).unwrap();
            }
            for resource in a.into_iter().chain(b) {
                assert_eq!(store.revision(resource), Ok(Revision(0)));
            }
            for _ in 0..16 {
                store.begin_callback();
                store.reclaim();
            }
            assert_eq!(store.usage().free_pages, 5);
            assert_eq!(store.usage().reserved_pages, 0);
        }
        for group in groups {
            store.release_capture_group(group).unwrap();
        }
        teardown(&mut store, &[a[0], a[1], b[0], b[1]], 5);
    });
}
#[shoop_test]
fn prepared_interval_authority_rejects_foreign_tokens_frame_overflow_and_wrong_windows() {
    let mut store = store(32);
    let mut peer = self::store(32);
    let a = resources(&mut store);
    let b = resources(&mut peer);
    let group = bind(&mut store, a);
    let foreign = bind(&mut peer, b);
    let mut limited = indexed_store(32, 1, 1);
    let limited_ids = resources(&mut limited);
    let limited_group = bind(&mut limited, limited_ids);
    assert_no_heap(|| {
        let limited_take = limited
            .prepare_capture_take(limited_group, &[MidiState::default()])
            .unwrap();
        let initial_usage = limited.usage();
        assert_eq!(
            limited.reserve_prepared_capture_interval(limited_take, Frame(10), 1),
            Err(Error::Capacity)
        );
        assert_eq!(limited.usage(), initial_usage);
        for resource in limited_ids {
            assert_eq!(limited.revision(resource), Ok(Revision(0)));
        }
        limited.cancel_capture_take(limited_take).unwrap();
        limited.release_capture_group(limited_group).unwrap();
        teardown(&mut limited, &limited_ids, 32);
        let take = store
            .prepare_capture_take(group, &[MidiState::default()])
            .unwrap();
        let other = peer
            .prepare_capture_take(foreign, &[MidiState::default()])
            .unwrap();
        let before = store.usage();
        assert_eq!(
            store.reserve_prepared_capture_interval(other, Frame(10), 1),
            Err(Error::InvalidId)
        );
        assert_eq!(
            store.reserve_prepared_capture_interval(take, Frame(u64::MAX), 1),
            Err(Error::Exhausted)
        );
        for frames in [0, 5] {
            assert_eq!(
                store.reserve_prepared_capture_interval(take, Frame(10), frames),
                Err(Error::InvalidShape)
            );
        }
        assert_eq!(
            store
                .planner_io(Frame(10), Frame(12))
                .capture()
                .reserve_prepared(take, Frame(9), 1),
            Err(Error::InvalidFrame)
        );
        assert_eq!(
            store
                .planner_io(Frame(10), Frame(12))
                .capture()
                .reserve_prepared(take, Frame(10), 3),
            Err(Error::InvalidFrame)
        );
        assert_eq!(store.usage(), before);
        let interval = store
            .planner_io(Frame(10), Frame(12))
            .capture()
            .reserve_prepared(take, Frame(10), 2)
            .unwrap();
        assert_eq!(
            store
                .planner_io(Frame(11), Frame(13))
                .capture()
                .activate(take, Frame(11)),
            Err(Error::InvalidFrame)
        );
        store
            .planner_io(Frame(10), Frame(12))
            .capture()
            .cancel_interval(interval)
            .unwrap();
        store
            .planner_io(Frame(10), Frame(12))
            .capture()
            .cancel_take(take)
            .unwrap();
        assert_eq!(
            store.reserve_prepared_capture_interval(take, Frame(10), 1),
            Err(Error::InvalidState)
        );
        peer.cancel_capture_take(other).unwrap();
        store.release_capture_group(group).unwrap();
        peer.release_capture_group(foreign).unwrap();
        teardown(&mut store, &a, 32);
        teardown(&mut peer, &b, 32);
    });
}
