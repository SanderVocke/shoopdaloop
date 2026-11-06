//! Real DSP banks exercised through production protocol admission and graph rendering.
#![cfg(any(feature = "builtin-fx", feature = "oxisynth"))]
use shoop_rt_engine::{testing::OfflineHost, EngineDescription, HostPorts, Segment};
use shoop_rt_protocol::*;
use shoop_wasm_test_support::{
    heap::{assert_no_heap, TrackingAllocator},
    shoop_test,
};
#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

fn description(channels: u8, kind: ProcessorKind) -> EngineDescription {
    EngineDescription {
        sample_rate: 8000,
        quantum: 16,
        max_frames: 128,
        ramp_frames: 0,
        strip_channels: channels,
        host: HostPorts::numbered(channels.max(2) as usize, channels.max(2) as usize, 1),
        segments: vec![Segment {
            strips: 3,
            loops: 4,
        }],
        resource_pages: 1024,
        page_bytes: 512,
        processors: ProcessorSettings {
            builtin_fx: if kind == ProcessorKind::BuiltInFx {
                ProcessorPolicy::On
            } else {
                ProcessorPolicy::Off
            },
            oxisynth: if kind == ProcessorKind::OxiSynth {
                ProcessorPolicy::On
            } else {
                ProcessorPolicy::Off
            },
            fx_instances: 1,
            synth_instances: 1,
            memory_mb: 128,
        },
        ..EngineDescription::default()
    }
}
fn tick(h: &mut OfflineHost, frames: usize) {
    assert_no_heap(|| h.process(frames)).unwrap();
}
fn request(h: &mut OfflineHost, request: Request) -> Outcome {
    let id = assert_no_heap(|| h.request(request)).unwrap();
    for _ in 0..4000 {
        while let Some(message) = assert_no_heap(|| h.receive()) {
            if let EngineMessage::Result {
                correlation,
                outcome,
            } = message
            {
                if correlation == id {
                    return outcome;
                }
            }
        }
        tick(h, 16);
    }
    panic!("request timed out: {request:?}");
}
fn done(h: &mut OfflineHost, r: Request) {
    assert_eq!(request(h, r), Outcome::Done, "{r:?}");
}
fn strip() -> StripId {
    StripId(SlotId::new(0, 1))
}
fn looped() -> LoopSlotId {
    LoopSlotId(SlotId::new(0, 1))
}
#[cfg(feature = "builtin-fx")]
#[shoop_test]
fn wet_take_keeps_first_source_settings_after_equal_time_midi_and_before_later_cc() {
    recorded_take_boundary(ProcessorKind::BuiltInFx);
}
#[cfg(feature = "oxisynth")]
#[shoop_test]
fn synth_take_keeps_first_source_settings_and_preset_independently_of_later_edits() {
    recorded_take_boundary(ProcessorKind::OxiSynth);
}
fn recorded_take_boundary(kind: ProcessorKind) {
    use processors::{Control, CC_BASE};
    let (channels, target, mapping, first, last, mapping_index) = match kind {
        ProcessorKind::BuiltInFx => (1, 8, CC_BASE + 8, -18., 6., 40),
        ProcessorKind::OxiSynth => (2, 0, CC_BASE, 0., 1., 3),
        _ => unreachable!(),
    };
    let d = description(channels, kind);
    let mut h =
        OfflineHost::new_with_processors(&d, shoop_host_processors::prepare(&d).unwrap()).unwrap();
    create(&mut h, channels, kind);
    if kind == ProcessorKind::OxiSynth {
        done(
            &mut h,
            Request::ProcessorControl {
                strip: strip(),
                control: Control::Parameter {
                    parameter: 2,
                    value: 40.,
                },
            },
        );
    }
    done(
        &mut h,
        Request::ProcessorControl {
            strip: strip(),
            control: Control::Parameter {
                parameter: mapping,
                value: 7.,
            },
        },
    );
    assert_eq!(h.endpoint().recorded_processor_state(looped()), None);
    let correlation = h
        .request(Request::LoopTransition {
            loop_id: looped(),
            mode: LoopMode::Recording,
            delay: None,
        })
        .unwrap();
    let start = h.next_frame();
    let mut accepted = false;
    for _ in 0..4 {
        while let Some(message) = h.receive() {
            if matches!(message, EngineMessage::Result { correlation: id, outcome: Outcome::Done } if id == correlation)
            {
                accepted = true;
            }
        }
        h.midi_input_mut(0).push(0, 16, &[0xb0, 7, 0]).unwrap();
        h.midi_input_mut(0).push(7, 16, &[0xb0, 7, 127]).unwrap();
        tick(&mut h, 16);
    }
    assert!(accepted);
    assert_eq!(
        h.endpoint().processor_state(0).unwrap().values[target as usize],
        last
    );
    tick(&mut h, 16);
    mode(&mut h, LoopMode::Stopped);
    let recorded = h
        .endpoint()
        .recorded_processor_state(looped())
        .expect("committed take settings");
    assert!((start..start + 4 * d.quantum as u64).contains(&recorded.source_frame));
    assert_eq!(recorded.source_frame % d.quantum as u64, 0);
    assert_eq!(recorded.values[target as usize], first);
    assert_eq!(recorded.values[mapping_index], 7.);
    done(
        &mut h,
        Request::ProcessorControl {
            strip: strip(),
            control: Control::Parameter {
                parameter: target,
                value: if kind == ProcessorKind::BuiltInFx {
                    -3.
                } else {
                    0.4
                },
            },
        },
    );
    assert_eq!(
        h.endpoint().recorded_processor_state(looped()),
        Some(recorded)
    );
    done(
        &mut h,
        Request::ProcessorControl {
            strip: strip(),
            control: Control::Restore {
                values: recorded.values,
                replace_mappings: false,
            },
        },
    );
    assert_eq!(
        h.endpoint().processor_state(0).unwrap().values[target as usize],
        first
    );
    mode(&mut h, LoopMode::RecordingDryIntoWet);
    tick(&mut h, 16);
    mode(&mut h, LoopMode::Stopped);
    let rerecorded = h.endpoint().recorded_processor_state(looped()).unwrap();
    assert!(rerecorded.source_frame > recorded.source_frame);
    assert_eq!(rerecorded.values[target as usize], first);
    done(&mut h, Request::LoopClear { loop_id: looped() });
    assert_eq!(h.endpoint().recorded_processor_state(looped()), None);
}

