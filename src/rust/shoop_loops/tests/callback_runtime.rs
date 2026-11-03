#[path = "../../rt_runtime/tests/support/mod.rs"]
mod support;
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock, MidiBuffer},
    executor::{Command, GraphRuntime, PreparedNode},
    graph::{Connection, Endpoint, GraphDescription, PortCounts, PortType},
    midi_state::MidiState,
    planner::ConsumerAccess,
    processor::{Cadence, ProcessBlock, Processor},
    resource::*,
    schedule::Action,
    transport::Lane,
    Config, Error, Frame, NodeId, PlannerId, Revision,
};
use shoop_loops::{
    audio::Smoothing,
    media,
    runtime::{
        callback::{self, CallbackNode, CallbackPlanner},
        Binding, Settings,
    },
    *,
};
use shoop_wasm_test_support::shoop_test;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
#[path = "support/scoped_runtime.rs"]
mod fixture;
use fixture::*;

#[shoop_test]
fn real_dry_fx_wet_capture_uses_shared_wrapping_timing_and_actual_midi_checkpoint() {
    for origin in [0, (1u64 << 32) + 37] {
        for partitions in [vec![17], vec![3, 4, 1, 9], vec![1; 17]] {
            let (mut control, mut runtime, calls) = setup_at(32, origin);
            command(
                &mut control,
                callback::Operation::Transition {
                    target: id(),
                    at: Frame(origin),
                    timing: Timing::At,
                    transition: Transition::Play {
                        position: 0,
                        retrigger: false,
                    },
                },
            );
            command(
                &mut control,
                callback::Operation::Capture {
                    target: id(),
                    start: Frame(origin + 5),
                    end: Frame(origin + 15),
                },
            );
            let mut audio = Vec::new();
            let mut midi = Vec::new();
            for frames in &partitions {
                let (left, right, events) = render(&mut runtime, *frames).unwrap();
                assert_eq!(
                    right,
                    left.iter().map(|sample| sample * 10.0).collect::<Vec<_>>()
                );
                audio.extend(left);
                midi.extend(events);
            }
            assert_eq!(
                audio,
                (0..17)
                    .map(|frame| 3.0 * (frame % 4 + 1) as f32)
                    .collect::<Vec<_>>()
            );
            let midi: Vec<_> = midi
                .into_iter()
                .map(|(frame, data)| (frame - origin, data))
                .collect();
            assert_eq!(
                midi,
                vec![
                    (1, vec![0xf8]),
                    (2, vec![0x90, 72, 91]),
                    (5, vec![0xf8]),
                    (9, vec![0xf8]),
                    (9, vec![0x80, 72, 64]),
                    (13, vec![0xf8])
                ]
            );
            assert_eq!(calls.load(Ordering::Relaxed), partitions.len());
            let mut at = origin;
            assert_eq!(
                calls.snapshot(),
                partitions
                    .iter()
                    .map(|frames| {
                        let call = (at, *frames);
                        at += *frames as u64;
                        call
                    })
                    .collect::<Vec<_>>()
            );
            let replies: Vec<_> = std::iter::from_fn(|| control.try_execution()).collect();
            assert_eq!(replies.len(), 2);
            let captured = replies
                .iter()
                .find_map(|reply| match reply.operation_result {
                    Some(callback::Reply::Captured {
                        media, start, end, ..
                    }) => {
                        assert_eq!((start, end), (Frame(origin + 5), Frame(origin + 15)));
                        Some(media)
                    }
                    _ => None,
                })
                .unwrap();
            let (mut planner, mut resources) = runtime.into_planner_and_resources();
            assert_eq!(planner.frame(), Frame(origin + 17));
            assert_eq!(planner.state(id()).unwrap().position, 1);
            assert_eq!(planner.work_counts().0 as usize, partitions.len());
            let view = planner.ledger().view(captured).unwrap();
            let source = view.audio.unwrap();
            assert_eq!(source.snapshot.description.frames, 10);
            let mut cursor = resources.render_io().cursor_at(source.index, 0).unwrap();
            let mut bytes = vec![0; 80];
            resources.render_io().read(&mut cursor, &mut bytes).unwrap();
            let actual: Vec<_> = bytes
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
                .collect();
            let expected: Vec<_> = (5..15)
                .flat_map(|frame| {
                    let x = 3.0 * (frame % 4 + 1) as f32;
                    [x, x * 10.0]
                })
                .collect();
            assert_eq!(actual, expected);
            let source = view.midi.unwrap();
            let mut seek = resources.render_io().midi_seek(source.index, 0).unwrap();
            assert_eq!(
                seek.state.channel(0).unwrap().note(72).unwrap().velocity(),
                91
            );
            let mut events = Vec::new();
            while let Some(event) = resources.render_io().midi_next(&mut seek.cursor).unwrap() {
                let mut bytes = vec![0; event.byte_len()];
                resources.render_io().midi_read(event, &mut bytes).unwrap();
                events.push((event.frame(), bytes));
            }
            assert_eq!(
                events,
                [
                    (0, vec![0xf8]),
                    (4, vec![0xf8]),
                    (4, vec![0x80, 72, 64]),
                    (8, vec![0xf8])
                ]
            );
            planner.release_all(&mut resources).unwrap();
            assert_eq!(resources.capture_group_usage().free_groups, 2);
        }
    }
}

