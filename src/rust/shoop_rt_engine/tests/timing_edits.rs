mod support;
use shoop_rt_protocol::*;
use shoop_wasm_test_support::shoop_test;
use support::*;

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

fn done(h: &mut Harness, request: Request, quantum: usize) {
    assert_eq!(
        h.apply_in(request, quantum),
        Outcome::Done,
        "Q={quantum} {request:?}"
    );
}
fn edit(
    h: &mut Harness,
    target: LoopSlotId,
    previous: TimingState,
    change: TimingEdit,
    quantum: usize,
) -> Outcome {
    h.apply_in(
        Request::LoopTiming {
            loop_id: target,
            expected_revision: previous.revision,
            expected_content: previous.content_revision,
            edit: change,
        },
        quantum,
    )
}
fn applied(outcome: Outcome) -> TimingState {
    let Outcome::TimingApplied(state) = outcome else {
        panic!("timing refused: {outcome:?}")
    };
    state
}

#[shoop_test]
fn window_edits_keep_raw_recordings_atomic_results_and_recoverable_state_at_every_quantum() {
    for quantum in [1, 16, 32, 64, 128] {
        let mut settings = description();
        settings.quantum = quantum;
        settings.max_frames = quantum.max(16);
        let mut h = Harness::new(&settings);
        h.input = Box::new(|channel, _| (channel + 1) as f32);
        let strip = strip(0, 1);
        let target = loop_id(0, 1);
        done(
            &mut h,
            Request::CreateStrip {
                strip,
                shape: shape(2, 2, false),
            },
            quantum,
        );
        for channel in 0..2 {
            done(
                &mut h,
                Request::Connect {
                    port: port(strip, PortKind::AudioIn, channel),
                    host: audio_in(channel as u32),
                    connected: true,
                },
                quantum,
            );
            done(&mut h, route(strip, channel, channel, true), quantum);
        }
        done(
            &mut h,
            Request::CreateLoop {
                loop_id: target,
                strip,
            },
            quantum,
        );
        done(
            &mut h,
            Request::LoopTransition {
                loop_id: target,
                mode: LoopMode::Recording,
                delay: None,
            },
            quantum,
        );
        h.run(8 * quantum, quantum);
        done(
            &mut h,
            Request::LoopTransition {
                loop_id: target,
                mode: LoopMode::Stopped,
                delay: None,
            },
            quantum,
        );
        let (
            _,
            Telemetry::Loop {
                content_revision, ..
            },
        ) = h.last_loop(target).unwrap()
        else {
            panic!("missing loop")
        };
        let state = TimingState {
            content_revision,
            ..TimingState::default()
        };
        let state = applied(edit(
            &mut h,
            target,
            state,
            TimingEdit {
                length: Some((2 * quantum) as u64),
                ..TimingEdit::default()
            },
            quantum,
        ));
        assert!(state.raw_audio_frames >= (8 * quantum) as u64);
        assert_eq!(state.length, (2 * quantum) as u64);
        let mut channels = [ChannelTimingUpdate::default(); MAX_TIMING_UPDATES];
        channels[0] = ChannelTimingUpdate {
            channel: 1,
            start_offset: Some(state.raw_audio_frames as i32),
            ..Default::default()
        };
        let state = applied(edit(
            &mut h,
            target,
            state,
            TimingEdit {
                count: 1,
                channels,
                ..Default::default()
            },
            quantum,
        ));
        done(
            &mut h,
            Request::LoopTransition {
                loop_id: target,
                mode: LoopMode::Playing,
                delay: None,
            },
            quantum,
        );
        h.run(8 * quantum, quantum);
        assert!(h.outputs[0]
            .iter()
            .rev()
            .take(quantum)
            .all(|sample| *sample == 1.));
        assert!(h.outputs[1]
            .iter()
            .rev()
            .take(quantum)
            .all(|sample| *sample == 0.));
        let (total, samples, _) = h.read(target, BulkSelector::Audio { channel: 1 });
        assert_eq!(total, state.raw_audio_frames);
        assert_eq!(samples, vec![2.; total as usize]);
        let foreign = ChannelTimingUpdate {
            channel: 7,
            start_offset: Some(0),
            ..Default::default()
        };
        let mut invalid = TimingEdit {
            count: 1,
            ..Default::default()
        };
        invalid.channels[0] = foreign;
        assert_eq!(
            edit(&mut h, target, state, invalid, quantum),
            Outcome::Refused(Refusal::InvalidId)
        );
        channels[0].start_offset = Some(0);
        channels[1] = channels[0];
        assert!(matches!(
            edit(
                &mut h,
                target,
                state,
                TimingEdit {
                    count: 2,
                    channels,
                    ..Default::default()
                },
                quantum
            ),
            Outcome::Refused(_)
        ));
        assert!(matches!(
            edit(
                &mut h,
                target,
                TimingState {
                    revision: state.revision - 1,
                    ..state
                },
                TimingEdit {
                    start_offset: Some(0),
                    ..Default::default()
                },
                quantum
            ),
            Outcome::Refused(_)
        ));
        if quantum > 1 {
            assert_eq!(
                edit(
                    &mut h,
                    target,
                    state,
                    TimingEdit {
                        length: Some(quantum as u64 + 1),
                        ..Default::default()
                    },
                    quantum
                ),
                Outcome::Refused(Refusal::InvalidValue)
            );
        }
        assert_no_heap(|| h.host.client.resync()).unwrap();
        h.pump();
        assert!(h.states.contains(&StateRecord::LoopTiming {
            loop_id: target,
            timing: state
        }));
        let empty = applied(edit(
            &mut h,
            target,
            state,
            TimingEdit {
                length: Some(0),
                ..Default::default()
            },
            quantum,
        ));
        let (total, samples, _) = h.read(target, BulkSelector::Audio { channel: 0 });
        assert_eq!(total, empty.raw_audio_frames);
        assert_eq!(samples, vec![1.; total as usize]);
        done(
            &mut h,
            Request::LoopTransition {
                loop_id: target,
                mode: LoopMode::Recording,
                delay: None,
            },
            quantum,
        );
        assert!(matches!(
            edit(
                &mut h,
                target,
                empty,
                TimingEdit {
                    start_offset: Some(2),
                    length: Some(quantum as u64),
                    ..Default::default()
                },
                quantum
            ),
            Outcome::Refused(_)
        ));
        let moved = applied(edit(
            &mut h,
            target,
            empty,
            TimingEdit {
                start_offset: Some(2),
                preplay: Some(quantum as u32),
                ..Default::default()
            },
            quantum,
        ));
        assert_eq!(moved.windows.audio[0].start_offset, 2);
        let (_, Telemetry::Loop { mode, .. }) = h.last_loop(target).unwrap() else {
            panic!("missing capture")
        };
        assert_eq!(mode, LoopMode::Recording);
    }
}