#[cfg(feature = "builtin-fx")]
#[shoop_test]
fn global_fx_controls_defer_without_dsp_and_merge_with_live_offsets_on_activation() {
    use processors::{Control, CC_BASE, STAGE_BASE};
    let mut d = description(1, ProcessorKind::BuiltInFx);
    d.host = HostPorts::numbered(2, 2, 2);
    let mut h =
        OfflineHost::new_with_processors(&d, shoop_host_processors::prepare(&d).unwrap()).unwrap();
    create(&mut h, 1, ProcessorKind::BuiltInFx);
    for (parameter, value) in [
        (STAGE_BASE + 5, 0.),
        (STAGE_BASE + 1, 1.),
        (7, 0.),
        (CC_BASE + 8, 7.),
    ] {
        done(
            &mut h,
            Request::ProcessorControl {
                strip: strip(),
                control: Control::Parameter { parameter, value },
            },
        );
    }
    done(
        &mut h,
        Request::Connect {
            port: EnginePort::GlobalFxControl,
            host: HostPortId(5),
            connected: true,
        },
    );
    h.input_mut(0).fill(1.);
    h.midi_input_mut(0).push(3, 16, &[0xb0, 7, 0]).unwrap();
    tick(&mut h, 16);
    assert_eq!(h.endpoint().processor_state(0).unwrap().values[8], -18.);
    h.midi_input_mut(1).push(7, 16, &[0xb0, 7, 127]).unwrap();
    tick(&mut h, 16);
    assert_eq!(h.endpoint().processor_state(0).unwrap().values[8], -18.);
    assert_eq!(
        h.endpoint().processor_state(0).unwrap().phase,
        shoop_rt_engine::ProcessorPhase::Owned
    );
    assert_eq!(
        h.endpoint().processor_state(0).unwrap().global_midi.pending,
        1
    );
    assert_eq!(&h.output(0)[..16], &[0.; 16]);
    done(
        &mut h,
        Request::StripControl {
            strip: strip(),
            control: StripControl::Monitoring(true),
        },
    );
    for _ in 0..3 {
        tick(&mut h, 128);
    }
    assert_eq!(h.endpoint().processor_state(0).unwrap().values[8], 6.);
    assert_eq!(
        h.endpoint().processor_state(0).unwrap().global_midi.pending,
        0
    );
    assert_eq!(
        h.endpoint().processor_state(0).unwrap().global_midi.drained,
        1
    );
    assert!(h.output(0)[..128]
        .iter()
        .all(|sample| (sample - 1.9952623).abs() < 1e-5));
    h.midi_input_mut(0).push(5, 16, &[0xb0, 7, 0]).unwrap();
    h.midi_input_mut(1).push(5, 16, &[0xb0, 7, 127]).unwrap();
    tick(&mut h, 16);
    assert_eq!(h.endpoint().processor_state(0).unwrap().values[8], 6.);
    h.midi_input_mut(0).push(10, 16, &[0xb0, 7, 0]).unwrap();
    h.midi_input_mut(1).push(2, 16, &[0xb0, 7, 127]).unwrap();
    tick(&mut h, 16);
    assert_eq!(h.endpoint().processor_state(0).unwrap().values[8], -18.);
}
#[cfg(feature = "oxisynth")]
#[shoop_test]
fn global_notes_and_global_overflow_never_trigger_or_reset_ordinary_synth_voices() {
    let mut d = description(2, ProcessorKind::OxiSynth);
    d.host = HostPorts::numbered(2, 2, 3);
    d.midi_events = 32;
    d.midi_bytes = 256;
    let mut h =
        OfflineHost::new_with_processors(&d, shoop_host_processors::prepare(&d).unwrap()).unwrap();
    create(&mut h, 2, ProcessorKind::OxiSynth);
    done(
        &mut h,
        Request::Connect {
            port: EnginePort::GlobalFxControl,
            host: HostPortId(6),
            connected: true,
        },
    );
    done(
        &mut h,
        Request::Connect {
            port: EnginePort::GlobalFxControl,
            host: HostPortId(5),
            connected: true,
        },
    );
    done(
        &mut h,
        Request::StripControl {
            strip: strip(),
            control: StripControl::Monitoring(true),
        },
    );
    h.midi_input_mut(1).push(3, 128, &[0x90, 60, 120]).unwrap();
    for _ in 0..4 {
        tick(&mut h, 128);
    }
    assert!(h.output(0)[..128].iter().all(|sample| sample.abs() < 1e-5));
    h.midi_input_mut(0).push(3, 128, &[0x90, 60, 120]).unwrap();
    for _ in 0..4 {
        tick(&mut h, 128);
    }
    assert!(h.output(0)[..128].iter().any(|sample| sample.abs() > 0.001));
    for controller in 0..32 {
        h.midi_input_mut(1)
            .push(3, 128, &[0xb0, controller, 64])
            .unwrap();
    }
    h.midi_input_mut(2).push(3, 128, &[0xb0, 7, 1]).unwrap();
    tick(&mut h, 128);
    tick(&mut h, 128);
    assert_eq!(
        h.endpoint().processor_state(0).unwrap().phase,
        shoop_rt_engine::ProcessorPhase::Running
    );
    assert!(h.output(0)[..128].iter().any(|sample| sample.abs() > 0.001));
}

