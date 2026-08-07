use assert_no_alloc::assert_no_alloc;
use shoop_engine::carla_processor::{
    spawn_processor_bridge, CarlaProcessor, CarlaProcessorLifecycle, FakeCarlaProcessor,
};
use shoop_engine::carla_subprocess::{
    CarlaWorkerTestMode, SubprocessCarlaProcessor, SupervisedCarlaProcessor,
};
use shoop_engine::lv2_carla::CarlaLv2Host;
use shoop_engine::FXChainType;
use shoop_plugin_protocol::{ChainId, ProcessGeneration, WorkerExitKind};

fn record_ci_benchmark(kind: &str, header: &str, row: &str) {
    if std::env::var_os("CI").is_none() {
        return;
    }
    use std::io::Write;
    let workspace = std::env::var_os("GITHUB_WORKSPACE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().expect("locate benchmark working directory"));
    let directory = workspace.join("carla-subprocess-benchmarks");
    std::fs::create_dir_all(&directory).expect("create benchmark artifact directory");
    let path = directory.join(format!(
        "{kind}-{}-{}.csv",
        std::env::consts::OS,
        std::env::consts::ARCH
    ));
    let needs_header = !path.exists() || path.metadata().is_ok_and(|metadata| metadata.len() == 0);
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .expect("open benchmark artifact");
    if needs_header {
        writeln!(file, "{header}").expect("write benchmark header");
    }
    writeln!(file, "{row}").expect("write benchmark row");
}

fn worker_executable() -> &'static str {
    static PATH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    PATH.get_or_init(|| {
        std::env::var("NEXTEST_BIN_EXE_shoopdaloop")
            .unwrap_or_else(|_| env!("CARGO_BIN_EXE_shoopdaloop").to_owned())
    })
}

fn wait_until_not_ready(processor: &mut impl CarlaProcessor) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while processor.is_ready() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
fn fake_worker_round_trips_without_carla_installed() {
    let executable = worker_executable();
    let mut processor = SubprocessCarlaProcessor::spawn_test_worker(
        executable,
        FXChainType::CarlaPatchbay16x,
        48_000,
        64,
        ChainId(101),
        ProcessGeneration(1),
        CarlaWorkerTestMode::Fake,
    )
    .expect("start fake worker");
    processor.set_active(true);
    processor.audio_input_mut(15).unwrap()[..4].copy_from_slice(&[0.25, -0.5, 0.75, 1.0]);
    processor
        .set_midi_input_events(0, &[(3, &[0x90, 60, 100])])
        .unwrap();
    processor.process(4).unwrap();
    assert_eq!(
        processor.audio_output(15).unwrap()[..4],
        [0.25, -0.5, 0.75, 1.0]
    );
    assert_eq!(
        processor.midi_output_events(0).unwrap(),
        vec![(3, vec![0x90, 60, 100])]
    );
    processor.restore_state("fake checkpoint").unwrap();
    assert_eq!(processor.save_state().unwrap(), "fake checkpoint");
    processor.set_visible(true).unwrap();
    assert!(processor.is_visible());
    processor.set_visible(false).unwrap();
    assert!(!processor.is_visible());
}

