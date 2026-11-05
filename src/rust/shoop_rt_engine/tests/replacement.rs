mod support;
use shoop_rt_protocol::*;
use shoop_wasm_test_support::shoop_test;
use support::*;

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

fn edit(h: &mut Harness, id: u64, edit: ReplacementEdit) {
    h.done(Request::ReplacementEdit { id, edit });
}
fn preset(gain: f32) -> StripPreset {
    StripPreset {
        automatic_monitoring: false,
        input_gain: 1.,
        input_balance: 0.,
        monitoring: false,
        output_gain: gain,
        output_balance: 0.,
        output_mute: false,
    }
}
fn upload(h: &mut Harness, id: u64, midi: bool, frames: u64, bytes: &[u8]) {
    h.done(Request::ContentBegin(ContentDescription {
        id,
        midi,
        frames,
        bytes: bytes.len() as u64,
    }));
    for (index, bytes) in bytes.chunks(256).enumerate() {
        h.done(Request::ContentWrite {
            id,
            offset: (index * 256) as u64,
            data: ContentChunk::new(bytes).unwrap(),
        });
    }
    h.done(Request::ContentFinish { id });
}
fn old_playback(h: &mut Harness) -> LoopSlotId {
    h.input = Box::new(|_, _| 1.);
    let strip = strip(0, 1);
    let loop_id = loop_id(0, 1);
    h.done(Request::CreateStrip {
        strip,
        shape: shape(1, 1, true),
    });
    h.done(Request::CreateLoop { loop_id, strip });
    h.done(Request::Connect {
        port: port(strip, PortKind::AudioIn, 0),
        host: audio_in(0),
        connected: true,
    });
    h.done(route(strip, 0, 0, true));
    h.done(Request::LoopTransition {
        loop_id,
        mode: LoopMode::Recording,
        delay: None,
    });
    h.run(7, 1);
    h.done(Request::LoopTransition {
        loop_id,
        mode: LoopMode::Playing,
        delay: None,
    });
    h.run(8, 4);
    assert_eq!(h.outputs[0].last(), Some(&1.));
    loop_id
}

