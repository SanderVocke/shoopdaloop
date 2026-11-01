use super::*;
use shoop_wasm_test_support::shoop_test;
fn channel(object: u32, frames: u64, offset: i64) -> image::Channel {
    image::Channel {
        object: Some(object),
        frames,
        gain: 1.,
        start_offset: offset,
        capture_alignment: 0,
        preplay: 0,
        recording_started_at: None,
    }
}
fn content() -> image::Loop {
    image::Loop {
        id: LoopId::from_raw(1),
        name: "User take/?".into(),
        sync: false,
        frames: 4,
        gain: 0.5,
        balance: 0.,
        audio: Vec::new(),
        midi: Vec::new(),
    }
}
#[shoop_test]
fn exact_audio_export_maps_the_logical_window_without_baking_in_gain() {
    let mut content = content();
    content.audio.push(channel(1, 2, -1));
    content.audio[0].gain = 3.;
    let image = Arc::new(image::Image {
        manifest: image::Manifest::Loop {
            sample_rate: 48000,
            content,
        },
        payloads: vec![image::Payload::Audio(Arc::new(vec![-0.0, 0.25]))],
    });
    let Value::File { name, bytes, .. } = (Work::Encode {
        image,
        kind: IoTaskKind::ExportLoopAudio,
        wav: false,
        standard: false,
        channels: vec![0],
    })
    .run()
    .unwrap() else {
        panic!()
    };
    assert_eq!(name, "User_take__.shoop-audio");
    let audio = decode_loop_audio(&bytes).unwrap();
    assert_eq!(audio.channels[0].samples, [0., -0.0, 0.25, 0.]);
    assert_eq!(audio.channels[0].samples[1].to_bits(), (-0.0f32).to_bits());
}
#[shoop_test]
fn exact_midi_export_folds_preroll_and_preserves_logical_event_order() {
    let mut content = content();
    content.frames = 3;
    content.midi.push(image::MidiChannel {
        channel: channel(1, 8, 2),
        initial: vec![vec![0xc0, 12]],
    });
    let packets = [
        (0, vec![0xb0, 7, 90]),
        (1, vec![0x90, 60, 100]),
        (2, vec![0x80, 60, 0]),
        (3, vec![0x90, 62, 99]),
        (6, vec![0x80, 62, 0]),
    ];
    let events = packets
        .into_iter()
        .map(|(frame, data)| MidiEventState {
            frame,
            data: data.into(),
        })
        .collect::<Vec<_>>();
    let image = Arc::new(image::Image {
        manifest: image::Manifest::Loop {
            sample_rate: 48000,
            content,
        },
        payloads: vec![image::Payload::Midi(events.into())],
    });
    let Value::File { bytes, .. } = (Work::Encode {
        image,
        kind: IoTaskKind::ExportLoopMidi,
        wav: false,
        standard: false,
        channels: Vec::new(),
    })
    .run()
    .unwrap() else {
        panic!()
    };
    let midi = decode_exact_midi(&bytes).unwrap();
    assert_eq!(midi.length_frames, 3);
    assert!(midi.start_state.contains(&vec![0xc0, 12]));
    assert!(midi.start_state.contains(&vec![0xb0, 7, 90]));
    assert!(midi.start_state.contains(&vec![0x90, 60, 100]));
    assert_eq!(
        midi.events,
        [
            ExactMidiEvent {
                frame: 0,
                order: 2,
                data: vec![0x80, 60, 0]
            },
            ExactMidiEvent {
                frame: 1,
                order: 3,
                data: vec![0x90, 62, 99]
            }
        ]
    );
}
#[shoop_test]
fn importing_unrestorable_midi_initial_state_refuses_before_staging() {
    let midi = ExactMidi {
        sample_rate: 1000,
        length_frames: 4,
        start_state: vec![vec![0xb0, 6, 40]],
        events: Vec::new(),
    };
    let error = (Work::Prepare {
        decoded: Decoded::Midi(midi),
        mapping: Vec::new(),
        update_length: true,
    })
    .run()
    .err()
    .unwrap();
    assert!(error.to_string().contains("parameter history"));
}
#[shoop_test]
fn a_final_system_reset_makes_prior_parameter_history_restorable() {
    let midi = ExactMidi {
        sample_rate: 1000,
        length_frames: 4,
        start_state: vec![vec![0xb0, 6, 40], vec![0xff]],
        events: Vec::new(),
    };
    assert!(matches!(
        (Work::Prepare {
            decoded: Decoded::Midi(midi),
            mapping: Vec::new(),
            update_length: true
        })
        .run(),
        Ok(Value::Image(_))
    ));
}
