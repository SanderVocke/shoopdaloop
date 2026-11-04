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

#[shoop_test]
fn removing_playing_strips_waits_for_the_stop_tick_and_preserves_other_audio() {
    for quantum in [16, 32, 64] {
        for ramp in [4, quantum + 3] {
            let mut d = description();
            d.quantum = quantum;
            d.max_frames = 128;
            d.ramp_frames = ramp as u32;
            let mut h = Harness::new(&d);
            h.input = Box::new(|port, _| if port == 0 { 1. } else { 0.25 });
            for index in 0..2 {
                let track = strip(index, 1);
                h.done(Request::CreateStrip {
                    strip: track,
                    shape: shape(1, 1, false),
                });
                for (kind, host) in [
                    (PortKind::AudioIn, audio_in(index)),
                    (PortKind::AudioOut, audio_out(index)),
                ] {
                    h.done(Request::Connect {
                        port: port(track, kind, 0),
                        host,
                        connected: true,
                    });
                }
                h.done(Request::StripControl {
                    strip: track,
                    control: StripControl::Monitoring(true),
                });
            }
            let recorded = loop_id(0, 1);
            h.done(Request::CreateLoop {
                loop_id: recorded,
                strip: strip(0, 1),
            });
            h.done(transition(recorded, LoopMode::Recording));
            h.run(2 * quantum, 13);
            h.done(transition(recorded, LoopMode::Playing));
            h.run(2 * quantum, 13);
            assert!(h.outputs[0].last().is_some_and(|&sample| sample > 0.));
            let before = h.outputs[1].len();
            h.done(Request::RemoveStrip { strip: strip(0, 1) });
            h.run(2 * quantum, 13);
            assert_eq!(h.outputs[0].last(), Some(&0.));
            assert!(h.outputs[1][before..].iter().all(|&sample| sample == 0.25));
            h.done(Request::CreateStrip {
                strip: strip(0, 2),
                shape: shape(1, 1, false),
            });
            h.done(Request::CreateLoop {
                loop_id: loop_id(0, 2),
                strip: strip(0, 2),
            });
        }
    }
}

fn record(q: usize, partition: usize) -> (Vec<f32>, Vec<(u64, Vec<u8>)>) {
    let mut d = description();
    d.quantum = q;
    d.max_frames = 128;
    d.page_bytes = 4096;
    d.ramp_frames = 0;
    d.midi_events = 256;
    d.midi_bytes = 4096;
    let mut h = Harness::new(&d);
    let Some(EngineMessage::Welcome { limits, .. }) = h.lifecycle.first() else {
        panic!("welcome")
    };
    assert_eq!(limits.quantum, q as u32);
    assert_eq!(limits.processing_latency_frames, q as u32);
    let track = strip(0, 1);
    let looped = loop_id(0, 1);
    h.done(Request::CreateStrip {
        strip: track,
        shape: shape(1, 1, true),
    });
    for (kind, host) in [
        (PortKind::AudioIn, audio_in(0)),
        (PortKind::AudioOut, audio_out(0)),
        (PortKind::MidiIn, midi_in()),
        (PortKind::MidiOut, midi_out()),
    ] {
        h.done(Request::Connect {
            port: port(track, kind, 0),
            host,
            connected: true,
        });
    }
    h.done(Request::CreateLoop {
        loop_id: looped,
        strip: track,
    });
    h.run((q - h.frame() as usize % q) % q, 1);
    let start = h.frame();
    h.input = Box::new(move |port, frame| {
        if port == 0 {
            (frame - start + 1) as f32
        } else {
            0.
        }
    });
    h.midi_input = vec![
        (start + 3, vec![0x90, 60, 100]),
        (start + q as u64 + 7, vec![0x80, 60, 0]),
    ];
    h.send(transition(looped, LoopMode::Recording));
    h.run(4 * q, partition);
    h.input = Box::new(|_, _| 0.);
    let played = h.outputs[0].len();
    let play_frame = h.frame();
    h.send(transition(looped, LoopMode::Playing));
    h.run(4 * q, partition);
    assert!(h.outputs[0][played..played + q].iter().all(|&v| v == 0.));
    assert_eq!(
        &h.outputs[0][played + q..],
        &(1..=3 * q).map(|v| v as f32).collect::<Vec<_>>()
    );
    assert!(
        h.midi
            .contains(&(play_frame + q as u64 + 3, vec![0x90, 60, 100])),
        "Q={q}, play={play_frame}, MIDI={:?}",
        h.midi
    );
    assert!(h
        .midi
        .contains(&(play_frame + 2 * q as u64 + 7, vec![0x80, 60, 0])));
    let audio = h.outputs[0][played..].to_vec();
    let midi = h.midi.clone();
    let (total, samples, _) = h.read(looped, BulkSelector::Audio { channel: 0 });
    assert_eq!(total, 4 * q as u64);
    assert_eq!(samples, (1..=4 * q).map(|v| v as f32).collect::<Vec<_>>());
    assert_eq!(
        h.apply(Request::ContentBegin(ContentDescription {
            id: 100,
            midi: false,
            frames: q as u64 + 1,
            bytes: (q as u64 + 1) * 8
        })),
        Outcome::Refused(Refusal::InvalidValue)
    );
    h.done(transition(looped, LoopMode::Stopped));
    h.done(Request::RemoveLoop { loop_id: looped });
    h.done(Request::CreateLoop {
        loop_id: loop_id(0, 2),
        strip: track,
    });
    (audio, midi)
}

