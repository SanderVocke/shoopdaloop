mod support;

use rt_runtime::{
    buffer::MidiBuffer, midi_state::MidiState, resource::*, Budgets, Config, Error, Frame,
    ResourceId,
};
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;

#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

#[derive(Clone, Copy)]
struct Expected {
    start: Frame,
    marker: u32,
    initial: u8,
    frames: usize,
    active: bool,
}
#[derive(Clone, Copy)]
struct Lease {
    snapshot: Snapshot,
    index: ReadIndex,
    expected: Expected,
    member: usize,
}
fn samples(marker: u32, frame: usize) -> [f32; 2] {
    let left = (marker + frame as u32 * 3) as f32 + 0.25;
    [left, -(left + 0.5)]
}
fn packet(marker: u32, frame: usize) -> ([u8; 8], usize) {
    match frame {
        3 => ([0xf8, 0, 0, 0, 0, 0, 0, 0], 1),
        6 => ([0xf0, 1, 2, 3, 4, 5, 6, 0xf7], 8),
        _ => (
            [
                0x90,
                60,
                ((marker + frame as u32) % 127 + 1) as u8,
                0,
                0,
                0,
                0,
                0,
            ],
            3,
        ),
    }
}
fn check(store: &mut ResourceStore, lease: Lease) {
    let expected = lease.expected;
    let events: Vec<_> = (0..expected.frames)
        .filter(|frame| frame % 3 == 0)
        .map(|frame| (frame as u64, packet(expected.marker, frame)))
        .collect();
    let bytes: Vec<_> = if lease.member == 0 {
        (0..expected.frames)
            .flat_map(|frame| {
                samples(expected.marker, frame)
                    .into_iter()
                    .flat_map(f32::to_le_bytes)
            })
            .collect()
    } else {
        events
            .iter()
            .flat_map(|(frame, (packet, count))| {
                frame
                    .to_le_bytes()
                    .into_iter()
                    .chain((*count as u32).to_le_bytes())
                    .chain(packet[..*count].iter().copied())
            })
            .collect()
    };
    assert_no_heap(|| {
        assert_eq!(lease.snapshot.description.frames, expected.frames as u64);
        assert_eq!(lease.snapshot.description.bytes, bytes.len());
        assert_eq!(
            lease.snapshot.cut_frame,
            Frame(expected.start.0 + expected.frames as u64)
        );
        assert_eq!(
            lease.snapshot.capture,
            Some(CaptureInfo {
                start_frame: expected.start,
                next_frame: lease.snapshot.cut_frame,
                active: expected.active,
                incomplete: None,
            })
        );
        assert_eq!(store.read_index(lease.snapshot.id), Ok(lease.index));
        let before = store.usage();
        assert_eq!(
            store.render_io().cursor_at(lease.index, usize::MAX),
            Err(Error::InvalidOffset)
        );
        assert_eq!(store.usage(), before);
        let mut cursor = store.render_io().cursor_at(lease.index, 0).unwrap();
        let mut output = [0; 17];
        let mut offset = 0;
        while offset < bytes.len() {
            let count = store.render_io().read(&mut cursor, &mut output).unwrap();
            assert!(count > 0);
            assert_eq!(&output[..count], &bytes[offset..offset + count]);
            offset += count;
        }
        assert_eq!(store.render_io().read(&mut cursor, &mut output), Ok(0));
        for index in 0..16 {
            let offset = (expected.marker as usize + index * 17) % (bytes.len() + 1);
            let mut cursor = store.render_io().cursor_at(lease.index, offset).unwrap();
            let count = store.render_io().read(&mut cursor, &mut output).unwrap();
            assert_eq!(&output[..count], &bytes[offset..offset + count]);
        }
        if lease.member == 1 {
            let mut cursor = store.render_io().midi_cursor(lease.index).unwrap();
            for &(frame, (packet, count)) in &events {
                let actual = store.render_io().midi_next(&mut cursor).unwrap().unwrap();
                assert_eq!(actual.frame(), frame);
                let mut bytes = [0; 8];
                assert_eq!(store.render_io().midi_read(actual, &mut bytes), Ok(count));
                assert_eq!(&bytes[..count], &packet[..count]);
            }
            assert!(store.render_io().midi_next(&mut cursor).unwrap().is_none());
            for frame in 0..=expected.frames as u64 {
                let velocity = events
                    .iter()
                    .rev()
                    .find(|(at, (packet, _))| *at < frame && packet[0] == 0x90)
                    .map_or(expected.initial, |(_, (packet, _))| packet[2]);
                let position = store.render_io().midi_seek(lease.index, frame).unwrap();
                assert_eq!(
                    position
                        .state
                        .channel(0)
                        .unwrap()
                        .note(60)
                        .unwrap()
                        .velocity(),
                    velocity
                );
                assert!(position.replayed_events < 4);
            }
        }
        assert_eq!(store.usage(), before);
    });
}
fn leases(
    store: &ResourceStore,
    snapshots: [Option<Snapshot>; 2],
    expected: Expected,
) -> [Lease; 2] {
    std::array::from_fn(|member| {
        let snapshot = snapshots[member].unwrap();
        Lease {
            snapshot,
            index: store.read_index(snapshot.id).unwrap(),
            expected,
            member,
        }
    })
}
fn drain(store: &mut ResourceStore) {
    assert_no_heap(|| {
        for _ in 0..1024 {
            let usage = store.usage();
            if usage.retired_pages == 0 && usage.retired_versions == 0 {
                return;
            }
            store.begin_callback();
            store.reclaim();
        }
        panic!("prepared retirement storage did not drain");
    });
}
fn state(
    store: &ResourceStore,
    group: CaptureGroupId,
    resources: [ResourceId; 2],
) -> (PoolUsage, CaptureGroupInfo, [ResourceInfo; 2]) {
    (
        store.usage(),
        store.capture_group_info(group).unwrap(),
        resources.map(|resource| store.info(resource).unwrap()),
    )
}

