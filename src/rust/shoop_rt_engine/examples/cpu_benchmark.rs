//! Release-build callback/allocator comparison through the real engine endpoint.
#[path = "../tests/support/mod.rs"]
mod support;

use shoop_rt_engine::{AudioRenderMode, Segment};
use shoop_rt_protocol::*;
use shoop_wasm_test_support::heap::{measure, Counts};
use std::time::Instant;
use support::*;

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

fn transition(index: usize, mode: LoopMode) -> Request {
    Request::LoopTransition {
        loop_id: loop_id(index as u32, 1),
        mode,
        delay: None,
    }
}

fn main() {
    let mut mode = AudioRenderMode::Current;
    let mut voices = 4usize;
    let mut callbacks = 1024usize;
    let mut phase = String::from("play");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let value = args.next().expect("flags require values");
        match arg.as_str() {
            "--mode" => mode = AudioRenderMode::parse(&value).expect("audio mode"),
            "--voices" => voices = value.parse().expect("voice count"),
            "--callbacks" => callbacks = value.parse().expect("callback count"),
            "--phase" => phase = value,
            _ => panic!("usage: --mode current --voices N --callbacks N --phase play|record-play|record|clear"),
        }
    }
    assert!(matches!(mode, AudioRenderMode::Current));
    assert!((1..=16).contains(&voices) && (1..=4096).contains(&callbacks));
    let mut d = audio_description(mode);
    d.sample_rate = 48_000;
    d.quantum = 32;
    d.max_frames = 128;
    d.history.seconds = 0;
    d.smoothing_frames = 0;
    d.ramp_frames = 0;
    d.resource_pages = 65_536;
    d.page_bytes = 1024;
    d.segments = vec![Segment {
        strips: 1,
        loops: voices as u32,
    }];
    let mut h = Harness::new(&d);
    let track = strip(0, 1);
    h.input = Box::new(|port, frame| (((frame * 7 + port as u64) % 128) as f32 - 64.) / 256.);
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
    for index in 0..voices {
        h.done(Request::CreateLoop {
            loop_id: loop_id(index as u32, 1),
            strip: track,
        });
        h.done(transition(index, LoopMode::Recording));
    }
    h.run(2048, 128);
    for index in 0..voices {
        h.done(transition(index, LoopMode::Playing));
    }
    h.run(512, 128);
    let mut pending = Vec::new();
    match phase.as_str() {
        "play" => {}
        "record-play" => h.done(transition(0, LoopMode::Recording)),
        "record" => {
            for index in 0..voices {
                h.done(transition(index, LoopMode::Recording));
            }
        }
        "clear" => {
            for index in 0..voices {
                pending.push(h.send(Request::LoopClear {
                    loop_id: loop_id(index as u32, 1),
                }));
            }
        }
        _ => panic!("unknown phase"),
    }
    let mut rows = Vec::with_capacity(callbacks);
    for callback in 0..callbacks {
        let start = h.frame();
        for channel in 0..2 {
            for (offset, sample) in h.host.input_mut(channel)[..128].iter_mut().enumerate() {
                *sample = (((start + offset as u64 + channel as u64) % 128) as f32 - 64.) / 256.;
            }
        }
        let started = Instant::now();
        let (result, heap) = measure(|| h.host.process(128));
        let elapsed = started.elapsed().as_nanos();
        result.expect("callback failed");
        if phase == "play" || mode == AudioRenderMode::Current {
            assert_eq!(
                heap,
                Counts::default(),
                "steady playback/native callback allocated"
            );
        }
        rows.push((callback, elapsed, heap));
        h.pump();
    }
    for request in pending {
        assert_eq!(h.results.get(&request), Some(&Outcome::Done));
    }
    println!("mode,phase,voices,callback,elapsed_ns,allocations,reallocations,deallocations");
    for (callback, ns, heap) in rows {
        println!(
            "{},{},{},{},{},{},{},{}",
            mode.name(),
            phase,
            voices,
            callback,
            ns,
            heap.allocations,
            heap.reallocations,
            heap.deallocations
        );
    }
}
