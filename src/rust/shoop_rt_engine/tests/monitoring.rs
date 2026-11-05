mod support;
use shoop_rt_protocol::*;
use shoop_wasm_test_support::shoop_test;
use support::*;

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

fn transport(index: u32, mode: LoopMode) -> Request {
    Request::LoopTransition {
        loop_id: loop_id(index, 1),
        mode,
        delay: None,
    }
}
fn aligned(h: &mut Harness, q: usize) -> u64 {
    let pad = (q as u64 - h.frame() % q as u64) % q as u64;
    h.run(pad as usize, 7);
    h.frame()
}
fn setup(q: usize) -> Harness {
    let mut d = description();
    d.segments[1].loops = 4;
    d.quantum = q;
    d.max_frames = 128;
    d.ramp_frames = 0;
    let mut h = Harness::new(&d);
    h.done(Request::CreateStrip {
        strip: strip(0, 1),
        shape: shape(1, 1, true),
    });
    for (kind, host) in [
        (PortKind::AudioIn, audio_in(0)),
        (PortKind::AudioOut, audio_out(0)),
        (PortKind::MidiIn, midi_in()),
        (PortKind::MidiOut, midi_out()),
    ] {
        h.done(Request::Connect {
            port: port(strip(0, 1), kind, 0),
            host,
            connected: true,
        });
    }
    for index in [0, 1] {
        h.done(Request::CreateLoop {
            loop_id: loop_id(index, 1),
            strip: strip(0, 1),
        });
    }
    aligned(&mut h, q);
    h.send(transport(0, LoopMode::Recording));
    h.run(12 * q, 7);
    h.done(transport(0, LoopMode::Stopped));
    h.done(Request::StripControl {
        strip: strip(0, 1),
        control: StripControl::AutomaticMonitoring(true),
    });
    h.input = Box::new(|port, _| if port == 0 { 0.25 } else { 0. });
    h
}
fn span(target: u32, start: u64, end: u64, mode: LoopMode) -> CompositeSpan {
    CompositeSpan {
        target: loop_id(target, 1),
        start,
        end,
        occurrence_start: start,
        ordinal: 0,
        mode,
    }
}
fn description_for(
    slot: u32,
    id: u64,
    version: u64,
    unit: usize,
    iterations: u64,
    kind: CompositeKind,
    source: Option<LoopSlotId>,
) -> CompositeDescription {
    CompositeDescription {
        id,
        loop_id: loop_id(slot, 1),
        sync: source,
        previous: None,
        expected_version: version - 1,
        version,
        kind,
        unit_frames: unit as u64,
        iterations,
        entries: 1,
        segments: 1,
        sample_rate: 1000,
        quantum: 1,
    }
}
fn stage(h: &mut Harness, mut d: CompositeDescription, spans: &[CompositeSpan]) {
    d.entries = spans.iter().map(|span| span.ordinal + 1).max().unwrap_or(0);
    d.segments = spans.len() as u32;
    h.done(Request::Composite(CompositeCommand::Begin(d)));
    for (index, span) in spans.iter().enumerate() {
        h.done(Request::Composite(CompositeCommand::Span {
            id: d.id,
            index: index as u32,
            span: *span,
        }));
    }
}
fn install(h: &mut Harness, d: CompositeDescription, spans: &[CompositeSpan]) {
    h.done(Request::CreateComposite {
        loop_id: d.loop_id,
        strip: None,
    });
    stage(h, d, spans);
    h.done(Request::Composite(CompositeCommand::Commit { id: d.id }));
}
fn automatic(h: &Harness) -> bool {
    h.telemetry
        .iter()
        .rev()
        .find_map(|(_, record)| match record {
            Telemetry::Strip {
                strip: id,
                automatic_monitoring,
                ..
            } if *id == strip(0, 1) => Some(*automatic_monitoring),
            _ => None,
        })
        .unwrap()
}
fn packets(bytes: &[u8]) -> Vec<(u64, Vec<u8>)> {
    let mut rest = bytes;
    let mut result = Vec::new();
    while !rest.is_empty() {
        let frame = u64::from_le_bytes(rest[..8].try_into().unwrap());
        let len = u32::from_le_bytes(rest[8..12].try_into().unwrap()) as usize;
        result.push((frame, rest[12..12 + len].to_vec()));
        rest = &rest[12 + len..];
    }
    result
}

