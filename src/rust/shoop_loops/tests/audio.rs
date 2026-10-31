#[path = "../../rt_runtime/tests/support/mod.rs"]
mod support;
use rt_runtime::{resource::*, Budgets, Config, Error as MediaError, ResourceId};
use shoop_loops::{audio::*, Request, *};
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;
#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
fn store() -> ResourceStore {
    ResourceStore::with_capture_groups(
        Config {
            max_frames: 64,
            resource_slots: 8,
            resource_pages: 128,
            page_bytes: 16,
            budgets: Budgets {
                transfer_bytes: 16,
                validation_steps: 1,
                ..Budgets::default()
            },
            ..Config::default()
        }
        .prepare()
        .unwrap(),
        None,
        CaptureGroupConfig {
            groups: 1,
            members_per_group: 1,
            staging_bytes_per_group: 512,
        },
    )
    .unwrap()
}
fn upload(store: &mut ResourceStore, channels: u32, samples: &[f32]) -> Snapshot {
    let resource = store.create().unwrap();
    let bytes: Vec<_> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    let token = store
        .begin_upload(
            resource,
            Revision(0),
            Description {
                format: Format::Audio { channels },
                frames: (samples.len() / channels as usize) as u64,
                bytes: bytes.len(),
            },
        )
        .unwrap();
    for (page, chunk) in bytes.chunks(16).enumerate() {
        store.begin_callback();
        store.write_upload(token, page * 16, chunk).unwrap();
    }
    store.finish_upload(token).unwrap();
    store.commit(token).unwrap();
    store.snapshot(resource, Frame(0)).unwrap()
}
fn reclaim(store: &mut ResourceStore) {
    for _ in 0..160 {
        store.begin_callback();
        store.reclaim();
    }
}
fn cleanup(store: &mut ResourceStore, snapshots: &[Snapshot], resources: &[ResourceId]) {
    assert_no_heap(|| {
        for snapshot in snapshots {
            store.release_snapshot(snapshot.id).unwrap();
        }
        for resource in resources {
            store.remove(*resource).unwrap();
        }
        reclaim(store);
        assert_eq!(store.usage().free_versions, 8);
        assert_eq!(store.usage().free_snapshots, 8);
        assert_eq!(store.usage().free_pages, 128);
    });
}
fn oracle(smooth: u32) -> [[f32; 2]; 40] {
    let mut result = [[0.; 2]; 40];
    let mut playing = false;
    let mut position = 0;
    let mut length = 5;
    let mut replacement = false;
    let mut previous = [0.; 2];
    let mut anchor = [0.; 2];
    let mut elapsed = smooth;
    for (frame, output) in result.iter_mut().enumerate() {
        let restart = match frame {
            0 => {
                playing = true;
                position = 0;
                true
            }
            7 => {
                position = 3;
                true
            }
            11 => {
                playing = false;
                position = 0;
                false
            }
            13 => {
                playing = true;
                position = 1;
                true
            }
            17 => {
                replacement = true;
                length = 3;
                position = 0;
                true
            }
            _ if playing && position == length => {
                position = 0;
                true
            }
            _ => false,
        };
        if !playing {
            previous = [0.; 2];
            elapsed = smooth;
            continue;
        }
        if restart {
            anchor = previous;
            elapsed = 0;
        }
        let raw = if replacement {
            20. + position as f32
        } else {
            1. + position as f32
        };
        let gained = [raw * 2., -raw * 2.];
        if elapsed < smooth {
            elapsed += 1;
            let weight = elapsed as f32 / smooth as f32;
            for channel in 0..2 {
                output[channel] = anchor[channel] * (1. - weight) + gained[channel] * weight;
            }
        } else {
            *output = gained;
        }
        previous = *output;
        position += 1;
    }
    result
}
#[shoop_test]
fn resolver_driven_audio_matches_scalar_samples_and_smoothing_across_host_cuts() {
    let target = LoopId::from_parts(0, 1).unwrap();
    for (smooth, journal_events) in [(0, 0), (0, 1), (0, 1024), (4, 0), (4, 1), (4, 1024)] {
        let expected = oracle(smooth);
        for partitions in [vec![40], vec![1; 40], vec![7, 6, 4, 9, 14]] {
            let mut store = store();
            let old = upload(
                &mut store,
                2,
                &[1., -1., 2., -2., 3., -3., 4., -4., 5., -5.],
            );
            let new = upload(&mut store, 2, &[20., -20., 21., -21., 22., -22.]);
            let indices = [
                store.read_index(old.id).unwrap(),
                store.read_index(new.id).unwrap(),
            ];
            let topology = compile(
                Limits {
                    journal_events,
                    ..Limits::default()
                },
                &TopologyDescription {
                    loops: vec![LoopSpec {
                        id: target,
                        kind: LoopKind::Primitive,
                        length: 5,
                        sync_source: None,
                    }],
                    triggers: vec![],
                    programs: vec![],
                },
            )
            .unwrap();
            let mut resolver = Resolver::prepare(topology).unwrap();
            let mut renderer =
                AudioRenderer::prepare(2, 64, Smoothing::LinearRestart { frames: smooth }).unwrap();
            let mut left = [0.; 40];
            let mut right = [0.; 40];
            assert_no_heap(|| {
                for (sequence, (at, transition)) in [
                    (
                        0,
                        Transition::Play {
                            position: 0,
                            retrigger: false,
                        },
                    ),
                    (
                        7,
                        Transition::Seek {
                            position: 3,
                            retrigger: false,
                        },
                    ),
                    (11, Transition::Stop),
                    (
                        13,
                        Transition::Play {
                            position: 1,
                            retrigger: false,
                        },
                    ),
                    (
                        17,
                        Transition::Replace {
                            length: 3,
                            revision: Revision(1),
                            preserve_position: false,
                        },
                    ),
                ]
                .into_iter()
                .enumerate()
                {
                    resolver
                        .submit(Request {
                            target,
                            sequence: Sequence(sequence as u64 + 1),
                            at: Frame(at),
                            timing: Timing::At,
                            transition,
                        })
                        .unwrap();
                }
                let mut ceiling = 0;
                for &frames in &partitions {
                    ceiling += frames;
                    store.begin_callback();
                    let mut cursor = store.cursor(old.id).unwrap();
                    assert_eq!(store.read(&mut cursor, &mut [0; 16]), Ok(16));
                    loop {
                        let interval = resolver.preview(Frame(ceiling), &[]).unwrap();
                        let decision = resolver.decisions().unwrap()[0];
                        let state = decision.state;
                        resolver.commit().unwrap();
                        let restart = decision.playback_discontinuity;
                        let start = interval.start.0 as usize;
                        let end = interval.end.0 as usize;
                        renderer
                            .render(
                                &mut store.render_io(),
                                Playback {
                                    source: state.plays().then_some(
                                        indices[usize::from(state.content_revision.0 != 0)],
                                    ),
                                    position: state.position,
                                    gain: 2.,
                                    balance: 0.0,
                                    discontinuity: restart,
                                },
                                &mut [&mut left[start..end], &mut right[start..end]],
                            )
                            .unwrap();
                        assert_eq!(store.copy_remaining(), 0);
                        resolver.finish(interval.end).unwrap();
                        if interval.end.0 == ceiling {
                            break;
                        }
                    }
                }
                for frame in 0..40 {
                    assert_eq!(
                        [left[frame], right[frame]],
                        expected[frame],
                        "frame {frame}, smoothing {smooth}"
                    );
                }
            });
            cleanup(&mut store, &[old, new], &[old.resource, new.resource]);
        }
    }
}
#[shoop_test]
fn configurable_channels_cross_pages_and_keep_removed_revision_playable() {
    for channels in [1, 2, 5] {
        let mut store = store();
        let source: Vec<f32> = (0..channels * 23).map(|i| i as f32 - 30.).collect();
        let snapshot = upload(&mut store, channels as u32, &source);
        let index = store.read_index(snapshot.id).unwrap();
        let mut renderer = AudioRenderer::prepare(channels, 64, Smoothing::None).unwrap();
        let mut buffers = vec![vec![0.; 19]; channels];
        let mut outputs: Vec<&mut [f32]> = buffers.iter_mut().map(Vec::as_mut_slice).collect();
        assert_no_heap(|| {
            store.remove(snapshot.resource).unwrap();
            assert_eq!(store.render_io().snapshot(index), Ok(snapshot));
            renderer
                .render(
                    &mut store.render_io(),
                    Playback {
                        source: Some(index),
                        position: 3,
                        gain: -0.5,
                        balance: 0.0,
                        discontinuity: true,
                    },
                    &mut outputs,
                )
                .unwrap();
            for channel in 0..channels {
                for frame in 0..19 {
                    assert_eq!(
                        outputs[channel][frame],
                        source[(frame + 3) * channels + channel] * -0.5
                    );
                }
            }
            store.release_snapshot(snapshot.id).unwrap();
            reclaim(&mut store);
            assert_eq!(store.usage().free_pages, 128);
        });
    }
}
#[shoop_test]
fn smoothing_changes_apply_from_the_next_restart() {
    let mut store = store();
    let snapshot = upload(&mut store, 1, &[1.; 8]);
    let index = store.read_index(snapshot.id).unwrap();
    let mut renderer =
        AudioRenderer::prepare(1, 8, Smoothing::LinearRestart { frames: 2 }).unwrap();
    let mut render = |renderer: &mut AudioRenderer,
                      source: Option<ReadIndex>,
                      gain: f32,
                      discontinuity: bool,
                      frames: usize| {
        let mut output = [9.; 8];
        assert_no_heap(|| {
            renderer
                .render(
                    &mut store.render_io(),
                    Playback {
                        source,
                        position: 0,
                        gain,
                        balance: 0.0,
                        discontinuity,
                    },
                    &mut [&mut output[..frames]],
                )
                .unwrap();
        });
        output[..frames].to_vec()
    };
    assert_eq!(render(&mut renderer, Some(index), 1., true, 1), [0.5]);
    renderer.set_smoothing(Smoothing::LinearRestart { frames: 4 });
    // The ramp in progress keeps its length.
    assert_eq!(render(&mut renderer, Some(index), 1., false, 2), [1., 1.]);
    assert_eq!(render(&mut renderer, None, 1., false, 1), [0.]);
    assert_eq!(
        render(&mut renderer, Some(index), 1., true, 5),
        [0.25, 0.5, 0.75, 1., 1.]
    );
    renderer.set_smoothing(Smoothing::None);
    assert_eq!(render(&mut renderer, Some(index), 0.5, true, 2), [0.5, 0.5]);
}

