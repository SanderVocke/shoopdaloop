//! Structural transactions while rendering: unaffected paths continue without a
//! gap, removed and rerouted paths ramp, reused slots start fresh and pool
//! limits are refused.

mod support;

use shoop_rt_protocol::*;
use shoop_wasm_test_support::shoop_test;
use support::*;

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

const RAMP: usize = 4;

fn transition(id: LoopSlotId, mode: LoopMode) -> Request {
    Request::LoopTransition {
        loop_id: id,
        mode,
        delay: None,
    }
}
fn connect(strip: StripId, kind: PortKind, host: HostPortId, connected: bool) -> Request {
    Request::Connect {
        port: port(strip, kind, 0),
        host,
        connected,
    }
}
fn ramp_input(port: usize, frame: u64) -> f32 {
    if port == 0 {
        1.
    } else {
        frame as f32 / 1024.
    }
}

/// Frames `from..` of an output, which must equal `expected` until a change
/// begins, then follow `ramp(sample, step)` for the ramp and `after` afterwards.
/// Returns the frame the ramp began.
fn ramped(
    output: &[f32],
    from: usize,
    expected: impl Fn(usize) -> f32,
    ramp: impl Fn(usize, usize) -> f32,
    after: impl Fn(usize) -> f32,
) -> usize {
    let start = (from..output.len())
        .find(|&frame| output[frame] != expected(frame))
        .expect("the change is audible");
    for step in 0..RAMP {
        assert_eq!(
            output[start + step],
            ramp(start + step, step),
            "ramp step {step}"
        );
    }
    for frame in start + RAMP..output.len() {
        assert_eq!(output[frame], after(frame), "after the ramp at {frame}");
    }
    start
}

