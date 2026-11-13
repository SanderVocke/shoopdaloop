use rt_fpga_adapter::bridge::trace;
use rt_fpga_sim::{SimResult, bridge_sim::MarlinBus, runtime};
use std::path::PathBuf;

fn main() -> SimResult<()> {
    let mut args = std::env::args().skip(1);
    let log = args
        .next()
        .ok_or("usage: fpga-bridge-replay UART_LOG [ARTIFACT_DIRECTORY] [board|external|direct]")?;
    let artifacts = PathBuf::from(args.next().unwrap_or_else(|| "work/bridge-replay".into()));
    let profile = args.next().unwrap_or_else(|| "board".into());
    if args.next().is_some() || !["board", "external", "direct"].contains(&profile.as_str()) {
        return Err("expected board, external or direct replay profile".into());
    }
    let records = trace::parse(&std::fs::read_to_string(log)?)?;
    std::fs::create_dir_all(&artifacts)?;
    let rt = runtime(&artifacts)?;
    let mut bus = match profile.as_str() {
        "board" => MarlinBus::board(&rt)?,
        "external" => MarlinBus::new(&rt, true)?,
        _ => MarlinBus::new(&rt, false)?,
    };
    trace::replay(&mut bus, &records).map_err(|error| error.to_string())?;
    println!("PASS {} exact register accesses ({profile})", records.len());
    Ok(())
}