#[shoop_test]
fn complete_content_and_controls_publish_together_while_old_playback_survives_preparation() {
    let mut h = Harness::new(&description());
    let old = old_playback(&mut h);
    let audio = [0.25f32, 0.5]
        .repeat(4)
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect::<Vec<_>>();
    let mut midi = Vec::new();
    for (frame, message) in [(0u64, [0x90, 60, 100]), (2, [0x80, 60, 0])] {
        midi.extend_from_slice(&frame.to_le_bytes());
        midi.extend_from_slice(&3u32.to_le_bytes());
        midi.extend_from_slice(&message);
    }
    upload(&mut h, 1, false, 4, &audio);
    upload(&mut h, 2, true, 4, &midi);
    let from = h.outputs[0].len();
    h.done(Request::ReplacementBegin { id: 1 });
    let bus = BusId(SlotId::new(0, 2));
    edit(&mut h, 1, ReplacementEdit::Bus { bus, channels: 2 });
    let strip = strip(0, 2);
    let loaded = loop_id(0, 2);
    edit(
        &mut h,
        1,
        ReplacementEdit::Strip {
            strip,
            shape: shape(2, 2, true),
            controls: preset(0.5),
        },
    );
    edit(
        &mut h,
        1,
        ReplacementEdit::Loop {
            loop_id: loaded,
            strip,
            sync: None,
            frames: 4,
            audio: Some(1),
            midi: Some(2),
            gain: 0.75,
            balance: 1.,
        },
    );
    for channel in 0..2 {
        edit(
            &mut h,
            1,
            ReplacementEdit::Route {
                port: StripPort {
                    strip,
                    kind: PortKind::AudioOut,
                    index: channel,
                },
                bus: BusPort {
                    bus,
                    index: channel,
                },
            },
        );
        edit(
            &mut h,
            1,
            ReplacementEdit::Connect {
                port: EnginePort::Bus(BusPort {
                    bus,
                    index: channel,
                }),
                host: audio_out(u32::from(channel)),
            },
        );
    }
    edit(
        &mut h,
        1,
        ReplacementEdit::Connect {
            port: port(strip, PortKind::MidiOut, 0),
            host: midi_out(),
        },
    );
    edit(
        &mut h,
        1,
        ReplacementEdit::BusControl {
            bus,
            control: BusControl::Gain(0.5),
        },
    );
    h.done(Request::ReplacementPrepare { id: 1 });
    h.run(12, 4);
    assert!(h.outputs[0][from..].iter().all(|&sample| sample == 1.));
    assert!(matches!(
        h.last_loop(old),
        Some((
            _,
            Telemetry::Loop {
                mode: LoopMode::Playing,
                ..
            }
        ))
    ));
    assert_eq!(
        h.apply(Request::ReplacementCommit { id: 99 }),
        Outcome::Refused(Refusal::InvalidId)
    );
    h.done(Request::ReplacementCommit { id: 1 });
    assert!(matches!(
        h.last_loop(loaded),
        Some((
            _,
            Telemetry::Loop {
                mode: LoopMode::Stopped,
                length: 4,
                ..
            }
        ))
    ));
    assert_eq!(h.outputs[0].last(), Some(&0.));
    h.done(Request::LoopTransition {
        loop_id: loaded,
        mode: LoopMode::Playing,
        delay: None,
    });
    h.run(8, 4);
    assert_eq!(&h.outputs[0][h.outputs[0].len() - 8..], &[0.; 8]);
    assert_eq!(&h.outputs[1][h.outputs[1].len() - 8..], &[0.09375; 8]);
    assert_eq!(
        h.read(loaded, BulkSelector::AudioFrames).1,
        [0.25, 0.5].repeat(4)
    );
    assert_eq!(h.read(loaded, BulkSelector::Midi).2, midi);
    assert_eq!(
        h.apply(Request::ReplacementCommit { id: 1 }),
        Outcome::Refused(Refusal::InvalidId)
    );
}

#[shoop_test]
fn cancelling_prepared_replacement_discards_future_controls_and_keeps_old_music() {
    let mut h = Harness::new(&description());
    let old = old_playback(&mut h);
    h.done(Request::ReplacementBegin { id: 1 });
    let next = strip(0, 2);
    edit(
        &mut h,
        1,
        ReplacementEdit::Strip {
            strip: next,
            shape: shape(1, 1, false),
            controls: preset(0.),
        },
    );
    edit(
        &mut h,
        1,
        ReplacementEdit::Loop {
            loop_id: loop_id(0, 2),
            strip: next,
            sync: None,
            frames: 0,
            audio: None,
            midi: None,
            gain: 1.,
            balance: 0.,
        },
    );
    let from = h.outputs[0].len();
    h.done(Request::ReplacementPrepare { id: 1 });
    h.done(Request::ReplacementCancel { id: 1 });
    assert_eq!(
        h.apply(Request::ReplacementCommit { id: 1 }),
        Outcome::Refused(Refusal::InvalidId)
    );
    h.done(Request::CreateStrip {
        strip: strip(1, 1),
        shape: shape(0, 0, false),
    });
    h.run(16, 4);
    assert!(h.outputs[0][from..].iter().all(|&sample| sample == 1.));
    assert!(matches!(
        h.last_loop(old),
        Some((
            _,
            Telemetry::Loop {
                mode: LoopMode::Playing,
                ..
            }
        ))
    ));
}

