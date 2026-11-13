use rt_fpga_sim::{SimResult, Simulator, mono_trace, runtime};
use std::{path::PathBuf, time::Instant};
fn main() -> SimResult<()> {
    let mut args = std::env::args().skip(1);
    let case = PathBuf::from(
        args.next()
            .unwrap_or_else(|| "src/rust/rt_fpga_sim/fixtures/mono-loop.json".into()),
    );
    let output = PathBuf::from(args.next().unwrap_or_else(|| "work/marlin-mono".into()));
    let wave = args.next().map(PathBuf::from);
    std::fs::create_dir_all(&output)?;
    let trace: mono_trace::Trace = serde_json::from_slice(&std::fs::read(&case)?)?;
    let time = Instant::now();
    let rt = runtime(&output)?;
    let sim = Simulator::new(&rt, wave.as_deref(), true)?;
    println!("model build/load {:?}", time.elapsed());
    let report = mono_trace::run(&trace, sim).map_err(|e| {
        format!(
            "{e}; replay: cargo run -p rt_fpga_sim --features sim --bin fpga-mono-demo -- {} {} {}",
            case.display(),
            output.display(),
            output.join("failure.vcd").display()
        )
    })?;
    std::fs::write(
        output.join("result.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!(
        "PASS {}: {} quanta, {} clocks, {} read-buffer bytes; {} compilation, {} gain writes; {} instructions / {} state words / {} payload bytes; graph {} us, configuration {} us",
        report.name,
        report.frames.len(),
        report.clocks,
        report.read_bytes,
        report.compilations,
        report.gain_writes,
        report.instructions,
        report.state_words,
        report.image_bytes,
        report.compile_micros,
        report.configure_micros
    );
    Ok(())
}