#[test]
fn fake_worker_covers_malformed_peer_log_flood_abort_error_and_hang() {
    let executable = worker_executable();
    let malformed = SubprocessCarlaProcessor::spawn_test_worker(
        executable,
        FXChainType::CarlaRack,
        48_000,
        64,
        ChainId(102),
        ProcessGeneration(1),
        CarlaWorkerTestMode::MalformedHandshake,
    );
    assert!(malformed.is_err());

    let flood = SubprocessCarlaProcessor::spawn_test_worker(
        executable,
        FXChainType::CarlaRack,
        48_000,
        64,
        ChainId(103),
        ProcessGeneration(1),
        CarlaWorkerTestMode::FloodLogs,
    )
    .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        let logs = flood.generation_logs();
        if logs[0].stdout_dropped_bytes > 0 && logs[0].stderr_dropped_bytes > 0 {
            assert_eq!(logs[0].stdout.len(), 64 * 1024);
            assert_eq!(logs[0].stderr.len(), 64 * 1024);
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "fake flood logs were not drained"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    drop(flood);

    for (chain_id, mode) in [
        (104, CarlaWorkerTestMode::Abort),
        (105, CarlaWorkerTestMode::ProcessError),
    ] {
        let mut supervisor = SupervisedCarlaProcessor::launch_test_worker(
            executable,
            FXChainType::CarlaRack,
            48_000,
            64,
            ChainId(chain_id),
            mode,
        )
        .unwrap();
        assert!(supervisor.is_ready());
        supervisor.set_active(true);
        supervisor.process(64).unwrap();
        wait_until_not_ready(&mut supervisor);
        assert_eq!(supervisor.lifecycle(), CarlaProcessorLifecycle::Crashed);
        assert_eq!(supervisor.exit_kind(), WorkerExitKind::UnexpectedExit);
    }

    let mut hung = SubprocessCarlaProcessor::spawn_test_worker(
        executable,
        FXChainType::CarlaRack,
        48_000,
        32,
        ChainId(106),
        ProcessGeneration(1),
        CarlaWorkerTestMode::Hang,
    )
    .unwrap();
    hung.set_active(true);
    let started = std::time::Instant::now();
    hung.process(32).unwrap();
    assert!(started.elapsed() < std::time::Duration::from_millis(20));
    assert_eq!(hung.deadline_misses(), 1);
    hung.terminate_worker_for_test().unwrap();

    let mut hung_shutdown = SubprocessCarlaProcessor::spawn_test_worker(
        executable,
        FXChainType::CarlaRack,
        48_000,
        32,
        ChainId(116),
        ProcessGeneration(1),
        CarlaWorkerTestMode::HangShutdown,
    )
    .unwrap();
    assert_eq!(
        hung_shutdown.shutdown_requested(),
        WorkerExitKind::Unresponsive
    );
}

#[test]
fn fake_supervisor_restarts_saves_while_down_and_isolates_chains() {
    let executable = worker_executable();
    let mut first = SupervisedCarlaProcessor::launch_test_worker(
        executable,
        FXChainType::CarlaRack,
        48_000,
        64,
        ChainId(107),
        CarlaWorkerTestMode::Fake,
    )
    .unwrap();
    let mut second = SupervisedCarlaProcessor::launch_test_worker(
        executable,
        FXChainType::CarlaRack,
        48_000,
        64,
        ChainId(108),
        CarlaWorkerTestMode::Fake,
    )
    .unwrap();
    assert_ne!(first.worker_id(), second.worker_id());
    first.restore_state("retained fake checkpoint").unwrap();
    first.set_active(true);
    second.set_active(true);
    for generation in 2..=4 {
        first.terminate_worker_for_test().unwrap();
        assert!(!first.is_ready());
        assert_eq!(first.save_state().unwrap(), "retained fake checkpoint");
        second.process(64).unwrap();
        assert!(second.is_ready());
        first.restart_without_ui_for_test().unwrap();
        assert_eq!(first.generation(), generation);
        assert!(first.is_active());
        assert_eq!(first.save_state().unwrap(), "retained fake checkpoint");
    }
}

#[test]
fn fake_worker_deadline_wait_is_bounded_for_all_supported_buffer_sizes() {
    let executable = worker_executable();
    for (index, frames) in [32_u32, 64, 128, 256, 512, 1024].into_iter().enumerate() {
        let mut processor = SubprocessCarlaProcessor::spawn_test_worker(
            executable,
            FXChainType::CarlaRack,
            48_000,
            frames,
            ChainId(120 + index as u64),
            ProcessGeneration(1),
            CarlaWorkerTestMode::Hang,
        )
        .unwrap();
        processor.set_active(true);
        let period = std::time::Duration::from_secs_f64(frames as f64 / 48_000.0);
        let started = std::time::Instant::now();
        processor.process(frames as usize).unwrap();
        let elapsed = started.elapsed();
        assert_eq!(processor.deadline_misses(), 1);
        assert!(
            elapsed <= period.saturating_mul(5) + std::time::Duration::from_millis(20),
            "{frames}-frame deadline fallback took {elapsed:?} for {period:?} period"
        );
        processor.terminate_worker_for_test().unwrap();
    }
}

#[test]
fn fake_direct_and_subprocess_transport_benchmark_matrix() {
    const ITERATIONS: usize = 40;
    let executable = worker_executable();
    println!("mode,channels,frames,p50_us,p95_us,worst_us,deadline_misses");
    let mut chain_id = 140_u64;
    for (chain_type, channels) in [
        (FXChainType::CarlaRack, 2_usize),
        (FXChainType::CarlaPatchbay16x, 16_usize),
    ] {
        for frames in [32_usize, 64, 128, 256, 512, 1024] {
            for mode in ["direct", "subprocess"] {
                let host: Box<dyn CarlaProcessor> = if mode == "direct" {
                    Box::new(FakeCarlaProcessor::new(chain_type, channels, 1024))
                } else {
                    chain_id += 1;
                    Box::new(
                        SupervisedCarlaProcessor::launch_test_worker(
                            executable,
                            chain_type,
                            48_000,
                            frames as u32,
                            ChainId(chain_id),
                            CarlaWorkerTestMode::Fake,
                        )
                        .unwrap(),
                    )
                };
                let (control, mut endpoint) =
                    spawn_processor_bridge(host, 48_000, frames as u32).unwrap();
                control.set_active(true);
                for _ in 0..5 {
                    endpoint.process(frames).unwrap();
                }
                let misses_before = control.deadline_misses();
                let period = std::time::Duration::from_secs_f64(frames as f64 / 48_000.0);
                let mut samples = Vec::with_capacity(ITERATIONS);
                for _ in 0..ITERATIONS {
                    let started = std::time::Instant::now();
                    endpoint.process(frames).unwrap();
                    let elapsed = started.elapsed();
                    samples.push(elapsed.as_secs_f64() * 1_000_000.0);
                    if let Some(idle) = period.checked_sub(elapsed) {
                        std::thread::sleep(idle);
                    }
                }
                samples.sort_by(f64::total_cmp);
                let misses = control.deadline_misses() - misses_before;
                let p50 = samples[samples.len() / 2];
                let p95 = samples[samples.len() * 95 / 100];
                let worst = samples[samples.len() - 1];
                let row =
                    format!("{mode},{channels},{frames},{p50:.3},{p95:.3},{worst:.3},{misses}");
                println!("{row}");
                record_ci_benchmark(
                    "fixture",
                    "mode,channels,frames,p50_us,p95_us,worst_us,deadline_misses",
                    &row,
                );
                // Deadline misses are load-sensitive on shared CI runners and are
                // retained in the artifact for comparison. The hard gate here is
                // callback boundedness; functional round trips and deterministic
                // deadline fallback are covered by dedicated tests.
                assert!(
                    worst <= period.as_secs_f64() * 5_000_000.0 + 20_000.0,
                    "{mode} {channels}ch/{frames} worst callback was {worst:.3}us"
                );
            }
        }
    }
}

#[test]
fn real_carla_direct_and_subprocess_transport_benchmark_matrix_when_available() {
    const ITERATIONS: usize = 40;
    let executable = worker_executable();
    match CarlaLv2Host::instantiate(FXChainType::CarlaRack, 48_000, 32) {
        Ok(probe) => drop(probe),
        Err(error) => {
            eprintln!("skipping real Carla benchmark matrix: {error}");
            return;
        }
    }
    println!("mode,channels,frames,p50_us,p95_us,worst_us,deadline_misses");
    let mut chain_id = 180_u64;
    for (chain_type, channels) in [
        (FXChainType::CarlaRack, 2_usize),
        (FXChainType::CarlaPatchbay16x, 16_usize),
    ] {
        for frames in [32_usize, 64, 128, 256, 512, 1024] {
            for mode in ["real_direct", "real_subprocess"] {
                let host: Box<dyn CarlaProcessor> = if mode == "real_direct" {
                    Box::new(
                        CarlaLv2Host::instantiate(chain_type, 48_000, frames as u32)
                            .expect("instantiate direct Carla benchmark host"),
                    )
                } else {
                    chain_id += 1;
                    let supervisor = SupervisedCarlaProcessor::launch(
                        executable,
                        chain_type,
                        48_000,
                        frames as u32,
                        ChainId(chain_id),
                    )
                    .unwrap();
                    assert_ne!(
                        supervisor.lifecycle(),
                        CarlaProcessorLifecycle::Unavailable,
                        "real subprocess benchmark unavailable: {:?}",
                        supervisor.crash_summary()
                    );
                    Box::new(supervisor)
                };
                let (control, mut endpoint) =
                    spawn_processor_bridge(host, 48_000, frames as u32).unwrap();
                control.set_active(true);
                for _ in 0..5 {
                    endpoint.process(frames).unwrap();
                }
                let misses_before = control.deadline_misses();
                let period = std::time::Duration::from_secs_f64(frames as f64 / 48_000.0);
                let mut samples = Vec::with_capacity(ITERATIONS);
                for _ in 0..ITERATIONS {
                    let started = std::time::Instant::now();
                    endpoint.process(frames).unwrap();
                    let elapsed = started.elapsed();
                    samples.push(elapsed.as_secs_f64() * 1_000_000.0);
                    if let Some(idle) = period.checked_sub(elapsed) {
                        std::thread::sleep(idle);
                    }
                }
                samples.sort_by(f64::total_cmp);
                let misses = control.deadline_misses() - misses_before;
                let p50 = samples[samples.len() / 2];
                let p95 = samples[samples.len() * 95 / 100];
                let worst = samples[samples.len() - 1];
                let row =
                    format!("{mode},{channels},{frames},{p50:.3},{p95:.3},{worst:.3},{misses}");
                println!("{row}");
                record_ci_benchmark(
                    "real-carla",
                    "mode,channels,frames,p50_us,p95_us,worst_us,deadline_misses",
                    &row,
                );
            }
        }
    }
}

#[test]
fn self_spawned_carla_worker_processes_and_preserves_state() {
    let executable = worker_executable();
    let mut processor = match SubprocessCarlaProcessor::spawn(
        executable,
        FXChainType::CarlaRack,
        48_000,
        256,
        ChainId(1),
        ProcessGeneration(1),
    ) {
        Ok(processor) => processor,
        Err(error) if error.to_string().contains("not found in LV2_PATH") => {
            eprintln!("skipping Carla worker test: {error}");
            return;
        }
        Err(error) => panic!("could not start Carla worker: {error:#}"),
    };

    // Regression: an idle worker must not self-terminate merely because no
    // control request arrives within the parent-side request timeout.
    std::thread::sleep(std::time::Duration::from_millis(2_200));
    assert!(processor.is_ready());

    processor.set_active(true);
    for (index, sample) in processor.audio_input_mut(0).unwrap()[..256]
        .iter_mut()
        .enumerate()
    {
        *sample = (index as f32 * 0.01).sin();
    }
    processor
        .set_midi_input_events(0, &[(7, &[0x90, 64, 100]), (31, &[0x80, 64, 0])])
        .unwrap();
    processor.process(256).expect("subprocess block");
    assert_no_alloc(|| {
        processor
            .process(256)
            .expect("allocation-free subprocess block");
    });
    assert!(processor.audio_output(0).unwrap()[..256]
        .iter()
        .all(|sample| sample.is_finite()));

    processor.use_serialized_reference_transport_for_benchmark();
    processor
        .process(256)
        .expect("serialized reference subprocess block");
    let _reference_midi = processor.midi_output_events(0).unwrap();

    let state = processor.save_state().expect("worker state save");
    assert!(state.starts_with('{'));
    processor
        .restore_state(&state)
        .expect("worker state restore");
    processor
        .set_midi_input_events(0, &[(0, &[0xf0, 1, 2, 3, 0xf7])])
        .unwrap();
    let status = processor.status().expect("worker status");
    assert!(status.ready);
    assert_eq!(status.midi_input_overflows, 1);
    assert!(status.active);
    assert_eq!(status.processed_blocks, 3);
}

#[test]
fn subprocess_external_ui_show_hide_when_opted_in() {
    if std::env::var("SHOOP_TEST_CARLA_UI").as_deref() != Ok("1") {
        eprintln!("skipping subprocess external-UI test; set SHOOP_TEST_CARLA_UI=1");
        return;
    }
    let executable = worker_executable();
    let mut processor = SubprocessCarlaProcessor::spawn(
        executable,
        FXChainType::CarlaRack,
        48_000,
        256,
        ChainId(3),
        ProcessGeneration(1),
    )
    .expect("start Carla UI worker");
    processor.set_visible(true).expect("show worker UI");
    assert!(processor.is_visible());
    processor.set_visible(false).expect("hide worker UI");
    assert!(!processor.is_visible());
}

#[test]
fn requested_worker_shutdown_reaps_and_removes_shared_memory() {
    let executable = worker_executable();
    let mut processor = SubprocessCarlaProcessor::spawn_test_worker(
        executable,
        FXChainType::CarlaRack,
        48_000,
        256,
        ChainId(4),
        ProcessGeneration(1),
        CarlaWorkerTestMode::Fake,
    )
    .expect("start fake worker");
    let shared_memory_path = processor.shared_memory_path().to_path_buf();
    let started = std::time::Instant::now();
    assert_eq!(processor.shutdown_requested(), WorkerExitKind::Requested);
    drop(processor);
    assert!(started.elapsed() < std::time::Duration::from_secs(3));
    assert!(!shared_memory_path.exists());
}

#[test]
fn supervisor_detects_crash_preserves_checkpoint_and_starts_new_generation() {
    let executable = worker_executable();
    let mut supervisor = SupervisedCarlaProcessor::launch(
        executable,
        FXChainType::CarlaRack,
        48_000,
        256,
        ChainId(2),
    )
    .expect("construct supervisor");
    if supervisor.lifecycle() == CarlaProcessorLifecycle::Unavailable {
        eprintln!(
            "skipping Carla supervisor test: {}",
            supervisor
                .crash_summary()
                .unwrap_or_else(|| "worker unavailable".to_owned())
        );
        return;
    }

    supervisor.set_active(true);
    let checkpoint = supervisor.save_state().expect("initial checkpoint");
    supervisor
        .terminate_worker_for_test()
        .expect("terminate worker");
    assert!(!supervisor.is_ready());
    assert_eq!(supervisor.lifecycle(), CarlaProcessorLifecycle::Crashed);
    assert_eq!(supervisor.exit_kind(), WorkerExitKind::UnexpectedExit);
    assert!(supervisor
        .crash_summary()
        .is_some_and(|summary| summary.contains("unexpected")));
    assert_eq!(supervisor.generation(), 1);
    assert_eq!(
        supervisor.save_state().expect("fallback checkpoint"),
        checkpoint
    );

    supervisor
        .restart_without_ui_for_test()
        .expect("restart worker");
    assert!(supervisor.is_ready());
    assert!(supervisor.is_active());
    assert_eq!(supervisor.generation(), 2);
    assert_eq!(supervisor.save_state().unwrap(), checkpoint);
    for expected_generation in 3..=4 {
        supervisor.terminate_worker_for_test().unwrap();
        assert!(!supervisor.is_ready());
        supervisor.restart_without_ui_for_test().unwrap();
        assert!(supervisor.is_ready());
        assert!(supervisor.is_active());
        assert_eq!(supervisor.generation(), expected_generation);
        assert_eq!(supervisor.save_state().unwrap(), checkpoint);
    }
    let generations: Vec<_> = supervisor
        .generation_logs()
        .into_iter()
        .map(|log| log.generation)
        .collect();
    assert_eq!(generations, vec![1, 2, 3, 4]);
}

#[test]
fn abnormal_parent_helper() {
    let Some(report_path) = std::env::var_os("SHOOP_TEST_ABNORMAL_PARENT_REPORT") else {
        return;
    };
    let executable = worker_executable();
    let processor = SubprocessCarlaProcessor::spawn_test_worker(
        executable,
        FXChainType::CarlaRack,
        48_000,
        256,
        ChainId(98),
        ProcessGeneration(1),
        CarlaWorkerTestMode::Fake,
    )
    .expect("start abnormal-parent fake worker");
    std::fs::write(
        report_path,
        format!(
            "{}\n{}",
            processor.worker_id(),
            processor.shared_memory_path().display()
        ),
    )
    .unwrap();
    // Deliberately bypass all destructors: the worker must observe parent TCP
    // disconnect and remove its generation-specific shared-memory file itself.
    std::mem::forget(processor);
    std::process::exit(0);
}

#[test]
fn worker_exits_and_cleans_ipc_after_abnormal_parent_termination() {
    #[cfg(target_os = "linux")]
    // SAFETY: prctl only changes this test process into a child subreaper, so the
    // deliberately orphaned worker can be reaped in containerized CI.
    unsafe {
        assert_eq!(libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1), 0);
    }
    let directory = tempfile::tempdir().unwrap();
    let report = directory.path().join("worker-report.txt");
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "abnormal_parent_helper", "--nocapture"])
        .env("SHOOP_TEST_ABNORMAL_PARENT_REPORT", &report)
        .status()
        .expect("launch abnormal parent helper");
    assert!(status.success());
    let report = std::fs::read_to_string(&report).unwrap();
    let mut lines = report.lines();
    let worker_pid: u32 = lines.next().unwrap().parse().unwrap();
    let shared_memory_path = std::path::PathBuf::from(lines.next().unwrap());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while shared_memory_path.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        !shared_memory_path.exists(),
        "worker left stale IPC after its parent exited: {}",
        shared_memory_path.display()
    );
    #[cfg(target_os = "linux")]
    {
        let reap_deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let mut status = 0;
            // SAFETY: worker_pid identifies the orphan adopted by this subreaper;
            // status points to valid storage for the duration of waitpid.
            let result =
                unsafe { libc::waitpid(worker_pid as libc::pid_t, &mut status, libc::WNOHANG) };
            if result == worker_pid as libc::pid_t {
                break;
            }
            assert!(
                result >= 0 && std::time::Instant::now() < reap_deadline,
                "orphaned worker {worker_pid} was not reapable (waitpid={result})"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}

#[test]
fn startup_failure_is_classified_without_losing_the_chain_handle() {
    let mut supervisor = SupervisedCarlaProcessor::launch(
        "/definitely/missing/shoopdaloop-worker",
        FXChainType::CarlaRack,
        48_000,
        256,
        ChainId(99),
    )
    .expect("construct supervisor even when startup fails");
    assert!(!supervisor.is_ready());
    assert_eq!(supervisor.lifecycle(), CarlaProcessorLifecycle::Unavailable);
    assert_eq!(supervisor.exit_kind(), WorkerExitKind::StartupFailure);
    assert!(supervisor
        .crash_summary()
        .is_some_and(|summary| summary.contains("startup failed")));
}

#[test]
fn separate_chains_use_independent_worker_processes() {
    let executable = worker_executable();
    let mut first = SupervisedCarlaProcessor::launch(
        executable,
        FXChainType::CarlaRack,
        48_000,
        256,
        ChainId(11),
    )
    .unwrap();
    let mut second = SupervisedCarlaProcessor::launch(
        executable,
        FXChainType::CarlaRack,
        48_000,
        256,
        ChainId(12),
    )
    .unwrap();
    if !first.is_ready() || !second.is_ready() {
        eprintln!("skipping independent worker test because Carla is unavailable");
        return;
    }
    assert_ne!(first.worker_id(), second.worker_id());
    first.set_active(true);
    second.set_active(true);
    first.terminate_worker_for_test().unwrap();
    assert!(!first.is_ready());
    assert!(second.is_ready());
    second.process(256).expect("unaffected worker block");
    assert!(second.is_ready());
}