#[shoop_test]
fn stereo_balance_scales_one_side_and_leaves_other_layouts_unscaled() {
    for (channels, balance, expected) in [
        (2, -0.25, vec![0.5, 0.75 * 1.0]),
        (2, 0.5, vec![0.5 * 0.5, 1.0]),
        (2, 0.0, vec![0.5, 1.0]),
        (1, -1.0, vec![0.5]),
        (3, 1.0, vec![0.5, 1.0, 1.5]),
    ] {
        let mut store = store();
        let source: Vec<f32> = (1..=channels).map(|channel| channel as f32 * 0.5).collect();
        let snapshot = upload(&mut store, channels as u32, &source);
        let index = store.read_index(snapshot.id).unwrap();
        let mut renderer = AudioRenderer::prepare(channels, 4, Smoothing::None).unwrap();
        let mut buffers = vec![vec![0.; 1]; channels];
        let mut outputs: Vec<&mut [f32]> = buffers.iter_mut().map(Vec::as_mut_slice).collect();
        assert_no_heap(|| {
            renderer
                .render(
                    &mut store.render_io(),
                    Playback {
                        source: Some(index),
                        position: 0,
                        gain: 1.,
                        balance,
                        discontinuity: true,
                    },
                    &mut outputs,
                )
                .unwrap();
        });
        let rendered: Vec<f32> = buffers.iter().map(|buffer| buffer[0]).collect();
        assert_eq!(rendered, expected, "channels {channels}, balance {balance}");
    }
    let mut store = store();
    let snapshot = upload(&mut store, 2, &[1., 1.]);
    let index = store.read_index(snapshot.id).unwrap();
    let mut renderer = AudioRenderer::prepare(2, 4, Smoothing::None).unwrap();
    let (mut left, mut right) = ([9.], [9.]);
    for balance in [1.5, -1.01, f32::NAN] {
        assert_eq!(
            renderer.render(
                &mut store.render_io(),
                Playback {
                    source: Some(index),
                    position: 0,
                    gain: 1.,
                    balance,
                    discontinuity: true,
                },
                &mut [&mut left, &mut right],
            ),
            Err(MediaError::InvalidParameter)
        );
        assert_eq!((left, right), ([9.], [9.]));
    }
}