#[shoop_test]
fn upcoming_recording_monitors_the_first_quantum_and_releases_audio_and_midi_at_its_end() {
    for q in [1, 16, 32, 64, 128] {
        let mut h = setup(q);
        let mut d = description_for(3, 1, 1, 4 * q, 4, CompositeKind::Explicit, None);
        d.quantum = q as u32;
        install(
            &mut h,
            d,
            &[
                span(1, 1, 3, LoopMode::Recording),
                CompositeSpan {
                    ordinal: 1,
                    ..span(1, 3, 4, LoopMode::Stopped)
                },
            ],
        );
        let origin = aligned(&mut h, q);
        let first = origin + 4 * q as u64;
        h.midi_input.push((first, vec![0x90, 60, 100]));
        h.send(transport(3, LoopMode::Playing));
        assert_no_heap(|| h.host.process(0)).unwrap();
        h.run(18 * q, 7);
        let delay = if q == 1 { 0 } else { q };
        for n in 0..16 * q {
            assert_eq!(
                h.outputs[0][origin as usize + delay + n],
                if n < 12 * q { 0.25 } else { 0. },
                "q={q}, n={n}"
            );
        }
        let (length, audio, _) = h.read(loop_id(1, 1), BulkSelector::Audio { channel: 0 });
        assert_eq!(length, (8 * q) as u64);
        assert_eq!(audio, vec![0.25; 8 * q]);
        let (_, _, midi) = h.read(loop_id(1, 1), BulkSelector::Midi);
        assert!(packets(&midi).contains(&(0, vec![0x90, 60, 100])));
        assert!(h
            .midi
            .contains(&(first + delay as u64, vec![0x90, 60, 100])));
        assert!(h
            .midi
            .contains(&(origin + (12 * q + delay) as u64, vec![0x80, 60, 0])));
        assert!(!automatic(&h));
    }
}

#[shoop_test]
fn independent_stop_releases_automatic_demand_without_muting_a_manually_prearmed_track() {
    let mut h = setup(1);
    install(
        &mut h,
        description_for(3, 1, 1, 4, 4, CompositeKind::Explicit, None),
        &[
            span(1, 1, 3, LoopMode::Recording),
            CompositeSpan {
                ordinal: 1,
                ..span(1, 3, 4, LoopMode::Stopped)
            },
        ],
    );
    h.done(Request::StripControl {
        strip: strip(0, 1),
        control: StripControl::Monitoring(true),
    });
    let origin = h.frame();
    h.send(transport(3, LoopMode::Playing));
    h.run(5, 1);
    assert!(automatic(&h));
    h.send(transport(1, LoopMode::Stopped));
    h.run(1, 1);
    assert!(!automatic(&h));
    assert_eq!(h.outputs[0][origin as usize + 5], 0.25);
    h.done(Request::StripControl {
        strip: strip(0, 1),
        control: StripControl::Monitoring(false),
    });
    let from = h.outputs[0].len();
    h.run(4, 1);
    assert_eq!(&h.outputs[0][from..], &[0.; 4]);
    assert!(h.telemetry.iter().rev().any(|(_, record)| matches!(record, Telemetry::Composite { loop_id: id, mode: LoopMode::Playing, position, .. } if *id == loop_id(3, 1) && *position >= 6)));
    h.done(transport(3, LoopMode::Stopped));
    h.done(Request::StripControl {
        strip: strip(0, 1),
        control: StripControl::Monitoring(true),
    });
    let from = h.outputs[0].len();
    h.run(4, 1);
    assert!(!automatic(&h));
    assert_eq!(&h.outputs[0][from..], &[0.25; 4]);
}

#[shoop_test]
fn nested_recording_releases_during_parent_repeat_wait_and_rearms_on_actual_restart() {
    let mut h = setup(1);
    install(
        &mut h,
        description_for(3, 1, 1, 4, 1, CompositeKind::Explicit, None),
        &[span(1, 0, 1, LoopMode::Recording)],
    );
    install(
        &mut h,
        description_for(4, 2, 1, 4, 1, CompositeKind::Regular, Some(loop_id(0, 1))),
        &[span(3, 0, 1, LoopMode::Playing)],
    );
    h.done(Request::LoopRepeatSync {
        loop_id: loop_id(4, 1),
        enabled: true,
    });
    let origin = h.frame();
    h.send(transport(0, LoopMode::Playing));
    h.send(transport(4, LoopMode::Playing));
    h.run(4, 1);
    assert!(automatic(&h));
    h.run(4, 1);
    assert!(!automatic(&h));
    assert_eq!(
        &h.outputs[0][origin as usize + 4..origin as usize + 8],
        &[0.; 4]
    );
    assert!(h.telemetry.iter().rev().any(|(_, record)| matches!(record, Telemetry::Composite { loop_id: id, waiting_repeat: true, .. } if *id == loop_id(4, 1))));
    h.send(transport(0, LoopMode::Stopped));
    h.run(1, 1);
    assert!(automatic(&h));
    assert_eq!(h.outputs[0][origin as usize + 8], 0.25);
}

