mod support;

use shoop_rt_protocol::*;
use shoop_wasm_test_support::shoop_test;
use support::*;

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

#[shoop_test]
fn audio_only_refuses_midi_without_consuming_an_audio_slot_or_disabling_capture() {
    for mode in audio_modes() {
        let d = audio_description(mode);
        let capacity = d.history_capacity().unwrap();
        assert_eq!(capacity.midi_payload_bytes, 0);
        assert_eq!(capacity.midi_metadata_bytes, 0);
        assert_eq!(d.midi_output_capacity().unwrap().bytes, 0);
        let mut h = Harness::new(&d);
        let track = strip(0, 1);
        assert_eq!(
            h.apply(Request::CreateStrip {
                strip: track,
                shape: shape(2, 2, true)
            }),
            Outcome::Refused(Refusal::Unsupported)
        );
        h.done(Request::CreateStrip {
            strip: track,
            shape: shape(2, 2, false),
        });
        assert_eq!(
            h.apply(Request::InjectMidi {
                strip: track,
                message: MidiMessage::new(&[0x90, 60, 100]).unwrap()
            }),
            Outcome::Refused(Refusal::Unsupported)
        );
        h.done(Request::Connect {
            port: port(track, PortKind::AudioIn, 0),
            host: audio_in(0),
            connected: true,
        });
        let id = loop_id(0, 1);
        h.done(Request::CreateLoop {
            loop_id: id,
            strip: track,
        });
        h.input = Box::new(|_, _| 0.375);
        h.done(Request::LoopTransition {
            loop_id: id,
            mode: LoopMode::Recording,
            delay: None,
        });
        h.run(20, 13);
        h.done(Request::LoopTransition {
            loop_id: id,
            mode: LoopMode::Playing,
            delay: None,
        });
        let (_, samples, _) = h.read(id, BulkSelector::Audio { channel: 0 });
        assert!(samples.len() >= 20);
        assert!(samples.iter().all(|&v| v == 0.375));
    }
}
