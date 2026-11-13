#![cfg(all(feature = "sim", target_os = "linux"))]
mod support;
use shoop_app_api::{LoopAction, LoopMode};
use shoop_wasm_test_support::shoop_test;
use support::app::Native;
#[shoop_test]
fn actual_app_bootstrap_and_sync_loop_contract() {
    let mut native = Native::new().unwrap();
    let snapshot = native.app.snapshot();
    assert_eq!(snapshot.tracks.len(), 1);
    assert_eq!(snapshot.tracks[0].name, "Sync");
    assert_eq!(snapshot.tracks[0].loops.len(), 1);
    assert!(native.app.loop_state().empty);
    native.app.immediate().unwrap();
    native.app.action(LoopAction::RecordClicked).unwrap();
    native.app.pump(20);
    assert_eq!(native.app.loop_state().mode, LoopMode::Recording);
    native.app.action(LoopAction::PlayClicked).unwrap();
    native.app.pump(20);
    assert_eq!(native.app.loop_state().mode, LoopMode::Playing);
    assert!(native.app.loop_state().length_frames > 0);
    native.app.action(LoopAction::StopClicked).unwrap();
    native.app.pump(4);
    assert_eq!(native.app.loop_state().mode, LoopMode::Stopped);
    let trace = native.trace.borrow();
    let descriptions = trace
        .iter()
        .map(|entry| entry.description.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(descriptions.contains("StageGraph"));
    assert!(descriptions.contains("ConfigurationActivate"));
    assert!(descriptions.contains("Smoothing"));
    assert!(
        trace
            .iter()
            .any(|entry| entry.graph.as_ref().is_some_and(|g| g.steps.len() > 3))
    );
    let output =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../work/fpga-app-contract");
    std::fs::create_dir_all(&output).unwrap();
    std::fs::write(
        output.join("commands.txt"),
        trace
            .iter()
            .map(|entry| {
                format!(
                    "{}\npayload={}\ngraph={:?}\n",
                    entry.description,
                    entry.payload.len(),
                    entry.graph
                )
            })
            .collect::<String>(),
    )
    .unwrap();
    std::fs::write(output.join("startup.txt"), format!("{snapshot:#?}")).unwrap();
}

#[shoop_test]
fn common_app_trace_covers_real_input_controls_waveform_clear_and_scheduled_stop() {
    let trace: support::app::Trace = serde_json::from_str(include_str!(
        "../../rt_fpga_sim/fixtures/app-sync-loop.json"
    ))
    .unwrap();
    let mut native = Native::new().unwrap();
    let states = native.replay(&trace).unwrap();
    assert_eq!(states[3].mode, LoopMode::Recording);
    assert_eq!(states[4].mode, LoopMode::Playing);
    assert!(states[4].length_frames > 512);
    assert!(states[11].empty);
    assert_eq!(states[15].mode, LoopMode::Stopped);
    assert!(
        native
            .output
            .borrow()
            .iter()
            .flatten()
            .any(|sample| sample.abs() > 0.0001)
    );
    let commands = native
        .trace
        .borrow()
        .iter()
        .map(|entry| entry.description.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(commands.contains("Clear"));
    assert!(commands.contains("ReadStart"));
    assert!(commands.contains("AfterTicks"));
    assert!(commands.contains("LinearRestart { frames: 144 }"));
    let output =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../work/fpga-app-contract");
    std::fs::write(
        output.join("replay.txt"),
        format!("{states:#?}\n{commands}"),
    )
    .unwrap();
}

#[shoop_test]
fn native_long_recording_still_supports_details_selection() {
    use support::app::Action;
    let mut native = Native::new().unwrap();
    for action in [
        Action::ConnectInput,
        Action::ConnectOutput,
        Action::GlobalSync { enabled: false },
    ] {
        native.app.apply(&action).unwrap();
        native.app.pump(20);
    }
    native.app.apply(&Action::Record).unwrap();
    native.app.pump(4);
    let mut remaining = 240_000;
    while remaining > 0 {
        let n = remaining.min(512);
        native.render.borrow_mut().process(n).unwrap();
        remaining -= n;
    }
    native.app.apply(&Action::Stop).unwrap();
    native.app.pump(8);
    eprintln!(
        "before details: {:?}, client={:?}",
        native.app.loop_state(),
        native.app.client.state()
    );
    native.app.apply(&Action::Details).unwrap();
    native.app.pump(600);
    eprintln!(
        "after details: {:?}, client={:?}, service={:?}",
        native.app.loop_state(),
        native.app.client.state(),
        native.app.service.failure()
    );
    assert!(native.app.snapshot().details.is_some());
}