#[cfg(feature = "builtin-fx")]
#[shoop_test]
fn restored_fx_settings_start_at_one_source_boundary_after_the_declared_delay() {
    use processors::{Control, STAGE_BASE};
    let d = description(1, ProcessorKind::BuiltInFx);
    let mut h =
        OfflineHost::new_with_processors(&d, shoop_host_processors::prepare(&d).unwrap()).unwrap();
    create(&mut h, 1, ProcessorKind::BuiltInFx);
    done(
        &mut h,
        Request::ProcessorControl {
            strip: strip(),
            control: Control::Parameter {
                parameter: STAGE_BASE + 5,
                value: 0.,
            },
        },
    );
    done(
        &mut h,
        Request::StripControl {
            strip: strip(),
            control: StripControl::Monitoring(true),
        },
    );
    h.input_mut(0).fill(1.);
    tick(&mut h, 128);
    assert_eq!(&h.output(0)[32..128], &[1.; 96]);
    let mut values = h.endpoint().processor_state(0).unwrap().values;
    values[24] = 1.;
    values[7] = 0.;
    values[8] = -6.;
    let id = assert_no_heap(|| {
        h.request(Request::ProcessorControl {
            strip: strip(),
            control: Control::Restore {
                values,
                replace_mappings: false,
            },
        })
    })
    .unwrap();
    let mut samples = Vec::new();
    let mut applied = false;
    for _ in 0..5 {
        tick(&mut h, 16);
        samples.extend_from_slice(&h.output(0)[..16]);
        while let Some(message) = assert_no_heap(|| h.receive()) {
            if let EngineMessage::Result {
                correlation,
                outcome,
            } = message
            {
                if correlation == id {
                    assert_eq!(outcome, Outcome::Done);
                    applied = true;
                }
            }
        }
    }
    assert!(applied);
    assert_eq!(&samples[..32], &[1.; 32]);
    assert!(samples[32..]
        .iter()
        .all(|sample| (sample - 0.5011872).abs() < 1e-5));
}
#[cfg(feature = "builtin-fx")]
#[shoop_test]
fn bounded_state_restore_preflights_every_value_and_mapping_before_changing_dsp() {
    use processors::{Control, CC_BASE};
    let d = description(1, ProcessorKind::BuiltInFx);
    let mut h =
        OfflineHost::new_with_processors(&d, shoop_host_processors::prepare(&d).unwrap()).unwrap();
    create(&mut h, 1, ProcessorKind::BuiltInFx);
    let change = |control| Request::ProcessorControl {
        strip: strip(),
        control,
    };
    done(
        &mut h,
        change(Control::Parameter {
            parameter: 5,
            value: 18.,
        }),
    );
    done(
        &mut h,
        change(Control::Parameter {
            parameter: CC_BASE + 5,
            value: 7.,
        }),
    );
    let before = h.endpoint().processor_state(0).unwrap().values;
    for invalid in [31, 55] {
        let mut values = before;
        values[0] = -12.;
        values[invalid] = 99.;
        assert!(matches!(
            request(
                &mut h,
                change(Control::Restore {
                    values,
                    replace_mappings: true
                })
            ),
            Outcome::Refused(_)
        ));
        assert_eq!(h.endpoint().processor_state(0).unwrap().values, before);
    }
    let mut duplicate = before;
    duplicate[32] = 7.;
    assert!(matches!(
        request(
            &mut h,
            change(Control::Restore {
                values: duplicate,
                replace_mappings: true
            })
        ),
        Outcome::Refused(_)
    ));
    assert_eq!(h.endpoint().processor_state(0).unwrap().values, before);
    let mut values = before;
    values[5] = 24.;
    values[37] = -1.;
    done(
        &mut h,
        change(Control::Restore {
            values,
            replace_mappings: false,
        }),
    );
    assert_eq!(h.endpoint().processor_state(0).unwrap().values[5], 24.);
    assert_eq!(h.endpoint().processor_state(0).unwrap().values[37], 7.);
    done(
        &mut h,
        change(Control::Restore {
            values,
            replace_mappings: true,
        }),
    );
    assert_eq!(h.endpoint().processor_state(0).unwrap().values[37], -1.);
    done(
        &mut h,
        Request::StripControl {
            strip: strip(),
            control: StripControl::Monitoring(true),
        },
    );
    h.input_mut(0).fill(0.5);
    tick(&mut h, 64);
    assert_eq!(
        h.endpoint().processor_state(0).unwrap().phase,
        shoop_rt_engine::ProcessorPhase::Running
    );
    let mut changed = h.endpoint().processor_state(0).unwrap().values;
    changed[5] = 3.;
    done(
        &mut h,
        change(Control::Restore {
            values: changed,
            replace_mappings: true,
        }),
    );
    tick(&mut h, 64);
    let state = h.endpoint().processor_state(0).unwrap();
    assert_eq!(state.values[5], 3.);
    assert_eq!(state.frame.0, h.next_frame());
    assert_eq!(&h.output(0)[16..64], &[0.5; 48]);
}
fn create(h: &mut OfflineHost, channels: u8, kind: ProcessorKind) {
    create_with_host_width(h, channels, kind, channels.max(2));
}
fn create_with_host_width(
    h: &mut OfflineHost,
    channels: u8,
    kind: ProcessorKind,
    host_channels: u8,
) {
    h.connect().unwrap();
    while h.receive().is_some() {}
    done(
        h,
        Request::CreateProcessedStrip {
            strip: strip(),
            shape: StripShape {
                audio_inputs: channels,
                audio_outputs: channels,
                midi_input: true,
                midi_output: false,
            },
            kind,
        },
    );
    for c in 0..channels {
        for (kind, host) in [
            (PortKind::AudioIn, c as u32),
            (PortKind::AudioOut, host_channels as u32 + c as u32),
        ] {
            done(
                h,
                Request::Connect {
                    port: EnginePort::Strip(StripPort {
                        strip: strip(),
                        kind,
                        index: c,
                    }),
                    host: HostPortId(host),
                    connected: true,
                },
            );
        }
    }
    done(
        h,
        Request::Connect {
            port: EnginePort::Strip(StripPort {
                strip: strip(),
                kind: PortKind::MidiIn,
                index: 0,
            }),
            host: HostPortId(host_channels as u32 * 2),
            connected: true,
        },
    );
    done(
        h,
        Request::CreateLoop {
            loop_id: looped(),
            strip: strip(),
        },
    );
}
fn mode(h: &mut OfflineHost, mode: LoopMode) {
    done(
        h,
        Request::LoopTransition {
            loop_id: looped(),
            mode,
            delay: None,
        },
    );
}

