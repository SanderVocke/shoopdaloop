use rt_profiling::*;
use std::time::Instant;

fn main() {
    let mut collector = Collector::default();
    collector.status(Status {
        generation: 1,
        available: true,
        ..Status::default()
    });
    let nodes = 128;
    let ticks = MAX_CAPTURE_TRACES / 2;
    for tick in 0..ticks {
        let start = 1_000_000 + tick as u64 * 20_000;
        let samples: Vec<_> = core::iter::once(Sample {
            key: PHASE | Phase::GraphStart as u64,
            at: start,
        })
        .chain((0..nodes).map(|key| Sample {
            key,
            at: start + (key + 1) * 100,
        }))
        .collect();
        for (i, chunk) in samples.chunks(CHUNK_SAMPLES).enumerate() {
            let mut packet = Packet {
                generation: 1,
                capture: 7,
                sequence: tick as u64 * 2 + 1,
                frame: tick as u64 * 32,
                epoch: 1,
                frames: 32,
                start_ns: start,
                end_ns: start + 12_800,
                index: (i * CHUNK_SAMPLES) as u32,
                total: samples.len() as u32,
                len: chunk.len() as u8,
                ..Packet::default()
            };
            packet.samples[..chunk.len()].copy_from_slice(chunk);
            collector.receive(packet);
        }
        collector.receive(Packet {
            generation: 1,
            capture: 7,
            sequence: tick as u64 * 2 + 2,
            frame: tick as u64 * 32,
            epoch: 1,
            frames: 32,
            start_ns: start - 50,
            end_ns: start + 12_850,
            budget_ns: 666_667,
            flags: CALLBACK | HOST | if tick + 1 == ticks { CAPTURE_END } else { 0 },
            ..Packet::default()
        });
    }
    let mut times = Vec::new();
    for i in 0..256 {
        let tick = i * (ticks / 256);
        let started = Instant::now();
        assert!(collector.select(CaptureRequest {
            connection: 0,
            generation: 1,
            capture: 7,
            callback: tick as u64 * 2 + 2,
            tick: tick as u64 * 2 + 1,
            offset: 0
        }));
        assert_eq!(
            collector.view().detail.as_ref().unwrap().nodes.len(),
            nodes as usize
        );
        times.push(started.elapsed().as_nanos() as u64);
    }
    let retained_bytes = collector.raw_capture().len() * core::mem::size_of::<Packet>();
    println!("{{\"synthetic_capture\":true,\"traces\":{},\"raw_bytes\":{retained_bytes},\"nodes_per_tick\":{nodes},\"detail_query_ns\":{times:?}}}", collector.view().index.len());
}
