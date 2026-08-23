#![cfg(all(not(target_arch = "wasm32"), feature = "native-fx"))]

use shoop_engine::carla_native::carla_runtime_availability;
use shoop_engine::carla_processor::{CarlaProcessor, ProcessorLatencyDiagnostic};
use shoop_engine::carla_subprocess::{
    CarlaWorkerTestMode, SubprocessCarlaProcessor, SupervisedCarlaProcessor,
};
use shoop_engine::FXChainType;
use shoop_plugin_protocol::{ChainId, ProcessGeneration};

#[shoop_wasm_test_support::shoop_test]
fn application_executable_serves_the_hidden_fake_carla_worker_entry() {
    let executable = std::env::var_os("NEXTEST_BIN_EXE_shoopdaloop")
        .or_else(|| std::env::var_os("CARGO_BIN_EXE_shoopdaloop"))
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_shoopdaloop").into());
    let mut worker = SubprocessCarlaProcessor::spawn_test_worker(
        &executable,
        FXChainType::CarlaRack,
        48_000,
        128,
        ChainId(41),
        ProcessGeneration(1),
        CarlaWorkerTestMode::Fake,
    )
    .expect("application executable should complete the worker handshake");
    assert!(worker.is_ready());
    assert_eq!(worker.latency().range.unwrap().min(), 0);
    assert_eq!(
        worker.latency_diagnostic(),
        ProcessorLatencyDiagnostic::Manual
    );
    worker.set_active(true);
    worker.set_visible(true).unwrap();
    assert!(worker.is_visible());
    assert_eq!(worker.save_state().unwrap(), "{}");
}

#[shoop_wasm_test_support::shoop_test]
fn application_supervisor_recovers_checkpoint_activity_and_logs() {
    let executable = std::env::var_os("NEXTEST_BIN_EXE_shoopdaloop")
        .or_else(|| std::env::var_os("CARGO_BIN_EXE_shoopdaloop"))
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_shoopdaloop").into());
    let mut worker = SupervisedCarlaProcessor::launch_test_worker(
        &executable,
        FXChainType::CarlaRack,
        48_000,
        64,
        ChainId(43),
        CarlaWorkerTestMode::Fake,
    )
    .unwrap();
    worker.restore_state("checkpoint").unwrap();
    worker.set_active(true);
    let latency_before_restart = worker.latency();
    worker.terminate_worker_for_test().unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while worker.is_ready() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(
        worker.lifecycle(),
        shoop_engine::carla_processor::CarlaProcessorLifecycle::Crashed
    );
    worker.toggle_or_recover().unwrap();
    assert_eq!(worker.generation(), 2);
    assert!(worker.is_ready());
    assert!(worker.is_active());
    assert!(worker.is_visible());
    assert_eq!(worker.latency(), latency_before_restart);
    assert_eq!(
        worker.latency_diagnostic(),
        ProcessorLatencyDiagnostic::Manual
    );
    assert_eq!(worker.save_state().unwrap(), "checkpoint");
    assert!(!worker.generation_logs().is_empty());
    worker.clear_logs();
    assert!(worker
        .generation_logs()
        .iter()
        .all(|log| log.stdout.is_empty() && log.stderr.is_empty()));
}

#[shoop_wasm_test_support::shoop_test]
fn application_worker_hosts_the_real_carla_native_runtime_when_available() {
    if let Err(reason) = carla_runtime_availability() {
        if std::env::var_os("SHOOP_REQUIRE_CARLA_TESTS").is_some() {
            panic!("required real Carla worker runtime is unavailable: {reason:#}");
        }
        eprintln!("skipping real Carla worker test: {reason}");
        return;
    }
    let executable = std::env::var_os("NEXTEST_BIN_EXE_shoopdaloop")
        .or_else(|| std::env::var_os("CARGO_BIN_EXE_shoopdaloop"))
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_shoopdaloop").into());
    let mut worker = SubprocessCarlaProcessor::spawn(
        &executable,
        FXChainType::CarlaRack,
        48_000,
        64,
        ChainId(42),
        ProcessGeneration(1),
    )
    .expect("application executable should host Carla Native in its worker");
    assert_eq!(
        worker.latency_diagnostic(),
        ProcessorLatencyDiagnostic::CarlaRackAggregate
    );
    assert_eq!(worker.latency().range.unwrap().min(), 0);
    worker.set_active(true);
    let mut processed = false;
    for _ in 0..8 {
        worker.audio_input_mut(0).unwrap()[..64].fill(0.125);
        worker.audio_input_mut(1).unwrap()[..64].fill(-0.125);
        if let Err(error) = worker.process(64) {
            panic!(
                "Carla worker process failed: {error:#}; logs: {:?}",
                worker.generation_logs()
            );
        }
        if worker.audio_output(0).unwrap()[..64] == [0.125; 64] {
            processed = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(
        processed,
        "Carla worker did not process within eight blocks"
    );
    let legacy = include_str!("../../shoop_engine/test_data/carla_legacy_rack_loaded_state.json");
    worker.restore_state(legacy).unwrap();
    let mut restored_processed = false;
    for _ in 0..100 {
        worker.audio_input_mut(0).unwrap()[..64].fill(0.125);
        worker.audio_input_mut(1).unwrap()[..64].fill(-0.125);
        worker.process(64).unwrap();
        if worker.audio_output(0).unwrap()[..64]
            .iter()
            .any(|sample| sample.abs() > 0.05)
        {
            restored_processed = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(restored_processed, "legacy Carla state did not process");
    let state = worker.save_state().unwrap();
    assert!(state.starts_with("shoop-carla-native-state:2:rack:"));
    worker.restore_state(&state).unwrap();
}
