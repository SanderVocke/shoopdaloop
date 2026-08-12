use anyhow::Result;
use shoop_engine::carla_native::CarlaNativeHost;
use shoop_engine::carla_processor::CarlaProcessor;
use shoop_engine::FXChainType;
use std::hint::black_box;
use std::time::Instant;

const SAMPLE_RATE: u32 = 48_000;
const WARMUP_BLOCKS: usize = 100;
const MEASURED_BLOCKS: usize = 2_000;

fn benchmark_blocks(default: usize) -> usize {
    if std::env::var_os("SHOOP_BENCHMARK_SMOKE").is_some() {
        default.min(3)
    } else {
        default
    }
}
const BUFFER_SIZES: [u32; 6] = [32, 64, 128, 256, 512, 1024];

fn benchmark(chain_type: FXChainType, frames: u32) -> Result<()> {
    let mut host = CarlaNativeHost::instantiate(chain_type, SAMPLE_RATE, frames)?;
    host.set_active(true);
    for channel in 0..host.info().audio_inputs {
        let input = host
            .audio_input_mut(channel)
            .expect("discovered audio input must have storage");
        for (index, sample) in input[..frames as usize].iter_mut().enumerate() {
            *sample = ((index + channel) as f32 * 0.01).sin();
        }
    }
    for _ in 0..benchmark_blocks(WARMUP_BLOCKS) {
        host.process(frames as usize)?;
    }

    let started = Instant::now();
    let measured_blocks = benchmark_blocks(MEASURED_BLOCKS);
    for _ in 0..measured_blocks {
        black_box(host.process(frames as usize)?);
    }
    let elapsed = started.elapsed();
    let block_ns = elapsed.as_nanos() as f64 / measured_blocks as f64;
    let budget_ns = frames as f64 * 1_000_000_000.0 / SAMPLE_RATE as f64;
    println!(
        "chain={chain_type:?} channels={} frames={frames} mean_us={:.3} budget_percent={:.3}",
        host.info().audio_inputs,
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