#[shoop_test]
fn edits_keep_other_paths_continuous_and_ramp_removed_paths() {
    let mut harness = Harness::new(&description());
    harness.input = Box::new(ramp_input);
    let (x, y, z) = (strip(0, 1), strip(1, 1), strip(2, 1));
    let (played, recording) = (loop_id(0, 1), loop_id(1, 1));
    harness.done(Request::CreateStrip {
        strip: x,
        shape: shape(1, 1, false),
    });
    harness.done(connect(x, PortKind::AudioIn, audio_in(0), true));
    harness.done(connect(x, PortKind::AudioOut, audio_out(0), true));
    harness.done(Request::StripControl {
        strip: x,
        control: StripControl::Monitoring(true),
    });
    harness.done(Request::CreateStrip {
        strip: y,
        shape: shape(1, 1, false),
    });
    harness.done(connect(y, PortKind::AudioIn, audio_in(1), true));
    harness.done(connect(y, PortKind::AudioOut, audio_out(1), true));
    for id in [played, recording] {
        harness.done(Request::CreateLoop {
            loop_id: id,
            strip: y,
        });
    }
    let recorded = harness.frame();
    harness.done(transition(played, LoopMode::Recording));
    harness.run(8 - (harness.frame() - recorded) as usize, 1);
    let start = harness.frame();
    harness.done(transition(played, LoopMode::Playing));
    harness.run(5, 3);
    let content = |frame: usize| (recorded as usize + (frame - start as usize) % 8) as f32 / 1024.;
    let playing = |frame: usize| content(frame);
    assert!((start as usize..harness.outputs[1].len()).all(|f| harness.outputs[1][f] == playing(f)));
    assert!(harness.outputs[0][start as usize..]
        .iter()
        .all(|s| *s == 1.));

    // Recording continues across every later edit.
    let take = harness.frame();
    harness.done(transition(recording, LoopMode::Recording));

    // Adding a strip with a loop and connections changes nothing audible.
    let from = harness.outputs[0].len();
    for request in [
        Request::CreateStrip {
            strip: z,
            shape: shape(1, 1, false),
        },
        Request::CreateLoop {
            loop_id: loop_id(4, 1),
            strip: z,
        },
        connect(z, PortKind::AudioOut, audio_out(1), true),
        connect(z, PortKind::AudioIn, audio_in(1), true),
    ] {
        assert_eq!(harness.apply_in(request, 3), Outcome::Done);
    }
    assert!(harness.outputs[0][from..].iter().all(|s| *s == 1.));
    assert!((from..harness.outputs[1].len()).all(|f| harness.outputs[1][f] == playing(f)));

    // A disconnected output fades out; the other output is unaffected.
    let from = harness.outputs[0].len();
    assert_eq!(
        harness.apply_in(connect(x, PortKind::AudioOut, audio_out(0), false), 3),
        Outcome::Done
    );
    harness.run(6, 3);
    ramped(
        &harness.outputs[0],
        from,
        |_| 1.,
        |_, step| 1. - (step + 1) as f32 / RAMP as f32,
        |_| 0.,
    );
    assert!((from..harness.outputs[1].len()).all(|f| harness.outputs[1][f] == playing(f)));

    // Reconnecting fades in again.
    let from = harness.outputs[0].len();
    assert_eq!(
        harness.apply_in(connect(x, PortKind::AudioOut, audio_out(0), true), 3),
        Outcome::Done
    );
    harness.run(6, 3);
    ramped(
        &harness.outputs[0],
        from,
        |_| 0.,
        |_, step| (step + 1) as f32 / RAMP as f32,
        |_| 1.,
    );

    // Removing a playing loop fades it before it stops and leaves the graph.
    let from = harness.outputs[1].len();
    assert_eq!(
        harness.apply_in(Request::RemoveLoop { loop_id: played }, 3),
        Outcome::Done
    );
    harness.run(6, 3);
    let removal = ramped(
        &harness.outputs[1],
        from,
        playing,
        |frame, step| playing(frame) * (1. - (step + 1) as f32 / RAMP as f32),
        |_| 0.,
    );
    assert!(harness.outputs[0][from..].iter().all(|s| *s == 1.));
    assert!(removal > from);

    // Each route to a Master channel ramps in on that channel's output only.
    let from = harness.outputs[0].len();
    assert_eq!(harness.apply_in(route(x, 0, 0, true), 3), Outcome::Done);
    harness.run(6, 3);
    ramped(
        &harness.outputs[0],
        from,
        |_| 1.,
        |_, step| 1. + (step + 1) as f32 / RAMP as f32,
        |_| 2.,
    );
    assert!(harness.outputs[1][from..].iter().all(|s| *s == 0.));
    let from = harness.outputs[1].len();
    assert_eq!(harness.apply_in(route(x, 0, 1, true), 3), Outcome::Done);
    harness.run(6, 3);
    ramped(
        &harness.outputs[1],
        from,
        |_| 0.,
        |_, step| (step + 1) as f32 / RAMP as f32,
        |_| 1.,
    );
    assert!(harness.outputs[0][from..].iter().all(|s| *s == 2.));

    // Removing the strip fades everything it fed.
    let from = harness.outputs[0].len();
    assert_eq!(
        harness.apply_in(Request::RemoveStrip { strip: x }, 3),
        Outcome::Done
    );
    harness.run(6, 3);
    let removal = ramped(
        &harness.outputs[0],
        from,
        |_| 2.,
        |_, step| 2. * (1. - (step + 1) as f32 / RAMP as f32),
        |_| 0.,
    );
    ramped(
        &harness.outputs[1],
        removal,
        |_| 1.,
        |_, step| 1. - (step + 1) as f32 / RAMP as f32,
        |_| 0.,
    );

    // The take kept recording through every edit, without a gap.
    let stop = harness.frame();
    harness.done(transition(recording, LoopMode::Stopped));
    let (total, samples, _) = harness.read(recording, BulkSelector::Audio { channel: 0 });
    let expected: Vec<f32> = (take..take + total).map(|f| f as f32 / 1024.).collect();
    assert!(total >= stop - take);
    assert_eq!(samples, expected);
}

