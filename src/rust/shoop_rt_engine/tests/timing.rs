//! The legacy timing reference recorded in the plan's baseline: 1 kHz, one-frame
//! callbacks and an input ramp of `frame / 1000` connected to both strips.
//! Frames are relative to the moment the configured engine is idle. A request
//! made "at" a frame is sent after that frame's callback, as the legacy
//! snapshot was observed.

mod support;

use shoop_rt_protocol::*;
use shoop_wasm_test_support::shoop_test;
use std::collections::BTreeMap;
use support::*;

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

#[derive(Clone, Copy, Debug, PartialEq)]
struct View {
    mode: LoopMode,
    next: Option<(LoopMode, u32)>,
    length: u64,
}

/// The loop's settled view at each rendered frame.
fn views(harness: &Harness, id: LoopSlotId, origin: u64) -> BTreeMap<u64, View> {
    let mut views = BTreeMap::new();
    for (frame, record) in &harness.telemetry {
        if let Telemetry::Loop {
            loop_id,
            mode,
            next_mode,
            next_delay,
            length,
            ..
        } = record
        {
            if *loop_id == id && *frame > origin {
                views.insert(
                    frame - 1 - origin,
                    View {
                        mode: *mode,
                        next: next_mode.zip(*next_delay),
                        length: *length,
                    },
                );
            }
        }
    }
    views
}
fn next_of(harness: &Harness, id: LoopSlotId) -> Option<(LoopMode, u32)> {
    match harness.last_loop(id) {
        Some((
            _,
            Telemetry::Loop {
                next_mode,
                next_delay,
                ..
            },
        )) => next_mode.zip(next_delay),
        _ => panic!("no telemetry"),
    }
}
fn transition(id: LoopSlotId, mode: LoopMode, delay: Option<u32>) -> Request {
    Request::LoopTransition {
        loop_id: id,
        mode,
        delay,
    }
}
fn modes(views: &BTreeMap<u64, View>, range: std::ops::Range<u64>) -> Vec<LoopMode> {
    range.map(|frame| views[&frame].mode).collect()
}