#[shoop_test]
fn full_endpoint_recording_export_and_slot_reuse_are_partition_invariant() {
    for q in [16, 32, 64, 128] {
        let reference = record(q, 128);
        for partition in [1, 7, 31] {
            assert_eq!(
                record(q, partition),
                reference,
                "Q={q}, partition={partition}"
            );
        }
    }
}

#[shoop_test]
fn host_midi_accumulation_overflow_recovers_without_stopping_audio() {
    let mut d = description();
    d.quantum = 32;
    d.max_frames = 32;
    d.midi_events = 1;
    d.ramp_frames = 0;
    let mut h = Harness::new(&d);
    let track = strip(0, 1);
    h.done(Request::CreateStrip {
        strip: track,
        shape: shape(1, 1, true),
    });
    for (kind, host) in [
        (PortKind::AudioIn, audio_in(0)),
        (PortKind::AudioOut, audio_out(0)),
        (PortKind::MidiIn, midi_in()),
        (PortKind::MidiOut, midi_out()),
    ] {
        h.done(Request::Connect {
            port: port(track, kind, 0),
            host,
            connected: true,
        });
    }
    h.done(Request::StripControl {
        strip: track,
        control: StripControl::Monitoring(true),
    });
    h.run((32 - h.frame() as usize % 32) % 32, 1);
    h.input = Box::new(|_, _| 1.);
    let start = h.frame();
    h.midi_input = vec![(start, vec![0xb0, 1, 1]), (start + 16, vec![0xb0, 1, 2])];
    h.run(32, 16);
    assert!(h.host.render.diagnostics().midi_overflows > 0);
    assert_eq!(h.host.render.phase(), shoop_rt_engine::Phase::Running);
    h.run(96, 16);
    assert!(h.outputs[0][h.outputs[0].len() - 64..]
        .iter()
        .all(|&v| v == 1.));
    assert_eq!(h.host.render.phase(), shoop_rt_engine::Phase::Running);
}

#[shoop_test]
fn synchronized_recording_and_delayed_play_keep_exact_aligned_capture_lengths() {
    for q in [16, 32, 64, 128] {
        let mut d = description();
        d.quantum = q;
        d.max_frames = 128;
        d.page_bytes = 4096;
        d.ramp_frames = 0;
        let mut h = Harness::new(&d);
        let track = strip(0, 1);
        let (clock, follower) = (loop_id(0, 1), loop_id(1, 1));
        h.done(Request::CreateStrip {
            strip: track,
            shape: shape(1, 1, false),
        });
        h.done(Request::Connect {
            port: port(track, PortKind::AudioIn, 0),
            host: audio_in(0),
            connected: true,
        });
        for looped in [clock, follower] {
            h.done(Request::CreateLoop {
                loop_id: looped,
                strip: track,
            });
        }
        h.input = Box::new(|port, frame| if port == 0 { frame as f32 } else { 0. });
        h.send(transition(clock, LoopMode::Recording));
        h.run(4 * q, q);
        let play_start = h.frame();
        h.done(transition(clock, LoopMode::Playing));
        h.done(Request::LoopSync {
            loop_id: follower,
            source: Some(clock),
        });
        let requested = h.frame();
        let period = 4 * q as u64;
        let start = play_start + (requested - play_start).div_ceil(period) * period + period;
        let recording = h.send(Request::LoopTransition {
            loop_id: follower,
            mode: LoopMode::Recording,
            delay: Some(1),
        });
        h.run((start - requested) as usize, q);
        assert!(matches!(
            h.last_loop(follower),
            Some((
                _,
                Telemetry::Loop {
                    mode: LoopMode::Stopped,
                    ..
                }
            ))
        ));
        h.run(q, q);
        assert_eq!(h.results[&recording], Outcome::Done);
        let playing = h.send(Request::LoopTransition {
            loop_id: follower,
            mode: LoopMode::Playing,
            delay: Some(0),
        });
        h.run(3 * q, q);
        assert!(matches!(
            h.last_loop(follower),
            Some((
                _,
                Telemetry::Loop {
                    mode: LoopMode::Recording,
                    ..
                }
            ))
        ));
        h.run(q, q);
        assert_eq!(h.results[&playing], Outcome::Done);
        let (length, audio, _) = h.read(follower, BulkSelector::Audio { channel: 0 });
        assert_eq!(length, period);
        assert_eq!(
            audio,
            (start..start + period)
                .map(|f| f as f32)
                .collect::<Vec<_>>()
        );
    }
}