#[shoop_test]
fn pools_and_identities_are_enforced() {
    let mut harness = Harness::new(&description());
    let created = |strip| Request::CreateStrip {
        strip,
        shape: shape(1, 1, true),
    };
    assert_eq!(
        harness.apply(created(strip(4, 1))),
        Outcome::Refused(Refusal::Capacity)
    );
    assert_eq!(
        harness.apply(Request::CreateStrip {
            strip: strip(0, 1),
            shape: shape(3, 1, false),
        }),
        Outcome::Refused(Refusal::Capacity)
    );
    assert_eq!(
        harness.apply(created(strip(0, 0))),
        Outcome::Refused(Refusal::InvalidId)
    );
    harness.done(created(strip(0, 1)));
    assert_eq!(
        harness.apply(created(strip(0, 2))),
        Outcome::Refused(Refusal::InvalidId)
    );
    harness.done(Request::RemoveStrip { strip: strip(0, 1) });
    assert_eq!(
        harness.apply(created(strip(0, 1))),
        Outcome::Refused(Refusal::InvalidId)
    );
    harness.done(created(strip(0, 3)));
    // The last slot belongs to the second pool segment.
    harness.done(created(strip(3, 1)));
    assert_eq!(
        harness.apply(Request::StripControl {
            strip: strip(0, 1),
            control: StripControl::OutputMute(true),
        }),
        Outcome::Refused(Refusal::InvalidId)
    );
    assert_eq!(
        harness.apply(Request::CreateLoop {
            loop_id: loop_id(6, 1),
            strip: strip(0, 3),
        }),
        Outcome::Refused(Refusal::Capacity)
    );
    assert_eq!(
        harness.apply(Request::CreateLoop {
            loop_id: loop_id(0, 1),
            strip: strip(1, 1),
        }),
        Outcome::Refused(Refusal::InvalidId)
    );
    for slot in 0..6 {
        harness.done(Request::CreateLoop {
            loop_id: loop_id(slot, 1),
            strip: if slot < 4 { strip(0, 3) } else { strip(3, 1) },
        });
    }
    assert_eq!(
        harness.apply(Request::CreateLoop {
            loop_id: loop_id(5, 2),
            strip: strip(0, 3),
        }),
        Outcome::Refused(Refusal::InvalidId)
    );
    harness.done(Request::LoopSync {
        loop_id: loop_id(1, 1),
        source: Some(loop_id(0, 1)),
    });
    harness.done(Request::LoopSync {
        loop_id: loop_id(2, 1),
        source: Some(loop_id(1, 1)),
    });
    assert_eq!(
        harness.apply(Request::LoopSync {
            loop_id: loop_id(0, 1),
            source: Some(loop_id(2, 1)),
        }),
        Outcome::Refused(Refusal::InvalidValue)
    );
    assert_eq!(
        harness.apply(Request::Connect {
            port: port(strip(0, 3), PortKind::AudioIn, 1),
            host: audio_in(0),
            connected: true,
        }),
        Outcome::Refused(Refusal::InvalidValue)
    );
    assert_eq!(
        harness.apply(Request::Connect {
            port: port(strip(0, 3), PortKind::AudioIn, 0),
            host: audio_out(0),
            connected: true,
        }),
        Outcome::Refused(Refusal::InvalidId)
    );
    assert_eq!(
        harness.apply(Request::LoopGain {
            loop_id: loop_id(0, 1),
            gain: f32::NAN,
        }),
        Outcome::Refused(Refusal::InvalidValue)
    );
    assert_eq!(
        harness.apply(Request::SetSmoothing { frames: 9 }),
        Outcome::Refused(Refusal::InvalidValue)
    );
    harness.done(Request::SetSmoothing { frames: 0 });
    // Removing a sync source unsyncs its followers; a stale loop is refused.
    harness.done(Request::RemoveLoop {
        loop_id: loop_id(0, 1),
    });
    assert_eq!(
        harness.apply(Request::LoopTransition {
            loop_id: loop_id(0, 1),
            mode: LoopMode::Playing,
            delay: None,
        }),
        Outcome::Refused(Refusal::InvalidId)
    );
    harness.done(Request::LoopSync {
        loop_id: loop_id(2, 1),
        source: Some(loop_id(1, 1)),
    });
    harness.done(Request::LoopSync {
        loop_id: loop_id(1, 1),
        source: None,
    });
    // Removing a strip removes its loops.
    harness.done(Request::RemoveStrip { strip: strip(0, 3) });
    for slot in 1..4 {
        assert_eq!(
            harness.apply(Request::LoopClear {
                loop_id: loop_id(slot, 1),
            }),
            Outcome::Refused(Refusal::InvalidId)
        );
    }
    harness.done(Request::LoopClear {
        loop_id: loop_id(4, 1),
    });
}