#[cfg(feature = "builtin-fx")]
#[shoop_test]
fn fx_monitoring_capture_and_both_playback_paths_share_one_delay_at_every_width() {
    for channels in 1..=8 {
        let d = description(channels, ProcessorKind::BuiltInFx);
        let mut h =
            OfflineHost::new_with_processors(&d, shoop_host_processors::prepare(&d).unwrap())
                .unwrap();
        create(&mut h, channels, ProcessorKind::BuiltInFx);
        let invalid = Request::CreateProcessedStrip {
            strip: StripId(SlotId::new(1, 1)),
            shape: StripShape {
                audio_inputs: channels,
                audio_outputs: channels,
                midi_input: true,
                midi_output: false,
            },
            kind: ProcessorKind::BuiltInFx,
        };
        assert_eq!(
            request(&mut h, invalid),
            Outcome::Refused(Refusal::Capacity)
        );
        done(
            &mut h,
            Request::StripControl {
                strip: strip(),
                control: StripControl::Monitoring(true),
            },
        );
        for c in 0..channels as usize {
            h.input_mut(c).fill(c as f32 + 0.25);
        }
        tick(&mut h, 64);
        for c in 0..channels as usize {
            assert_eq!(
                &h.output(c)[32..64],
                &[c as f32 + 0.25; 32],
                "monitor width={channels} c={c}"
            );
        }
        mode(&mut h, LoopMode::Recording);
        tick(&mut h, 64);
        mode(&mut h, LoopMode::Playing);
        for c in 0..channels as usize {
            h.input_mut(c).fill(0.);
        }
        done(
            &mut h,
            Request::StripControl {
                strip: strip(),
                control: StripControl::Monitoring(false),
            },
        );
        tick(&mut h, 128);
        for c in 0..channels as usize {
            assert_eq!(
                &h.output(c)[64..128],
                &[c as f32 + 0.25; 64],
                "wet width={channels} c={c}"
            );
        }
        mode(&mut h, LoopMode::PlayingDryThroughWet);
        tick(&mut h, 128);
        for c in 0..channels as usize {
            assert_eq!(
                &h.output(c)[64..128],
                &[c as f32 + 0.25; 64],
                "dry width={channels} c={c}"
            );
        }
        mode(&mut h, LoopMode::RecordingDryIntoWet);
        tick(&mut h, 128);
        mode(&mut h, LoopMode::Playing);
        tick(&mut h, 128);
        for c in 0..channels as usize {
            assert_eq!(
                &h.output(c)[64..128],
                &[c as f32 + 0.25; 64],
                "rerecorded width={channels} c={c}"
            );
        }
        mode(&mut h, LoopMode::Stopped);
        tick(&mut h, 128);
        assert!(h.output(0)[32..128].iter().all(|v| *v == 0.));
    }
}

#[cfg(feature = "oxisynth")]
#[shoop_test]
fn synth_production_monitoring_ignores_dry_audio_and_cleans_voices_on_sleep_and_reuse() {
    let d = description(2, ProcessorKind::OxiSynth);
    let mut h =
        OfflineHost::new_with_processors(&d, shoop_host_processors::prepare(&d).unwrap()).unwrap();
    create(&mut h, 2, ProcessorKind::OxiSynth);
    done(
        &mut h,
        Request::StripControl {
            strip: strip(),
            control: StripControl::Monitoring(true),
        },
    );
    h.input_mut(0).fill(100.);
    h.input_mut(1).fill(-100.);
    tick(&mut h, 128);
    assert!(h.output(0)[32..128].iter().all(|v| v.abs() < 1e-6));
    h.midi_input_mut(0).push(3, 128, &[0x90, 60, 120]).unwrap();
    tick(&mut h, 128);
    tick(&mut h, 128);
    assert!(h.output(0)[..128].iter().any(|v| v.abs() > 1e-5));
    done(
        &mut h,
        Request::StripControl {
            strip: strip(),
            control: StripControl::Monitoring(false),
        },
    );
    tick(&mut h, 128);
    assert!(h.output(0)[32..128].iter().all(|v| v.abs() < 1e-6));
    done(
        &mut h,
        Request::StripControl {
            strip: strip(),
            control: StripControl::Monitoring(true),
        },
    );
    tick(&mut h, 128);
    assert!(h.output(0)[32..128].iter().all(|v| v.abs() < 1e-6));
    done(&mut h, Request::RemoveStrip { strip: strip() });
    tick(&mut h, 128);
    assert!(h.output(0)[..128].iter().all(|v| v.abs() < 1e-6));
}

#[cfg(feature = "builtin-fx")]
#[shoop_test]
fn hosted_impulses_preserve_sample_offsets_and_count_node_and_host_delay_once() {
    for quantum in [1, 16, 32, 64] {
        let mut d = description(2, ProcessorKind::BuiltInFx);
        d.quantum = quantum;
        let mut h =
            OfflineHost::new_with_processors(&d, shoop_host_processors::prepare(&d).unwrap())
                .unwrap();
        create(&mut h, 2, ProcessorKind::BuiltInFx);
        done(
            &mut h,
            Request::StripControl {
                strip: strip(),
                control: StripControl::Monitoring(true),
            },
        );
        h.input_mut(0).fill(0.);
        h.input_mut(1).fill(0.);
        h.input_mut(0)[3] = 0.75;
        h.input_mut(1)[5] = -0.5;
        let delay = quantum + if quantum > 1 { quantum } else { 0 };
        let mut actual = [Vec::new(), Vec::new()];
        for (index, frames) in [7, 13, 1, 27, 79, 1, 43, 85].into_iter().enumerate() {
            if index > 0 {
                h.input_mut(0).fill(0.);
                h.input_mut(1).fill(0.);
            }
            tick(&mut h, frames);
            for c in 0..2 {
                actual[c].extend_from_slice(&h.output(c)[..frames]);
            }
        }
        for c in 0..2 {
            for (frame, &sample) in actual[c].iter().enumerate() {
                let expected = if frame == delay + [3, 5][c] {
                    [0.75, -0.5][c]
                } else {
                    0.
                };
                assert_eq!(sample, expected, "Q={quantum} c={c} frame={frame}");
            }
        }
    }
}