#[shoop_test]
fn cancelling_inflight_preparation_answers_both_requests_and_recycles_every_stage() {
    let mut h = Harness::new(&description());
    old_playback(&mut h);
    let from = h.outputs[0].len();
    for id in 1..=16 {
        h.done(Request::ReplacementBegin { id });
        let next = strip(0, 2);
        edit(
            &mut h,
            id,
            ReplacementEdit::Strip {
                strip: next,
                shape: shape(1, 1, false),
                controls: preset(0.),
            },
        );
        edit(
            &mut h,
            id,
            ReplacementEdit::Loop {
                loop_id: loop_id(0, 2),
                strip: next,
                sync: None,
                frames: 0,
                audio: None,
                midi: None,
                gain: 1.,
                balance: 0.,
            },
        );
        let preparation = h.send(Request::ReplacementPrepare { id });
        let cancellation = h.send(Request::ReplacementCancel { id });
        h.settle();
        assert_eq!(h.results[&preparation], Outcome::Refused(Refusal::Busy));
        assert_eq!(h.results[&cancellation], Outcome::Done);
    }
    assert!(h.outputs[0][from..].iter().all(|&sample| sample == 1.));
}

#[shoop_test]
fn failed_commit_preserves_recording_and_all_captured_frames() {
    let mut h = Harness::new(&description());
    let old = old_playback(&mut h);
    let start = h.frame();
    h.done(Request::LoopTransition {
        loop_id: old,
        mode: LoopMode::Recording,
        delay: None,
    });
    h.done(Request::ReplacementBegin { id: 1 });
    let next = strip(0, 2);
    edit(
        &mut h,
        1,
        ReplacementEdit::Strip {
            strip: next,
            shape: shape(1, 1, false),
            controls: preset(0.),
        },
    );
    edit(
        &mut h,
        1,
        ReplacementEdit::Loop {
            loop_id: loop_id(0, 2),
            strip: next,
            sync: None,
            frames: 0,
            audio: None,
            midi: None,
            gain: 1.,
            balance: 0.,
        },
    );
    h.done(Request::ReplacementPrepare { id: 1 });
    assert_eq!(
        h.apply(Request::ReplacementCommit { id: 1 }),
        Outcome::Refused(Refusal::Busy)
    );
    assert!(matches!(
        h.last_loop(old),
        Some((
            _,
            Telemetry::Loop {
                mode: LoopMode::Recording,
                ..
            }
        ))
    ));
    let expected = h.frame() - start;
    h.done(Request::LoopTransition {
        loop_id: old,
        mode: LoopMode::Stopped,
        delay: None,
    });
    let (frames, data, _) = h.read(old, BulkSelector::AudioFrames);
    assert_eq!(frames, expected);
    assert_eq!(data, [1., 0.].repeat(expected as usize));
    h.done(Request::CreateStrip {
        strip: strip(1, 1),
        shape: shape(0, 0, false),
    });
    h.done(Request::LoopTransition {
        loop_id: old,
        mode: LoopMode::Playing,
        delay: None,
    });
    h.run(8, 4);
    assert_eq!(&h.outputs[0][h.outputs[0].len() - 8..], &[1.; 8]);
}

#[shoop_test]
fn commit_is_irrevocable_and_empty_replacement_uses_the_same_cut() {
    let mut h = Harness::new(&description());
    old_playback(&mut h);
    h.done(Request::ReplacementBegin { id: 1 });
    h.done(Request::ReplacementPrepare { id: 1 });
    let commit = h.send(Request::ReplacementCommit { id: 1 });
    let cancel = h.send(Request::ReplacementCancel { id: 1 });
    h.settle();
    assert_eq!(h.results[&commit], Outcome::Done);
    assert_eq!(h.results[&cancel], Outcome::Refused(Refusal::Busy));
    h.run(8, 4);
    assert_eq!(&h.outputs[0][h.outputs[0].len() - 8..], &[0.; 8]);
    assert_eq!(
        h.apply(Request::ReplacementBegin { id: 1 }),
        Outcome::Refused(Refusal::InvalidId)
    );
    h.done(Request::ReplacementBegin { id: 2 });
    h.done(Request::ReplacementPrepare { id: 2 });
    h.done(Request::ReplacementCommit { id: 2 });
}