#[shoop_test]
fn a_staged_recording_candidate_cannot_prearm_the_active_playback_plan() {
    let mut h = setup(1);
    install(
        &mut h,
        description_for(3, 1, 1, 4, 4, CompositeKind::Explicit, None),
        &[span(1, 0, 4, LoopMode::Playing)],
    );
    h.send(transport(3, LoopMode::Playing));
    h.run(1, 1);
    let candidate = description_for(3, 2, 2, 4, 4, CompositeKind::Explicit, None);
    stage(&mut h, candidate, &[span(1, 0, 4, LoopMode::Recording)]);
    let commit = h.send(Request::Composite(CompositeCommand::Commit { id: 2 }));
    h.run(2, 1);
    assert!(!h.results.contains_key(&commit));
    assert!(!automatic(&h));
    h.done(transport(3, LoopMode::Stopped));
    assert!(matches!(
        h.results[&commit],
        Outcome::CompositeApplied { version: 2, .. }
    ));
    assert!(!automatic(&h));
    h.send(transport(3, LoopMode::Playing));
    h.run(1, 1);
    assert!(automatic(&h));
}

#[shoop_test]
fn a_delayed_start_prearms_only_when_its_admitted_countdown_reaches_zero() {
    let mut h = setup(1);
    install(
        &mut h,
        description_for(3, 1, 1, 4, 1, CompositeKind::Explicit, Some(loop_id(0, 1))),
        &[span(1, 0, 1, LoopMode::Recording)],
    );
    let origin = h.frame();
    h.send(Request::LoopTransition {
        loop_id: loop_id(3, 1),
        mode: LoopMode::Playing,
        delay: Some(2),
    });
    h.send(transport(0, LoopMode::Playing));
    h.run(1, 1);
    assert!(!automatic(&h));
    h.run(12, 1);
    assert!(automatic(&h));
    assert!(h.telemetry.iter().rev().any(|(_, record)| matches!(record, Telemetry::Composite { loop_id: id, mode: LoopMode::Stopped, .. } if *id == loop_id(3, 1))));
    h.run(12, 1);
    assert!(automatic(&h));
    assert_eq!(h.outputs[0][origin as usize + 24], 0.25);
    h.done(transport(3, LoopMode::Stopped));
    assert!(!automatic(&h));
}

#[shoop_test]
fn overlapping_recorders_share_monitoring_and_release_held_notes_only_after_the_last_demand() {
    let mut h = setup(1);
    h.done(Request::CreateLoop {
        loop_id: loop_id(2, 1),
        strip: strip(0, 1),
    });
    for (parent, id, child, iterations) in [(3, 1, 1, 1), (4, 2, 2, 2)] {
        install(
            &mut h,
            description_for(parent, id, 1, 4, iterations, CompositeKind::Explicit, None),
            &[span(child, 0, iterations, LoopMode::Recording)],
        );
    }
    let origin = h.frame();
    h.midi_input.push((origin + 1, vec![0x90, 61, 90]));
    h.send(transport(3, LoopMode::Playing));
    h.send(transport(4, LoopMode::Playing));
    h.run(5, 1);
    assert!(automatic(&h));
    assert!(!h
        .midi
        .iter()
        .any(|(frame, bytes)| *frame < origin + 8 && bytes == &[0x80, 61, 0]));
    h.run(4, 1);
    assert!(!automatic(&h));
    assert_eq!(
        &h.outputs[0][origin as usize..origin as usize + 8],
        &[0.25; 8]
    );
    assert_eq!(h.outputs[0][origin as usize + 8], 0.);
    assert!(h.midi.contains(&(origin + 8, vec![0x80, 61, 0])));
}

