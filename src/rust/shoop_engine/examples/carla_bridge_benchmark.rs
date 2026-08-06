use anyhow::{Context, Result};
use shoop_engine::carla_processor::{spawn_processor_bridge, CarlaProcessor, CarlaProcessorInfo};
use shoop_engine::carla_subprocess::{SubprocessCarlaProcessor, SupervisedCarlaProcessor};
use shoop_engine::lv2_carla::CarlaLv2Host;
use shoop_engine::FXChainType;
use shoop_plugin_protocol::{ChainId, ProcessGeneration};
use std::path::Path;
use std::time::Instant;

const SAMPLE_RATE: u32 = 48_000;
const WARMUP: usize = 100;
const ITERATIONS: usize = 500;

fn percentile(sorted: &[f64], percentile: f64) -> f64 {
    let index = ((sorted.len() - 1) as f64 * percentile).round() as usize;
    sorted[index]
}

fn measure(
    mode: &str,
    host: Box<dyn CarlaProcessor>,
    frames: usize,
) -> Result<(CarlaProcessorInfo, Vec<f64>, u64)> {
    let (control, mut endpoint) = spawn_processor_bridge(host, SAMPLE_RATE, frames as u32)?;
    let info = control.info();
    control.set_active(true);
    for channel in 0..info.audio_inputs {
        endpoint
            .audio_input_mut(channel)
            .context("missing benchmark audio input")?[..frames]
            .fill(0.125);
    }
    for _ in 0..WARMUP {
        endpoint.process(frames)?;
    }
    let misses_before = control.deadline_misses();
    let mut elapsed = Vec::with_capacity(ITERATIONS);
    let period = std::time::Duration::from_secs_f64(frames as f64 / SAMPLE_RATE as f64);
    for _ in 0..ITERATIONS {
        let started = Instant::now();
        endpoint.process(frames)?;
        let processing = started.elapsed();
        elapsed.push(processing.as_secs_f64() * 1_000_000.0);
        if let Some(idle) = period.checked_sub(processing) {
            std::thread::sleep(idle);
        }
    }
    elapsed.sort_by(f64::total_cmp);
    let misses = control.deadline_misses().saturating_sub(misses_before);
    eprintln!("completed {mode} {}ch/{frames}", info.audio_inputs);
    Ok((info, elapsed, misses))
}

fn print_row(mode: &str, chain: &str, frames: usize, values: &[f64], misses: u64) {
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    println!(
        "{mode},{chain},{frames},{mean:.3},{:.3},{:.3},{:.3},{:.3},{misses}",
        percentile(values, 0.50),
        percentile(values, 0.95),
        percentile(values, 0.99),
        values[values.len() - 1],
    );
}

fn main() -> Result<()> {
    let worker = std::env::args_os().nth(1).context(
        "usage: carla_bridge_benchmark <installed-shoopdaloop-executable> [all|direct|subprocess|reference]",
    )?;
    let worker = Path::new(&worker);
    let selected_mode = std::env::args().nth(2).unwrap_or_else(|| "all".to_owned());
    if !matches!(
        selected_mode.as_str(),
        "all" | "direct" | "subprocess" | "reference"
    ) {
        anyhow::bail!("benchmark mode must be all, direct, subprocess, or reference");
    }
    println!("mode,chain,frames,mean_us,p50_us,p95_us,p99_us,max_us,deadline_misses");
    let mut chain_id = 10_000_u64;
    for (chain_name, chain_type) in [
        ("rack_2ch", FXChainType::CarlaRack),
        ("patchbay_16ch", FXChainType::CarlaPatchbay16x),
    ] {
        for frames in [32, 64, 128, 256, 512, 1024] {
            if matches!(selected_mode.as_str(), "all" | "direct") {
                let direct = CarlaLv2Host::instantiate(chain_type, SAMPLE_RATE, frames as u32)
                    .with_context(|| format!("could not instantiate direct {chain_name}"))?;
                let (_, values, misses) = measure("direct", Box::new(direct), frames)?;
                print_row("direct", chain_name, frames, &values, misses);
            }

            if matches!(selected_mode.as_str(), "all" | "subprocess") {
                chain_id += 1;
                let subprocess = SupervisedCarlaProcessor::launch(
                    worker,
                    chain_type,
                    SAMPLE_RATE,
                    frames as u32,
                    ChainId(chain_id),
                )?;
                if subprocess.lifecycle()
                    == shoop_engine::carla_processor::CarlaProcessorLifecycle::Unavailable
                {
                    anyhow::bail!(
                        "subprocess {chain_name} unavailable: {}",
                        subprocess
                            .crash_summary()
                            .unwrap_or_else(|| "unknown error".to_owned())
                    );
                }
                let (_, values, misses) = measure("subprocess", Box::new(subprocess), frames)?;
                print_row("subprocess", chain_name, frames, &values, misses);
            }

            if selected_mode == "reference" {
                chain_id += 1;
                let mut reference = SubprocessCarlaProcessor::spawn(
                    worker,
                    chain_type,
                    SAMPLE_RATE,
                    frames as u32,
                    ChainId(chain_id),
                    ProcessGeneration(1),
                )?;
                reference.use_serialized_reference_transport_for_benchmark();
                let (_, values, misses) =
                    measure("serialized_reference", Box::new(reference), frames)?;
                print_row("serialized_reference", chain_name, frames, &values, misses);
            }
        }
    }
    Ok(())
}