#[shoop_test]
fn reused_slots_start_fresh_and_clear_empties_a_loop() {
    let mut harness = Harness::new(&description());
    harness.input = Box::new(|port, frame| if port == 0 { 0.25 + frame as f32 } else { 0. });
    let track = strip(0, 1);
    let looped = loop_id(0, 1);
    harness.done(Request::CreateStrip {
        strip: track,
        shape: shape(1, 1, false),
    });
    harness.done(connect(track, PortKind::AudioIn, audio_in(0), true));
    harness.done(connect(track, PortKind::AudioOut, audio_out(0), true));
    harness.done(Request::StripControl {
        strip: track,
        control: StripControl::OutputGain(0.5),
    });
    harness.done(Request::CreateLoop {
        loop_id: looped,
        strip: track,
    });
    harness.done(transition(looped, LoopMode::Recording));
    harness.run(5, 2);
    harness.done(transition(looped, LoopMode::Playing));
    harness.run(5, 2);
    assert!(harness.outputs[0].iter().rev().take(4).all(|s| *s != 0.));

    // Clearing a playing loop silences it and empties its content.
    harness.done(Request::LoopClear { loop_id: looped });
    harness.run(4, 2);
    assert!(harness.outputs[0].iter().rev().take(4).all(|s| *s == 0.));
    let (total, ..) = harness.read(looped, BulkSelector::Audio { channel: 0 });
    assert_eq!(total, 0);
    let Some((_, Telemetry::Loop { mode, length, .. })) = harness.last_loop(looped) else {
        panic!("loop telemetry")
    };
    assert_eq!((mode, length), (LoopMode::Stopped, 0));
    assert_eq!(
        harness.apply(transition(looped, LoopMode::Playing)),
        Outcome::Refused(Refusal::InvalidValue)
    );

    // Clearing while recording stops the take first.
    harness.done(transition(looped, LoopMode::Recording));
    harness.run(3, 1);
    harness.done(Request::LoopClear { loop_id: looped });
    let (total, ..) = harness.read(looped, BulkSelector::Audio { channel: 0 });
    assert_eq!(total, 0);

    // A new loop generation in a used slot starts empty and stopped.
    harness.done(transition(looped, LoopMode::Recording));
    harness.run(3, 1);
    harness.done(transition(looped, LoopMode::Playing));
    harness.done(Request::RemoveLoop { loop_id: looped });
    let reused = loop_id(0, 2);
    harness.done(Request::CreateLoop {
        loop_id: reused,
        strip: track,
    });
    harness.run(2, 1);
    let (total, ..) = harness.read(reused, BulkSelector::Audio { channel: 0 });
    assert_eq!(total, 0);
    let Some((_, Telemetry::Loop { mode, length, .. })) = harness.last_loop(reused) else {
        panic!("loop telemetry")
    };
    assert_eq!((mode, length), (LoopMode::Stopped, 0));

    // A new strip generation starts with default controls and connections.
    harness.done(Request::RemoveStrip { strip: track });
    let fresh = strip(0, 2);
    harness.done(Request::CreateStrip {
        strip: fresh,
        shape: shape(1, 1, false),
    });
    harness.done(connect(fresh, PortKind::AudioIn, audio_in(0), true));
    harness.done(connect(fresh, PortKind::AudioOut, audio_out(0), true));
    harness.done(Request::StripControl {
        strip: fresh,
        control: StripControl::Monitoring(true),
    });
    harness.run(8, 4);
    let frame = harness.outputs[0].len() - 1;
    assert_eq!(harness.outputs[0][frame], 0.25 + frame as f32);
}

#[shoop_test]
fn repeated_create_and_remove_hands_slots_over_without_leaking() {
    let description = description();
    let mut harness = Harness::new(&description);
    harness.input = Box::new(|port, frame| if port == 0 { frame as f32 } else { 0. });
    let track = strip(0, 1);
    harness.done(Request::CreateStrip {
        strip: track,
        shape: shape(1, 1, true),
    });
    harness.done(connect(track, PortKind::AudioIn, audio_in(0), true));
    harness.done(connect(track, PortKind::AudioOut, audio_out(0), true));
    // More cycles than the planner has media metadata slots.
    for generation in 1..=100 {
        let id = loop_id(5, generation);
        harness.done(Request::CreateLoop {
            loop_id: id,
            strip: track,
        });
        if generation % 3 == 0 {
            harness.done(Request::RemoveLoop { loop_id: id });
            continue;
        }
        harness.done(transition(id, LoopMode::Recording));
        harness.run(2, 2);
        harness.done(transition(id, LoopMode::Playing));
        if generation % 2 == 0 {
            harness.done(Request::LoopClear { loop_id: id });
        }
        harness.done(Request::RemoveLoop { loop_id: id });
    }
    let render = harness.host.into_render();
    let mut stopped = render.into_stopped();
    stopped.cleanup().unwrap();
    assert_eq!(stopped.media_usage().references, 0);
    assert_eq!(
        stopped.resource_usage().free_pages,
        description.resource_pages
    );
}