#[cfg(feature = "oxisynth")]
#[shoop_test]
fn hosted_synth_matches_unbuffered_notes_with_sample_offsets_after_exact_declared_delay() {
    use rt_buffered::{Backend, Batch, BatchTag};
    use rt_runtime::{
        buffer::{AudioPorts, MidiBuffer},
        Frame,
    };
    let d = description(2, ProcessorKind::OxiSynth);
    let mut h =
        OfflineHost::new_with_processors(&d, shoop_host_processors::prepare(&d).unwrap()).unwrap();
    create(&mut h, 2, ProcessorKind::OxiSynth);
    done(
        &mut h,
        Request::StripControl {
            strip: strip(),
            control: StripControl::Monitoring(true),
        },
    );
    let mut raw = shoop_dsp::backends::SynthBackend::prepare(8000., 16).unwrap();
    let mut input = MidiBuffer::new(64, 512).unwrap();
    let zeros = [0.; 32];
    let mut output = [0.; 32];
    let mut expected = [Vec::new(), Vec::new()];
    for tick_index in 0..9 {
        input.clear();
        if tick_index == 1 {
            input.push(5, 16, &[0x90, 60, 120]).unwrap();
        }
        if tick_index == 6 {
            input.push(7, 16, &[0x80, 60, 0]).unwrap();
        }
        assert_no_heap(|| {
            raw.process(&mut Batch {
                observer: None,
                tag: BatchTag {
                    generation: 1,
                    sequence: tick_index,
                    source: Frame(tick_index * 16),
                },
                frames: 16,
                audio: AudioPorts::new(&zeros, &mut output, 16, 0, 16).unwrap(),
                midi_inputs: core::slice::from_ref(&input),
                midi_controls: None,
                midi_outputs: &mut [],
                parameters: &[],
            })
        })
        .unwrap();
        if tick_index > 0 {
            for c in 0..2 {
                expected[c].extend_from_slice(&output[c * 16..c * 16 + 16]);
            }
        }
    }
    h.midi_input_mut(0).push(5, 128, &[0x90, 60, 120]).unwrap();
    h.midi_input_mut(0).push(87, 128, &[0x80, 60, 0]).unwrap();
    tick(&mut h, 128);
    for c in 0..2 {
        assert!(h.output(c)[..32].iter().all(|sample| sample.abs() < 1e-6));
        assert_eq!(&h.output(c)[32..128], &expected[c][..96], "channel={c}");
    }
}

#[cfg(feature = "builtin-fx")]
#[shoop_test]
fn recorded_wet_bypasses_enabled_dsp_and_dry_playback_processes_exactly_once() {
    let d = description(2, ProcessorKind::BuiltInFx);
    let mut bank = shoop_host_processors::prepare(&d).unwrap();
    for instance in &mut bank.instances {
        instance
            .backend
            .control(processors::STAGE_BASE + 1, 1.)
            .unwrap();
        instance
            .backend
            .control(processors::STAGE_BASE + 5, 0.)
            .unwrap();
        instance.backend.control(7, 0.).unwrap();
        instance.backend.control(8, -6.).unwrap();
    }
    let mut h = OfflineHost::new_with_processors(&d, bank).unwrap();
    create(&mut h, 2, ProcessorKind::BuiltInFx);
    h.input_mut(0).fill(0.25);
    h.input_mut(1).fill(0.25);
    done(
        &mut h,
        Request::StripControl {
            strip: strip(),
            control: StripControl::Monitoring(true),
        },
    );
    for _ in 0..8 {
        tick(&mut h, 128);
        assert!(
            h.output(0)[..128].iter().all(|sample| sample.is_finite()),
            "host output {:?}",
            &h.output(0)[..16]
        );
    }
    let wet = h.output(0)[127];
    assert!((wet - 0.25 * 10.0_f32.powf(-6. / 20.)).abs() < 1e-6);
    mode(&mut h, LoopMode::Recording);
    tick(&mut h, 128);
    mode(&mut h, LoopMode::Playing);
    h.input_mut(0).fill(0.);
    h.input_mut(1).fill(0.);
    done(
        &mut h,
        Request::StripControl {
            strip: strip(),
            control: StripControl::Monitoring(false),
        },
    );
    for mode_value in [
        LoopMode::Playing,
        LoopMode::PlayingDryThroughWet,
        LoopMode::RecordingDryIntoWet,
        LoopMode::Playing,
    ] {
        mode(&mut h, mode_value);
        tick(&mut h, 128);
        for &sample in &h.output(0)[64..128] {
            assert!(
                (sample - wet).abs() < 1e-6,
                "{mode_value:?}: {sample} vs {wet}"
            );
        }
    }
}

#[cfg(feature = "oxisynth")]
#[shoop_test]
fn synth_wet_capture_and_dry_midi_replay_both_remain_audible_without_monitoring() {
    let d = description(2, ProcessorKind::OxiSynth);
    let mut h =
        OfflineHost::new_with_processors(&d, shoop_host_processors::prepare(&d).unwrap()).unwrap();
    create(&mut h, 2, ProcessorKind::OxiSynth);
    mode(&mut h, LoopMode::Recording);
    h.midi_input_mut(0).push(5, 128, &[0x90, 60, 120]).unwrap();
    h.midi_input_mut(0).push(119, 128, &[0x80, 60, 0]).unwrap();
    tick(&mut h, 128);
    for mode_value in [
        LoopMode::Playing,
        LoopMode::PlayingDryThroughWet,
        LoopMode::RecordingDryIntoWet,
        LoopMode::Playing,
    ] {
        mode(&mut h, mode_value);
        tick(&mut h, 128);
        assert!(
            h.output(0)[32..128]
                .iter()
                .any(|sample| sample.abs() > 1e-4),
            "{mode_value:?}"
        );
    }
}

