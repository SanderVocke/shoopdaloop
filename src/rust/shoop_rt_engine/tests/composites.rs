mod support;
use shoop_rt_protocol::*;
use shoop_wasm_test_support::shoop_test;
use support::*;

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

fn transition(loop_id: LoopSlotId, mode: LoopMode) -> Request {
    Request::LoopTransition {
        loop_id,
        mode,
        delay: None,
    }
}
fn setup() -> Harness {
    setup_with_source(10)
}
fn setup_with_source(source_frames: usize) -> Harness {
    let mut d = description();
    d.composite_entries = 2;
    d.composite_segments = 2;
    d.timing_relations = 6;
    setup_engine(d, source_frames)
}
fn setup_engine(mut d: shoop_rt_engine::EngineDescription, source_frames: usize) -> Harness {
    d.ramp_frames = 0;
    let mut h = Harness::new(&d);
    h.done(Request::CreateStrip {
        strip: strip(0, 1),
        shape: shape(1, 1, false),
    });
    h.done(Request::Connect {
        port: port(strip(0, 1), PortKind::AudioIn, 0),
        host: audio_in(0),
        connected: true,
    });
    h.done(route(strip(0, 1), 0, 0, true));
    for index in 0..4 {
        h.done(Request::CreateLoop {
            loop_id: loop_id(index, 1),
            strip: strip(0, 1),
        });
    }
    for (index, frames) in [(0, source_frames), (1, 4), (2, 4)] {
        h.input = Box::new(move |_, _| index as f32);
        h.send(transition(loop_id(index, 1), LoopMode::Recording));
        h.run(frames, 1);
        h.done(transition(loop_id(index, 1), LoopMode::Stopped));
    }
    h
}
fn plan(id: u64, version: u64) -> CompositeDescription {
    CompositeDescription {
        id,
        loop_id: loop_id(3, 2),
        sync: Some(loop_id(0, 1)),
        previous: (version == 1).then_some(loop_id(3, 1)),
        expected_version: version - 1,
        version,
        kind: CompositeKind::Regular,
        unit_frames: 2,
        iterations: 4,
        entries: 2,
        segments: 2,
        sample_rate: 1000,
        quantum: 1,
    }
}
fn spans() -> [CompositeSpan; 2] {
    [
        CompositeSpan {
            target: loop_id(1, 1),
            start: 0,
            end: 2,
            occurrence_start: 0,
            ordinal: 0,
            mode: LoopMode::Playing,
        },
        CompositeSpan {
            target: loop_id(2, 1),
            start: 2,
            end: 4,
            occurrence_start: 2,
            ordinal: 1,
            mode: LoopMode::Playing,
        },
    ]
}
fn stage(h: &mut Harness, d: CompositeDescription, spans: &[CompositeSpan]) {
    h.done(Request::Composite(CompositeCommand::Begin(d)));
    for (index, &span) in spans.iter().enumerate() {
        h.done(Request::Composite(CompositeCommand::Span {
            id: d.id,
            index: index as u32,
            span,
        }));
    }
}