#[shoop_test]
fn seeded_capture_indexes_keep_immutable_prefixes_across_partitions_reuse_and_exhausted_maintenance(
) {
    let (mut witness, counts) =
        support::measure(|| Vec::<u8>::with_capacity(std::hint::black_box(1)));
    assert!(counts.allocations > 0);
    let (_, counts) = support::measure(|| witness.reserve_exact(std::hint::black_box(129)));
    assert!(counts.reallocations > 0);
    std::hint::black_box(&witness);
    let (_, counts) = support::measure(|| drop(witness));
    assert!(counts.deallocations > 0);
    let config = Config {
        max_frames: 32,
        resource_slots: 8,
        resource_pages: 128,
        page_bytes: 16,
        midi_events_per_port: 16,
        midi_bytes_per_port: 64,
        budgets: Budgets {
            transfer_bytes: 16,
            validation_steps: 1,
            ..Budgets::default()
        },
        ..Config::default()
    }
    .prepare()
    .unwrap();
    for seed in [0x1a73_u32, 0x89f1, 0x719d, 0xc481] {
        let mut canonical = None;
        for parts in [vec![32], vec![1; 32], vec![0, 5, 3, 0, 11, 13, 0]] {
            let mut store = ResourceStore::with_capture_groups(
                config,
                Some(MidiIndexConfig {
                    versions: 3,
                    checkpoints_per_version: 8,
                    replay_events: 4,
                }),
                CaptureGroupConfig {
                    groups: 1,
                    members_per_group: 2,
                    staging_bytes_per_group: 512,
                },
            )
            .unwrap();
            let mut foreign = ResourceStore::new(config).unwrap();
            let baseline = store.usage();
            let resources = [store.create().unwrap(), store.create().unwrap()];
            let background_resource = store.create().unwrap();
            let upload = store
                .begin_upload(
                    background_resource,
                    store.revision(background_resource).unwrap(),
                    Description {
                        format: Format::Audio { channels: 1 },
                        frames: 4,
                        bytes: 16,
                    },
                )
                .unwrap();
            let background = assert_no_heap(|| {
                store.begin_callback();
                store.write_upload(upload, 0, &[0xa5; 16]).unwrap();
                store.finish_upload(upload).unwrap();
                store.commit(upload).unwrap();
                store.snapshot(background_resource, Frame(0))
            })
            .unwrap();
            let group = store
                .bind_capture_group(&[
                    CaptureMember {
                        resource: resources[0],
                        format: Format::Audio { channels: 2 },
                        midi_events: 0,
                        midi_bytes: 0,
                    },
                    CaptureMember {
                        resource: resources[1],
                        format: Format::Midi,
                        midi_events: 16,
                        midi_bytes: 64,
                    },
                ])
                .unwrap();
            let mut midi = MidiBuffer::new(16, 64).unwrap();
            let mut previous: Option<[Lease; 2]> = None;
            let mut observed = Vec::new();
            for round in 0..4 {
                let expected = Expected {
                    start: Frame((1_u64 << 63) + round as u64 * 64),
                    marker: seed + round * 137,
                    initial: ((seed + round) % 127 + 1) as u8,
                    frames: 32,
                    active: false,
                };
                let mut initial = MidiState::default();
                initial
                    .apply_packet(&[0x90, 60, expected.initial], 3)
                    .unwrap();
                let take =
                    assert_no_heap(|| store.prepare_capture_take(group, &[initial])).unwrap();
                assert_no_heap(|| store.activate_capture_take(take, expected.start)).unwrap();
                if let Some(old) = previous {
                    for lease in old {
                        assert_eq!(store.currentness(lease.snapshot.id), Ok(Currentness::Stale));
                        check(&mut store, lease);
                    }
                }
                let cancelled =
                    assert_no_heap(|| store.reserve_capture_interval(take, expected.start, 1))
                        .unwrap();
                assert_no_heap(|| store.stage_capture_audio(cancelled, 0, &[1., 2.])).unwrap();
                let before = state(&store, group, resources);
                assert_eq!(
                    assert_no_heap(|| store.commit_capture_interval(cancelled)),
                    Err(Error::Incomplete)
                );
                assert_eq!(
                    assert_no_heap(|| store.freeze_capture_take(
                        take,
                        expected.start,
                        None,
                        &mut [None; 2]
                    )),
                    Err(Error::InvalidState)
                );
                assert_eq!(state(&store, group, resources), before);
                assert_no_heap(|| store.cancel_capture_interval(cancelled)).unwrap();
                let next =
                    assert_no_heap(|| store.reserve_capture_interval(take, expected.start, 1))
                        .unwrap();
                let before = state(&store, group, resources);
                assert_ne!(next, cancelled);
                assert_eq!(
                    assert_no_heap(|| store.stage_capture_audio(cancelled, 0, &[1., 2.])),
                    Err(Error::InvalidId)
                );
                assert_eq!(
                    assert_no_heap(|| store.stage_capture_audio(next, usize::MAX, &[1., 2.])),
                    Err(Error::InvalidOffset)
                );
                assert_eq!(
                    assert_no_heap(|| store.stage_capture_audio(next, 0, &[1.])),
                    Err(Error::InvalidShape)
                );
                assert_eq!(state(&store, group, resources), before);
                assert_no_heap(|| store.cancel_capture_interval(next)).unwrap();
                let mut at = 0;
                let mut prefix = None;
                for &frames in &parts {
                    if frames == 0 {
                        continue;
                    }
                    assert_no_heap(|| {
                        store.begin_callback();
                        let mut cursor = store.cursor(background.id).unwrap();
                        assert_eq!(store.read(&mut cursor, &mut [0; 16]), Ok(16));
                        assert_eq!(store.copy_remaining(), 0);
                    });
                    let background_work = store.work();
                    let ceiling = at + frames;
                    while at < ceiling {
                        let end = ceiling.min(if at == 0 {
                            1
                        } else if at < 8 {
                            8
                        } else {
                            32
                        });
                        let count = end - at;
                        let audio: Vec<_> = (at..end)
                            .flat_map(|frame| samples(expected.marker, frame))
                            .collect();
                        assert_no_heap(|| {
                            midi.clear();
                            for frame in at..end {
                                if frame % 3 == 0 {
                                    let (packet, bytes) = packet(expected.marker, frame);
                                    midi.push((frame - at) as u32, count, &packet[..bytes])
                                        .unwrap();
                                }
                            }
                            let interval = store
                                .reserve_capture_interval(
                                    take,
                                    Frame(expected.start.0 + at as u64),
                                    count,
                                )
                                .unwrap();
                            if round % 2 == 0 {
                                store.stage_capture_audio(interval, 0, &audio).unwrap();
                            } else {
                                store.stage_capture_midi(interval, 1, &midi).unwrap();
                            }
                            for resource in resources {
                                assert_eq!(
                                    store.info(resource).unwrap().description.frames,
                                    at as u64
                                );
                            }
                            if round % 2 == 0 {
                                store.stage_capture_midi(interval, 1, &midi).unwrap();
                            } else {
                                store.stage_capture_audio(interval, 0, &audio).unwrap();
                            }
                            store.commit_capture_interval(interval).unwrap();
                            for resource in resources {
                                let info = store.info(resource).unwrap();
                                assert_eq!(info.description.frames, end as u64);
                                assert_eq!(
                                    info.capture.unwrap().next_frame,
                                    Frame(expected.start.0 + end as u64)
                                );
                            }
                            assert_eq!(store.usage().reserved_pages, 0);
                            assert_eq!(store.copy_remaining(), 0);
                        });
                        at = end;
                        if at == 1 {
                            let before = state(&store, group, resources);
                            for resource in resources {
                                assert_eq!(
                                    assert_no_heap(
                                        || store.snapshot(resource, Frame(expected.start.0 + 1))
                                    ),
                                    Err(Error::Budget)
                                );
                            }
                            assert_eq!(state(&store, group, resources), before);
                        }
                        if at == 8 {
                            let snapshots = resources.map(|resource| {
                                Some(
                                    assert_no_heap(|| {
                                        store.snapshot(resource, Frame(expected.start.0 + 8))
                                    })
                                    .unwrap(),
                                )
                            });
                            prefix = Some(leases(
                                &store,
                                snapshots,
                                Expected {
                                    frames: 8,
                                    active: true,
                                    ..expected
                                },
                            ));
                        }
                        assert_eq!(store.work(), background_work);
                    }
                }
                assert_eq!(at, 32);
                let mut frozen = [None; 2];
                assert_no_heap(|| {
                    store.freeze_capture_take(take, Frame(expected.start.0 + 32), None, &mut frozen)
                })
                .unwrap();
                let current = leases(&store, frozen, expected);
                let prefix = prefix.unwrap();
                for lease in prefix.into_iter().chain(current) {
                    check(&mut store, lease);
                }
                for lease in current {
                    assert_eq!(
                        store.currentness(lease.snapshot.id),
                        Ok(Currentness::Current)
                    );
                    assert_eq!(
                        assert_no_heap(|| foreign.render_io().cursor_at(lease.index, 0)),
                        Err(Error::InvalidId)
                    );
                }
                let mut duplicates = Vec::new();
                let mut saturated = false;
                for _ in 0..=8 {
                    match assert_no_heap(|| {
                        store.snapshot(resources[0], Frame(expected.start.0 + 32))
                    }) {
                        Ok(snapshot) => duplicates.push(snapshot),
                        Err(error) => {
                            assert_eq!(error, Error::Capacity);
                            saturated = true;
                            break;
                        }
                    }
                }
                assert!(saturated);
                let before = state(&store, group, resources);
                assert_eq!(
                    assert_no_heap(|| store.snapshot(resources[1], Frame(expected.start.0 + 32))),
                    Err(Error::Capacity)
                );
                assert_eq!(state(&store, group, resources), before);
                assert_no_heap(|| store.release_snapshot(prefix[0].snapshot.id)).unwrap();
                let reused =
                    assert_no_heap(|| store.snapshot(resources[0], Frame(expected.start.0 + 32)))
                        .unwrap();
                assert_eq!(reused.id.slot, prefix[0].snapshot.id.slot);
                assert_ne!(reused.id.generation, prefix[0].snapshot.id.generation);
                assert_eq!(
                    assert_no_heap(|| store.render_io().cursor_at(prefix[0].index, 0)),
                    Err(Error::InvalidId)
                );
                assert_no_heap(|| {
                    store.release_snapshot(reused.id).unwrap();
                    store.release_snapshot(prefix[1].snapshot.id).unwrap();
                    for snapshot in &duplicates {
                        store.release_snapshot(snapshot.id).unwrap();
                    }
                    if let Some(old) = previous {
                        for lease in old {
                            store.release_snapshot(lease.snapshot.id).unwrap();
                        }
                    }
                });
                for lease in current {
                    check(&mut store, lease);
                }
                observed.push((
                    expected.start,
                    current.map(|lease| {
                        (
                            lease.snapshot.description,
                            lease.snapshot.cut_frame,
                            lease.snapshot.capture,
                        )
                    }),
                ));
                previous = Some(current);
                drain(&mut store);
            }
            if let Some(before) = &canonical {
                assert_eq!(&observed, before);
            } else {
                canonical = Some(observed);
            }
            assert_no_heap(|| {
                for lease in previous.unwrap() {
                    store.release_snapshot(lease.snapshot.id).unwrap();
                }
                store.release_snapshot(background.id).unwrap();
                store.release_capture_group(group).unwrap();
                for resource in resources.into_iter().chain([background_resource]) {
                    store.remove(resource).unwrap();
                }
            });
            drain(&mut store);
            assert_eq!(store.usage(), baseline);
            assert_eq!(store.midi_index_usage().free_versions, 3);
        }
    }
}