#[shoop_test]
fn only_occupied_slots_are_rendered() {
    let mut harness = Harness::new(&description());
    harness.run(1, 1);
    assert_eq!(harness.host.render.diagnostics().nodes_processed, 0);
    let track = strip(3, 1);
    harness.done(Request::CreateStrip {
        strip: track,
        shape: shape(2, 2, true),
    });
    harness.done(Request::CreateLoop {
        loop_id: loop_id(5, 1),
        strip: track,
    });
    harness.run(1, 1);
    // Master, the strip's two stages and the loop's playback, capture and fader.
    assert_eq!(harness.host.render.diagnostics().nodes_processed, 6);
    harness.done(Request::RemoveStrip { strip: track });
    harness.run(1, 1);
    assert_eq!(harness.host.render.diagnostics().nodes_processed, 1);
}

#[shoop_test]
fn master_outputs_are_connections_that_cross_fade_and_resync() {
    let mut harness = Harness::new(&description());
    harness.input = Box::new(|port, _| if port == 0 { 1. } else { 0. });
    let track = strip(0, 1);
    harness.done(Request::CreateStrip {
        strip: track,
        shape: shape(1, 1, false),
    });
    harness.done(connect(track, PortKind::AudioIn, audio_in(0), true));
    harness.done(Request::StripControl {
        strip: track,
        control: StripControl::Monitoring(true),
    });
    harness.done(route(track, 0, 0, true));
    harness.done(route(track, 0, 1, true));
    harness.run(4, 2);
    // The prepared wiring feeds both host outputs from the mono strip.
    assert_eq!(harness.outputs[0].last(), Some(&1.));
    assert_eq!(harness.outputs[1].last(), Some(&1.));
    let wired = |harness: &mut Harness| {
        harness.states.clear();
        assert_no_heap(|| harness.host.client.resync()).unwrap();
        harness.run(2, 1);
        let mut connections: Vec<_> = harness
            .states
            .iter()
            .filter_map(|record| match record {
                StateRecord::Connection {
                    port: EnginePort::Bus(BusPort { index, .. }),
                    host,
                } => Some((*index, *host)),
                _ => None,
            })
            .collect();
        connections.sort();
        connections
    };
    assert_eq!(wired(&mut harness), [(0, audio_out(0)), (1, audio_out(1))]);
    let bus = |index, host, connected| Request::Connect {
        port: master(index),
        host,
        connected,
    };
    let from = harness.outputs[0].len();
    assert_eq!(
        harness.apply_in(bus(0, audio_out(0), false), 3),
        Outcome::Done
    );
    harness.run(6, 3);
    ramped(
        &harness.outputs[0],
        from,
        |_| 1.,
        |_, step| 1. - (step + 1) as f32 / RAMP as f32,
        |_| 0.,
    );
    assert!(harness.outputs[1][from..].iter().all(|s| *s == 1.));
    let from = harness.outputs[1].len();
    assert_eq!(
        harness.apply_in(bus(0, audio_out(1), true), 3),
        Outcome::Done
    );
    harness.run(6, 3);
    ramped(
        &harness.outputs[1],
        from,
        |_| 1.,
        |_, step| 1. + (step + 1) as f32 / RAMP as f32,
        |_| 2.,
    );
    assert_eq!(harness.apply(bus(0, audio_out(1), true)), Outcome::Done);
    assert_eq!(
        harness.apply(bus(2, audio_out(0), true)),
        Outcome::Refused(Refusal::InvalidValue)
    );
    assert_eq!(
        harness.apply(bus(0, audio_in(0), true)),
        Outcome::Refused(Refusal::InvalidId)
    );
    assert_eq!(wired(&mut harness), [(0, audio_out(1)), (1, audio_out(1))]);
}