#[shoop_test]
fn callback_target_actions_are_refused_at_exact_frames_without_cutting_dsp_or_replanning() {
    let (mut control, mut runtime, calls) = setup(32);
    play(&mut control);
    schedule(
        &mut control,
        Frame(3),
        Action::Parameter {
            node: node(1),
            parameter: 0,
            value: 1.0,
        },
        &[],
    );
    schedule(
        &mut control,
        Frame(5),
        Action::Processor {
            node: node(1),
            operation: (),
        },
        &[],
    );
    schedule(
        &mut control,
        Frame(7),
        Action::Midi {
            node: node(1),
            port: 0,
        },
        &[0x90, 60, 100],
    );
    schedule(
        &mut control,
        Frame(11),
        Action::Planner {
            planner: planner(),
            operation: callback::Operation::State { target: id() },
        },
        &[],
    );
    assert!(render(&mut runtime, 0).unwrap().0.is_empty());
    assert_eq!(runtime.next_frame(), Frame(0));
    assert_eq!(calls.load(Ordering::Relaxed), 0);
    assert!(control.try_execution().is_none());
    let (audio, _, _) = render(&mut runtime, 17).unwrap();
    assert_eq!(
        audio,
        (0..17)
            .map(|frame| 3.0 * (frame % 4 + 1) as f32)
            .collect::<Vec<_>>()
    );
    let replies: Vec<_> = std::iter::from_fn(|| control.try_execution()).collect();
    assert_eq!(replies.len(), 5);
    for (reply, expected) in replies
        .iter()
        .filter(|reply| reply.result.is_err())
        .zip([3, 5, 7, 11])
    {
        assert_eq!(reply.result, Err(Error::InvalidFrame));
        assert_eq!(reply.delivered_frame, Some(Frame(expected)));
        assert_eq!(reply.actual_frame, Frame(expected));
    }
    assert_eq!(
        replies.iter().filter(|reply| reply.result.is_err()).count(),
        4
    );
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    let (mut planner, mut resources) = runtime.into_planner_and_resources();
    assert_eq!(planner.work_counts().0, 1);
    assert_eq!(planner.frame(), Frame(17));
    planner.release_all(&mut resources).unwrap();
}

#[shoop_test]
fn actual_consumer_faults_cancel_capture_atomically_and_do_not_replay_time() {
    for target in [node(1), node(2)] {
        let (mut control, mut runtime, calls) = setup(32);
        play(&mut control);
        command(
            &mut control,
            callback::Operation::Capture {
                target: id(),
                start: Frame(5),
                end: Frame(15),
            },
        );
        schedule(
            &mut control,
            Frame(0),
            Action::Parameter {
                node: target,
                parameter: 0,
                value: 1.0,
            },
            &[],
        );
        let error = if target == node(1) {
            Error::Capacity
        } else {
            Error::Incomplete
        };
        assert_eq!(render(&mut runtime, 17).err(), Some(error));
        assert_eq!(runtime.next_frame(), Frame(17));
        let replies: Vec<_> = std::iter::from_fn(|| control.try_execution()).collect();
        let capture = replies
            .iter()
            .find(|reply| {
                matches!(
                    reply.action,
                    Action::Planner {
                        operation: callback::Operation::Capture { .. },
                        ..
                    }
                )
            })
            .unwrap();
        assert_eq!(capture.result, Err(error));
        assert!(capture.operation_result.is_none());
        let (audio, _, _) = render(&mut runtime, 3).unwrap();
        assert_eq!(
            audio,
            (17..20)
                .map(|frame| 3.0 * (frame % 4 + 1) as f32)
                .collect::<Vec<_>>()
        );
        assert_eq!(calls.load(Ordering::Relaxed), 2);
        let (mut planner, mut resources) = runtime.into_planner_and_resources();
        assert_eq!(planner.frame(), Frame(20));
        assert_eq!(planner.ledger().latest(id()).unwrap(), None);
        assert_eq!(planner.state(id()).unwrap().position, 4);
        planner.release_all(&mut resources).unwrap();
    }
}

