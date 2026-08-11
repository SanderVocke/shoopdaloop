#![cfg(all(not(target_arch = "wasm32"), feature = "native-fx"))]

use shoop_engine::carla_native::carla_runtime_availability;
use shoop_engine::carla_processor::CarlaProcessor;
use shoop_engine::carla_subprocess::{CarlaWorkerTestMode, SubprocessCarlaProcessor};
use shoop_engine::FXChainType;
use shoop_plugin_protocol::{ChainId, ProcessGeneration};

#[test]
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
    worker.set_active(true);
    worker.set_visible(true).unwrap();
    assert!(worker.is_visible());
    assert_eq!(worker.save_state().unwrap(), "{}");
}

#[test]
fn application_worker_hosts_the_real_carla_native_runtime_when_available() {
    if let Err(reason) = carla_runtime_availability() {
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
    let state = worker.save_state().unwrap();
    assert!(state.starts_with("shoop-carla-native-state:1:"));
    worker.restore_state(&state).unwrap();
}