#[shoop_test]
fn converted_composites_render_their_frame_plan_after_source_loss_and_resync_fresh_ids() {
    let mut h = setup();
    stage(&mut h, plan(1, 1), &spans());
    h.done(Request::Composite(CompositeCommand::Commit { id: 1 }));
    assert_eq!(
        h.apply(transition(loop_id(3, 1), LoopMode::Playing)),
        Outcome::Refused(Refusal::InvalidId)
    );
    h.done(Request::LoopSync {
        loop_id: loop_id(3, 2),
        source: Some(loop_id(0, 1)),
    });
    let origin = h.frame();
    h.send(transition(loop_id(0, 1), LoopMode::Playing));
    h.send(transition(loop_id(3, 2), LoopMode::Playing));
    h.run(3, 1);
    h.send(transition(loop_id(0, 1), LoopMode::Stopped));
    h.run(18, 1);
    assert_eq!(
        &h.outputs[0][origin as usize..origin as usize + 20],
        &(0..20)
            .map(|n| if n % 8 < 4 { 1. } else { 2. })
            .collect::<Vec<_>>()
    );
    let states: Vec<_> = h
        .telemetry
        .iter()
        .filter_map(|(_, t)| match t {
            Telemetry::Composite {
                loop_id: target,
                frame,
                active_version,
                iteration,
                passes,
                waiting_repeat,
                ..
            } if *target == loop_id(3, 2) && *frame >= origin => Some((
                *frame,
                *active_version,
                *iteration,
                *passes,
                *waiting_repeat,
            )),
            _ => None,
        })
        .collect();
    assert!(!states.is_empty());
    for (frame, version, iteration, passes, waiting) in states {
        assert_eq!(
            (version, iteration, passes, waiting),
            (1, (frame - origin) % 8 / 2, (frame - origin) / 8, false)
        );
    }
    h.done(transition(loop_id(3, 2), LoopMode::Stopped));
    h.states.clear();
    assert_no_heap(|| h.host.client.resync()).unwrap();
    h.pump();
    assert!(h.states.iter().any(|s|matches!(s,StateRecord::Composite {description,..} if description.loop_id==loop_id(3,2) && description.version==1 && description.unit_frames==2)));
    assert_eq!(
        h.states
            .iter()
            .filter(|s| matches!(s, StateRecord::CompositeSpan { .. }))
            .count(),
        2
    );
}

#[shoop_test]
fn malformed_or_stale_programs_refuse_without_changing_the_working_arrangement() {
    let mut h = setup();
    let mut excess = plan(1, 1);
    excess.entries = 3;
    assert_eq!(
        h.apply(Request::Composite(CompositeCommand::Begin(excess))),
        Outcome::Refused(Refusal::Capacity)
    );
    let mut bad = plan(1, 1);
    bad.unit_frames = 0;
    assert_eq!(
        h.apply(Request::Composite(CompositeCommand::Begin(bad))),
        Outcome::Refused(Refusal::InvalidValue)
    );
    stage(&mut h, plan(1, 1), &spans());
    h.done(Request::Composite(CompositeCommand::Commit { id: 1 }));
    let mut bad = plan(2, 2);
    bad.expected_version = 0;
    assert_eq!(
        h.apply(Request::Composite(CompositeCommand::Begin(bad))),
        Outcome::Refused(Refusal::InvalidId)
    );
    h.done(Request::Composite(CompositeCommand::Begin(plan(2, 2))));
    let mut invalid = spans()[0];
    invalid.target = loop_id(1, 9);
    assert_eq!(
        h.apply(Request::Composite(CompositeCommand::Span {
            id: 2,
            index: 0,
            span: invalid
        })),
        Outcome::Refused(Refusal::InvalidId)
    );
    assert_eq!(
        h.apply(Request::Composite(CompositeCommand::Commit { id: 2 })),
        Outcome::Refused(Refusal::InvalidValue)
    );
    h.done(Request::Composite(CompositeCommand::Cancel { id: 2 }));
    h.done(transition(loop_id(3, 2), LoopMode::Playing));
    assert!(h.telemetry.iter().rev().any(|(_, t)| matches!(
        t,
        Telemetry::Composite {
            active_version: 1,
            ..
        }
    )));
}

#[shoop_test]
fn a_live_plan_update_publishes_its_own_length_and_media_at_one_eligible_boundary() {
    let mut h = setup();
    stage(&mut h, plan(1, 1), &spans());
    h.done(Request::Composite(CompositeCommand::Commit { id: 1 }));
    h.done(transition(loop_id(3, 2), LoopMode::Playing));
    let mut d = plan(2, 2);
    d.iterations = 6;
    let mut events = spans();
    events[1].end = 6;
    stage(&mut h, d, &events);
    h.done(Request::Composite(CompositeCommand::Commit { id: 2 }));
    h.run(32, 1);
    let mut checked = 0;
    for (_, t) in &h.telemetry {
        if let Telemetry::Composite {
            frame,
            active_version: 2,
            pending_version,
            position,
            length,
            mode: LoopMode::Playing,
            ..
        } = t
        {
            assert_eq!(*pending_version, None);
            assert_eq!(*length, 12);
            assert_eq!(
                h.outputs[0][*frame as usize],
                if *position < 4 { 1. } else { 2. }
            );
            checked += 1;
        }
    }
    assert!(checked >= 32);
}