#[shoop_test]
fn exhausted_capture_preserves_owning_export_and_playback_continues_without_false_success() {
    let (mut control, mut runtime, calls) = setup(32);
    play(&mut control);
    command(
        &mut control,
        callback::Operation::Capture {
            target: id(),
            start: Frame(5),
            end: Frame(15),
        },
    );
    render(&mut runtime, 17).unwrap();
    while control.try_execution().is_some() {}
    command(
        &mut control,
        callback::Operation::Export {
            target: id(),
            latest: true,
        },
    );
    render(&mut runtime, 1).unwrap();
    let export = std::iter::from_fn(|| control.try_execution())
        .find_map(|reply| match reply.operation_result {
            Some(callback::Reply::Export { id, media }) => Some((id, media)),
            _ => None,
        })
        .unwrap();
    command(
        &mut control,
        callback::Operation::Capture {
            target: id(),
            start: Frame(18),
            end: Frame(100_000),
        },
    );
    let mut refusal = None;
    for _ in 0..256 {
        render(&mut runtime, 32).unwrap();
        for reply in std::iter::from_fn(|| control.try_execution()) {
            if matches!(
                reply.action,
                Action::Planner {
                    operation: callback::Operation::Capture { .. },
                    ..
                }
            ) {
                refusal = Some(reply);
            }
        }
        if refusal.is_some() {
            break;
        }
    }
    let refusal = refusal.expect("finite capture pool must be exhausted");
    assert!(matches!(
        refusal.result,
        Err(Error::Capacity | Error::Exhausted)
    ));
    assert!(refusal.operation_result.is_none());
    let next = runtime.next_frame();
    let before = calls.load(Ordering::Relaxed);
    let (audio, _, _) = render(&mut runtime, 3).unwrap();
    assert_eq!(calls.load(Ordering::Relaxed), before + 1);
    assert_eq!(
        audio,
        (next.0..next.0 + 3)
            .map(|frame| 3.0 * (frame % 4 + 1) as f32)
            .collect::<Vec<_>>()
    );
    let (mut planner, mut resources) = runtime.into_planner_and_resources();
    assert_eq!(planner.ledger().exported(export.0).unwrap(), export.1);
    assert_eq!(planner.ledger().latest(id()).unwrap(), Some(export.1.media));
    let source = export.1.view.audio.unwrap();
    let mut cursor = resources.render_io().cursor_at(source.index, 0).unwrap();
    let mut bytes = [0; 80];
    assert_eq!(
        resources.render_io().read(&mut cursor, &mut bytes).unwrap(),
        80
    );
    assert_eq!(f32::from_le_bytes(bytes[..4].try_into().unwrap()), 6.0);
    planner.release_all(&mut resources).unwrap();
}

#[shoop_test]
fn real_interleaved_loop_region_feeds_full_block_bus_with_exact_audio_and_midi() {
    let (mut control, mut runtime, calls) = setup_bus();
    assert!(control
        .submit(
            Lane::Normal,
            Command::Schedule {
                at: Frame(0),
                action: Action::Planner {
                    planner: planner(),
                    operation: shoop_loops::runtime::Operation::Transition {
                        target: id(),
                        timing: Timing::At,
                        transition: Transition::Play {
                            position: 0,
                            retrigger: false
                        }
                    }
                },
                payload: Box::new([])
            }
        )
        .is_ok());
    let mut left = [0.0; 17];
    let mut right = [0.0; 17];
    let mut midi = [MidiBuffer::new(4096, 16384).unwrap()];
    support::assert_no_heap(|| {
        runtime.process(
            AudioBlock {
                frame: Frame(0),
                frames: 17,
                inputs: &[],
                outputs: &mut [&mut left, &mut right],
            },
            MidiBlock {
                inputs: &[],
                outputs: &mut midi,
            },
        )
    })
    .unwrap();
    assert_eq!(
        left.to_vec(),
        (0..17)
            .map(|frame| 3.0 * (frame % 4 + 1) as f32)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        right.to_vec(),
        left.iter().map(|x| x * 10.0).collect::<Vec<_>>()
    );
    let actual: Vec<_> = (0..midi[0].len())
        .map(|i| {
            let (at, data) = midi[0].get(i).unwrap();
            (at, data.to_vec())
        })
        .collect();
    assert_eq!(
        actual,
        [
            (1, vec![0xf8]),
            (2, vec![0x90, 72, 91]),
            (5, vec![0xf8]),
            (9, vec![0xf8]),
            (9, vec![0x80, 72, 64]),
            (13, vec![0xf8])
        ]
    );
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert_eq!(calls.snapshot(), [(0, 17)]);
    assert_eq!(runtime.diagnostics().subranges, 5);
    assert_eq!(runtime.diagnostics().nodes_processed, 11);
    assert!(matches!(
        control.try_execution().unwrap().operation_result,
        Some(shoop_loops::runtime::Reply::Transition(_))
    ));
    let (planner, _) = runtime.into_planner_and_resources();
    assert_eq!(planner.media_usage().free_media, 7);
}

