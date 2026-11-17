use anyhow::{bail, Result};
use clap::Parser;
use shoop_rt_benchmark::{
    cpu,
    fixture::{Fixture, Pattern, Preset},
    reference, runner,
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
    #[arg(long)]
    output: PathBuf,
    #[arg(long)]
    fixture_only: bool,
    #[arg(long, default_value = "dramsim3")]
    memory_model: String,
}
fn main() -> Result<()> {
    let args = Args::parse();
    let fixture = Fixture::new(args.preset, args.pattern);
    if args.fixture_only {
        std::fs::write(args.output, serde_json::to_vec_pretty(&fixture)?)?;
        return Ok(());
    }
    let run = match args.backend.as_str() {
        "cpu" => {
            let (control, driver) = cpu::prepare()?;
            runner::run("cpu", fixture, control, driver)
        }
        "fpga-reference" => {
            let (control, driver) = reference::prepare()?;
            runner::run("fpga-reference", fixture, control, driver)
        }
        #[cfg(feature = "rtl")]
        "fpga-rtl" => shoop_rt_benchmark::rtl::run(fixture, &args.memory_model, &args.output)?,
        _ => bail!("unknown backend {}", args.backend),
    };
    std::fs::write(&args.output, serde_json::to_vec_pretty(&run)?)?;
    if run.status != "completed" && run.status != "unsupported" {
        bail!("{}", run.error.as_deref().unwrap_or("benchmark failed"));
    }
    Ok(())
}
