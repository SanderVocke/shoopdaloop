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
    d.history.seconds = if mode == AudioRenderMode::FpgaReference {
        0
    } else {
        1
    };
    d.smoothing_frames = 0;
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
    let program = if mode == AudioRenderMode::FpgaReference {
        let program = h.host.render.fpga_program().unwrap();
        Some((
            program.instructions.clone(),
            program.instructions.as_ptr() as usize,
        ))
    } else {
        None
    };
    h.run(5 * q, partition);
    for index in 0..2 {
        h.done(transition(loop_id(index, 1), LoopMode::Playing));
    }
    h.done(Request::StripControl {
        strip: track,
        control: StripControl::Monitoring(true),
    });
    h.run(8 * q, partition);
    let padding = (q - h.frame() as usize % q) % q;
    h.run(padding, partition);
    h.run(q, q);
    if matches!(
        mode,
        AudioRenderMode::Chunks | AudioRenderMode::ChunksDescriptors
    ) {
        assert!(
            h.host.render.resource_work().1.copied_bytes <= q * 16,
            "ordinary playback must use persistent chunk references"
        );
    }
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
    if let Some((instructions, address)) = program {
        let current = h.host.render.fpga_program().unwrap();
        assert_eq!(
            current.instructions, instructions,
            "loop modes and controls must not alter the program"
        );
        assert_eq!(
            current.instructions.as_ptr() as usize,
            address,
            "the program must not be replaced between quanta"
        );
    }
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
            for mode in audio_modes()
                .into_iter()
                .chain([AudioRenderMode::FpgaReference])
            {
                let (actual, media) = render(mode, q, partition);
                if mode == AudioRenderMode::FpgaReference {
                    assert_eq!(media.len(), reference.1.len());
                    assert!(
                        media.iter().zip(&reference.1).all(|(&a, &b)| close(a, b)),
                        "fixed-point capture differs, Q={q}"
                    );
                } else {
                    assert_eq!(media, reference.1, "recording {mode:?}, Q={q}");
                }
                for (channel, expected) in actual.iter().zip(&reference.0) {
                    assert_eq!(channel.len(), expected.len(), "timing {mode:?}, Q={q}");
                    for (i, (&sample, &expected)) in channel.iter().zip(expected).enumerate() {
                        if !matches!(
                            mode,
                            AudioRenderMode::ChunksDescriptors | AudioRenderMode::FpgaReference
                        ) || i % q == 0
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

#[shoop_test]
fn fpga_reference_refuses_unsupported_requests_without_faulting_the_renderer() {
    let mut d = audio_description(AudioRenderMode::FpgaReference);
    d.history.seconds = 0;
    d.smoothing_frames = 0;
    let mut h = Harness::new(&d);
    assert_eq!(
        h.apply(Request::CreateStrip {
            strip: strip(0, 1),
            shape: shape(2, 2, true)
        }),
        Outcome::Refused(Refusal::Unsupported)
    );
    h.done(Request::CreateStrip {
        strip: strip(0, 1),
        shape: shape(1, 1, false),
    });
    assert_eq!(
        h.apply(Request::SetSmoothing { frames: 4 }),
        Outcome::Refused(Refusal::Unsupported)
    );
    h.done(Request::CreateLoop {
        loop_id: loop_id(0, 1),
        strip: strip(0, 1),
    });
    assert_eq!(
        h.apply(transition(loop_id(0, 1), LoopMode::Replacing)),
        Outcome::Refused(Refusal::Unsupported)
    );
    h.done(Request::Connect {
        port: port(strip(0, 1), PortKind::AudioIn, 0),
        host: audio_in(0),
        connected: true,
    });
    h.done(route(strip(0, 1), 0, 0, true));
    h.input = Box::new(|_, _| 0.25);
    h.done(transition(loop_id(0, 1), LoopMode::Recording));
    h.run(16, 13);
    h.done(transition(loop_id(0, 1), LoopMode::Playing));
    h.run(64, 13);
    let (_, samples, _) = h.read(loop_id(0, 1), BulkSelector::Audio { channel: 0 });
    assert!(samples.iter().any(|sample| *sample == 0.25));
}