#[shoop_test]
fn smoothing_applies_to_current_and_later_loops_within_the_limit() {
    let mut harness = Harness::new(&description());
    harness.input = Box::new(|port, _| if port == 0 { 1. } else { 0. });
    let track = strip(0, 1);
    let (first, second) = (loop_id(0, 1), loop_id(1, 1));
    harness.done(Request::CreateStrip {
        strip: track,
        shape: shape(1, 1, false),
    });
    harness.done(connect(track, PortKind::AudioIn, audio_in(0), true));
    harness.done(connect(track, PortKind::AudioOut, audio_out(0), true));
    harness.done(Request::CreateLoop {
        loop_id: first,
        strip: track,
    });
    harness.done(transition(first, LoopMode::Recording));
    harness.run(5, 5);
    harness.done(transition(first, LoopMode::Stopped));
    assert_eq!(
        harness.apply(Request::SetSmoothing { frames: 9 }),
        Outcome::Refused(Refusal::InvalidValue)
    );
    harness.done(Request::SetSmoothing { frames: 4 });
    let ramp = [0.25, 0.5, 0.75, 1., 1.];
    let start = harness.frame() as usize;
    harness.done(transition(first, LoopMode::Playing));
    harness.run(5, 2);
    assert_eq!(&harness.outputs[0][start..start + 5], &ramp);
    harness.done(transition(first, LoopMode::Stopped));

    // A loop created afterwards uses the current smoothing.
    harness.done(Request::CreateLoop {
        loop_id: second,
        strip: track,
    });
    harness.done(transition(second, LoopMode::Recording));
    harness.run(5, 5);
    harness.done(transition(second, LoopMode::Stopped));
    let start = harness.frame() as usize;
    harness.done(transition(second, LoopMode::Playing));
    harness.run(5, 2);
    assert_eq!(&harness.outputs[0][start..start + 5], &ramp);
}

#[shoop_test]
fn routes_connect_single_strip_outputs_to_single_master_channels() {
    let mut harness = Harness::new(&description());
    harness.input = Box::new(|port, _| if port == 0 { 1. } else { 0.5 });
    let (stereo, mono) = (strip(0, 1), strip(1, 1));
    harness.done(Request::CreateStrip {
        strip: stereo,
        shape: shape(2, 2, false),
    });
    harness.done(Request::CreateStrip {
        strip: mono,
        shape: shape(1, 1, false),
    });
    for (index, host) in [audio_in(0), audio_in(1)].into_iter().enumerate() {
        harness.done(Request::Connect {
            port: port(stereo, PortKind::AudioIn, index as u8),
            host,
            connected: true,
        });
    }
    for track in [stereo, mono] {
        harness.done(Request::StripControl {
            strip: track,
            control: StripControl::Monitoring(true),
        });
    }
    harness.done(route(stereo, 0, 0, true));
    harness.done(route(stereo, 1, 1, true));
    harness.run(4, 2);
    assert_eq!(harness.outputs[0].last(), Some(&1.));
    assert_eq!(harness.outputs[1].last(), Some(&0.5));
    // Removing one channel's route leaves the other.
    let from = harness.outputs[0].len();
    assert_eq!(
        harness.apply_in(route(stereo, 0, 0, false), 3),
        Outcome::Done
    );
    harness.run(6, 3);
    ramped(
        &harness.outputs[0],
        from,
        |_| 1.,
        |_, step| 1. - (step + 1) as f32 / RAMP as f32,
        |_| 0.,
    );
    assert!(harness.outputs[1][from..].iter().all(|s| *s == 0.5));
    // A mono strip routed to the left channel only.
    harness.done(connect(mono, PortKind::AudioIn, audio_in(0), true));
    harness.done(route(mono, 0, 0, true));
    harness.run(6, 3);
    assert_eq!(harness.outputs[0].last(), Some(&1.));
    assert_eq!(harness.outputs[1].last(), Some(&0.5));
    assert_eq!(harness.apply(route(mono, 0, 0, true)), Outcome::Done);
    for refused in [route(mono, 1, 0, true), route(mono, 0, 2, true)] {
        assert_eq!(
            harness.apply(refused),
            Outcome::Refused(Refusal::InvalidValue)
        );
    }
    assert_eq!(
        harness.apply(Request::Route {
            port: StripPort {
                strip: mono,
                kind: PortKind::MidiOut,
                index: 0,
            },
            bus: BusPort {
                bus: BusId::Master,
                index: 0,
            },
            connected: true,
        }),
        Outcome::Refused(Refusal::InvalidValue)
    );
    harness.states.clear();
    assert_no_heap(|| harness.host.client.resync()).unwrap();
    harness.run(2, 1);
    let mut routes: Vec<_> = harness
        .states
        .iter()
        .filter_map(|record| match record {
            StateRecord::Route { port, bus } => Some((port.strip, port.index, bus.index)),
            _ => None,
        })
        .collect();
    routes.sort();
    assert_eq!(routes, [(stereo, 1, 1), (mono, 0, 0)]);
    assert_eq!(harness.host.client.limits().unwrap().master_channels, 2);
}
