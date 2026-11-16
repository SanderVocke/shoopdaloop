mod support;

use shoop_rt_protocol::*;
use shoop_wasm_test_support::shoop_test;
use support::*;

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

fn transition(id: LoopSlotId, mode: LoopMode, delay: Option<u32>) -> Request {
    Request::LoopTransition {
        loop_id: id,
        mode,
        delay,
    }
}

#[shoop_test]
fn records_and_plays_back_through_a_strip() {
    let mut harness = Harness::new(&description());
    let track = strip(0, 1);
    harness.done(Request::CreateStrip {
        strip: track,
        shape: shape(1, 1, true),
    });
    harness.done(Request::Connect {
        port: port(track, PortKind::AudioIn, 0),
        host: audio_in(0),
        connected: true,
    });
    harness.done(Request::Connect {
        port: port(track, PortKind::AudioOut, 0),
        host: audio_out(0),
        connected: true,
    });
    let looped = loop_id(2, 1);
    harness.done(Request::CreateLoop {
        loop_id: looped,
        strip: track,
    });
    let start = harness.frame();
    harness.input = Box::new(move |port, frame| {
        if port == 0 {
            (frame - start + 1) as f32
        } else {
            0.
        }
    });
    harness.send(transition(looped, LoopMode::Recording, None));
    harness.run(6, 4);
    harness.input = Box::new(|_, _| 0.);
    harness.send(transition(looped, LoopMode::Playing, None));
    let played = harness.outputs[0].len();
    harness.run(12, 5);
    assert_eq!(
        &harness.outputs[0][played..],
        &[1., 2., 3., 4., 5., 6., 1., 2., 3., 4., 5., 6.]
    );
    let (total, samples, _) = harness.read(looped, BulkSelector::Audio { channel: 0 });
    assert_eq!(total, 6);
    assert_eq!(samples, [1., 2., 3., 4., 5., 6.]);
}

#[shoop_test]
fn lost_midi_input_keeps_the_last_paired_capture_and_unrelated_audio_running() {
    let mut h = Harness::new(&description());
    let track = strip(0, 1);
    h.done(Request::CreateStrip {
        strip: track,
        shape: shape(1, 1, true),
    });
    for (kind, host) in [
        (PortKind::AudioIn, audio_in(0)),
        (PortKind::AudioOut, audio_out(0)),
        (PortKind::MidiIn, HostPortId(4)),
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
    let looped = loop_id(0, 1);
    h.done(Request::CreateLoop {
        loop_id: looped,
        strip: track,
    });
    h.input = Box::new(|_, _| 1.);
    h.done(transition(looped, LoopMode::Recording, None));
    h.run(8, 4);
    let before = h.frame();
    h.host.midi_input_mut(0).mark_input_loss();
    h.run(1, 1);
    let from = h.outputs[0].len();
    h.run(16, 4);
    assert!(h.outputs[0][from..].iter().all(|sample| *sample == 1.));
    h.done(transition(looped, LoopMode::Stopped, None));
    let (frames, samples, _) = h.read(looped, BulkSelector::Audio { channel: 0 });
    assert!(frames != 0 && frames <= before);
    assert!(samples.iter().all(|sample| *sample == 1.));
    let from = h.frame();
    h.run(8, 4);
    assert_eq!(h.frame(), from + 8);
}