#[shoop_test]
fn refusals_and_zero_frames_do_not_change_output_or_smoothing_state() {
    let mut store = store();
    let snapshot = upload(&mut store, 1, &[4., 8., 12., 16.]);
    let index = store.read_index(snapshot.id).unwrap();
    let stereo = upload(&mut store, 2, &[1., 2.]);
    let wrong = store.read_index(stereo.id).unwrap();
    let empty = upload(&mut store, 1, &[]);
    let empty_index = store.read_index(empty.id).unwrap();
    let mut foreign = self::store();
    let other = upload(&mut foreign, 1, &[1.]);
    let foreign_index = foreign.read_index(other.id).unwrap();
    let mut renderer =
        AudioRenderer::prepare(1, 4, Smoothing::LinearRestart { frames: 4 }).unwrap();
    let mut output = [99.];
    assert_no_heap(|| {
        renderer
            .render(
                &mut store.render_io(),
                Playback {
                    source: Some(index),
                    position: 0,
                    gain: 1.,
                    balance: 0.0,
                    discontinuity: true,
                },
                &mut [&mut output],
            )
            .unwrap();
        assert_eq!(output, [1.]);
        for (source, position, gain, expected) in [
            (index, 4, 1., MediaError::InvalidFrame),
            (index, u64::MAX, 1., MediaError::InvalidFrame),
            (index, 1, f32::NAN, MediaError::InvalidParameter),
            (wrong, 0, 1., MediaError::InvalidShape),
            (empty_index, 0, 1., MediaError::InvalidFrame),
            (foreign_index, 0, 1., MediaError::InvalidId),
        ] {
            assert_eq!(
                renderer.render(
                    &mut store.render_io(),
                    Playback {
                        source: Some(source),
                        position,
                        gain,
                        balance: 0.0,
                        discontinuity: true
                    },
                    &mut [&mut output]
                ),
                Err(expected)
            );
            assert_eq!(output, [1.]);
        }
        renderer
            .render(
                &mut store.render_io(),
                Playback {
                    source: None,
                    position: u64::MAX,
                    gain: f32::NAN,
                    balance: 0.0,
                    discontinuity: true,
                },
                &mut [&mut []],
            )
            .unwrap();
        renderer
            .render(
                &mut store.render_io(),
                Playback {
                    source: Some(index),
                    position: 1,
                    gain: 1.,
                    balance: 0.0,
                    discontinuity: false,
                },
                &mut [&mut output],
            )
            .unwrap();
        assert_eq!(output, [4.]);
        renderer
            .render(
                &mut store.render_io(),
                Playback {
                    source: None,
                    position: 0,
                    gain: 1.,
                    balance: 0.0,
                    discontinuity: false,
                },
                &mut [&mut output],
            )
            .unwrap();
        assert_eq!(output, [0.]);
        renderer
            .render(
                &mut store.render_io(),
                Playback {
                    source: Some(index),
                    position: 0,
                    gain: 1.,
                    balance: 0.0,
                    discontinuity: true,
                },
                &mut [&mut output],
            )
            .unwrap();
        assert_eq!(output, [1.]);
        store.release_snapshot(snapshot.id).unwrap();
        assert_eq!(
            renderer.render(
                &mut store.render_io(),
                Playback {
                    source: Some(index),
                    position: 1,
                    gain: 1.,
                    balance: 0.0,
                    discontinuity: false
                },
                &mut [&mut output]
            ),
            Err(MediaError::InvalidId)
        );
        assert_eq!(output, [1.]);
    });
    cleanup(
        &mut store,
        &[stereo, empty],
        &[snapshot.resource, stereo.resource, empty.resource],
    );
    cleanup(&mut foreign, &[other], &[other.resource]);
    for (channels, frames) in [(0, 1), (1, 0), (usize::MAX, 2), (2, usize::MAX)] {
        assert!(matches!(
            AudioRenderer::prepare(channels, frames, Smoothing::None),
            Err(MediaError::InvalidConfig)
        ));
        assert!(matches!(
            AudioCapture::prepare(channels, frames),
            Err(MediaError::InvalidConfig)
        ));
    }
}
#[shoop_test]
fn planar_capture_freezes_immediately_and_separate_playback_fx_capture_keeps_old_take() {
    for partitions in [vec![5], vec![1; 5], vec![2, 3]] {
        let mut store = store();
        let donor = upload(&mut store, 1, &[0.; 4]);
        let resource = store.create().unwrap();
        let group = store
            .bind_capture_group(&[CaptureMember {
                resource,
                format: Format::Audio { channels: 2 },
                midi_events: 0,
                midi_bytes: 0,
            }])
            .unwrap();
        let mut capture = AudioCapture::prepare(2, 64).unwrap();
        let mut renderer = AudioRenderer::prepare(2, 64, Smoothing::None).unwrap();
        let left = [1., 2., 3., 4., 5.];
        let right = [-1., -2., -3., -4., -5.];
        let mut rendered = [[0.; 5]; 2];
        let mut frozen = [None];
        let mut processed = [None];
        assert_no_heap(|| {
            let take = store.prepare_capture_take(group, &[]).unwrap();
            store.activate_capture_take(take, Frame(0)).unwrap();
            store.begin_callback();
            let mut cursor = store.cursor(donor.id).unwrap();
            assert_eq!(store.read(&mut cursor, &mut [0; 16]), Ok(16));
            let mut offset = 0;
            for frames in &partitions {
                let interval = store
                    .reserve_capture_interval(take, Frame(offset as u64), *frames)
                    .unwrap();
                capture
                    .stage(
                        &mut store.render_io(),
                        interval,
                        0,
                        &[
                            &left[offset..offset + frames],
                            &right[offset..offset + frames],
                        ],
                    )
                    .unwrap();
                assert_eq!(
                    store.info(resource).unwrap().description.frames,
                    offset as u64
                );
                store.commit_capture_interval(interval).unwrap();
                offset += frames;
            }
            store
                .freeze_capture_take(take, Frame(5), None, &mut frozen)
                .unwrap();
            let old = frozen[0].unwrap();
            let old_index = store.read_index(old.id).unwrap();
            let [l, r] = &mut rendered;
            renderer
                .render(
                    &mut store.render_io(),
                    Playback {
                        source: Some(old_index),
                        position: 0,
                        gain: 1.,
                        balance: 0.0,
                        discontinuity: true,
                    },
                    &mut [l, r],
                )
                .unwrap();
            assert_eq!(*l, left);
            assert_eq!(*r, right);
            let next = store.prepare_capture_take(group, &[]).unwrap();
            store.activate_capture_take(next, Frame(5)).unwrap();
            let mut offset = 0;
            for frames in &partitions {
                let interval = store
                    .reserve_capture_interval(next, Frame(5 + offset as u64), *frames)
                    .unwrap();
                renderer
                    .render(
                        &mut store.render_io(),
                        Playback {
                            source: Some(old_index),
                            position: offset as u64,
                            gain: 2.,
                            balance: 0.0,
                            discontinuity: offset == 0,
                        },
                        &mut [
                            &mut l[offset..offset + frames],
                            &mut r[offset..offset + frames],
                        ],
                    )
                    .unwrap();
                capture
                    .stage(
                        &mut store.render_io(),
                        interval,
                        0,
                        &[&l[offset..offset + frames], &r[offset..offset + frames]],
                    )
                    .unwrap();
                store.commit_capture_interval(interval).unwrap();
                offset += frames;
            }
            store
                .freeze_capture_take(next, Frame(10), None, &mut processed)
                .unwrap();
            let index = store.read_index(processed[0].unwrap().id).unwrap();
            renderer
                .render(
                    &mut store.render_io(),
                    Playback {
                        source: Some(index),
                        position: 0,
                        gain: 1.,
                        balance: 0.0,
                        discontinuity: true,
                    },
                    &mut [l, r],
                )
                .unwrap();
            for frame in 0..5 {
                assert_eq!(l[frame], left[frame] * 2.);
                assert_eq!(r[frame], right[frame] * 2.);
            }
            renderer
                .render(
                    &mut store.render_io(),
                    Playback {
                        source: Some(old_index),
                        position: 0,
                        gain: 1.,
                        balance: 0.0,
                        discontinuity: true,
                    },
                    &mut [l, r],
                )
                .unwrap();
            assert_eq!(*l, left);
            assert_eq!(*r, right);
            assert_eq!(store.copy_remaining(), 0);
            store.release_capture_group(group).unwrap();
        });
        cleanup(
            &mut store,
            &[frozen[0].unwrap(), processed[0].unwrap(), donor],
            &[resource, donor.resource],
        );
    }
}

