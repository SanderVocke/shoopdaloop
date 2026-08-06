use assert_no_alloc::assert_no_alloc;
use shoop_engine::carla_processor::{CarlaProcessor, CarlaProcessorLifecycle};
use shoop_engine::carla_subprocess::{SubprocessCarlaProcessor, SupervisedCarlaProcessor};
use shoop_engine::FXChainType;
use shoop_plugin_protocol::{ChainId, ProcessGeneration, WorkerExitKind};

#[test]
fn self_spawned_carla_worker_processes_and_preserves_state() {
    let executable = env!("CARGO_BIN_EXE_shoopdaloop");
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
    let executable = env!("CARGO_BIN_EXE_shoopdaloop");
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
    let executable = env!("CARGO_BIN_EXE_shoopdaloop");
    let mut processor = match SubprocessCarlaProcessor::spawn(
        executable,
        FXChainType::CarlaRack,
        48_000,
        256,
        ChainId(4),
        ProcessGeneration(1),
    ) {
        Ok(processor) => processor,
        Err(error) => {
            eprintln!("skipping requested-shutdown test: {error}");
            return;
        }
    };
    let shared_memory_path = processor.shared_memory_path().to_path_buf();
    let started = std::time::Instant::now();
    assert_eq!(processor.shutdown_requested(), WorkerExitKind::Requested);
    drop(processor);
    assert!(started.elapsed() < std::time::Duration::from_secs(3));
    assert!(!shared_memory_path.exists());
}

#[test]
fn supervisor_detects_crash_preserves_checkpoint_and_starts_new_generation() {
    let executable = env!("CARGO_BIN_EXE_shoopdaloop");
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
    let executable = env!("CARGO_BIN_EXE_shoopdaloop");
    let processor = match SubprocessCarlaProcessor::spawn(
        executable,
        FXChainType::CarlaRack,
        48_000,
        256,
        ChainId(98),
        ProcessGeneration(1),
    ) {
        Ok(processor) => processor,
        Err(error) => {
            std::fs::write(report_path, format!("skip:{error}")).unwrap();
            std::process::exit(0);
        }
    };
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
    let directory = tempfile::tempdir().unwrap();
    let report = directory.path().join("worker-report.txt");
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "abnormal_parent_helper", "--nocapture"])
        .env("SHOOP_TEST_ABNORMAL_PARENT_REPORT", &report)
        .status()
        .expect("launch abnormal parent helper");
    assert!(status.success());
    let report = std::fs::read_to_string(&report).unwrap();
    if report.starts_with("skip:") {
        eprintln!("skipping abnormal-parent test: {report}");
        return;
    }
    let mut lines = report.lines();
    let _worker_pid: u32 = lines.next().unwrap().parse().unwrap();
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
    let executable = env!("CARGO_BIN_EXE_shoopdaloop");
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
