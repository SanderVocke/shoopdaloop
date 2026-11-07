mod support;

use shoop_rt_engine::AudioRenderMode;
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

fn render(mode: AudioRenderMode, q: usize, partition: usize) -> ([Vec<f32>; 2], Vec<f32>) {
    let mut d = audio_description(mode);
    d.quantum = q;
    d.max_frames = 128;
    d.history.seconds = 1;
    d.ramp_frames = (q * 3) as u32;
    let mut h = Harness::new(&d);
    let track = strip(0, 1);
    h.input = Box::new(|port, frame| ((frame * 7 + port as u64 * 11) % 101) as f32 / 200. - 0.2);
    h.done(Request::CreateStrip {
        strip: track,
        shape: shape(2, 2, false),
    });
    for channel in 0..2 {
        h.done(Request::Connect {
            port: port(track, PortKind::AudioIn, channel),
            host: audio_in(channel as u32),
            connected: true,
        });
        h.done(route(track, channel, channel, true));
    }
    for index in 0..2 {
        let loop_id = loop_id(index, 1);
        h.done(Request::CreateLoop {
            loop_id,
            strip: track,
        });
        h.done(transition(loop_id, LoopMode::Recording));
    }
    h.run(5 * q, partition);
    for index in 0..2 {
        h.done(transition(loop_id(index, 1), LoopMode::Playing));
    }
    h.done(Request::StripControl {
        strip: track,
        control: StripControl::Monitoring(true),
    });
    h.run(8 * q, partition);
    h.done(Request::StripControl {
        strip: track,
        control: StripControl::OutputGain(0.6),
    });
    h.done(Request::StripControl {
        strip: track,
        control: StripControl::OutputBalance(-0.4),
    });
    h.done(Request::BusControl {
        bus: BusId::MASTER,
        control: BusControl::Gain(0.7),
    });
    h.run(8 * q, partition);
    h.done(Request::StripControl {
        strip: track,
        control: StripControl::OutputMute(true),
    });
    h.run(4 * q, partition);
    h.done(Request::StripControl {
        strip: track,
        control: StripControl::OutputMute(false),
    });
    h.run(4 * q, partition);
    h.done(transition(loop_id(0, 1), LoopMode::Stopped));
    h.run(4 * q, partition);
    h.done(transition(loop_id(0, 1), LoopMode::Playing));
    h.run(8 * q, partition);
    let frozen = h.read(loop_id(0, 1), BulkSelector::Audio { channel: 0 }).1;
    h.run(2304, partition);
    assert_eq!(
        h.read(loop_id(0, 1), BulkSelector::Audio { channel: 0 }).1,
        frozen
    );
    h.done(Request::LoopClear {
        loop_id: loop_id(1, 1),
    });
    h.done(Request::RemoveLoop {
        loop_id: loop_id(1, 1),
    });
    h.done(Request::CreateLoop {
        loop_id: loop_id(1, 2),
        strip: track,
    });
    h.done(transition(loop_id(1, 2), LoopMode::Recording));
    h.run(4 * q, partition);
    h.done(transition(loop_id(1, 2), LoopMode::Playing));
    h.run(8 * q, partition);
    (h.outputs, frozen)
}

fn close(actual: f32, expected: f32) -> bool {
    (actual - expected).abs() <= 1e-6 + 1e-6 * expected.abs()
}

#[shoop_test]
fn reference_graph_preserves_samples_capture_controls_wrap_and_callback_partitioning() {
    for q in [1, 32] {
        let reference = render(AudioRenderMode::Current, q, 13);
        for partition in [1, 13, 128] {
            for mode in audio_modes() {
                let (actual, media) = render(mode, q, partition);
                assert_eq!(media, reference.1, "recording {mode:?}, Q={q}");
                for (channel, expected) in actual.iter().zip(&reference.0) {
                    assert_eq!(channel.len(), expected.len(), "timing {mode:?}, Q={q}");
                    for (i, (&sample, &expected)) in channel.iter().zip(expected).enumerate() {
                        if mode != AudioRenderMode::ChunksDescriptors
                            || i % q == 0
                            || i % q == q - 1
                        {
                            assert!(close(sample,expected),"{mode:?}, Q={q}, partition={partition}, sample={i}: {sample} != {expected}");
                        }
                    }
                }
            }
        }
    }
}
