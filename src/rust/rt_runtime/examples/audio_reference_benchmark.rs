//! Short, single-threaded screen of the actual reference visitor (not a callback benchmark).
use rt_runtime::audio_refs::{GainEnvelope, RenderArena};
use rt_runtime::resource::ResourceStore;
use rt_runtime::{Config, Quantum};
use std::{hint::black_box, time::Instant};

fn main() {
    let store = ResourceStore::new(
        Config::default().prepare().unwrap(),
        Quantum::new(32).unwrap(),
    )
    .unwrap();
    println!("frames,gain,operation,ns_per_quantum");
    for frames in [1, 32, 256] {
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
            let mut arena = RenderArena::prepare(frames * 2, 0);
            let source = arena.allocate(frames).unwrap();
            for (i, sample) in arena.output(source, frames).iter_mut().enumerate() {
                *sample = ((i * 7 % 101) as f32 - 50.) / 100.;
            }
            let sequence = arena.transient(source, frames).scaled(gain);
            let target = arena.allocate(frames).unwrap();
            let mut output = vec![0.; frames];
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
                            .accumulate(black_box(sequence), target, frames, black_box(&store))
                            .unwrap(),
                    }
                }
                println!(
                    "{frames},{name},{operation},{:.3}",
                    started.elapsed().as_nanos() as f64 / iterations as f64
                );
                black_box(&arena);
            }
        }
    }
}