#[shoop_test]
fn automatic_policy_can_change_during_primitive_recording_without_changing_manual_intent_or_capture(
) {
    let mut h = setup(1);
    h.send(transport(1, LoopMode::Recording));
    h.run(2, 1);
    assert!(automatic(&h));
    h.done(Request::StripControl {
        strip: strip(0, 1),
        control: StripControl::AutomaticMonitoring(false),
    });
    h.run(1, 1);
    assert!(!automatic(&h));
    assert_eq!(*h.outputs[0].last().unwrap(), 0.);
    assert!(matches!(
        h.last_loop(loop_id(1, 1)),
        Some((
            _,
            Telemetry::Loop {
                mode: LoopMode::Recording,
                ..
            }
        ))
    ));
    h.done(Request::StripControl {
        strip: strip(0, 1),
        control: StripControl::Monitoring(true),
    });
    assert_eq!(*h.outputs[0].last().unwrap(), 0.25);
    h.done(Request::StripControl {
        strip: strip(0, 1),
        control: StripControl::AutomaticMonitoring(true),
    });
    h.run(1, 1);
    assert!(automatic(&h));
    h.done(transport(1, LoopMode::Stopped));
    assert!(!automatic(&h));
    assert_eq!(*h.outputs[0].last().unwrap(), 0.25);
    let (_, audio, _) = h.read(loop_id(1, 1), BulkSelector::Audio { channel: 0 });
    assert!(audio.iter().all(|sample| *sample == 0.25));
}

#[shoop_test]
fn a_continuing_nested_clock_keeps_phase_until_completion_then_reenters_at_parent_wrap() {
    let mut h = setup(1);
    install(
        &mut h,
        description_for(3, 1, 1, 4, 4, CompositeKind::Explicit, None),
        &[
            span(1, 0, 1, LoopMode::Recording),
            CompositeSpan {
                ordinal: 1,
                ..span(1, 1, 4, LoopMode::Stopped)
            },
        ],
    );
    install(
        &mut h,
        description_for(4, 2, 1, 4, 1, CompositeKind::Regular, None),
        &[span(3, 0, 1, LoopMode::Playing)],
    );
    let origin = h.frame();
    h.send(transport(4, LoopMode::Playing));
    h.run(4, 1);
    assert!(automatic(&h));
    h.run(8, 1);
    assert!(!automatic(&h));
    assert_eq!(
        &h.outputs[0][origin as usize + 4..origin as usize + 12],
        &[0.; 8]
    );
    assert!(h.telemetry.iter().rev().any(|(_, record)| matches!(record, Telemetry::Composite { loop_id: id, mode: LoopMode::Playing, position, .. } if *id == loop_id(3, 1) && *position >= 8)));
    h.run(8, 1);
    assert!(automatic(&h));
    assert_eq!(
        &h.outputs[0][origin as usize + 16..origin as usize + 20],
        &[0.25; 4]
    );
}

#[shoop_test]
fn a_completed_nested_one_pass_program_reenters_on_the_next_parent_pass() {
    for q in [1, 16, 32, 64, 128] {
        for child_iterations in [2, 4] {
            let mut h = setup(q);
            let program = |slot, id, iterations, kind| {
                let mut d = description_for(slot, id, 1, 4 * q, iterations, kind, None);
                d.quantum = q as u32;
                d
            };
            install(
                &mut h,
                program(3, 1, child_iterations, CompositeKind::Explicit),
                &[
                    span(1, 0, 1, LoopMode::Recording),
                    CompositeSpan {
                        ordinal: 1,
                        ..span(1, 1, child_iterations, LoopMode::Stopped)
                    },
                ],
            );
            install(
                &mut h,
                program(4, 2, 4, CompositeKind::Regular),
                &[span(3, 0, 4, LoopMode::Playing)],
            );
            let delay = if q == 1 { 0 } else { q };
            let origin = aligned(&mut h, q) as usize + delay;
            h.send(transport(4, LoopMode::Playing));
            h.run(16 * q + delay, 7);
            assert_eq!(
                &h.outputs[0][origin..origin + 4 * q],
                vec![0.25; 4 * q].as_slice()
            );
            assert_eq!(
                &h.outputs[0][origin + 4 * q..origin + 12 * q],
                vec![0.; 8 * q].as_slice()
            );
            h.run(4 * q, 7);
            assert_eq!(
                &h.outputs[0][origin + 16 * q..origin + 20 * q],
                vec![0.25; 4 * q].as_slice(),
                "q={q}, child_iterations={child_iterations}"
            );
            h.run(2 * q, 7);
            assert!(!automatic(&h));
            let (length, audio, _) = h.read(loop_id(1, 1), BulkSelector::Audio { channel: 0 });
            assert_eq!(length, (4 * q) as u64);
            assert_eq!(audio, vec![0.25; 4 * q]);
        }
    }
}

