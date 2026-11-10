//! Single-threaded screen of persistent interleaved reference reads and meters.
use rt_runtime::audio_refs::{GainEnvelope, RenderArena};
use rt_runtime::resource::{shared_audio, ResourceStore};
use rt_runtime::{AudioRenderMode, Config, Frame, Quantum};
use std::{hint::black_box, time::Instant};

fn main() {
    println!("frames,channels,channel,gain,operation,ns_per_quantum");
    for frames in [1, 32, 128] {
        for channels in [1, 2, 3] {
            let mut store = ResourceStore::new(
                Config {
                    max_frames: 256,
                    ..Config::default()
                }
                .prepare()
                .unwrap(),
                Quantum::new(frames).unwrap(),
            )
            .unwrap();
            store.set_audio_render_mode(AudioRenderMode::Compact);
            let pool = store
                .prepare_audio_store(
                    shared_audio::Config {
                        channels: channels as u32,
                        history_frames: frames * 2,
                        views: 4,
                        pages: 32,
                    },
                    Frame(0),
                )
                .unwrap();
            let history = store.audio_history(pool, 0).unwrap();
            let input: Vec<f32> = (0..frames * channels)
                .map(|i| ((i * 7 % 101) as f32 - 50.) / 100.)
                .collect();
            store.begin_callback();
            store
                .render_io()
                .append_audio_source(history, Frame(0), &input)
                .unwrap();
            for channel in 0..channels {
                for (name, gain) in [
                    ("unity", GainEnvelope::constant(1.)),
                    ("constant", GainEnvelope::constant(0.7)),
                    (
                        "ramp",
                        GainEnvelope {
                            start: 0.3,
                            end: 0.9,
                        },
                    ),
                ] {
                    let sequence = store
                        .audio_history_sequence(history, Frame(0), channel)
                        .unwrap()
                        .unwrap()
                        .scaled(gain);
                    let mut arena = RenderArena::prepare(frames, 0);
                    let target = arena.allocate(frames).unwrap();
                    let mut output = vec![0.; frames];
                    arena
                        .read_into(sequence, &mut output, &store, false)
                        .unwrap();
                    let step = if frames > 1 {
                        (gain.end - gain.start) / (frames - 1) as f32
                    } else {
                        0.
                    };
                    let mut value = gain.start;
                    let mut expected_peak = 0u32;
                    for (frame, sample) in output.iter().enumerate() {
                        if frame + 1 == frames && frames > 1 {
                            value = gain.end;
                        }
                        let expected = input[frame * channels + channel] * value;
                        assert_eq!(sample.to_bits(), expected.to_bits());
                        expected_peak = expected_peak.max(expected.to_bits() & 0x7fff_ffff);
                        value += step;
                    }
                    assert_eq!(
                        arena.peak(sequence, frames, &store).unwrap().to_bits(),
                        expected_peak
                    );
                    let iterations = 20_000;
                    for operation in ["read", "peak", "accumulate"] {
                        let started = Instant::now();
                        for _ in 0..iterations {
                            match operation {
                                "read" => arena
                                    .read_into(
                                        black_box(sequence),
                                        black_box(&mut output),
                                        black_box(&store),
                                        false,
                                    )
                                    .unwrap(),
                                "peak" => {
                                    black_box(
                                        arena
                                            .peak(black_box(sequence), frames, black_box(&store))
                                            .unwrap(),
                                    );
                                }
                                _ => arena
                                    .accumulate(
                                        black_box(sequence),
                                        target,
                                        frames,
                                        black_box(&store),
                                    )
                                    .unwrap(),
                            }
                        }
                        println!(
                            "{frames},{channels},{channel},{name},{operation},{:.3}",
                            started.elapsed().as_nanos() as f64 / iterations as f64
                        );
                        black_box(&arena);
                    }
                }
            }
        }
    }
}
