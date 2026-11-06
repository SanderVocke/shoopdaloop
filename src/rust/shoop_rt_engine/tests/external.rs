//! External dry/wet roles over ordinary host ports, without any concrete DSP.
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
fn strip() -> StripId {
    StripId(SlotId::new(0, 1))
}
fn looped() -> LoopSlotId {
    LoopSlotId(SlotId::new(0, 1))
}
fn description() -> EngineDescription {
    EngineDescription {
        external_dry_wet: true,
        quantum: 16,
        max_frames: 128,
        ramp_frames: 0,
        strip_channels: 8,
        host: HostPorts::numbered(16, 16, 1),
        segments: vec![Segment {
            strips: 2,
            loops: 2,
        }],
        resource_pages: 1024,
        page_bytes: 512,
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
    panic!("request timed out: {request:?}")
}
fn done(h: &mut OfflineHost, r: Request) {
    assert_eq!(request(h, r), Outcome::Done, "{r:?}");
}
fn connect(h: &mut OfflineHost, kind: PortKind, index: u8, host: u32) {
    done(
        h,
        Request::Connect {
            port: EnginePort::Strip(StripPort {
                strip: strip(),
                kind,
                index,
            }),
            host: HostPortId(host),
            connected: true,
        },
    );
}
fn create(h: &mut OfflineHost, dry: u8, wet: u8, midi: bool) {
    h.connect().unwrap();
    while h.receive().is_some() {}
    done(
        h,
        Request::CreateProcessedStrip {
            strip: strip(),
            shape: StripShape {
                audio_inputs: dry,
                audio_outputs: wet,
                midi_input: midi,
                midi_output: false,
            },
            kind: ProcessorKind::External,
        },
    );
    for c in 0..dry {
        connect(h, PortKind::AudioIn, c, c as u32);
        connect(h, PortKind::AudioSend, c, 24 + c as u32);
    }
    for c in 0..wet {
        connect(h, PortKind::AudioReturn, c, 8 + c as u32);
        connect(h, PortKind::AudioOut, c, 16 + c as u32);
    }
    if midi {
        connect(h, PortKind::MidiIn, 0, 32);
        connect(h, PortKind::MidiSend, 0, 33);
    }
    done(
        h,
        Request::CreateLoop {
            strip: strip(),
            loop_id: looped(),
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
#[shoop_test]
fn external_zero_sides_and_midi_only_preserve_playback_roles_and_dry_media() {
    for (dry, wet, midi) in [(0, 8, false), (8, 0, false), (0, 0, true)] {
        let mut h = OfflineHost::new(&description()).unwrap();
        create(&mut h, dry, wet, midi);
        for channel in 0..8 {
            h.input_mut(channel).fill(channel as f32 + 0.25);
            h.input_mut(8 + channel).fill(channel as f32 + 100.5);
        }
        let default = request(
            &mut h,
            Request::StripDefaultPlayback {
                strip: strip(),
                dry_through_wet: true,
            },
        );
        assert_eq!(
            default,
            if wet > 0 {
                Outcome::Done
            } else {
                Outcome::Refused(Refusal::Unsupported)
            }
        );
        mode(&mut h, LoopMode::Recording);
        if midi {
            h.midi_input_mut(0).push(3, 64, &[0x90, 60, 100]).unwrap();
            h.midi_input_mut(0).push(11, 64, &[0x80, 60, 0]).unwrap();
        }
        tick(&mut h, 64);
        mode(&mut h, LoopMode::Stopped);
        for channel in 0..16 {
            h.input_mut(channel).fill(0.);
        }
        mode(&mut h, LoopMode::Playing);
        tick(&mut h, 128);
        for channel in 0..wet as usize {
            assert_eq!(&h.output(channel)[32..128], &[channel as f32 + 100.5; 96]);
        }
        for value in [
            LoopMode::PlayingDryThroughWet,
            LoopMode::RecordingDryIntoWet,
            LoopMode::PlayingDryThroughWet,
        ] {
            mode(&mut h, value);
            tick(&mut h, 128);
            for channel in 0..dry as usize {
                assert_eq!(
                    &h.output(8 + channel)[32..128],
                    &[channel as f32 + 0.25; 96]
                );
            }
            if midi {
                let output = h.midi_output(0);
                assert!(
                    (0..output.len()).any(|index| output.get(index).unwrap().1 == [0x90, 60, 100])
                );
            }
        }
        mode(&mut h, LoopMode::Playing);
        tick(&mut h, 128);
        for channel in 0..16 {
            assert!(h.output(channel)[32..128]
                .iter()
                .all(|sample| *sample == 0.));
        }
    }
}

#[shoop_test]
fn loop_meter_observes_the_selected_role_width_in_an_unequal_external_track() {
    let mut d = description();
    d.telemetry_interval_frames = 0;
    let mut h = OfflineHost::new(&d).unwrap();
    create(&mut h, 2, 1, false);
    h.input_mut(0).fill(0.25);
    h.input_mut(1).fill(0.5);
    h.input_mut(8).fill(0.75);
    mode(&mut h, LoopMode::Recording);
    tick(&mut h, 64);
    mode(&mut h, LoopMode::Stopped);
    for channel in 0..16 {
        h.input_mut(channel).fill(0.);
    }
    for (value, expected) in [
        (LoopMode::Playing, &[0.75][..]),
        (LoopMode::PlayingDryThroughWet, &[0.25, 0.5][..]),
        (LoopMode::RecordingDryIntoWet, &[0.25, 0.5][..]),
        (LoopMode::Playing, &[0.][..]),
    ] {
        mode(&mut h, value);
        tick(&mut h, 64);
        let mut peaks = None;
        while let Some(message) = assert_no_heap(|| h.receive()) {
            if let EngineMessage::Telemetry(Telemetry::Loop {
                loop_id,
                mode,
                peaks: current,
                ..
            }) = message
            {
                if loop_id == looped() && mode == value {
                    peaks = Some(current);
                }
            }
        }
        assert_eq!(peaks.unwrap().as_slice(), expected, "{value:?}");
    }
}

#[shoop_test]
fn bulk_audio_roles_pin_both_eight_channel_sides_through_clear_and_small_copy_budgets() {
    for page_bytes in [32, 512] {
        let mut d = description();
        d.page_bytes = page_bytes;
        d.resource_pages = 8192;
        let mut h = OfflineHost::new(&d).unwrap();
        create(&mut h, 8, 8, false);
        for channel in 0..8 {
            h.input_mut(channel).fill(channel as f32 + 0.25);
            h.input_mut(8 + channel).fill(channel as f32 + 100.5);
        }
        mode(&mut h, LoopMode::Recording);
        tick(&mut h, 128);
        mode(&mut h, LoopMode::Stopped);
        let mut offset = 0;
        let mut cleared = false;
        for id in 1..100 {
            assert_no_heap(|| {
                h.bulk(BulkRead {
                    id,
                    loop_id: looped(),
                    selector: BulkSelector::AudioRoles,
                    offset,
                })
            })
            .unwrap();
            let chunk = (0..4000)
                .find_map(|_| {
                    while let Some(message) = assert_no_heap(|| h.receive()) {
                        if let EngineMessage::Bulk(chunk) = message {
                            if chunk.id == id {
                                return Some(chunk);
                            }
                        }
                    }
                    tick(&mut h, 16);
                    None
                })
                .expect("role read completed");
            assert_eq!(chunk.offset, offset);
            let BulkPayload::Audio { len, samples } = chunk.payload else {
                panic!("{chunk:?}");
            };
            assert!(len > 0);
            assert_eq!(len as usize % 16, 0);
            for frame in samples[..len as usize].chunks_exact(16) {
                for channel in 0..8 {
                    assert_eq!(frame[channel], channel as f32 + 0.25);
                    assert_eq!(frame[8 + channel], channel as f32 + 100.5);
                }
            }
            offset += u64::from(len) / 16;
            if offset >= chunk.total {
                break;
            }
            if !cleared {
                done(&mut h, Request::LoopClear { loop_id: looped() });
                cleared = true;
            }
        }
        assert!(cleared && offset >= 128);
    }
}

#[shoop_test]
fn global_external_controls_are_additive_and_never_enter_the_recorded_dry_lane() {
    let mut d = description();
    d.host.midi_inputs.push(Name::new("global_in").unwrap());
    let mut h = OfflineHost::new(&d).unwrap();
    h.connect().unwrap();
    while h.receive().is_some() {}
    done(
        &mut h,
        Request::CreateProcessedStrip {
            strip: strip(),
            kind: ProcessorKind::External,
            shape: StripShape {
                audio_inputs: 1,
                audio_outputs: 1,
                midi_input: true,
                midi_output: false,
            },
        },
    );
    connect(&mut h, PortKind::MidiIn, 0, 32);
    connect(&mut h, PortKind::MidiSend, 0, 34);
    done(
        &mut h,
        Request::Connect {
            port: EnginePort::GlobalFxControl,
            host: HostPortId(32),
            connected: true,
        },
    );
    done(
        &mut h,
        Request::Connect {
            port: EnginePort::GlobalFxControl,
            host: HostPortId(33),
            connected: true,
        },
    );
    done(
        &mut h,
        Request::CreateLoop {
            strip: strip(),
            loop_id: looped(),
        },
    );
    mode(&mut h, LoopMode::Recording);
    h.midi_input_mut(0).push(9, 128, &[0xb0, 7, 64]).unwrap();
    h.midi_input_mut(1).push(11, 128, &[0xb0, 9, 65]).unwrap();
    tick(&mut h, 128);
    let packets: Vec<_> = (0..h.midi_output(0).len())
        .map(|i| h.midi_output(0).get(i).unwrap())
        .collect();
    assert_eq!(
        packets,
        [
            (25, &[0xb0, 7, 64][..]),
            (25, &[0xb0, 7, 64][..]),
            (27, &[0xb0, 9, 65][..])
        ]
    );
    mode(&mut h, LoopMode::Stopped);
    mode(&mut h, LoopMode::PlayingDryThroughWet);
    let mut recorded = Vec::new();
    for _ in 0..4 {
        tick(&mut h, 128);
        for index in 0..h.midi_output(0).len() {
            recorded.push(h.midi_output(0).get(index).unwrap().1.to_vec());
        }
    }
    assert!(recorded.iter().any(|data| data == &[0xb0, 7, 64]));
    assert!(recorded.iter().all(|data| data != &[0xb0, 9, 65]));
    let cc7 = recorded
        .iter()
        .filter(|data| data == &&[0xb0, 7, 64])
        .count();
    assert!(
        cc7 <= 4,
        "the global copy must not duplicate recorded events"
    );
}
#[shoop_test]
fn removing_dry_playback_fades_its_send_and_keeps_the_other_loop_and_meter() {
    let mut d = description();
    d.quantum = 1;
    d.ramp_frames = 4;
    d.telemetry_interval_frames = 0;
    let mut h = OfflineHost::new(&d).unwrap();
    create(&mut h, 1, 1, false);
    h.input_mut(0).fill(1.);
    mode(&mut h, LoopMode::Recording);
    tick(&mut h, 64);
    mode(&mut h, LoopMode::Stopped);
    let other = LoopSlotId(SlotId::new(1, 1));
    done(
        &mut h,
        Request::CreateLoop {
            strip: strip(),
            loop_id: other,
        },
    );
    h.input_mut(0).fill(2.);
    let transition = |loop_id, mode| Request::LoopTransition {
        loop_id,
        mode,
        delay: None,
    };
    done(&mut h, transition(other, LoopMode::Recording));
    tick(&mut h, 64);
    done(&mut h, transition(other, LoopMode::PlayingDryThroughWet));
    mode(&mut h, LoopMode::PlayingDryThroughWet);
    tick(&mut h, 128);
    assert_eq!(&h.output(8)[16..128], &[3.; 112]);
    let mut metered = false;
    while let Some(message) = h.receive() {
        if let EngineMessage::Telemetry(Telemetry::Loop { loop_id, peaks, .. }) = message {
            if loop_id == looped() && peaks.as_slice().first() == Some(&1.) {
                metered = true;
            }
        }
    }
    assert!(metered, "dry playback must meter the dry role");
    let id = h
        .request(Request::RemoveLoop { loop_id: looped() })
        .unwrap();
    let mut samples = Vec::new();
    let mut completed = false;
    for _ in 0..64 {
        tick(&mut h, 128);
        samples.extend_from_slice(h.output(8));
        while let Some(message) = h.receive() {
            if let EngineMessage::Result {
                correlation,
                outcome,
            } = message
            {
                if correlation == id {
                    assert_eq!(outcome, Outcome::Done);
                    completed = true;
                }
            }
        }
        if completed {
            break;
        }
    }
    assert!(completed);
    assert!(samples.windows(4).any(|w| w == [2.75, 2.5, 2.25, 2.]));
    tick(&mut h, 128);
    assert_eq!(&h.output(8)[..128], &[2.; 128]);
}
#[shoop_test]
fn unequal_external_roles_capture_play_dry_and_rerecord_without_a_dsp_bank() {
    for (dry, wet) in [(1, 8), (8, 1), (8, 8), (2, 5), (5, 2)] {
        let mut h = OfflineHost::new(&description()).unwrap();
        create(&mut h, dry, wet, true);
        assert!(h.endpoint().processor_catalog().kinds.is_empty());
        for c in 0..dry as usize {
            h.input_mut(c).fill(c as f32 + 0.25);
        }
        for c in 0..wet as usize {
            h.input_mut(8 + c).fill(-(c as f32) - 0.5);
        }
        mode(&mut h, LoopMode::Recording);
        tick(&mut h, 64);
        for c in 0..dry as usize {
            assert_eq!(&h.output(8 + c)[16..64], &[c as f32 + 0.25; 48]);
        }
        for c in 0..wet as usize {
            assert_eq!(&h.output(c)[16..64], &[0.; 48]);
        }
        mode(&mut h, LoopMode::Playing);
        for c in 0..wet as usize {
            h.input_mut(8 + c).fill(3. + c as f32);
        }
        tick(&mut h, 128);
        for c in 0..wet as usize {
            assert_eq!(
                &h.output(c)[16..128],
                &[-(c as f32) - 0.5; 112],
                "wet dry={dry} wet={wet} c={c}"
            );
        }
        for c in 0..dry as usize {
            assert_eq!(&h.output(8 + c)[16..128], &[0.; 112]);
        }
        mode(&mut h, LoopMode::PlayingDryThroughWet);
        tick(&mut h, 128);
        for c in 0..dry as usize {
            assert_eq!(&h.output(8 + c)[16..128], &[c as f32 + 0.25; 112]);
        }
        for c in 0..wet as usize {
            assert_eq!(&h.output(c)[16..128], &[3. + c as f32; 112]);
        }
        for c in 0..dry as usize {
            h.input_mut(c).fill(99.);
        }
        mode(&mut h, LoopMode::RecordingDryIntoWet);
        tick(&mut h, 128);
        for c in 0..dry as usize {
            assert_eq!(&h.output(8 + c)[16..128], &[c as f32 + 0.25; 112]);
        }
        mode(&mut h, LoopMode::Playing);
        for c in 0..wet as usize {
            h.input_mut(8 + c).fill(5.);
        }
        tick(&mut h, 128);
        for c in 0..wet as usize {
            assert_eq!(&h.output(c)[16..128], &[3. + c as f32; 112]);
        }
        mode(&mut h, LoopMode::PlayingDryThroughWet);
        tick(&mut h, 128);
        for c in 0..dry as usize {
            assert_eq!(&h.output(8 + c)[16..128], &[c as f32 + 0.25; 112]);
        }
    }
}

#[shoop_test]
fn composite_dry_playback_and_wet_rerecord_preserve_unequal_roles_without_polling() {
    for (dry, wet) in [(1, 8), (8, 1), (2, 5)] {
        let mut h = OfflineHost::new(&description()).unwrap();
        create(&mut h, dry, wet, true);
        for c in 0..dry as usize {
            h.input_mut(c).fill(c as f32 + 0.25);
        }
        for c in 0..wet as usize {
            h.input_mut(8 + c).fill(-(c as f32) - 0.5);
        }
        mode(&mut h, LoopMode::Recording);
        tick(&mut h, 64);
        mode(&mut h, LoopMode::Stopped);
        let parent = LoopSlotId(SlotId::new(1, 1));
        done(
            &mut h,
            Request::CreateComposite {
                loop_id: parent,
                strip: Some(strip()),
            },
        );
        for (index, event_mode) in [
            LoopMode::PlayingDryThroughWet,
            LoopMode::RecordingDryIntoWet,
            LoopMode::Playing,
            LoopMode::PlayingDryThroughWet,
        ]
        .into_iter()
        .enumerate()
        {
            let id = index as u64 + 1;
            for c in 0..dry as usize {
                h.input_mut(c).fill(99.);
            }
            for c in 0..wet as usize {
                h.input_mut(8 + c).fill(id as f32 + c as f32);
            }
            done(
                &mut h,
                Request::Composite(CompositeCommand::Begin(CompositeDescription {
                    id,
                    loop_id: parent,
                    sync: None,
                    previous: None,
                    expected_version: id - 1,
                    version: id,
                    kind: CompositeKind::Explicit,
                    unit_frames: 16,
                    iterations: 64,
                    entries: 1,
                    segments: 1,
                    sample_rate: 48_000,
                    quantum: 16,
                })),
            );
            done(
                &mut h,
                Request::Composite(CompositeCommand::Span {
                    id,
                    index: 0,
                    span: CompositeSpan {
                        target: looped(),
                        start: 0,
                        end: 64,
                        occurrence_start: 0,
                        ordinal: 0,
                        mode: event_mode,
                    },
                }),
            );
            assert!(matches!(
                request(&mut h, Request::Composite(CompositeCommand::Commit { id })),
                Outcome::CompositeApplied { version, .. } if version == id
            ));
            done(
                &mut h,
                Request::LoopTransition {
                    loop_id: parent,
                    mode: LoopMode::Playing,
                    delay: None,
                },
            );
            tick(&mut h, 128);
            for c in 0..dry as usize {
                let expected = if event_mode == LoopMode::Playing {
                    0.
                } else {
                    c as f32 + 0.25
                };
                assert_eq!(&h.output(8 + c)[32..128], &[expected; 96]);
            }
            for c in 0..wet as usize {
                let expected = if event_mode == LoopMode::Playing {
                    2. + c as f32
                } else {
                    id as f32 + c as f32
                };
                assert_eq!(&h.output(c)[32..128], &[expected; 96]);
            }
            done(
                &mut h,
                Request::LoopTransition {
                    loop_id: parent,
                    mode: LoopMode::Stopped,
                    delay: None,
                },
            );
        }
    }
}
#[shoop_test]
fn external_live_midi_releases_notes_and_sustain_when_the_live_route_stops() {
    let mut h = OfflineHost::new(&description()).unwrap();
    create(&mut h, 1, 1, true);
    done(
        &mut h,
        Request::StripControl {
            strip: strip(),
            control: StripControl::Monitoring(true),
        },
    );
    h.midi_input_mut(0).push(3, 128, &[0x90, 60, 100]).unwrap();
    h.midi_input_mut(0).push(4, 128, &[0xb0, 64, 127]).unwrap();
    tick(&mut h, 128);
    assert_eq!(h.midi_output(0).get(0), Some((19, &[0x90, 60, 100][..])));
    done(
        &mut h,
        Request::StripControl {
            strip: strip(),
            control: StripControl::Monitoring(false),
        },
    );
    tick(&mut h, 64);
    let packets: Vec<_> = (0..h.midi_output(0).len())
        .map(|i| h.midi_output(0).get(i).unwrap().1.to_vec())
        .collect();
    assert!(packets.contains(&vec![0x80, 60, 0]));
    assert!(packets.contains(&vec![0xb0, 64, 0]));
}
#[shoop_test]
fn a_minimal_host_refuses_external_admission_before_mutating_its_pool() {
    let mut d = description();
    d.external_dry_wet = false;
    let mut h = OfflineHost::new(&d).unwrap();
    h.connect().unwrap();
    while h.receive().is_some() {}
    assert_eq!(
        request(
            &mut h,
            Request::CreateProcessedStrip {
                strip: strip(),
                shape: StripShape {
                    audio_inputs: 1,
                    audio_outputs: 1,
                    midi_input: false,
                    midi_output: false
                },
                kind: ProcessorKind::External
            }
        ),
        Outcome::Refused(Refusal::Unsupported)
    );
    done(
        &mut h,
        Request::CreateStrip {
            strip: strip(),
            shape: StripShape {
                audio_inputs: 1,
                audio_outputs: 1,
                midi_input: false,
                midi_output: false,
            },
        },
    );
    assert_eq!(
        request(
            &mut h,
            Request::Connect {
                port: EnginePort::Strip(StripPort {
                    strip: strip(),
                    kind: PortKind::AudioSend,
                    index: 0
                }),
                host: HostPortId(24),
                connected: true
            }
        ),
        Outcome::Refused(Refusal::InvalidValue)
    );
}

#[shoop_test]
fn send_and_return_connection_fades_are_independent() {
    let mut d = description();
    d.quantum = 1;
    d.ramp_frames = 4;
    let mut h = OfflineHost::new(&d).unwrap();
    create(&mut h, 1, 1, false);
    h.input_mut(0).fill(0.25);
    h.input_mut(8).fill(1.);
    h.input_mut(9).fill(3.);
    done(
        &mut h,
        Request::StripControl {
            strip: strip(),
            control: StripControl::Monitoring(true),
        },
    );
    tick(&mut h, 16);
    let port = EnginePort::Strip(StripPort {
        strip: strip(),
        kind: PortKind::AudioReturn,
        index: 0,
    });
    let mut ids = [
        Some(
            h.request(Request::Connect {
                port,
                host: HostPortId(8),
                connected: false,
            })
            .unwrap(),
        ),
        Some(
            h.request(Request::Connect {
                port,
                host: HostPortId(9),
                connected: true,
            })
            .unwrap(),
        ),
    ];
    let mut transient = Vec::new();
    let mut finished = None;
    for frame in 0..4000 {
        tick(&mut h, 1);
        assert_eq!(h.output(8)[0], 0.25);
        let wet = h.output(0)[0];
        if wet != 1. && wet != 3. {
            transient.push(wet);
        }
        while let Some(message) = h.receive() {
            if let EngineMessage::Result {
                correlation,
                outcome,
            } = message
            {
                for id in &mut ids {
                    if *id == Some(correlation) {
                        assert_eq!(outcome, Outcome::Done);
                        *id = None;
                    }
                }
            }
        }
        if ids.iter().all(Option::is_none) {
            finished.get_or_insert(frame);
        }
        if finished.is_some_and(|done| frame > done + 8) {
            break;
        }
    }
    assert!(finished.is_some());
    assert_eq!(transient, [1.5, 2., 2.5]);
    assert_eq!(h.output(0)[0], 3.);
}

#[shoop_test]
fn overflowing_live_note_cleanup_recovers_once_and_does_not_repeat_forever() {
    let mut d = description();
    d.midi_events = 32;
    d.midi_bytes = 256;
    let mut h = OfflineHost::new(&d).unwrap();
    create(&mut h, 1, 1, true);
    done(
        &mut h,
        Request::StripControl {
            strip: strip(),
            control: StripControl::Monitoring(true),
        },
    );
    for batch in 0..3 {
        for key in 0..32 {
            h.midi_input_mut(0)
                .push(0, 16, &[0x90, batch * 32 + key, 100])
                .unwrap();
        }
        tick(&mut h, 16);
    }
    done(
        &mut h,
        Request::StripControl {
            strip: strip(),
            control: StripControl::Monitoring(false),
        },
    );
    tick(&mut h, 64);
    tick(&mut h, 64);
    assert!(h.midi_output(0).is_empty());
    done(
        &mut h,
        Request::StripControl {
            strip: strip(),
            control: StripControl::Monitoring(true),
        },
    );
    h.midi_input_mut(0).push(3, 128, &[0x90, 61, 100]).unwrap();
    tick(&mut h, 128);
    assert_eq!(h.midi_output(0).len(), 1);
    assert_eq!(h.midi_output(0).get(0), Some((19, &[0x90, 61, 100][..])));
}
