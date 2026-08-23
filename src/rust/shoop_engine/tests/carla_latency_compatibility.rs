#![cfg(all(not(target_arch = "wasm32"), feature = "carla"))]

use shoop_engine::carla_native::CarlaNativeHost;
use shoop_engine::carla_processor::{CarlaProcessor, ProcessorLatencyDiagnostic};
use shoop_engine::FXChainType;

#[shoop_wasm_test_support::shoop_test]
fn unpatched_pinned_carla_remains_usable_with_unknown_manual_latency() {
    let Some(library) = std::env::var_os("SHOOP_CARLA_UNPATCHED_NATIVE_LIBRARY") else {
        eprintln!("skipping unpatched Carla compatibility test; no runtime path configured");
        return;
    };
    let Some(resources) = std::env::var_os("SHOOP_CARLA_UNPATCHED_RESOURCE_DIR") else {
        eprintln!("skipping unpatched Carla compatibility test; no resource path configured");
        return;
    };
    std::env::set_var("SHOOP_CARLA_NATIVE_LIBRARY", library);
    std::env::set_var("SHOOP_CARLA_RESOURCE_DIR", resources);

    let mut host = CarlaNativeHost::instantiate(FXChainType::CarlaRack, 48_000, 64)
        .expect("the pinned unpatched Carla runtime remains loadable");
    assert_eq!(
        host.latency_diagnostic(),
        ProcessorLatencyDiagnostic::Unsupported
    );
    assert!(host.latency().range.is_none());
    host.set_active(true);
    host.audio_input_mut(0).unwrap()[..4].copy_from_slice(&[0.25, 0.5, 0.75, 1.0]);
    host.process(4).unwrap();
    assert_eq!(host.audio_output(0).unwrap()[..4], [0.25, 0.5, 0.75, 1.0]);
}
