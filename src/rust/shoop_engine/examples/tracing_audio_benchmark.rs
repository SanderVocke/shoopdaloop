use shoop_engine::channel_mode::ChannelMode;
use shoop_engine::dummy_port::{DummyAudioPort, PortId};
use shoop_engine::engine;
use shoop_engine::port::PortDirection;
use shoop_engine::session::{Port, Session};
use std::sync::atomic::Ordering;
use std::time::Instant;

const SAMPLE_RATE: u32 = 48_000;
const QUANTUM: usize = 128;
const LOOPS: usize = 16;
const WARMUP_CYCLES: usize = 2_000;
const DEFAULT_CYCLES: usize = 20_000;

#[derive(Clone, Copy)]
enum Mode {
    Disabled,
    Coarse,
    Detailed,
}

impl Mode {
    fn parse(value: &str) -> Self {
        match value {
            "disabled" => Self::Disabled,
            "coarse" => Self::Coarse,
            "detailed" => Self::Detailed,
            _ => panic!("mode must be disabled, coarse, or detailed"),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Coarse => "coarse",
            Self::Detailed => "detailed",
        }
    }

    fn enabled(self) -> bool {
        !matches!(self, Self::Disabled)
    }
}

fn audio_port(id: u64, name: String, direction: PortDirection) -> Port {
    Port::Dummy(DummyAudioPort::new(PortId(id), &name, direction, QUANTUM))
}

fn benchmark_session() -> Session {
    let mut session = Session::default();
    session.set_sample_rate(SAMPLE_RATE);
    session.set_buffer_size(QUANTUM as u32);

    for index in 0..LOOPS {
        let input = session.add_port(audio_port(
            (index * 2 + 1) as u64,
            format!("input-{index}"),
            PortDirection::Input,
        ));
        let output = session.add_port(audio_port(
            (index * 2 + 2) as u64,
            format!("output-{index}"),
            PortDirection::Output,
        ));
        let loop_index = session.create_loop();
        let channel = session
            .add_audio_channel(loop_index, SAMPLE_RATE as usize, ChannelMode::Direct)
            .expect("create benchmark audio channel");
        session
            .connect_channel_input(channel, input)
            .expect("connect benchmark input");
        session
            .connect_channel_output(channel, output)
            .expect("connect benchmark output");
    }
    session
        .apply_graph_changes()
        .expect("build benchmark graph schedule");
    session
}

fn percentile(sorted: &[u64], numerator: usize, denominator: usize) -> u64 {
    let index = (sorted.len() - 1).saturating_mul(numerator) / denominator;
    sorted[index]
}

fn main() {
    let mode = Mode::parse(
        &std::env::args()
            .nth(1)
            .unwrap_or_else(|| "disabled".to_owned()),
    );
    let cycles = std::env::args()
        .nth(2)
        .map(|value| value.parse().expect("cycles must be an integer"))
        .unwrap_or(DEFAULT_CYCLES);
    assert!(cycles > 0, "cycles must be positive");

    let capture_directory = tempfile::tempdir().expect("create capture scratch directory");
    let mut capture = mode.enabled().then(|| {
        shoop_common::tracing_capture::ReusableCaptureSession::start(
            capture_directory.path(),
            "audio-benchmark",
        )
        .expect("start embedded Tracy benchmark capture")
    });
    let _client = mode.enabled().then(tracy_client::Client::start);
    if let Some(capture) = &capture {
        capture
            .wait_until_capturing()
            .expect("wait for embedded Tracy benchmark capture");
    }
    shoop_tracing::set_tracing_output_enabled(true);
    shoop_tracing::set_engine_detail_enabled(matches!(mode, Mode::Detailed));
    shoop_tracing::set_tracing_enabled(mode.enabled());

    let (mut engine, _handle) = engine::split(benchmark_session(), 8);
    for _ in 0..WARMUP_CYCLES {
        engine.run_cycle(QUANTUM);
    }
    engine.stats().callback_worst_ns.store(0, Ordering::Relaxed);
    engine
        .stats()
        .callback_budget_overruns
        .store(0, Ordering::Relaxed);

    let mut callback_ns = Vec::with_capacity(cycles);
    let workload_start = Instant::now();
    for _ in 0..cycles {
        let callback_start = Instant::now();
        engine.run_cycle(QUANTUM);
        callback_ns.push(callback_start.elapsed().as_nanos().min(u64::MAX as u128) as u64);
    }
    let elapsed_ns = workload_start.elapsed().as_nanos();
    callback_ns.sort_unstable();

    let budget_ns = QUANTUM as u64 * 1_000_000_000 / u64::from(SAMPLE_RATE);
    let externally_observed_overruns = callback_ns
        .iter()
        .filter(|duration| **duration > budget_ns)
        .count();
    let engine_overruns = engine
        .stats()
        .callback_budget_overruns
        .load(Ordering::Relaxed);
    let engine_worst_ns = engine.stats().callback_worst_ns.load(Ordering::Relaxed);

    println!(
        concat!(
            "RESULT mode={} cycles={} warmup_cycles={} loops={} quantum={} sample_rate={} ",
            "elapsed_ns={} cycles_per_second={:.3} budget_ns={} callback_p50_ns={} ",
            "callback_p95_ns={} callback_p99_ns={} callback_max_ns={} ",
            "external_budget_overruns={} engine_budget_overruns={} engine_worst_ns={}"
        ),
        mode.name(),
        cycles,
        WARMUP_CYCLES,
        LOOPS,
        QUANTUM,
        SAMPLE_RATE,
        elapsed_ns,
        cycles as f64 / (elapsed_ns as f64 / 1_000_000_000.0),
        budget_ns,
        percentile(&callback_ns, 50, 100),
        percentile(&callback_ns, 95, 100),
        percentile(&callback_ns, 99, 100),
        callback_ns[callback_ns.len() - 1],
        externally_observed_overruns,
        engine_overruns,
        engine_worst_ns,
    );

    shoop_tracing::set_engine_detail_enabled(false);
    shoop_tracing::set_tracing_enabled(false);
    if let Some(capture) = &mut capture {
        capture
            .stop(shoop_common::tracing_capture::CaptureDisposition::Discard)
            .expect("discard embedded Tracy benchmark capture");
        shoop_common::tracing_capture::shutdown_reusable_profiler()
            .expect("shut down embedded Tracy benchmark profiler");
    }
}
