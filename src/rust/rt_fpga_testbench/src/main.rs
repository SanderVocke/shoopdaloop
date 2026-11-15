use clap::{Parser, ValueEnum};
use rt_fpga_testbench::{Result, drivers::store::Geometry, memory::Mode, report, suites};
use serde_json::json;
use std::path::PathBuf;
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum Suite {
    All,
    State,
    Audio,
    Smoothing,
    Axi,
    Direct,
    Chunk,
    Media,
    BlockMedia,
    Deferred,
    Capacity,
    Reuse,
    Statistics,
    Stress,
    ReclaimTiming,
    Metadata,
    Resolver,
    Pipeline,
    AudioIo,
}
const ALL: &[Suite] = &[
    Suite::State,
    Suite::Audio,
    Suite::Smoothing,
    Suite::Axi,
    Suite::Direct,
    Suite::Chunk,
    Suite::Media,
    Suite::BlockMedia,
    Suite::Deferred,
    Suite::Capacity,
    Suite::Reuse,
    Suite::Statistics,
    Suite::Stress,
    Suite::ReclaimTiming,
    Suite::Metadata,
    Suite::Resolver,
    Suite::Pipeline,
    Suite::AudioIo,
];
#[derive(Parser, Debug)]
#[command(about = "Marlin execution component regression suites")]
struct Args {
    #[arg(value_enum, default_value = "all")]
    suite: Suite,
    #[arg(long, default_value = "all")]
    config: String,
    #[arg(long, value_enum, default_value = "instant")]
    memory_model: Mode,
    #[arg(long, default_value_t = 64)]
    axi_data_width: u64,
    #[arg(long)]
    random_cases: Option<u64>,
    #[arg(long, default_value = "work/fpga-testbench")]
    output: PathBuf,
    #[arg(long)]
    waves: bool,
    #[arg(long)]
    block_chunks: Option<u64>,
    #[arg(long, default_value_t = 1)]
    response_cycles: u64,
    #[arg(long, default_value_t = 30)]
    stress_seconds: u64,
    #[arg(long, default_value_t = 32)]
    q: u64,
    #[arg(long)]
    p: Option<u64>,
    #[arg(long, default_value_t = 3)]
    banks: u64,
    #[arg(long, default_value_t = 16)]
    channels: u64,
    #[arg(long)]
    slots: Option<u64>,
    #[arg(long, default_value_t = 13)]
    audio_period: u64,
    #[arg(long, default_value_t = 0)]
    audio_phase: u64,
}
fn selected<'a, const N: usize>(
    a: &Args,
    configs: &'a [(&str, [u64; N])],
) -> Result<Vec<(&'a str, [u64; N])>> {
    let result: Vec<_> = configs
        .iter()
        .copied()
        .filter(|(n, _)| a.config == "all" || a.config == *n)
        .collect();
    if result.is_empty() {
        return Err(format!("unknown configuration {}", a.config).into());
    }
    Ok(result)
}
fn geometry(a: &Args, c: [u64; 3], default_bc: u64) -> Result<Geometry> {
    let bc = a.block_chunks.unwrap_or(default_bc);
    if !bc.is_power_of_two() {
        return Err("block-chunks must be a positive power of two".into());
    }
    Ok(Geometry::new(
        [c[0], a.slots.unwrap_or(c[1] * bc), c[2]],
        bc,
    ))
}
fn run(a: &Args, suite: Suite) -> Result<()> {
    let mode = a.memory_model;
    let width = a.axi_data_width;
    if !matches!(width, 32 | 64) {
        return Err("AXI width must be 32 or 64".into());
    }
    match suite {
        Suite::All => unreachable!(),
        Suite::Smoothing => suites::smoothing::histories_and_extremes()?,
        Suite::State => {
            for (n, c) in selected(a, suites::state::CONFIGS)? {
                suites::state::run(n, c)?;
            }
        }
        Suite::Audio => {
            for (n, c) in selected(a, suites::audio::CONFIGS)? {
                suites::audio::run(n, c, a.random_cases.unwrap_or(150))?;
            }
        }

        Suite::Axi => suites::axi::run(width, mode)?,
        Suite::Direct => suites::direct::run(width)?,
        Suite::Chunk => {
            for (n, c) in selected(a, suites::chunk::CONFIGS)? {
                suites::chunk::run(n, c, width, mode, a.random_cases.unwrap_or(4))?;
            }
            suites::chunk::retirement("small", [8, 4, 2, 2], width, mode, 32)?;
        }
        Suite::Media => {
            for (n, c) in selected(a, &suites::media::CONFIGS[..3])? {
                suites::media::run(
                    n,
                    geometry(a, c, 1)?,
                    width,
                    mode,
                    a.random_cases.unwrap_or(40),
                )?;
            }
        }
        Suite::BlockMedia => suites::media::run_cases(
            geometry(a, [16, 8, 4], 32)?,
            width,
            mode,
            &[("block_ownership", suites::media::ownership::block_ownership)],
        )?,
        Suite::Deferred => suites::media::run_deferred(width, mode)?,
        Suite::Capacity => suites::media::run_capacity(width, mode)?,
        Suite::Reuse => suites::media::run_reuse(width, mode)?,
        Suite::Statistics => {
            for reuse in [false, true] {
                suites::media::run_statistics(width, mode, reuse)?;
            }
        }
        Suite::Stress => {
            if mode != Mode::Instant {
                return Err("preloaded ownership campaign uses functional memory".into());
            }
            for (loops, channels) in [(32, 1), (16, 2)] {
                suites::media::campaign::stress(loops, channels, a.stress_seconds)?;
            }
        }
        Suite::ReclaimTiming => {
            suites::media::campaign::timing(width, mode)?;
        }
        Suite::Metadata => {
            for large in 0..3 {
                suites::metadata::tables(width, large)?;
            }
            for words in [16, 64] {
                suites::metadata::coherent_cache_and_transport(width, words)?;
            }
        }
        Suite::Resolver => {
            for (_, c) in selected(a, &suites::media::CONFIGS[..3])? {
                suites::resolver::resolver_conformance(c)?;
            }
        }
        Suite::Pipeline => {
            for (n, c) in selected(a, &suites::media::CONFIGS[..4])? {
                let audio = if matches!(n, "default" | "wide") {
                    [32, 16, 32, 32, 16, 2, 2]
                } else {
                    [16, 8, 16, c[2], 8, 2, 2]
                };
                suites::pipeline::run(
                    n,
                    audio,
                    geometry(a, c, 1)?,
                    width,
                    mode,
                    a.response_cycles,
                    a.random_cases.unwrap_or(40),
                )?;
            }
        }

        Suite::AudioIo => {
            if a.config == "all" {
                for &c in suites::audio_io::CONFIGS {
                    suites::audio_io::run(c)?;
                }
            } else {
                suites::audio_io::run(suites::audio_io::Config {
                    q: a.q,
                    p: a.p.unwrap_or(a.q),
                    banks: a.banks,
                    channels: a.channels,
                    period_ps: a.audio_period * 1000,
                    phase_ps: a.audio_phase * 1000,
                })?;
            }
        }
    }
    Ok(())
}
fn main() {
    let a = Args::parse();
    if let Err(e) = execute(&a) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
fn execute(a: &Args) -> Result<()> {
    report::configure(&a.output, a.waves)?;
    let start = std::time::Instant::now();
    let selected = if a.suite == Suite::All {
        ALL
    } else {
        std::slice::from_ref(&a.suite)
    };
    let mut rows = Vec::new();
    let mut failure = None;
    for &suite in selected {
        if suite == Suite::Stress && a.memory_model != Mode::Instant {
            continue;
        }
        let begin = std::time::Instant::now();
        println!("Running {suite:?}");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(a, suite)));
        let error = match result {
            Ok(Ok(())) => None,
            Ok(Err(e)) => Some(e.to_string()),
            Err(p) => Some(
                p.downcast_ref::<String>()
                    .cloned()
                    .or_else(|| p.downcast_ref::<&str>().map(|s| (*s).into()))
                    .unwrap_or_else(|| "bench assertion failed".into()),
            ),
        };
        rows.push(json!({"suite":format!("{suite:?}"),"status":if error.is_some() {"failed"}else{"passed"},"wall_seconds":begin.elapsed().as_secs_f64(),"error":error}));
        if error.is_some() {
            failure = error;
            break;
        }
    }
    let result = json!({"format_version":1,"arguments":std::env::args().collect::<Vec<_>>(),"status":if failure.is_some() {"failed"}else{"passed"},"wall_seconds":start.elapsed().as_secs_f64(),"suites":rows,"provenance":report::provenance()});
    std::fs::write(
        a.output.join("run.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    if let Some(e) = failure {
        return Err(e.into());
    }
    println!("Passed; report {}", a.output.join("run.json").display());
    Ok(())
}
