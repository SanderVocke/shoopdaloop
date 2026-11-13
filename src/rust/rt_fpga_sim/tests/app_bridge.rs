#![cfg(all(feature = "app", not(target_arch = "wasm32")))]
//! The real application drives the core through the CPU host bridge's registers.
use rt_fpga_sim::{
    app::{Action, FLOAT_MAX_ERROR, FLOAT_RMS_ERROR, Fpga, Native, Trace},
    app_worker::{RunMode, Transport},
};
use shoop_wasm_test_support::shoop_test;

fn artifacts() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../work/marlin-app")
}

/// Replays the app's sync-loop trace next to the native engine. Audio is compared only
/// when the core exchanges its samples with the host.
fn replay_against_native(transport: Transport) {
    let compare_audio = transport != Transport::ExternalBridge;
    let mut fpga = Fpga::with_transport(artifacts(), RunMode::Stepped, None, transport).unwrap();
    let mut native = Native::new().unwrap();
    let trace: Trace =
        serde_json::from_str(include_str!("../fixtures/app-sync-loop.json")).unwrap();
    fpga.hardware.signal(true).unwrap();
    *native.signal.borrow_mut() = true;
    let mut installed = None;
    for (index, step) in trace.steps.iter().enumerate() {
        native.app.apply(&step.action).unwrap();
        fpga.app.apply(&step.action).unwrap();
        native.app.pump(step.ticks);
        fpga.app.pump(step.ticks);
        let n = native.app.loop_state();
        let h = fpga.app.loop_state();
        assert_eq!(
            (
                h.mode,
                h.empty,
                h.length_frames,
                h.position,
                h.content_revision,
                h.repeat_sync,
                h.next_mode,
                h.next_transition_delay
            ),
            (
                n.mode,
                n.empty,
                n.length_frames,
                n.position,
                n.content_revision,
                n.repeat_sync,
                n.next_mode,
                n.next_transition_delay
            ),
            "step {index}; failures: {:?}",
            fpga.failure.lock().unwrap()
        );
        if index == 1 {
            installed = Some(fpga.hardware.metrics().compilations);
        }
        if index > 1 {
            assert_eq!(
                Some(fpga.hardware.metrics().compilations),
                installed,
                "control recompiled at step {index}"
            );
        }
        if index == 10 {
            let n = native.app.snapshot();
            let h = fpga.app.snapshot();
            let n = n.details.as_ref().expect("native details");
            let h = h.details.as_ref().expect("hardware details");
            assert!(!n.loading && !h.loading);
            assert_eq!(h.channels.len(), n.channels.len());
            for (n, h) in n.channels.iter().zip(&h.channels) {
                assert_eq!(h.samples.len(), n.samples.len());
                if compare_audio {
                    for (n, h) in n.samples.iter().zip(h.samples.iter()) {
                        assert!((n - h).abs() <= FLOAT_MAX_ERROR);
                    }
                }
            }
        }
        let n = native.output.borrow();
        let h = fpga.output.lock().unwrap();
        assert_eq!(h.len(), n.len(), "output frame count, step {index}");
        if compare_audio {
            let errors = n
                .iter()
                .flatten()
                .zip(h.iter().flatten())
                .map(|(n, h)| (n - h).abs())
                .collect::<Vec<_>>();
            let max = errors.iter().copied().fold(0f32, f32::max);
            let rms = (errors.iter().map(|&e| f64::from(e).powi(2)).sum::<f64>()
                / errors.len() as f64)
                .sqrt();
            assert!(
                max <= FLOAT_MAX_ERROR && rms <= f64::from(FLOAT_RMS_ERROR),
                "step {index}: max={max}, rms={rms}"
            );
        } else {
            assert!(h.iter().flatten().all(|&s| s == 0.), "step {index}");
        }
    }
    assert!(fpga.failure.lock().unwrap().is_none());
    let quanta = fpga.hardware.frame() / 32;
    let metrics = fpga.hardware.simulation_metrics();
    eprintln!(
        "{transport:?}: {quanta} quanta, {:.1} register accesses and {:.0} simulated clock cycles per quantum",
        metrics.bus_accesses as f64 / quanta as f64,
        metrics.cycles as f64 / quanta as f64
    );
}

#[shoop_test]
fn app_replay_matches_native_through_the_bridge() {
    replay_against_native(Transport::Bridge);
}

#[shoop_test]
fn app_replay_with_core_generated_audio_matches_native_loop_observations() {
    replay_against_native(Transport::ExternalBridge);
}

#[shoop_test]
fn application_records_and_plays_through_the_bridge() {
    use shoop_app_api::{LoopAction, LoopMode};
    let mut fpga =
        Fpga::with_transport(artifacts(), RunMode::Stepped, None, Transport::Bridge).unwrap();
    fpga.hardware.signal(true).unwrap();
    fpga.app.apply(&Action::ConnectInput).unwrap();
    fpga.app.pump(20);
    fpga.app.apply(&Action::ConnectOutput).unwrap();
    fpga.app.pump(20);
    fpga.app.immediate().unwrap();
    fpga.app.action(LoopAction::RecordClicked).unwrap();
    fpga.app.pump(24);
    assert_eq!(fpga.app.loop_state().mode, LoopMode::Recording);
    fpga.app.action(LoopAction::PlayClicked).unwrap();
    fpga.app.pump(40);
    assert_eq!(
        fpga.app.loop_state().mode,
        LoopMode::Playing,
        "{:?}",
        fpga.failure.lock().unwrap()
    );
    assert!(fpga.app.loop_state().length_frames > 512);
    assert!(
        fpga.output
            .lock()
            .unwrap()
            .iter()
            .flatten()
            .any(|s| s.abs() > 0.0001)
    );
    assert!(fpga.failure.lock().unwrap().is_none());
}