#[shoop_test]
fn retained_raw_lead_in_starts_inside_quantum_and_carries_midi_pickups() {
    for quantum in [1usize, 16, 32, 64, 128] {
        let mut settings = description();
        settings.quantum = quantum;
        settings.max_frames = quantum.max(16);
        settings.resource_pages = 2048;
        let mut h = Harness::new(&settings);
        let track = strip(0, 1);
        let target = loop_id(0, 1);
        let source = loop_id(1, 1);
        done(
            &mut h,
            Request::CreateStrip {
                strip: track,
                shape: shape(0, 2, true),
            },
            quantum,
        );
        for loop_id in [target, source] {
            done(
                &mut h,
                Request::CreateLoop {
                    loop_id,
                    strip: track,
                },
                quantum,
            );
        }
        for channel in 0..2 {
            done(&mut h, route(track, channel, channel, true), quantum);
        }
        done(
            &mut h,
            Request::Connect {
                port: port(track, PortKind::MidiOut, 0),
                host: midi_out(),
                connected: true,
            },
            quantum,
        );
        let raw = (8 * quantum + 1) as u64;
        let origin = (4 * quantum + 1) as i32;
        let bytes: Vec<u8> = (0..raw * 2).flat_map(|_| 1f32.to_le_bytes()).collect();
        done(
            &mut h,
            Request::ContentBegin(ContentDescription {
                id: 1,
                midi: false,
                frames: raw,
                bytes: bytes.len() as u64,
            }),
            quantum,
        );
        let Some(EngineMessage::Welcome { limits, .. }) = h.lifecycle.first() else {
            panic!()
        };
        let maximum = usize::from(limits.content_chunk_bytes);
        for (chunk, bytes) in bytes.chunks(maximum).enumerate() {
            done(
                &mut h,
                Request::ContentWrite {
                    id: 1,
                    offset: (chunk * maximum) as u64,
                    data: ContentChunk::new(bytes).unwrap(),
                },
                quantum,
            );
        }
        done(&mut h, Request::ContentFinish { id: 1 }, quantum);
        let mut midi = Vec::new();
        for (frame, packet) in [
            (origin as u64 - 1, [0x90, 60, 100]),
            (origin as u64 + quantum as u64, [0x80, 60, 0]),
        ] {
            midi.extend_from_slice(&frame.to_le_bytes());
            midi.extend_from_slice(&3u32.to_le_bytes());
            midi.extend_from_slice(&packet);
        }
        done(
            &mut h,
            Request::ContentBegin(ContentDescription {
                id: 2,
                midi: true,
                frames: raw,
                bytes: midi.len() as u64,
            }),
            quantum,
        );
        done(
            &mut h,
            Request::ContentWrite {
                id: 2,
                offset: 0,
                data: ContentChunk::new(&midi).unwrap(),
            },
            quantum,
        );
        done(&mut h, Request::ContentFinish { id: 2 }, quantum);
        let mut windows = PlaybackWindows::default();
        windows.audio[0].start_offset = origin;
        windows.audio[0].preplay = (2 * quantum + 1) as u32;
        windows.audio[1].start_offset = origin;
        windows.audio[1].preplay = (quantum / 2) as u32;
        windows.midi.start_offset = origin;
        windows.midi.preplay = (2 * quantum + 1) as u32;
        for id in [1, 2] {
            done(
                &mut h,
                Request::ContentWindow {
                    id,
                    frames: (4 * quantum) as u64,
                    windows,
                },
                quantum,
            );
        }
        assert!(matches!(
            h.apply_in(
                Request::ContentApply {
                    loop_id: target,
                    frames: (4 * quantum) as u64,
                    audio: Some(1),
                    midi: Some(2)
                },
                quantum
            ),
            Outcome::ContentApplied { .. }
        ));
        let _ = applied(edit(
            &mut h,
            source,
            TimingState::default(),
            TimingEdit {
                length: Some((16 * quantum) as u64),
                ..Default::default()
            },
            quantum,
        ));
        done(
            &mut h,
            Request::LoopSync {
                loop_id: target,
                source: Some(source),
            },
            quantum,
        );
        done(
            &mut h,
            Request::LoopTransition {
                loop_id: source,
                mode: LoopMode::Playing,
                delay: None,
            },
            quantum,
        );
        let start = h.outputs[0].len();
        let scheduled = h.send(Request::LoopTransition {
            loop_id: target,
            mode: LoopMode::Playing,
            delay: Some(0),
        });
        let mut saw_stopped_audio = false;
        for _ in 0..32 {
            h.run(quantum, quantum);
            let (_, Telemetry::Loop { mode, .. }) = h.last_loop(target).unwrap() else {
                panic!()
            };
            let audible = h.outputs[0].iter().rev().take(quantum).any(|v| *v == 1.);
            if mode == LoopMode::Stopped && audible {
                saw_stopped_audio = true;
            }
            if mode == LoopMode::Playing {
                break;
            }
        }
        assert_eq!(h.results[&scheduled], Outcome::Done);
        assert!(saw_stopped_audio, "no stopped lead-in Q={quantum}");
        let a = &h.outputs[0][start..];
        let b = &h.outputs[1][start..];
        let first = a.iter().position(|v| *v == 1.).unwrap();
        let second = b.iter().position(|v| *v == 1.).unwrap();
        assert!(first < second);
        assert!(
            a[first..].iter().all(|v| *v == 1.),
            "lead/start gap Q={quantum}"
        );
        if quantum > 1 {
            assert_eq!(first % quantum, quantum - 1);
        }
        assert_eq!(
            h.midi
                .iter()
                .filter(|(_, packet)| packet == &[0x90, 60, 100])
                .count(),
            1,
            "pickup attacked twice Q={quantum}"
        );
        done(
            &mut h,
            Request::LoopTransition {
                loop_id: target,
                mode: LoopMode::Stopped,
                delay: None,
            },
            quantum,
        );
        h.run(2 * quantum, quantum);
        assert!(
            h.midi.iter().any(|(_, packet)| packet == &[0x80, 60, 0]),
            "missing pickup cleanup Q={quantum}: {:?}",
            h.midi
        );
    }
}