#[cfg(feature = "oxisynth")]
#[shoop_test]
fn ordinary_midi_overflow_clears_sounding_synth_voices_and_keeps_the_track_usable() {
    let mut d = description(2, ProcessorKind::OxiSynth);
    d.midi_events = 32;
    d.midi_bytes = 256;
    let mut h =
        OfflineHost::new_with_processors(&d, shoop_host_processors::prepare(&d).unwrap()).unwrap();
    create(&mut h, 2, ProcessorKind::OxiSynth);
    done(
        &mut h,
        Request::StripControl {
            strip: strip(),
            control: StripControl::Monitoring(true),
        },
    );
    h.midi_input_mut(0).push(5, 128, &[0x90, 60, 120]).unwrap();
    tick(&mut h, 128);
    tick(&mut h, 128);
    assert!(h.output(0)[..128].iter().any(|sample| sample.abs() > 1e-4));
    assert_no_heap(|| {
        h.request(Request::InjectMidi {
            strip: strip(),
            message: MidiMessage::new(&[0xb0, 1, 0]).unwrap(),
        })
    })
    .unwrap();
    for _ in 0..32 {
        h.midi_input_mut(0).push(0, 16, &[0xb0, 1, 0]).unwrap();
    }
    tick(&mut h, 16);
    tick(&mut h, 128);
    assert!(h.output(0)[32..128]
        .iter()
        .all(|sample| sample.abs() < 1e-6));
    h.midi_input_mut(0).push(5, 128, &[0x90, 64, 120]).unwrap();
    tick(&mut h, 128);
    tick(&mut h, 128);
    assert!(h.output(0)[..128].iter().any(|sample| sample.abs() > 1e-4));
}

#[cfg(feature = "builtin-fx")]
#[shoop_test]
fn invalid_processor_output_faults_only_its_owner_and_direct_audio_keeps_running() {
    use rt_buffered::{Backend, Batch};
    use rt_runtime::{graph::PortCounts, Error};
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    struct Corrupt {
        inner: Box<dyn Backend>,
        fail: Arc<AtomicBool>,
    }
    impl Backend for Corrupt {
        fn supports_observation(&self) -> bool {
            self.inner.supports_observation()
        }
        fn ports(&self) -> PortCounts {
            self.inner.ports()
        }
        fn validate_parameter(&self, id: u32, value: f32) -> Result<(), Error> {
            self.inner.validate_parameter(id, value)
        }
        fn parameter(&self, id: u32) -> Option<f32> {
            self.inner.parameter(id)
        }
        fn control(&mut self, id: u32, value: f32) -> Result<(), Error> {
            self.inner.control(id, value)
        }
        fn process(&mut self, batch: &mut Batch<'_, '_>) -> Result<(), Error> {
            self.inner.process(batch)?;
            if self.fail.load(Ordering::Relaxed) {
                batch.audio.output(0).unwrap()[0] = f32::NAN;
            }
            Ok(())
        }
        fn reset(&mut self) -> Result<(), Error> {
            self.inner.reset()
        }
        fn reset_step(&mut self, step: usize) -> Result<bool, Error> {
            self.inner.reset_step(step)
        }
        fn suspend(&mut self) -> Result<(), Error> {
            self.inner.suspend()
        }
    }
    let d = description(2, ProcessorKind::BuiltInFx);
    let fail = Arc::new(AtomicBool::new(false));
    let mut bank = shoop_host_processors::prepare(&d).unwrap();
    bank.instances = bank
        .instances
        .into_iter()
        .map(|mut instance| {
            instance.backend = Box::new(Corrupt {
                inner: instance.backend,
                fail: fail.clone(),
            });
            instance
        })
        .collect();
    let mut h = OfflineHost::new_with_processors(&d, bank).unwrap();
    create(&mut h, 2, ProcessorKind::BuiltInFx);
    let direct = StripId(SlotId::new(1, 1));
    done(
        &mut h,
        Request::CreateStrip {
            strip: direct,
            shape: StripShape {
                audio_inputs: 1,
                audio_outputs: 1,
                midi_input: false,
                midi_output: false,
            },
        },
    );
    for (kind, host) in [(PortKind::AudioIn, 1), (PortKind::AudioOut, 3)] {
        done(
            &mut h,
            Request::Connect {
                port: EnginePort::Strip(StripPort {
                    strip: direct,
                    kind,
                    index: 0,
                }),
                host: HostPortId(host),
                connected: true,
            },
        );
    }
    for track in [strip(), direct] {
        done(
            &mut h,
            Request::StripControl {
                strip: track,
                control: StripControl::Monitoring(true),
            },
        );
    }
    h.input_mut(0).fill(0.25);
    h.input_mut(1).fill(1.);
    tick(&mut h, 128);
    assert!(h.output(0)[64..128].iter().any(|sample| sample.abs() > 0.1));
    mode(&mut h, LoopMode::Recording);
    tick(&mut h, 128);
    mode(&mut h, LoopMode::Stopped);
    let old_take = h.endpoint().recorded_processor_state(looped()).unwrap();
    mode(&mut h, LoopMode::RecordingDryIntoWet);
    tick(&mut h, 32);
    fail.store(true, Ordering::Relaxed);
    tick(&mut h, 128);
    assert_eq!(&h.output(0)[32..128], &[0.; 96]);
    assert_eq!(&h.output(1)[32..128], &[1.; 96]);
    assert_eq!(
        h.endpoint().processor_state(0).unwrap().phase,
        shoop_rt_engine::ProcessorPhase::Faulted
    );
    assert_eq!(
        h.endpoint().recorded_processor_state(looped()),
        Some(old_take)
    );
    mode(&mut h, LoopMode::Playing);
    tick(&mut h, 128);
    assert!(h.output(0)[64..128].iter().any(|sample| sample.abs() > 0.1));
    done(&mut h, Request::RemoveStrip { strip: strip() });
    tick(&mut h, 128);
    assert_eq!(&h.output(1)[..128], &[1.; 128]);
}

