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
fn prepared() -> (
    ResourceStore,
    CaptureGroupId,
    CaptureTakeId,
    [rt_runtime::ResourceId; 2],
) {
    let config = Config {
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
    .unwrap();
    let mut store = ResourceStore::with_capture_groups(
        config,
        Some(MidiIndexConfig {
            versions: 2,
            checkpoints_per_version: 4,
            replay_events: 8,
        }),
        CaptureGroupConfig {
            groups: 2,
            members_per_group: 2,
            staging_bytes_per_group: 288,
        },
    )
    .unwrap();
    let resources = [store.create().unwrap(), store.create().unwrap()];
    let group = assert_no_heap(|| {
        store.bind_capture_group(&[
            CaptureMember {
                resource: resources[0],
                format: Format::Audio { channels: 2 },
                midi_events: 0,
                midi_bytes: 0,
            },
            CaptureMember {
                resource: resources[1],
                format: Format::Midi,
                midi_events: 8,
                midi_bytes: 128,
            },
        ])
    })
    .unwrap();
    let take =
        assert_no_heap(|| store.prepare_capture_take(group, &[MidiState::default()])).unwrap();
    (store, group, take, resources)
}
fn finish(
    store: &mut ResourceStore,
    group: CaptureGroupId,
    resources: [rt_runtime::ResourceId; 2],
    snapshots: [Option<Snapshot>; 2],
) {
    store.release_capture_group(group).unwrap();
    for snapshot in snapshots.into_iter().flatten() {
        store.release_snapshot(snapshot.id).unwrap();
    }
    for resource in resources {
        store.remove(resource).unwrap();
    }
    for _ in 0..128 {
        store.begin_callback();
        store.reclaim();
    }
    assert_eq!(store.usage().free_pages, 64);
    assert_eq!(store.usage().free_versions, 8);
    assert_eq!(store.usage().free_snapshots, 8);
    assert_eq!(store.usage().reserved_pages, 0);
    assert_eq!(store.midi_index_usage().free_versions, 2);
}
#[shoop_test]
fn staged_stereo_and_midi_match_independent_content_across_interval_partitions() {
    for partition in [&[8, 8][..], &[1; 16][..], &[3, 2, 5, 6][..]] {
        let (mut store, group, take, resources) = prepared();
        let mut input = MidiBuffer::new(8, 128).unwrap();
        assert_no_heap(|| {
            store.activate_capture_take(take, Frame(100)).unwrap();
            store.begin_callback();
            let other = store.create().unwrap();
            let upload = store
                .begin_upload(
                    other,
                    Revision(0),
                    Description {
                        format: Format::Audio { channels: 1 },
                        frames: 4,
                        bytes: 16,
                    },
                )
                .unwrap();
            store.write_upload(upload, 0, &[0; 16]).unwrap();
            assert_eq!(store.copy_remaining(), 0);
            let background = store.work();
            let mut at = 0usize;
            for &frames in partition {
                let mut audio = [0.; 16];
                input.clear();
                for offset in 0..frames {
                    audio[offset * 2] = (at + offset) as f32 + 0.25;
                    audio[offset * 2 + 1] = -(at as f32 + offset as f32) - 0.5;
                    input
                        .push(offset as u32, frames, &[0x90, 60, (at + offset + 1) as u8])
                        .unwrap();
                }
                let interval = store
                    .reserve_capture_interval(take, Frame(100 + at as u64), frames)
                    .unwrap();
                store.stage_capture_midi(interval, 1, &input).unwrap();
                assert_eq!(
                    store.info(resources[1]).unwrap().description.frames,
                    at as u64
                );
                store
                    .stage_capture_audio(interval, 0, &audio[..frames * 2])
                    .unwrap();
                store.commit_capture_interval(interval).unwrap();
                at += frames;
                assert_eq!(store.usage().reserved_pages, 0);
                for resource in resources {
                    let info = store.info(resource).unwrap();
                    assert_eq!(info.description.frames, at as u64);
                    assert_eq!(info.capture.unwrap().next_frame, Frame(100 + at as u64));
                }
            }
            assert_eq!(store.work(), background);
            assert_eq!(store.render_work().copied_bytes, 3 * (16 * 8 + 16 * 15));
            let mut snapshots = [None; 2];
            store
                .freeze_capture_take(take, Frame(116), None, &mut snapshots)
                .unwrap();
            let audio = store.read_index(snapshots[0].unwrap().id).unwrap();
            let mut cursor = store.render_io().cursor_at(audio, 0).unwrap();
            let mut bytes = [0; 128];
            let mut n = 0;
            while n < bytes.len() {
                n += store
                    .render_io()
                    .read(&mut cursor, &mut bytes[n..])
                    .unwrap();
            }
            for frame in 0..16 {
                assert_eq!(
                    &bytes[frame * 8..frame * 8 + 4],
                    &(frame as f32 + 0.25).to_le_bytes()
                );
                assert_eq!(
                    &bytes[frame * 8 + 4..frame * 8 + 8],
                    &(-(frame as f32) - 0.5).to_le_bytes()
                );
            }
            let midi = store.read_index(snapshots[1].unwrap().id).unwrap();
            let mut cursor = store.render_io().midi_cursor(midi).unwrap();
            for frame in 0..16 {
                let event = store.render_io().midi_next(&mut cursor).unwrap().unwrap();
                assert_eq!(event.frame(), frame);
                let mut packet = [0; 3];
                assert_eq!(store.render_io().midi_read(event, &mut packet), Ok(3));
                assert_eq!(packet, [0x90, 60, frame as u8 + 1]);
            }
            assert_eq!(store.render_io().midi_next(&mut cursor), Ok(None));
            for frame in 0..=16 {
                let state = store.render_io().midi_seek(midi, frame).unwrap();
                assert_eq!(
                    state.state.channel(0).unwrap().note(60).unwrap().velocity(),
                    frame as u8
                );
                assert!(state.replayed_events < 8);
            }
            store.cancel_upload(upload).unwrap();
            store.remove(other).unwrap();
            finish(&mut store, group, resources, snapshots);
        });
    }
}
#[shoop_test]
fn partial_staging_is_unpublished_and_cancelled_tokens_cannot_retarget() {
    let (mut store, group, take, resources) = prepared();
    let input = MidiBuffer::new(8, 128).unwrap();
    assert_no_heap(|| {
        store.activate_capture_take(take, Frame(0)).unwrap();
        let interval = store.reserve_capture_interval(take, Frame(0), 1).unwrap();
        assert!(store.usage().reserved_pages > 0);
        store.stage_capture_audio(interval, 0, &[1., 2.]).unwrap();
        assert_eq!(
            store.stage_capture_audio(interval, 0, &[1., 2.]),
            Err(Error::InvalidState)
        );
        assert_eq!(
            store.commit_capture_interval(interval),
            Err(Error::Incomplete)
        );
        assert_eq!(
            store.freeze_capture_take(take, Frame(0), None, &mut [None; 2]),
            Err(Error::InvalidState)
        );
        for resource in resources {
            assert_eq!(store.info(resource).unwrap().description.frames, 0);
        }
        store.cancel_capture_interval(interval).unwrap();
        assert_eq!(store.usage().reserved_pages, 0);
        let next = store.reserve_capture_interval(take, Frame(0), 1).unwrap();
        assert_ne!(next, interval);
        assert_eq!(
            store.stage_capture_audio(interval, 0, &[3., 4.]),
            Err(Error::InvalidId)
        );
        assert_eq!(
            store.cancel_capture_interval(interval),
            Err(Error::InvalidId)
        );
        assert_eq!(
            store.commit_capture_interval(interval),
            Err(Error::InvalidId)
        );
        assert_eq!(
            store.stage_capture_audio(next, 2, &[3., 4.]),
            Err(Error::InvalidOffset)
        );
        assert_eq!(
            store.stage_capture_audio(next, 0, &[3.]),
            Err(Error::InvalidShape)
        );
        store.stage_capture_audio(next, 0, &[3., 4.]).unwrap();
        store.stage_capture_midi(next, 1, &input).unwrap();
        store.commit_capture_interval(next).unwrap();
        assert_eq!(
            store.commit_capture_interval(next),
            Err(Error::InvalidState)
        );
        let mut snapshots = [None; 2];
        store
            .freeze_capture_take(take, Frame(1), None, &mut snapshots)
            .unwrap();
        assert_eq!(snapshots[0].unwrap().description.frames, 1);
        assert_eq!(
            snapshots[1].unwrap().description,
            Description {
                format: Format::Midi,
                frames: 1,
                bytes: 0
            }
        );
        finish(&mut store, group, resources, snapshots);
    });
}
#[shoop_test]
fn conservative_checkpoint_refusal_preserves_a_common_prefix_before_next_samples() {
    let (mut store, group, take, resources) = prepared();
    let mut input = MidiBuffer::new(8, 128).unwrap();
    input.push(0, 1, &[0x90, 60, 100]).unwrap();
    assert_no_heap(|| {
        store.activate_capture_take(take, Frame(0)).unwrap();
        for frame in 0..24 {
            let interval = store
                .reserve_capture_interval(take, Frame(frame), 1)
                .unwrap();
            store
                .stage_capture_audio(interval, 0, &[frame as f32, -(frame as f32)])
                .unwrap();
            store.stage_capture_midi(interval, 1, &input).unwrap();
            store.commit_capture_interval(interval).unwrap();
        }
        let before = store.usage();
        assert_eq!(
            store.reserve_capture_interval(take, Frame(24), 1),
            Err(Error::Capacity)
        );
        assert_eq!(store.usage(), before);
        for resource in resources {
            assert_eq!(store.info(resource).unwrap().description.frames, 24);
        }
        let mut snapshots = [None; 2];
        store
            .freeze_capture_take(take, Frame(24), Some(Error::Capacity), &mut snapshots)
            .unwrap();
        for snapshot in snapshots.iter().flatten() {
            assert_eq!(snapshot.description.frames, 24);
            assert_eq!(snapshot.cut_frame, Frame(24));
            assert_eq!(snapshot.capture.unwrap().incomplete, Some(Error::Capacity));
        }
        assert_eq!(snapshots[0].unwrap().description.bytes, 24 * 8);
        assert_eq!(snapshots[1].unwrap().description.bytes, 24 * 15);
        let index = store.read_index(snapshots[1].unwrap().id).unwrap();
        assert_eq!(
            store
                .render_io()
                .midi_seek(index, 24)
                .unwrap()
                .state
                .channel(0)
                .unwrap()
                .note(60)
                .unwrap()
                .velocity(),
            100
        );
        finish(&mut store, group, resources, snapshots);
    });
}
#[shoop_test]
fn immutable_group_prefixes_survive_staging_and_later_page_index_rotations() {
    let (mut store, group, take, resources) = prepared();
    let mut input = MidiBuffer::new(8, 128).unwrap();
    assert_no_heap(|| {
        store.activate_capture_take(take, Frame(0)).unwrap();
        store.begin_callback();
        let mut old = [None; 2];
        let mut at = 0;
        for frames in [4, 8, 8] {
            input.clear();
            for offset in 0..frames {
                input
                    .push(
                        offset,
                        frames as usize,
                        &[0x90, 60, (at + offset + 1) as u8],
                    )
                    .unwrap();
            }
            let interval = store
                .reserve_capture_interval(take, Frame(at as u64), frames as usize)
                .unwrap();
            store
                .stage_capture_audio(interval, 0, &[1.; 16][..frames as usize * 2])
                .unwrap();
            store.stage_capture_midi(interval, 1, &input).unwrap();
            if let Some(snapshot) = old[0] {
                let snapshot: Snapshot = snapshot;
                assert_eq!(
                    store.currentness(snapshot.id),
                    Ok(if at == 4 {
                        Currentness::Current
                    } else {
                        Currentness::Stale
                    })
                );
            }
            store.commit_capture_interval(interval).unwrap();
            at += frames;
            if at == 4 {
                old[0] = Some(store.snapshot(resources[0], Frame(4)).unwrap());
                old[1] = Some(store.snapshot(resources[1], Frame(4)).unwrap());
            }
        }
        let audio = store.read_index(old[0].unwrap().id).unwrap();
        let mut cursor = store.render_io().cursor_at(audio, 0).unwrap();
        let mut output = [0; 32];
        let mut count = 0;
        while count < output.len() {
            count += store
                .render_io()
                .read(&mut cursor, &mut output[count..])
                .unwrap();
        }
        for sample in output.chunks_exact(4) {
            assert_eq!(sample, &1f32.to_le_bytes());
        }
        let midi = store.read_index(old[1].unwrap().id).unwrap();
        assert_eq!(
            store
                .render_io()
                .midi_seek(midi, 4)
                .unwrap()
                .state
                .channel(0)
                .unwrap()
                .note(60)
                .unwrap()
                .velocity(),
            4
        );
        let mut events = store.render_io().midi_cursor(midi).unwrap();
        for frame in 0..4 {
            let event = store.render_io().midi_next(&mut events).unwrap().unwrap();
            assert_eq!(event.frame(), frame);
            let mut packet = [0; 3];
            store.render_io().midi_read(event, &mut packet).unwrap();
            assert_eq!(packet, [0x90, 60, frame as u8 + 1]);
        }
        assert_eq!(store.render_io().midi_next(&mut events), Ok(None));
        let mut snapshots = [None; 2];
        store
            .freeze_capture_take(take, Frame(20), None, &mut snapshots)
            .unwrap();
        for snapshot in old.into_iter().flatten() {
            store.release_snapshot(snapshot.id).unwrap();
        }
        finish(&mut store, group, resources, snapshots);
    });
}
#[shoop_test]
fn producer_bound_violations_leave_staged_transaction_cancellable() {
    let (mut store, group, take, resources) = prepared();
    let mut oversized = MidiBuffer::new(9, 128).unwrap();
    for _ in 0..9 {
        oversized.push(0, 1, &[0xf8]).unwrap();
    }
    let mut late = MidiBuffer::new(8, 128).unwrap();
    late.push(1, 2, &[0xf8]).unwrap();
    assert_no_heap(|| {
        store.activate_capture_take(take, Frame(0)).unwrap();
        let interval = store.reserve_capture_interval(take, Frame(0), 1).unwrap();
        store.stage_capture_audio(interval, 0, &[1., 2.]).unwrap();
        assert_eq!(
            store.stage_capture_midi(interval, 1, &oversized),
            Err(Error::Capacity)
        );
        assert_eq!(
            store.stage_capture_midi(interval, 1, &late),
            Err(Error::InvalidEvent)
        );
        assert_eq!(
            store.commit_capture_interval(interval),
            Err(Error::Incomplete)
        );
        for resource in resources {
            assert_eq!(store.info(resource).unwrap().description.frames, 0);
        }
        store.cancel_capture_interval(interval).unwrap();
        let mut snapshots = [None; 2];
        store
            .freeze_capture_take(take, Frame(0), None, &mut snapshots)
            .unwrap();
        finish(&mut store, group, resources, snapshots);
    });
}
