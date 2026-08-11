use anyhow::Result;
use shoop_engine::lv2_carla::CarlaLv2Host;
use shoop_engine::FXChainType;
use std::hint::black_box;
use std::time::Instant;

const SAMPLE_RATE: u32 = 48_000;
const WARMUP_BLOCKS: usize = 100;
const MEASURED_BLOCKS: usize = 2_000;
const BUFFER_SIZES: [u32; 6] = [32, 64, 128, 256, 512, 1024];

fn benchmark(chain_type: FXChainType, frames: u32) -> Result<()> {
    let mut host = CarlaLv2Host::instantiate(chain_type, SAMPLE_RATE, frames)?;
    host.set_active(true);
    for channel in 0..host.info.ports.audio_inputs.len() {
        let input = host
            .audio_input_mut(channel)
            .expect("discovered audio input must have storage");
        for (index, sample) in input[..frames as usize].iter_mut().enumerate() {
            *sample = ((index + channel) as f32 * 0.01).sin();
        }
    }
    for _ in 0..WARMUP_BLOCKS {
        host.process(frames as usize)?;
    }

    let started = Instant::now();
    for _ in 0..MEASURED_BLOCKS {
        black_box(host.process(frames as usize)?);
    }
    let elapsed = started.elapsed();
    let block_ns = elapsed.as_nanos() as f64 / MEASURED_BLOCKS as f64;
    let budget_ns = frames as f64 * 1_000_000_000.0 / SAMPLE_RATE as f64;
    println!(
        "chain={chain_type:?} channels={} frames={frames} mean_us={:.3} budget_percent={:.3}",
        host.info.ports.audio_inputs.len(),
        block_ns / 1_000.0,
        block_ns * 100.0 / budget_ns,
    );
    Ok(())
}

fn main() -> Result<()> {
    for chain_type in [FXChainType::CarlaRack, FXChainType::CarlaPatchbay16x] {
        for frames in BUFFER_SIZES {
            benchmark(chain_type, frames)?;
        }
    }
    Ok(())
}