#[cfg(feature = "builtin-fx")]
#[shoop_test]
fn rerecord_prerouting_keeps_processing_active_and_cancellation_keeps_monitoring_off() {
    let d = description(2, ProcessorKind::BuiltInFx);
    let mut h =
        OfflineHost::new_with_processors(&d, shoop_host_processors::prepare(&d).unwrap()).unwrap();
    create(&mut h, 2, ProcessorKind::BuiltInFx);
    done(
        &mut h,
        Request::StripControl {
            strip: strip(),
            control: StripControl::Monitoring(true),
        },
    );
    h.input_mut(0).fill(0.25);
    mode(&mut h, LoopMode::Recording);
    tick(&mut h, 128);
    mode(&mut h, LoopMode::Playing);
    done(
        &mut h,
        Request::LoopTransition {
            loop_id: looped(),
            mode: LoopMode::RecordingDryIntoWet,
            delay: Some(1),
        },
    );
    assert_eq!(
        h.endpoint().processor_state(0).unwrap().phase,
        shoop_rt_engine::ProcessorPhase::Running
    );
    mode(&mut h, LoopMode::Playing);
    tick(&mut h, 64);
    assert_eq!(
        h.endpoint().processor_state(0).unwrap().phase,
        shoop_rt_engine::ProcessorPhase::Owned
    );
}

#[cfg(feature = "builtin-fx")]
#[shoop_test]
fn simultaneous_live_and_dry_playback_midi_merge_by_offset_and_keep_applied_order() {
    use shoop_dsp::builtin_fx::BuiltInFxParameter;
    let d = description(2, ProcessorKind::BuiltInFx);
    let mut bank = shoop_host_processors::prepare(&d).unwrap();
    for instance in &mut bank.instances {
        instance
            .backend
            .control(processors::CC_BASE + 5, 13.)
            .unwrap();
    }
    let mut h = OfflineHost::new_with_processors(&d, bank).unwrap();
    create(&mut h, 2, ProcessorKind::BuiltInFx);
    mode(&mut h, LoopMode::Recording);
    for quantum in 0..8 {
        h.midi_input_mut(0)
            .push(quantum * 16, 128, &[0xb0, 13, 127])
            .unwrap();
    }
    tick(&mut h, 128);
    mode(&mut h, LoopMode::PlayingDryThroughWet);
    done(
        &mut h,
        Request::StripControl {
            strip: strip(),
            control: StripControl::Monitoring(true),
        },
    );
    h.midi_input_mut(0).push(15, 16, &[0xb0, 13, 1]).unwrap();
    tick(&mut h, 16);
    let state = h.endpoint().processor_state(0).unwrap();
    assert_eq!(state.phase, shoop_rt_engine::ProcessorPhase::Running);
    assert_eq!(state.values[5], BuiltInFxParameter::Drive.value_from_cc(1));
    tick(&mut h, 128);
}

#[cfg(feature = "builtin-fx")]
#[shoop_test]
fn stereo_loop_balance_uses_logical_roles_in_an_eight_channel_prepared_host() {
    let d = description(8, ProcessorKind::BuiltInFx);
    let mut bank = shoop_host_processors::prepare(&d).unwrap();
    for instance in &mut bank.instances {
        for stage in 0..6 {
            instance
                .backend
                .control(processors::STAGE_BASE + stage, 0.)
                .unwrap();
        }
    }
    let mut h = OfflineHost::new_with_processors(&d, bank).unwrap();
    create_with_host_width(&mut h, 2, ProcessorKind::BuiltInFx, 8);
    h.input_mut(0).fill(0.25);
    h.input_mut(1).fill(0.5);
    mode(&mut h, LoopMode::Recording);
    tick(&mut h, 64);
    mode(&mut h, LoopMode::Playing);
    done(
        &mut h,
        Request::LoopBalance {
            loop_id: looped(),
            balance: -1.,
        },
    );
    for mode_value in [LoopMode::Playing, LoopMode::PlayingDryThroughWet] {
        mode(&mut h, mode_value);
        tick(&mut h, 128);
        assert_eq!(&h.output(0)[64..128], &[0.25; 64], "{mode_value:?}");
        assert_eq!(&h.output(1)[64..128], &[0.; 64], "{mode_value:?}");
    }
}

