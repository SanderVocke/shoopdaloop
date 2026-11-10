//! Deterministic, single-threaded callback screen; setup and collection are untimed.
#[path = "../tests/support/mod.rs"]
mod support;
use shoop_rt_engine::{AudioRenderMode, Segment};
use shoop_rt_protocol::*;
use std::{hint::black_box, time::Instant};
use support::*;

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert!(
        [6, 7].contains(&args.len()),
        "mode idle|play|record buffer tracks callbacks [history_prefill_frames]"
    );
    let mode = match args[1].as_str() {
        "current" => AudioRenderMode::Current,
        "chunks" => AudioRenderMode::Chunks,
        "chunks-descriptors" => AudioRenderMode::ChunksDescriptors,
        "compact" => AudioRenderMode::Compact,
        _ => panic!("unknown renderer"),
    };
    let scenario = args[2].as_str();
    assert!(matches!(scenario, "idle" | "play" | "record"));
    let buffer: usize = args[3].parse().unwrap();
    let tracks: u32 = args[4].parse().unwrap();
    let callbacks: usize = args[5].parse().unwrap();
    let prefill: usize = args.get(6).map(|value| value.parse().unwrap()).unwrap_or(0);
    assert_eq!(prefill % 32, 0);
    assert!(tracks > 0 && callbacks > 0 && [32, 256].contains(&buffer));
    let mut d = audio_description(mode);
    d.sample_rate = 48000;
    d.quantum = 32;
    d.max_frames = 256;
    d.history.seconds = 1;
    d.history.audio_take_pages = 8192;
    d.resource_pages = 16384;
    d.page_bytes = 4096;
    d.pending_transitions = 128;
    d.telemetry_interval_frames = 4800;
    d.segments = vec![Segment {
        strips: tracks,
        loops: tracks * 8,
    }];
    let mut h = Harness::new(&d);
    h.input = Box::new(|port, frame| ((frame * 7 + port as u64 * 11) % 101) as f32 / 200. - 0.2);
    for track in 0..tracks {
        let id = strip(track, 1);
        h.done(Request::CreateStrip {
            strip: id,
            shape: shape(2, 2, false),
        });
        for channel in 0..2 {
            h.done(Request::Connect {
                port: port(id, PortKind::AudioIn, channel),
                host: audio_in(channel as u32),
                connected: true,
            });
            h.done(route(id, channel, channel, true));
        }
        for cell in 0..8 {
            h.done(Request::CreateLoop {
                loop_id: loop_id(track * 8 + cell, 1),
                strip: id,
            });
        }
    }
    h.run(prefill, buffer);
    if scenario != "idle" {
        for track in 0..tracks {
            for cell in 0..4 {
                h.done(Request::LoopTransition {
                    loop_id: loop_id(track * 8 + cell, 1),
                    mode: LoopMode::Recording,
                    delay: None,
                });
            }
        }
        h.run(9600, 256);
        if scenario == "play" {
            for track in 0..tracks {
                for cell in 0..4 {
                    h.done(Request::LoopTransition {
                        loop_id: loop_id(track * 8 + cell, 1),
                        mode: LoopMode::Playing,
                        delay: None,
                    });
                }
            }
        }
    }
    h.run(4096, buffer);
    let mut times = Vec::with_capacity(callbacks);
    let mut checksum = 0f64;
    let mut arena_materializations = 0u64;
    for _ in 0..callbacks {
        let first = h.frame();
        for channel in 0..2 {
            for (i, sample) in h.host.input_mut(channel)[..buffer].iter_mut().enumerate() {
                *sample =
                    (((first + i as u64) * 7 + channel as u64 * 11) % 101) as f32 / 200. - 0.2;
            }
        }
        let start = Instant::now();
        let report = black_box(h.host.process(buffer).unwrap());
        times.push(start.elapsed().as_nanos());
        arena_materializations += h.host.render.audio_graph_work().materialized;
        assert_eq!(report.phase, shoop_rt_engine::Phase::Running);
        for channel in 0..2 {
            for &sample in &h.host.output(channel)[..buffer] {
                assert!(sample.is_finite());
                checksum += f64::from(sample) * f64::from(sample);
            }
        }
        h.pump();
    }
    if scenario == "play" {
        assert!(checksum > 0., "playback produced silence");
    }
    if scenario != "idle" {
        h.run(4800, buffer);
        for track in 0..tracks {
            for cell in 0..4 {
                let (_, status) = h
                    .last_loop(loop_id(track * 8 + cell, 1))
                    .expect("missing loop telemetry");
                let Telemetry::Loop {
                    mode,
                    length,
                    capture_incomplete,
                    ..
                } = status
                else {
                    unreachable!()
                };
                assert_eq!(
                    mode,
                    if scenario == "play" {
                        LoopMode::Playing
                    } else {
                        LoopMode::Recording
                    },
                    "track={track} cell={cell} status={status:?}"
                );
                assert!(
                    length > 0 && !capture_incomplete,
                    "benchmark capture failed"
                );
            }
        }
    }
    times.sort_unstable();
    println!("mode,scenario,buffer,tracks,callbacks,p50_ns,p99_ns,max_ns,checksum,arena_materializations");
    println!(
        "{},{scenario},{buffer},{tracks},{callbacks},{},{},{},{checksum:.9},{arena_materializations}",
        args[1],
        times[times.len() / 2],
        times[(times.len() * 99).div_ceil(100) - 1],
        times[times.len() - 1]
    );
}