#[shoop_test]
fn repeat_waits_hold_the_active_version_and_pending_edits_cancel_without_blocking_controls() {
    let mut h = setup_with_source(256);
    stage(&mut h, plan(1, 1), &spans());
    h.done(Request::Composite(CompositeCommand::Commit { id: 1 }));
    h.done(Request::LoopSync {
        loop_id: loop_id(3, 2),
        source: Some(loop_id(0, 1)),
    });
    h.done(Request::LoopRepeatSync {
        loop_id: loop_id(3, 2),
        enabled: true,
    });
    h.send(transition(loop_id(0, 1), LoopMode::Playing));
    h.send(transition(loop_id(3, 2), LoopMode::Playing));
    h.run(9, 1);
    assert!(h.telemetry.iter().rev().any(|(_, t)| matches!(
        t,
        Telemetry::Composite {
            active_version: 1,
            passes: 1,
            waiting_repeat: true,
            ..
        }
    )));
    let mut edited = spans();
    edited[0].end = 1;
    edited[1].start = 1;
    edited[1].occurrence_start = 1;
    stage(&mut h, plan(2, 2), &edited);
    let pending = h.send(Request::Composite(CompositeCommand::Commit { id: 2 }));
    h.run(64, 1);
    assert!(!h.results.contains_key(&pending));
    assert!(h.telemetry.iter().rev().any(|(_, t)| matches!(
        t,
        Telemetry::Composite {
            active_version: 1,
            pending_version: Some(2),
            waiting_repeat: true,
            ..
        }
    )));
    let control = h.send(transition(loop_id(1, 1), LoopMode::Playing));
    h.run(2, 1);
    assert_eq!(h.results.get(&control), Some(&Outcome::Done));
    let cancel = h.send(Request::Composite(CompositeCommand::Cancel { id: 2 }));
    h.settle();
    assert_eq!(h.results.get(&cancel), Some(&Outcome::Done));
    assert_eq!(
        h.results.get(&pending),
        Some(&Outcome::Refused(Refusal::Busy))
    );
    assert!(!h.telemetry.iter().any(|(_, t)| matches!(
        t,
        Telemetry::Composite {
            active_version: 2,
            ..
        }
    )));
    h.done(transition(loop_id(0, 1), LoopMode::Stopped));
    h.run(1, 1);
    assert!(h.telemetry.iter().rev().take(12).any(|(_, t)| matches!(
        t,
        Telemetry::Composite {
            active_version: 1,
            passes: 1,
            waiting_repeat: false,
            ..
        }
    )));
}

#[shoop_test]
fn explicit_recording_sections_finish_on_frames_after_the_source_stops() {
    let mut h = setup();
    let mut d = plan(1, 1);
    d.kind = CompositeKind::Explicit;
    let mut events = spans();
    events[0].mode = LoopMode::Recording;
    stage(&mut h, d, &events);
    h.done(Request::Composite(CompositeCommand::Commit { id: 1 }));
    h.done(Request::LoopSync {
        loop_id: loop_id(3, 2),
        source: Some(loop_id(0, 1)),
    });
    h.input = Box::new(|_, _| 3.);
    h.send(transition(loop_id(0, 1), LoopMode::Playing));
    h.send(transition(loop_id(3, 2), LoopMode::Playing));
    h.run(3, 1);
    h.send(transition(loop_id(0, 1), LoopMode::Stopped));
    h.run(7, 1);
    assert!(matches!(
        h.last_loop(loop_id(3, 2)),
        Some((
            _,
            Telemetry::Loop {
                mode: LoopMode::Stopped,
                ..
            }
        ))
    ));
    let (frames, audio, _) = h.read(loop_id(1, 1), BulkSelector::Audio { channel: 0 });
    assert_eq!((frames, audio), (4, vec![3.; 4]));
}