#[shoop_test]
fn preparing_and_cancelling_a_private_replacement_preserves_live_recording_monitoring() {
    let mut h = setup(1);
    install(
        &mut h,
        description_for(3, 1, 1, 4, 1000, CompositeKind::Explicit, None),
        &[span(1, 0, 1000, LoopMode::Recording)],
    );
    h.send(transport(3, LoopMode::Playing));
    h.run(1, 1);
    assert!(automatic(&h));
    let from = h.outputs[0].len();
    h.done(Request::ReplacementBegin { id: 1 });
    h.done(Request::ReplacementEdit {
        id: 1,
        edit: ReplacementEdit::Strip {
            strip: strip(0, 2),
            shape: shape(1, 1, true),
            controls: StripPreset {
                automatic_monitoring: false,
                input_gain: 1.,
                input_balance: 0.,
                monitoring: false,
                output_gain: 1.,
                output_balance: 0.,
                output_mute: false,
            },
        },
    });
    h.done(Request::ReplacementEdit {
        id: 1,
        edit: ReplacementEdit::Loop {
            dry_audio: None,
            recorded_state: None,
            loop_id: loop_id(1, 2),
            strip: strip(0, 2),
            sync: None,
            frames: 0,
            audio: None,
            midi: None,
            gain: 1.,
            balance: 0.,
        },
    });
    h.done(Request::ReplacementPrepare { id: 1 });
    h.run(4, 1);
    assert!(automatic(&h));
    h.done(Request::ReplacementCancel { id: 1 });
    h.run(4, 1);
    assert!(automatic(&h));
    assert!(h.outputs[0][from..].iter().all(|sample| *sample == 0.25));
    assert!(matches!(
        h.last_loop(loop_id(1, 1)),
        Some((
            _,
            Telemetry::Loop {
                mode: LoopMode::Recording,
                ..
            }
        ))
    ));
}

#[shoop_test]
fn replacement_installs_automatic_policy_before_the_first_composite_recording_quantum() {
    for q in [1, 16, 32, 64, 128] {
        let mut h = setup(q);
        h.done(Request::StripControl {
            strip: strip(0, 1),
            control: StripControl::AutomaticMonitoring(false),
        });
        let track = strip(0, 2);
        let child = loop_id(1, 2);
        let parent = loop_id(3, 2);
        h.done(Request::ReplacementBegin { id: 1 });
        for edit in [
            ReplacementEdit::Strip {
                strip: track,
                shape: shape(1, 1, false),
                controls: StripPreset {
                    input_gain: 1.,
                    input_balance: 0.,
                    monitoring: false,
                    automatic_monitoring: true,
                    output_gain: 1.,
                    output_balance: 0.,
                    output_mute: false,
                },
            },
            ReplacementEdit::Loop {
                dry_audio: None,
                recorded_state: None,
                loop_id: child,
                strip: track,
                sync: None,
                frames: 0,
                audio: None,
                midi: None,
                gain: 1.,
                balance: 0.,
            },
            ReplacementEdit::Connect {
                port: port(track, PortKind::AudioIn, 0),
                host: audio_in(0),
            },
            ReplacementEdit::Connect {
                port: port(track, PortKind::AudioOut, 0),
                host: audio_out(0),
            },
            ReplacementEdit::Composite(CompositeDescription {
                id: 0,
                loop_id: parent,
                sync: None,
                previous: None,
                expected_version: 0,
                version: 1,
                kind: CompositeKind::Explicit,
                unit_frames: (4 * q) as u64,
                iterations: 1,
                entries: 1,
                segments: 1,
                sample_rate: 1000,
                quantum: q as u32,
            }),
            ReplacementEdit::CompositeSpan {
                loop_id: parent,
                index: 0,
                span: CompositeSpan {
                    target: child,
                    start: 0,
                    end: 1,
                    occurrence_start: 0,
                    ordinal: 0,
                    mode: LoopMode::Recording,
                },
            },
        ] {
            h.done(Request::ReplacementEdit { id: 1, edit });
        }
        h.done(Request::ReplacementPrepare { id: 1 });
        h.done(Request::ReplacementCommit { id: 1 });
        let origin = aligned(&mut h, q) as usize;
        h.send(Request::LoopTransition {
            loop_id: parent,
            mode: LoopMode::Playing,
            delay: None,
        });
        h.run(6 * q, 7);
        let delay = if q == 1 { 0 } else { q };
        assert_eq!(
            &h.outputs[0][origin + delay..origin + delay + 4 * q],
            &vec![0.25; 4 * q],
            "q={q}"
        );
        assert!(h.outputs[0][origin + delay + 4 * q..]
            .iter()
            .all(|sample| *sample == 0.));
        let (length, audio, _) = h.read(child, BulkSelector::Audio { channel: 0 });
        assert_eq!(length, (4 * q) as u64);
        assert_eq!(audio, vec![0.25; 4 * q]);
    }
}
