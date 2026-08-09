#![cfg(not(target_arch = "wasm32"))]

use shoop_engine::carla_processor::CarlaProcessor;
use shoop_engine::carla_subprocess::{CarlaWorkerTestMode, SubprocessCarlaProcessor};
use shoop_engine::FXChainType;
use shoop_plugin_protocol::{ChainId, ProcessGeneration};

#[test]
fn egui_executable_serves_the_hidden_fake_carla_worker_entry() {
    let executable = std::env::var_os("NEXTEST_BIN_EXE_shoopdaloop_egui")
        .or_else(|| std::env::var_os("CARGO_BIN_EXE_shoopdaloop_egui"))
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_shoopdaloop_egui").into());
    let mut worker = SubprocessCarlaProcessor::spawn_test_worker(
        &executable,
        FXChainType::CarlaRack,
        48_000,
        128,
        ChainId(41),
        ProcessGeneration(1),
        CarlaWorkerTestMode::Fake,
    )
    .expect("egui executable should complete the worker handshake");
    assert!(worker.is_ready());
    worker.set_active(true);
    worker.set_visible(true).unwrap();
    assert!(worker.is_visible());
    assert_eq!(worker.save_state().unwrap(), "{}");
}