#[shoop_test]
fn prepared_frame_programs_keep_their_media_under_every_quantum_and_partial_callback() {
    for q in [1, 16, 32, 64, 128] {
        let mut settings = description();
        settings.quantum = q;
        settings.max_frames = 128;
        let mut h = setup_engine(settings, 10 * q);
        let mut d = plan(1, 1);
        d.quantum = q as u32;
        d.unit_frames = 2 * q as u64;
        stage(&mut h, d, &spans());
        h.done(Request::Composite(CompositeCommand::Commit { id: 1 }));
        h.done(Request::LoopSync {
            loop_id: loop_id(3, 2),
            source: Some(loop_id(0, 1)),
        });
        let origin = h.frame().div_ceil(q as u64) * q as u64;
        h.send(transition(loop_id(0, 1), LoopMode::Playing));
        h.send(transition(loop_id(3, 2), LoopMode::Playing));
        h.run(3 * q, 7);
        h.send(transition(loop_id(0, 1), LoopMode::Stopped));
        assert_no_heap(|| h.host.process(0)).unwrap();
        h.run(20 * q, 11);
        let delay = if q == 1 { 0 } else { q };
        for n in 0..20 * q {
            assert_eq!(
                h.outputs[0][origin as usize + delay + n],
                if n % (8 * q) < 4 * q { 1. } else { 2. },
                "q={q}, n={n}"
            );
        }
        for (_, t) in &h.telemetry {
            if let Telemetry::Composite {
                frame,
                mode: LoopMode::Playing,
                active_version: 1,
                position,
                iteration,
                passes,
                ..
            } = t
            {
                let elapsed = frame - origin;
                assert_eq!(
                    (*position, *iteration, *passes),
                    (
                        elapsed % (8 * q as u64),
                        elapsed % (8 * q as u64) / (2 * q as u64),
                        elapsed / (8 * q as u64)
                    )
                );
            }
        }
    }
}

#[shoop_test]
fn an_at_limit_sparse_plan_larger_than_the_command_window_stages_in_bounded_batches() {
    let count = 8192usize;
    let mut settings = description();
    settings.composite_entries = count;
    settings.composite_segments = count;
    settings.buses = 1;
    settings.pending_transitions = 1;
    settings.reliable_credits = 2;
    let mut h = setup_engine(settings, 10);
    let mut d = plan(1, 1);
    d.entries = count as u32;
    d.segments = count as u32;
    d.iterations = (2 * count - 1) as u64;
    let events: Vec<_> = (0..count)
        .map(|i| CompositeSpan {
            target: loop_id(1, 1),
            start: (2 * i) as u64,
            end: (2 * i + 1) as u64,
            occurrence_start: (2 * i) as u64,
            ordinal: i as u32,
            mode: LoopMode::Playing,
        })
        .collect();
    stage(&mut h, d, &events);
    let commit = h.send(Request::Composite(CompositeCommand::Commit { id: 1 }));
    for _ in 0..1000 {
        if h.results.contains_key(&commit) {
            break;
        }
        h.run(16, 16);
    }
    assert!(matches!(
        h.results.get(&commit),
        Some(Outcome::CompositeApplied { version: 1, .. })
    ));
    h.settle();
    h.done(transition(loop_id(3, 2), LoopMode::Playing));
    assert!(h.telemetry.iter().any(|(_,t)|matches!(t,Telemetry::Composite {active_version:1,length,..} if *length==d.iterations*2)));
}