#[shoop_test]
fn complete_replacement_releases_displaced_capture_storage_for_the_next_upload() {
    let mut configured = description();
    configured.resource_pages = 64;
    let mut h = Harness::new(&configured);
    let old = old_playback(&mut h);
    h.done(Request::LoopTransition {
        loop_id: old,
        mode: LoopMode::Recording,
        delay: None,
    });
    h.run(1023, 16);
    h.done(Request::LoopTransition {
        loop_id: old,
        mode: LoopMode::Playing,
        delay: None,
    });
    h.done(Request::ReplacementBegin { id: 1 });
    h.done(Request::ReplacementPrepare { id: 1 });
    h.done(Request::ReplacementCommit { id: 1 });
    upload(&mut h, 1, false, 1536, &vec![0; 1536 * 8]);
    h.done(Request::ContentRelease { id: 1 });
}

#[shoop_test]
fn disconnect_abandons_preparation_but_finishes_an_admitted_commit() {
    for committed in [false, true] {
        let mut h = Harness::new(&description());
        let old = old_playback(&mut h);
        h.done(Request::ReplacementBegin { id: 1 });
        h.done(Request::ReplacementPrepare { id: 1 });
        if committed {
            h.send(Request::ReplacementCommit { id: 1 });
        }
        assert_no_heap(|| h.host.disconnect());
        h.pump();
        h.settle();
        assert_no_heap(|| h.host.connect()).unwrap();
        h.pump();
        h.settle();
        h.run(8, 4);
        let expected = if committed { 0. } else { 1. };
        assert_eq!(&h.outputs[0][h.outputs[0].len() - 8..], &[expected; 8]);
        if !committed {
            h.done(Request::LoopTransition {
                loop_id: old,
                mode: LoopMode::Stopped,
                delay: None,
            });
        }
        h.done(Request::ReplacementBegin { id: 2 });
        h.done(Request::ReplacementPrepare { id: 2 });
        h.done(Request::ReplacementCommit { id: 2 });
    }
}

#[shoop_test]
fn content_apply_keeps_loop_identity_and_playing_mode_and_refuses_active_capture() {
    let mut h = Harness::new(&description());
    let id = old_playback(&mut h);
    for generation in 1..=12 {
        let audio_id = generation * 2 - 1;
        let midi_id = generation * 2;
        let audio = [0.25f32, 0.]
            .repeat(4)
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect::<Vec<_>>();
        upload(&mut h, audio_id, false, 4, &audio);
        upload(&mut h, midi_id, true, 4, &[]);
        h.done(Request::ContentApply {
            loop_id: id,
            frames: 4,
            audio: Some(audio_id),
            midi: Some(midi_id),
        });
        h.run(8, 4);
        assert!(matches!(
            h.last_loop(id),
            Some((
                _,
                Telemetry::Loop {
                    mode: LoopMode::Playing,
                    length: 4,
                    ..
                }
            ))
        ));
        assert_eq!(&h.outputs[0][h.outputs[0].len() - 8..], &[0.25; 8]);
        assert_eq!(
            h.read(id, BulkSelector::AudioFrames).1,
            [0.25, 0.].repeat(4)
        );
    }
    h.done(Request::LoopTransition {
        loop_id: id,
        mode: LoopMode::Recording,
        delay: None,
    });
    upload(&mut h, 25, false, 4, &vec![0; 32]);
    upload(&mut h, 26, true, 4, &[]);
    assert_eq!(
        h.apply(Request::ContentApply {
            loop_id: id,
            frames: 4,
            audio: Some(25),
            midi: Some(26)
        }),
        Outcome::Refused(Refusal::Busy)
    );
    assert!(matches!(
        h.last_loop(id),
        Some((
            _,
            Telemetry::Loop {
                mode: LoopMode::Recording,
                ..
            }
        ))
    ));
    h.done(Request::ContentRelease { id: 25 });
    h.done(Request::ContentRelease { id: 26 });
}