#[shoop_test]
fn one_sample_content_executes_every_wrap_without_a_renderer_clock_or_quota() {
    let target = LoopId::from_parts(0, 1).unwrap();
    for journal_events in [0, 1, 1024] {
        for partitions in [vec![64, 64], vec![1; 128], vec![7, 53, 19, 49]] {
            let mut store = store();
            let snapshot = upload(&mut store, 1, &[0.75]);
            let index = store.read_index(snapshot.id).unwrap();
            let mut renderer = AudioRenderer::prepare(1, 64, Smoothing::None).unwrap();
            let topology = compile(
                Limits {
                    journal_events,
                    ..Limits::default()
                },
                &TopologyDescription {
                    loops: vec![LoopSpec {
                        id: target,
                        kind: LoopKind::Primitive,
                        length: 1,
                        sync_source: None,
                    }],
                    triggers: vec![],
                    programs: vec![],
                },
            )
            .unwrap();
            let mut resolver = Resolver::prepare(topology).unwrap();
            let mut output = [0.; 128];
            let mut wraps = 0;
            assert_no_heap(|| {
                resolver
                    .submit(Request {
                        target,
                        sequence: Sequence(1),
                        at: Frame(0),
                        timing: Timing::At,
                        transition: Transition::Play {
                            position: 0,
                            retrigger: false,
                        },
                    })
                    .unwrap();
                let mut end = 0;
                for frames in &partitions {
                    end += frames;
                    loop {
                        let interval = resolver.preview(Frame(end), &[]).unwrap();
                        let decision = resolver.decisions().unwrap()[0];
                        let state = decision.state;
                        assert!(decision.playback_discontinuity);
                        resolver.commit().unwrap();
                        wraps += resolver
                            .events()
                            .iter()
                            .filter(|e| e.kind == EventKind::Wrap)
                            .count();
                        assert_eq!(interval.end.0 - interval.start.0, 1);
                        renderer
                            .render(
                                &mut store.render_io(),
                                Playback {
                                    source: Some(index),
                                    position: state.position,
                                    gain: 1.,
                                    balance: 0.0,
                                    discontinuity: decision.playback_discontinuity,
                                },
                                &mut [
                                    &mut output[interval.start.0 as usize..interval.end.0 as usize]
                                ],
                            )
                            .unwrap();
                        resolver.finish(interval.end).unwrap();
                        if interval.end.0 == end {
                            break;
                        }
                    }
                }
                assert_eq!(output, [0.75; 128]);
                assert_eq!(resolver.state(target).unwrap().cycles, 127);
                assert_eq!(resolver.state(target).unwrap().position, 1);
                assert_eq!(wraps, if journal_events == 0 { 0 } else { 127 });
            });
            cleanup(&mut store, &[snapshot], &[snapshot.resource]);
        }
    }
}