#[shoop_test]
fn private_replacement_prepares_composites_inaudibly_and_loads_them_stopped_with_repeat_intent() {
    let mut h = setup();
    stage(&mut h, plan(1, 1), &spans());
    h.done(Request::Composite(CompositeCommand::Commit { id: 1 }));
    h.done(transition(loop_id(3, 2), LoopMode::Playing));
    h.done(Request::ReplacementBegin { id: 1 });
    let track = strip(0, 2);
    let child = loop_id(1, 2);
    let parent = loop_id(3, 3);
    let edits = [
        ReplacementEdit::Strip {
            strip: track,
            shape: shape(1, 1, false),
            controls: StripPreset {
                input_gain: 1.,
                input_balance: 0.,
                monitoring: false,
                output_gain: 1.,
                output_balance: 0.,
                output_mute: false,
            },
        },
        ReplacementEdit::Connect {
            port: port(track, PortKind::AudioIn, 0),
            host: audio_in(0),
        },
        ReplacementEdit::Loop {
            loop_id: child,
            strip: track,
            sync: None,
            frames: 0,
            audio: None,
            midi: None,
            gain: 1.,
            balance: 0.,
        },
        ReplacementEdit::Loop {
            loop_id: parent,
            strip: track,
            sync: None,
            frames: 0,
            audio: None,
            midi: None,
            gain: 1.,
            balance: 0.,
        },
        ReplacementEdit::Composite(CompositeDescription {
            id: 1,
            loop_id: parent,
            sync: None,
            previous: None,
            expected_version: 0,
            version: 1,
            kind: CompositeKind::Explicit,
            unit_frames: 2,
            iterations: 2,
            entries: 1,
            segments: 1,
            sample_rate: 1000,
            quantum: 1,
        }),
        ReplacementEdit::CompositeSpan {
            loop_id: parent,
            index: 0,
            span: CompositeSpan {
                target: child,
                start: 0,
                end: 2,
                occurrence_start: 0,
                ordinal: 0,
                mode: LoopMode::Recording,
            },
        },
        ReplacementEdit::RepeatSync {
            loop_id: parent,
            enabled: true,
        },
    ];
    for edit in edits {
        h.done(Request::ReplacementEdit { id: 1, edit });
    }
    h.done(Request::ReplacementPrepare { id: 1 });
    assert!(matches!(
        h.last_loop(loop_id(3, 2)),
        Some((
            _,
            Telemetry::Loop {
                mode: LoopMode::Playing,
                ..
            }
        ))
    ));
    h.done(Request::ReplacementCommit { id: 1 });
    assert!(matches!(
        h.last_loop(parent),
        Some((
            _,
            Telemetry::Loop {
                mode: LoopMode::Stopped,
                ..
            }
        ))
    ));
    h.states.clear();
    assert_no_heap(|| h.host.client.resync()).unwrap();
    h.pump();
    assert!(h.states.iter().any(|s|matches!(s,StateRecord::Composite {description,repeat_sync:true,..} if description.loop_id==parent)));
    h.input = Box::new(|_, _| 3.);
    h.send(transition(parent, LoopMode::Playing));
    h.run(5, 1);
    let (frames, audio, _) = h.read(child, BulkSelector::Audio { channel: 0 });
    assert_eq!((frames, audio), (4, vec![3.; 4]));
}

#[shoop_test]
fn recording_admitted_during_conversion_preparation_survives_the_live_empty_fence_refusal() {
    let mut h = setup();
    stage(&mut h, plan(1, 1), &spans());
    h.input = Box::new(|_, _| 3.);
    let recording = h.send(transition(loop_id(3, 1), LoopMode::Recording));
    let conversion = h.send(Request::Composite(CompositeCommand::Commit { id: 1 }));
    h.settle();
    assert_eq!(h.results.get(&recording), Some(&Outcome::Done));
    assert_eq!(
        h.results.get(&conversion),
        Some(&Outcome::Refused(Refusal::Busy))
    );
    assert!(matches!(
        h.last_loop(loop_id(3, 1)),
        Some((
            _,
            Telemetry::Loop {
                mode: LoopMode::Recording,
                ..
            }
        ))
    ));
    h.done(transition(loop_id(3, 1), LoopMode::Stopped));
    let (frames, audio, _) = h.read(loop_id(3, 1), BulkSelector::Audio { channel: 0 });
    assert!(frames > 0);
    assert_eq!(audio, vec![3.; frames as usize]);
}

#[shoop_test]
fn standalone_timing_slots_need_no_strip_or_signal_bindings_and_reuse_their_generation() {
    let mut h = Harness::new(&description());
    for generation in 1..=16 {
        let id = loop_id(0, generation);
        h.done(Request::CreateComposite {
            loop_id: id,
            strip: None,
        });
        h.done(Request::SetSmoothing { frames: 1 });
        h.done(transition(id, LoopMode::Playing));
        assert!(matches!(
            h.last_loop(id),
            Some((
                _,
                Telemetry::Loop {
                    mode: LoopMode::Stopped,
                    length: 0,
                    ..
                }
            ))
        ));
        h.done(Request::RemoveLoop { loop_id: id });
    }
}

