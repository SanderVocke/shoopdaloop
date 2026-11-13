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