#[shoop_test(no_wasm = "wall-clock callback measurement is a native CI artifact")]
#[ignore = "representative release measurement in CI"]
fn hosted_callback_measurement_at_admitted_channel_voice_and_midi_limits() {
    use std::time::{Duration, Instant};
    for (kind, channels) in [(ProcessorKind::BuiltInFx, 8), (ProcessorKind::OxiSynth, 2)] {
        if (kind == ProcessorKind::BuiltInFx && !cfg!(feature = "builtin-fx"))
            || (kind == ProcessorKind::OxiSynth && !cfg!(feature = "oxisynth"))
        {
            continue;
        }
        let mut d = description(channels, kind);
        d.sample_rate = 48000;
        d.quantum = 128;
        d.midi_events = 1024;
        d.midi_bytes = 4096;
        d.processors.memory_mb = 512;
        let mut h =
            OfflineHost::new_with_processors(&d, shoop_host_processors::prepare(&d).unwrap())
                .unwrap();
        create(&mut h, channels, kind);
        if kind == ProcessorKind::BuiltInFx {
            for stage in 0..6 {
                done(
                    &mut h,
                    Request::ProcessorControl {
                        strip: strip(),
                        control: processors::Control::Parameter {
                            parameter: processors::STAGE_BASE + stage,
                            value: 1.,
                        },
                    },
                );
            }
        }
        done(
            &mut h,
            Request::ProcessorControl {
                strip: strip(),
                control: processors::Control::Parameter {
                    parameter: processors::CC_BASE,
                    value: 7.,
                },
            },
        );
        done(
            &mut h,
            Request::StripControl {
                strip: strip(),
                control: StripControl::Monitoring(true),
            },
        );
        for channel in 0..channels as usize {
            h.input_mut(channel).fill(0.25);
        }
        let (mut maximum, mut total) = (Duration::ZERO, Duration::ZERO);
        for callback in 0..256 {
            for event in 0..d.midi_events {
                let message = if kind == ProcessorKind::OxiSynth && event < 256 {
                    [0x90 | (event / 128) as u8, (event % 128) as u8, 100]
                } else {
                    [0xb0, 7, (event % 128) as u8]
                };
                h.midi_input_mut(0).push(0, 128, &message).unwrap();
            }
            let started = Instant::now();
            tick(&mut h, 128);
            let elapsed = started.elapsed();
            maximum = maximum.max(elapsed);
            total += elapsed;
            assert_eq!(
                h.endpoint().processor_state(0).unwrap().phase,
                shoop_rt_engine::ProcessorPhase::Running,
                "callback={callback}"
            );
        }
        println!("kind={kind:?} channels={channels} polyphony_limit=256 midi_events_per_callback={} callbacks=256 rate={} quantum={} peak_us={} mean_us={} callback_period_us={} simulated_xruns=unavailable; non-real-time wall clock, no deadline guarantee", d.midi_events, d.sample_rate, d.quantum, maximum.as_micros(), total.as_micros() / 256, 128_000_000 / d.sample_rate);
    }
}

#[cfg(feature = "builtin-fx")]
#[shoop_test]
fn scheduled_dry_pickup_wakes_real_fx_before_the_playback_transition() {
    let mut d = description(2, ProcessorKind::BuiltInFx);
    d.telemetry_interval_frames = 0;
    let mut h =
        OfflineHost::new_with_processors(&d, shoop_host_processors::prepare(&d).unwrap()).unwrap();
    create(&mut h, 1, ProcessorKind::BuiltInFx);
    let source = LoopSlotId(SlotId::new(1, 1));
    done(
        &mut h,
        Request::CreateLoop {
            loop_id: source,
            strip: strip(),
        },
    );
    for (id, value) in [(1, 1f32), (2, 0f32)] {
        let bytes: Vec<_> = (0..129 * 2).flat_map(|_| value.to_le_bytes()).collect();
        done(
            &mut h,
            Request::ContentBegin(ContentDescription {
                id,
                midi: false,
                frames: 129,
                bytes: bytes.len() as u64,
            }),
        );
        let chunk = usize::from(h.endpoint().limits().content_chunk_bytes);
        for (index, data) in bytes.chunks(chunk).enumerate() {
            done(
                &mut h,
                Request::ContentWrite {
                    id,
                    offset: (index * chunk) as u64,
                    data: ContentChunk::new(data).unwrap(),
                },
            );
        }
        done(&mut h, Request::ContentFinish { id });
        let mut windows = PlaybackWindows::default();
        windows.audio[0].start_offset = 65;
        windows.audio[0].preplay = 49;
        windows.audio[1].start_offset = 65;
        windows.audio[1].preplay = 49;
        done(
            &mut h,
            Request::ContentWindow {
                id,
                frames: 64,
                windows,
            },
        );
    }
    done(
        &mut h,
        Request::ContentBegin(ContentDescription {
            id: 3,
            midi: true,
            frames: 129,
            bytes: 0,
        }),
    );
    done(&mut h, Request::ContentFinish { id: 3 });
    let mut windows = PlaybackWindows::default();
    for window in &mut windows.audio[..2] {
        window.start_offset = 65;
        window.preplay = 49;
    }
    done(
        &mut h,
        Request::ContentWindow {
            id: 3,
            frames: 64,
            windows,
        },
    );
    let imported = request(
        &mut h,
        Request::ContentApply {
            loop_id: looped(),
            frames: 64,
            dry_audio: Some(1),
            audio: Some(2),
            midi: Some(3),
            recorded_state: None,
        },
    );
    assert!(
        matches!(imported, Outcome::ContentApplied { .. }),
        "{imported:?}"
    );
    assert!(matches!(
        request(
            &mut h,
            Request::LoopTiming {
                loop_id: source,
                expected_revision: 0,
                expected_content: 0,
                edit: TimingEdit {
                    length: Some(256),
                    ..Default::default()
                }
            }
        ),
        Outcome::TimingApplied(_)
    ));
    done(
        &mut h,
        Request::LoopSync {
            loop_id: looped(),
            source: Some(source),
        },
    );
    done(
        &mut h,
        Request::LoopTransition {
            loop_id: source,
            mode: LoopMode::Playing,
            delay: None,
        },
    );
    done(
        &mut h,
        Request::LoopTransition {
            loop_id: looped(),
            mode: LoopMode::PlayingDryThroughWet,
            delay: Some(0),
        },
    );
    let mut early = false;
    let mut played = false;
    for _ in 0..32 {
        tick(&mut h, 7);
        let first = h.output(0)[..7].iter().any(|v| *v > 0.9);
        tick(&mut h, 9);
        let audible = first || h.output(0)[..9].iter().any(|v| *v > 0.9);
        while let Some(message) = h.receive() {
            if let EngineMessage::Telemetry(Telemetry::Loop { loop_id, mode, .. }) = message {
                if loop_id == looped() {
                    early |= mode == LoopMode::Stopped && audible;
                    played |= mode == LoopMode::PlayingDryThroughWet;
                }
            }
        }
        if played {
            break;
        }
    }
    assert!(
        early,
        "49 source frames of pickup must precede the transition after the 32-frame node/host delay"
    );
    assert!(played);
    assert_eq!(
        h.endpoint().processor_state(0).unwrap().phase,
        shoop_rt_engine::ProcessorPhase::Running
    );
}