#[shoop_test]
fn a_full_composite_pool_keeps_engine_and_all_loop_projection_records_in_one_round() {
    let mut h = Harness::new(&description());
    for slot in 0..6 {
        h.send(Request::CreateComposite {
            loop_id: loop_id(slot, 1),
            strip: None,
        });
    }
    h.settle();
    h.telemetry.clear();
    h.run(1, 1);
    assert!(h
        .telemetry
        .iter()
        .any(|(_, t)| matches!(t, Telemetry::Engine { .. })));
    for slot in 0..6 {
        assert!(h.telemetry.iter().any(
            |(_, t)| matches!(t,Telemetry::Composite {loop_id:id,..} if *id==loop_id(slot,1))
        ));
        assert!(h.last_loop(loop_id(slot, 1)).is_some());
    }
}

#[shoop_test]
fn nested_starts_and_iteration_seeks_apply_child_snapshots_in_the_same_quantum() {
    let mut h = setup_engine(description(), 10);
    stage(&mut h, plan(1, 1), &spans());
    h.done(Request::Composite(CompositeCommand::Commit { id: 1 }));
    let parent = loop_id(4, 1);
    h.done(Request::CreateComposite {
        loop_id: parent,
        strip: None,
    });
    let mut d = plan(2, 1);
    d.loop_id = parent;
    d.previous = None;
    d.entries = 1;
    d.segments = 1;
    d.unit_frames = 8;
    d.iterations = 2;
    stage(
        &mut h,
        d,
        &[CompositeSpan {
            target: loop_id(3, 2),
            start: 0,
            end: 2,
            occurrence_start: 0,
            ordinal: 0,
            mode: LoopMode::Playing,
        }],
    );
    h.done(Request::Composite(CompositeCommand::Commit { id: 2 }));
    let origin = h.frame();
    h.send(transition(parent, LoopMode::Playing));
    h.run(4, 1);
    assert_eq!(
        &h.outputs[0][origin as usize..origin as usize + 4],
        &[1.; 4]
    );
    let seek = h.frame();
    h.send(Request::LoopIteration {
        loop_id: loop_id(3, 2),
        iteration: 3,
        retrigger: false,
        seek: true,
    });
    h.run(1, 1);
    assert_eq!(h.outputs[0][seek as usize], 2.);
    h.run(3, 1);
    let state = h.telemetry.iter().rev().find_map(|(_, t)| match t {
        Telemetry::Composite {
            loop_id, position, ..
        } if *loop_id == parent => Some(*position),
        _ => None,
    });
    assert_eq!(state, Some(7));
}

#[shoop_test]
fn clearing_a_composite_restores_primitive_controls_and_ends_child_occurrences() {
    let mut h = setup();
    h.done(Request::LoopGain {
        loop_id: loop_id(3, 1),
        gain: 0.25,
    });
    let d = plan(1, 1);
    stage(&mut h, d, &spans());
    h.done(Request::Composite(CompositeCommand::Commit { id: 1 }));
    h.done(transition(loop_id(3, 2), LoopMode::Playing));
    h.run(2, 1);
    h.done(Request::ResetComposite {
        previous: loop_id(3, 2),
        loop_id: loop_id(3, 3),
    });
    h.settle();
    assert!(matches!(
        h.last_loop(loop_id(1, 1)).unwrap().1,
        Telemetry::Loop {
            mode: LoopMode::Stopped,
            ..
        }
    ));
    h.input = Box::new(|_, _| 1.0);
    h.send(transition(loop_id(3, 3), LoopMode::Recording));
    h.run(4, 1);
    h.done(transition(loop_id(3, 3), LoopMode::Stopped));
    h.done(transition(loop_id(3, 3), LoopMode::Playing));
    let first = h.outputs[0].len();
    h.run(4, 1);
    assert!(
        h.outputs[0][first..]
            .iter()
            .all(|sample| (*sample - 0.25).abs() < 1e-6),
        "{:?}",
        &h.outputs[0][first..]
    );
}
