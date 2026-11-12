use rt_fpga_sim::{SimResult, Simulator, differential, runtime};
use std::{path::PathBuf, time::Instant};
fn main() -> SimResult<()> {
    let mut args = std::env::args().skip(1);
    let case = PathBuf::from(
        args.next()
            .unwrap_or_else(|| "src/rust/rt_fpga_sim/fixtures/parallel.json".into()),
    );
    let cache = PathBuf::from(args.next().unwrap_or_else(|| "work/marlin-runtime".into()));
    let wave = args.next().map(PathBuf::from);
    std::fs::create_dir_all(&cache)?;
    let scenario = serde_json::from_slice(&std::fs::read(&case)?)?;
    let start = Instant::now();
    let rt = runtime(&cache)?;
    let sim = Simulator::new(&rt, wave.as_deref(), true)?;
    println!("model build/load {:?}", start.elapsed());
    let r = differential::run(&scenario, sim)?;
    println!(
        "PASS {} quanta / {} raw samples / {} cycles; graph {:?}, configure {:?}; {} instructions, {} state words, {} image bytes, {} gain writes",
        r.quanta,
        r.samples,
        r.cycles,
        r.compile,
        r.configure,
        r.instructions,
        r.state,
        r.image_bytes,
        r.gain_writes
    );
    Ok(())
}