#[shoop_test]
fn synchronized_transitions_match_the_legacy_reference() {
    let mut harness = Harness::new(&description());
    let (sync_strip, track) = (strip(0, 1), strip(1, 1));
    let (sync, a, b) = (loop_id(0, 1), loop_id(1, 1), loop_id(2, 1));
    for (index, id) in [sync_strip, track].into_iter().enumerate() {
        harness.done(Request::CreateStrip {
            strip: id,
            shape: shape(1, 1, false),
        });
        harness.done(Request::Connect {
            port: port(id, PortKind::AudioIn, 0),
            host: audio_in(0),
            connected: true,
        });
        harness.done(Request::Connect {
            port: port(id, PortKind::AudioOut, 0),
            host: audio_out(index as u32),
            connected: true,
        });
    }
    for (id, owner) in [(sync, sync_strip), (a, track), (b, track)] {
        harness.done(Request::CreateLoop {
            loop_id: id,
            strip: owner,
        });
    }
    harness.done(Request::LoopSync {
        loop_id: a,
        source: Some(sync),
    });
    let origin = harness.frame();
    harness.input = Box::new(move |port, frame| {
        if port == 0 && frame >= origin {
            (frame - origin) as f32 / 1000.
        } else {
            0.
        }
    });
    let until = |harness: &mut Harness, frame: u64| {
        while harness.frame() < origin + frame {
            harness.run(1, 1);
        }
    };
    until(&mut harness, 3);
    harness.send(transition(sync, LoopMode::Recording, None));
    until(&mut harness, 19);
    harness.send(transition(sync, LoopMode::Playing, None));
    until(&mut harness, 21);
    let record = harness.send(transition(a, LoopMode::Recording, Some(1)));
    assert_eq!(next_of(&harness, a), Some((LoopMode::Recording, 1)));
    let unsynced = harness.send(transition(b, LoopMode::Recording, Some(2)));
    until(&mut harness, 36);
    assert_eq!(next_of(&harness, a), Some((LoopMode::Recording, 0)));
    until(&mut harness, 52);
    let play = harness.send(transition(a, LoopMode::Playing, Some(2)));
    assert_eq!(next_of(&harness, a), Some((LoopMode::Playing, 2)));
    until(&mut harness, 100);
    let far = harness.send(transition(a, LoopMode::Stopped, Some(3)));
    assert_eq!(next_of(&harness, a), Some((LoopMode::Stopped, 3)));
    let near = harness.send(transition(a, LoopMode::Stopped, Some(1)));
    assert_eq!(next_of(&harness, a), Some((LoopMode::Stopped, 1)));
    until(&mut harness, 131 + 3 * 16 + 1);
    for correlation in [record, unsynced, play, far, near] {
        assert_eq!(harness.results[&correlation], Outcome::Done);
    }

    let views_sync = views(&harness, sync, origin);
    assert_eq!(modes(&views_sync, 0..3), [LoopMode::Stopped; 3]);
    assert_eq!(modes(&views_sync, 3..19), [LoopMode::Recording; 16]);
    assert_eq!(modes(&views_sync, 19..180), [LoopMode::Playing; 161]);
    assert_eq!(views_sync[&40].length, 16);

    let views_a = views(&harness, a, origin);
    assert_eq!(modes(&views_a, 0..51), [LoopMode::Stopped; 51]);
    assert_eq!(modes(&views_a, 51..99), [LoopMode::Recording; 48]);
    assert_eq!(modes(&views_a, 99..131), [LoopMode::Playing; 32]);
    assert_eq!(modes(&views_a, 131..180), [LoopMode::Stopped; 49]);
    assert_eq!(views_a[&99].length, 48);
    assert_eq!(views_a[&40].next, Some((LoopMode::Recording, 0)));
    assert_eq!(views_a[&60].next, Some((LoopMode::Playing, 2)));
    assert_eq!(views_a[&70].next, Some((LoopMode::Playing, 1)));
    assert_eq!(views_a[&110].next, Some((LoopMode::Stopped, 1)));
    assert_eq!(views_a[&120].next, Some((LoopMode::Stopped, 0)));
    assert_eq!(views_a[&131].next, None);

    let views_b = views(&harness, b, origin);
    assert_eq!(modes(&views_b, 0..21), [LoopMode::Stopped; 21]);
    assert_eq!(views_b[&21].mode, LoopMode::Recording);

    let expected = |first: u64, length: u64| -> Vec<f32> {
        (first..first + length)
            .map(|frame| frame as f32 / 1000.)
            .collect()
    };
    let (total, samples, _) = harness.read(sync, BulkSelector::Audio { channel: 0 });
    assert_eq!((total, samples), (16, expected(3, 16)));
    let (total, samples, _) = harness.read(a, BulkSelector::Audio { channel: 0 });
    assert_eq!((total, samples), (48, expected(51, 48)));
    let played = &harness.outputs[0][(origin + 19) as usize..(origin + 179) as usize];
    for (offset, sample) in played.iter().enumerate() {
        assert_eq!(*sample, (3 + offset as u64 % 16) as f32 / 1000.);
    }
    let played = &harness.outputs[1][(origin + 99) as usize..(origin + 131) as usize];
    assert_eq!(played, &expected(51, 32)[..]);
    assert!(
        harness.outputs[1][(origin + 131) as usize..(origin + 180) as usize]
            .iter()
            .all(|sample| *sample == 0.)
    );
}

#[shoop_test]
fn delays_of_an_unsynchronized_playing_loop_count_its_own_wraps() {
    let mut harness = Harness::new(&description());
    let track = strip(0, 1);
    let looped = loop_id(0, 1);
    harness.done(Request::CreateStrip {
        strip: track,
        shape: shape(1, 1, false),
    });
    harness.done(Request::CreateLoop {
        loop_id: looped,
        strip: track,
    });
    let origin = harness.frame();
    harness.send(transition(looped, LoopMode::Recording, None));
    harness.run(10, 1);
    harness.send(transition(looped, LoopMode::Playing, None));
    harness.run(13, 1);
    // Playing from 10 with length 10: wraps at 20, 30, 40.
    harness.send(transition(looped, LoopMode::Stopped, Some(1)));
    assert_eq!(next_of(&harness, looped), Some((LoopMode::Stopped, 1)));
    harness.run(30, 1);
    let views = views(&harness, looped, origin);
    assert_eq!(modes(&views, 10..40), [LoopMode::Playing; 30]);
    assert_eq!(modes(&views, 40..53), [LoopMode::Stopped; 13]);
    assert_eq!(views[&25].next, Some((LoopMode::Stopped, 1)));
    assert_eq!(views[&35].next, Some((LoopMode::Stopped, 0)));
}