#[shoop_test]
fn midi_overflow_uses_reserved_recovery_and_refuses_capture_instead_of_saving_truncation() {
    let (mut control, mut runtime, calls) = setup(32);
    play(&mut control);
    command(
        &mut control,
        callback::Operation::Capture {
            target: id(),
            start: Frame(5),
            end: Frame(15),
        },
    );
    schedule(
        &mut control,
        Frame(0),
        Action::Parameter {
            node: node(1),
            parameter: 1,
            value: 1.0,
        },
        &[],
    );
    let (audio, _, midi) = render(&mut runtime, 17).unwrap();
    assert_eq!(
        audio,
        (0..17)
            .map(|frame| 3.0 * (frame % 4 + 1) as f32)
            .collect::<Vec<_>>()
    );
    assert!(runtime.diagnostics().midi_overflows > 0);
    assert!(midi.is_empty());
    assert!(runtime.midi_recovery_pending(0).unwrap());
    let mut sink = MidiBuffer::new(32, 96).unwrap();
    assert_eq!(
        support::assert_no_heap(|| runtime.drain_midi_recovery(0, &mut sink, 17)).unwrap(),
        32
    );
    assert!(!runtime.midi_recovery_pending(0).unwrap());
    for event in 0..sink.len() {
        let (at, packet) = sink.get(event).unwrap();
        assert_eq!(at, 0);
        assert_eq!(
            packet,
            [
                0xb0 | (event / 2) as u8,
                if event % 2 == 0 { 120 } else { 123 },
                0
            ]
        );
    }
    let capture = std::iter::from_fn(|| control.try_execution())
        .find(|reply| {
            matches!(
                reply.action,
                Action::Planner {
                    operation: callback::Operation::Capture { .. },
                    ..
                }
            )
        })
        .unwrap();
    assert_eq!(capture.result, Err(Error::Full));
    assert!(capture.operation_result.is_none());
    let (_, _, midi) = render(&mut runtime, 4).unwrap();
    assert_eq!(midi, [(17, vec![0xf8])]);
    assert_eq!(calls.load(Ordering::Relaxed), 2);
    let (mut planner, mut resources) = runtime.into_planner_and_resources();
    assert!(planner.ledger().latest(id()).unwrap().is_none());
    planner.release_all(&mut resources).unwrap();
}

#[shoop_test]
fn missing_actual_consumer_completion_cancels_the_uncommitted_group() {
    let (mut control, mut runtime, _) = setup(32);
    play(&mut control);
    command(
        &mut control,
        callback::Operation::Capture {
            target: id(),
            start: Frame(5),
            end: Frame(15),
        },
    );
    schedule(
        &mut control,
        Frame(0),
        Action::Parameter {
            node: node(2),
            parameter: 1,
            value: 1.0,
        },
        &[],
    );
    assert_eq!(render(&mut runtime, 17).err(), Some(Error::Incomplete));
    let capture = std::iter::from_fn(|| control.try_execution())
        .find(|reply| {
            matches!(
                reply.action,
                Action::Planner {
                    operation: callback::Operation::Capture { .. },
                    ..
                }
            )
        })
        .unwrap();
    assert_eq!(capture.result, Err(Error::Incomplete));
    assert!(capture.operation_result.is_none());
    render(&mut runtime, 1).unwrap();
    let (mut planner, mut resources) = runtime.into_planner_and_resources();
    assert!(planner.ledger().latest(id()).unwrap().is_none());
    planner.release_all(&mut resources).unwrap();
}