#[shoop_test]
fn capture_shape_and_token_refusals_never_publish_a_partial_contribution() {
    let mut store = store();
    let resource = store.create().unwrap();
    let group = store
        .bind_capture_group(&[CaptureMember {
            resource,
            format: Format::Audio { channels: 2 },
            midi_events: 0,
            midi_bytes: 0,
        }])
        .unwrap();
    let mut capture = AudioCapture::prepare(2, 4).unwrap();
    let mut output = [None];
    assert_no_heap(|| {
        let take = store.prepare_capture_take(group, &[]).unwrap();
        store.activate_capture_take(take, Frame(0)).unwrap();
        let interval = store.reserve_capture_interval(take, Frame(0), 2).unwrap();
        assert_eq!(
            capture.stage(&mut store.render_io(), interval, 0, &[&[1.]]),
            Err(MediaError::InvalidShape)
        );
        assert_eq!(
            capture.stage(&mut store.render_io(), interval, 0, &[&[1.], &[]]),
            Err(MediaError::InvalidShape)
        );
        assert_eq!(
            capture.stage(&mut store.render_io(), interval, 0, &[&[], &[]]),
            Err(MediaError::InvalidFrame)
        );
        assert_eq!(
            capture.stage(&mut store.render_io(), interval, 0, &[&[1.; 5], &[2.; 5]]),
            Err(MediaError::InvalidShape)
        );
        assert_eq!(
            capture.stage(&mut store.render_io(), interval, 0, &[&[1.], &[2.]]),
            Err(MediaError::InvalidShape)
        );
        assert_eq!(store.info(resource).unwrap().description.frames, 0);
        capture
            .stage(&mut store.render_io(), interval, 0, &[&[1., 2.], &[3., 4.]])
            .unwrap();
        assert_eq!(
            capture.stage(&mut store.render_io(), interval, 0, &[&[1., 2.], &[3., 4.]]),
            Err(MediaError::InvalidState)
        );
        store.cancel_capture_interval(interval).unwrap();
        let next = store.reserve_capture_interval(take, Frame(0), 2).unwrap();
        assert_eq!(
            capture.stage(&mut store.render_io(), interval, 0, &[&[1., 2.], &[3., 4.]]),
            Err(MediaError::InvalidId)
        );
        store.cancel_capture_interval(next).unwrap();
        store
            .freeze_capture_take(take, Frame(0), None, &mut output)
            .unwrap();
        assert_eq!(output[0].unwrap().description.frames, 0);
        assert_eq!(output[0].unwrap().description.bytes, 0);
        store.release_capture_group(group).unwrap();
    });
    cleanup(&mut store, &[output[0].unwrap()], &[resource]);
}
