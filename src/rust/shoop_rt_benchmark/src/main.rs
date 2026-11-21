use anyhow::{bail, Result};
use clap::Parser;
use shoop_rt_benchmark::{
    cpu,
    fixture::{Fixture, Media, Pattern, Preset},
    host_timing, reference, runner,
};
use std::path::PathBuf;

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "cpu")]
    backend: String,
    #[arg(long, value_enum, default_value = "standard")]
    preset: Preset,
    #[arg(long, value_enum, default_value = "lifecycle")]
    pattern: Pattern,
    #[arg(long, value_enum, default_value = "audio")]
    media: Media,
    #[arg(long)]
    output: PathBuf,
    #[arg(long)]
    fixture_only: bool,
    #[arg(long, default_value = "dramsim3")]
    memory_model: String,
    #[arg(long)]
    dram_config: Option<PathBuf>,
    #[arg(long)]
    host_profile: Option<PathBuf>,
}
fn main() -> Result<()> {
    let args = Args::parse();
    let profile = args.host_profile.clone();
    if profile.is_some() {
        host_timing::enable();
    }
    let result = benchmark(args);
    if let Some(path) = profile {
        std::fs::write(path, serde_json::to_vec_pretty(&host_timing::finish())?)?;
    }
    result
}
fn benchmark(args: Args) -> Result<()> {
    let fixture = Fixture::with_media(args.preset, args.pattern, args.media);
    if args.fixture_only {
        std::fs::write(args.output, serde_json::to_vec_pretty(&fixture)?)?;
        return Ok(());
    }
    let run = {
        let _backend = host_timing::span("backend");
        match args.backend.as_str() {
            "cpu" => {
                let (control, driver) = {
                    let _prepare = host_timing::span("prepare");
                    cpu::prepare(&fixture)?
                };
                runner::run("cpu", fixture, control, driver)
            }
            "fpga-reference" => {
                let (control, driver) = {
                    let _prepare = host_timing::span("prepare");
                    reference::prepare(args.media)?
                };
                runner::run("fpga-reference", fixture, control, driver)
            }
            #[cfg(feature = "rtl")]
            "fpga-rtl" => shoop_rt_benchmark::rtl::run(
                fixture,
                &args.memory_model,
                args.dram_config.as_deref(),
                &args.output,
            )?,
            _ => bail!("unknown backend {}", args.backend),
        }
    };
    {
        let _output = host_timing::span("output_json");
        std::fs::write(&args.output, serde_json::to_vec_pretty(&run)?)?;
    }
    if run.status != "completed" && run.status != "unsupported" {
        bail!("{}", run.error.as_deref().unwrap_or("benchmark failed"));
    }
    Ok(())
}