#[shoop_test]
fn uploaded_midi_initial_state_survives_activation_and_bounded_export() {
    let mut h = Harness::new(&description());
    let id = old_playback(&mut h);
    upload(&mut h, 1, false, 4, &vec![0; 32]);
    h.done(Request::ContentBegin(ContentDescription {
        id: 2,
        midi: true,
        frames: 4,
        bytes: 0,
    }));
    for packet in [
        &[0xc0, 12][..],
        &[0xb0, 7, 91],
        &[0x90, 61, 99],
        &[0xe0, 1, 65],
    ] {
        h.done(Request::ContentInitial {
            id: 2,
            message: MidiMessage::new(packet).unwrap(),
        });
    }
    assert_eq!(
        h.apply(Request::ContentInitial {
            id: 2,
            message: MidiMessage::new(&[0xf0, 0xf7]).unwrap()
        }),
        Outcome::Refused(Refusal::InvalidValue)
    );
    h.done(Request::ContentFinish { id: 2 });
    h.done(Request::ContentApply {
        loop_id: id,
        frames: 4,
        audio: Some(1),
        midi: Some(2),
    });
    let bytes = h.read(id, BulkSelector::MidiInitial { channel: 0 }).2;
    let mut packets = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let count = bytes[at] as usize;
        packets.push(bytes[at + 1..at + 1 + count].to_vec());
        at += 1 + count;
    }
    for expected in [
        &[0xc0, 12][..],
        &[0xb0, 7, 91],
        &[0x90, 61, 99],
        &[0xe0, 1, 65],
    ] {
        assert!(
            packets.iter().any(|packet| packet == expected),
            "missing {expected:?}"
        );
    }
    assert_eq!(h.read(id, BulkSelector::Midi).2, Vec::<u8>::new());
}

#[shoop_test]
fn cancelling_bulk_returns_credits_and_releases_reader_exports_without_interrupting_music() {
    let mut configured = description();
    configured.resource_pages = 64;
    let mut h = Harness::new(&configured);
    let old = old_playback(&mut h);
    h.done(Request::LoopTransition {
        loop_id: old,
        mode: LoopMode::Recording,
        delay: None,
    });
    h.run(1023, 16);
    h.done(Request::LoopTransition {
        loop_id: old,
        mode: LoopMode::Playing,
        delay: None,
    });
    assert_no_heap(|| {
        h.host.bulk(BulkRead {
            id: 10,
            loop_id: old,
            selector: BulkSelector::AudioFrames,
            offset: 0,
        })
    })
    .unwrap();
    for _ in 0..200 {
        h.run(1, 1);
        if h.chunks.iter().any(|chunk| chunk.id == 10) {
            break;
        }
    }
    assert!(h.chunks.iter().any(|chunk| chunk.id == 10));
    for round in 0..2 {
        for index in 0..2 {
            assert_no_heap(|| {
                h.host.bulk(BulkRead {
                    id: 11 + round * 2 + index,
                    loop_id: old,
                    selector: BulkSelector::AudioFrames,
                    offset: 128 + u64::from(index) * 128,
                })
            })
            .unwrap();
        }
        h.pump();
        let from = h.outputs[0].len();
        h.done(Request::CancelBulk);
        assert!(h.outputs[0][from..].iter().all(|sample| *sample == 1.));
        for index in 0..2 {
            assert!(
                h.chunks
                    .iter()
                    .any(|chunk| chunk.id == 11 + round * 2 + index),
                "cancelled bulk request lost its credit"
            );
        }
    }
    h.done(Request::ReplacementBegin { id: 1 });
    h.done(Request::ReplacementPrepare { id: 1 });
    h.done(Request::ReplacementCommit { id: 1 });
    upload(&mut h, 1, false, 1536, &vec![0; 1536 * 8]);
    h.done(Request::ContentRelease { id: 1 });
}