#[shoop_test]
fn dense_one_frame_loop_boundaries_fit_prepared_plan_without_renewing_callback_work() {
    let (mut control, mut runtime, calls) = setup_details(32, 0, 1);
    play(&mut control);
    command(
        &mut control,
        callback::Operation::Capture {
            target: id(),
            start: Frame(5),
            end: Frame(31),
        },
    );
    let (left, right, midi) = render(&mut runtime, 32).unwrap();
    assert_eq!(left, vec![3.0; 32]);
    assert_eq!(right, vec![30.0; 32]);
    assert_eq!(
        midi.iter()
            .filter(|(_, bytes)| bytes == &[0xf8])
            .map(|(frame, _)| *frame)
            .collect::<Vec<_>>(),
        (0..32).collect::<Vec<_>>()
    );
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert_eq!(runtime.diagnostics().nodes_processed, 3);
    assert_eq!(runtime.diagnostics().scheduled_events, 2);
    let captures = std::iter::from_fn(|| control.try_execution())
        .filter(|reply| {
            matches!(
                reply.operation_result,
                Some(callback::Reply::Captured { .. })
            )
        })
        .count();
    assert_eq!(captures, 1);
    let (mut planner, mut resources) = runtime.into_planner_and_resources();
    assert_eq!(planner.work_counts(), (1, 32));
    assert_eq!(
        planner.plan_storage_bytes(),
        32 * std::mem::size_of::<rt_runtime::planner::DecisionSpan<callback::CallbackDecision>>()
    );
    planner.release_all(&mut resources).unwrap();
}

#[shoop_test]
fn inadequate_plan_storage_is_refused_before_touching_live_media_or_timing() {
    let (config, mut resources, topology, imports) = parts_length(1);
    let before = resources.usage();
    let groups = resources.capture_group_usage();
    let result = CallbackPlanner::prepare(
        config,
        topology,
        &mut resources,
        Settings {
            planner: planner(),
            normal_requests: 4,
            media_slots: 8,
            export_slots: 4,
        },
        vec![Binding {
            target: id(),
            playback: node(0),
            capture: node(2),
            channels: 2,
            midi: true,
            gain: 1.0,
            smoothing: Smoothing::None,
            midi_events: 1024,
            midi_bytes: 8192,
            packet_bytes: 8192,
        }],
        imports,
        31,
    );
    assert!(matches!(
        result,
        Err(shoop_loops::runtime::Failure::Runtime(Error::InvalidConfig))
    ));
    assert_eq!(resources.usage(), before);
    assert_eq!(resources.capture_group_usage(), groups);
}

#[shoop_test]
fn two_synced_owners_share_one_resolver_plan_and_keep_distinct_media_and_receipts() {
    let (mut control, mut runtime, calls) = setup_two_owners();
    play(&mut control);
    let other = LoopId::from_parts(1, 1).unwrap();
    command(
        &mut control,
        callback::Operation::Transition {
            target: other,
            at: Frame(1),
            timing: Timing::AfterTicks {
                source: id(),
                skip: 0,
            },
            transition: Transition::Play {
                position: 0,
                retrigger: false,
            },
        },
    );
    for target in [id(), other] {
        command(
            &mut control,
            callback::Operation::Capture {
                target,
                start: Frame(5),
                end: Frame(15),
            },
        );
    }
    let (audio, _, midi) = render(&mut runtime, 17).unwrap();
    assert_eq!(
        audio,
        (0..17)
            .map(|frame| 3.0 * (frame % 4 + 1) as f32 * if frame < 4 { 1.0 } else { 1.5 })
            .collect::<Vec<_>>()
    );
    assert_eq!(
        midi.iter()
            .filter(|(_, packet)| packet == &[0xf8])
            .map(|(frame, _)| *frame)
            .collect::<Vec<_>>(),
        [1, 5, 5, 9, 9, 13, 13]
    );
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    let replies: Vec<_> = std::iter::from_fn(|| control.try_execution()).collect();
    assert_eq!(replies.len(), 4);
    let transition = replies
        .iter()
        .find_map(|reply| match reply.operation_result {
            Some(callback::Reply::Transition(completion)) if completion.requested == Frame(1) => {
                Some(completion)
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(transition.effective, Frame(1));
    assert_eq!(transition.outcome, Outcome::Applied { frame: Frame(4) });
    let captures: Vec<_> = replies
        .iter()
        .filter_map(|reply| match reply.operation_result {
            Some(callback::Reply::Captured { target, media, .. }) => Some((target, media)),
            _ => None,
        })
        .collect();
    assert_eq!(captures.len(), 2);
    assert_ne!(captures[0].1, captures[1].1);
    let (mut planner, mut resources) = runtime.into_planner_and_resources();
    assert_eq!(planner.work_counts(), (1, 8));
    for target in [id(), other] {
        assert_eq!(planner.state(target).unwrap().position, 1);
    }
    for (target, media) in captures {
        assert_eq!(
            planner.ledger().view(media).unwrap().description.owner,
            target
        );
    }
    planner.release_all(&mut resources).unwrap();
}
