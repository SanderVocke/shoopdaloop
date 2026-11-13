#![cfg(all(feature = "app", not(target_arch = "wasm32")))]
use rt_fpga_sim::{app::Fpga, app_worker::RunMode};
use shoop_wasm_test_support::shoop_test;
#[shoop_test]
fn application_bootstraps_its_real_sync_graph_on_rtl() {
    let artifacts =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../work/marlin-app");
    let mut fpga = Fpga::new(artifacts, RunMode::Stepped).unwrap();
    assert_eq!(fpga.app.snapshot().tracks[0].name, "Sync");
    let drivers = fpga.app.snapshot().audio_drivers.clone();
    assert!(!drivers.supported);
    assert_eq!(
        drivers.active.unwrap().configured,
        shoop_app_api::AudioDriverConfig::Simulated
    );
    assert_eq!(fpga.hardware.store_usage().slot_limit, 32768);
    assert!(fpga.app.loop_state().empty);
    fpga.app.pump(20);
    assert!(
        fpga.failure.lock().unwrap().is_none(),
        "{:?}",
        fpga.failure.lock().unwrap()
    );
    let commands = fpga
        .trace
        .borrow()
        .iter()
        .map(|entry| entry.description.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let output =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../work/fpga-app-contract");
    std::fs::create_dir_all(&output).unwrap();
    std::fs::write(
        output.join("rtl-startup.txt"),
        format!("{commands}\n{:#?}", fpga.app.snapshot()),
    )
    .unwrap();
    assert!(commands.contains("ConfigurationActivate"));
    assert!(commands.contains("GraphActivate"));
}

#[shoop_test]
fn application_records_and_plays_through_rtl() {
    use shoop_app_api::{LoopAction, LoopMode};
    let artifacts =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../work/marlin-app");
    let mut fpga = Fpga::new(artifacts, RunMode::Stepped).unwrap();
    fpga.hardware.signal(true).unwrap();
    fpga.app
        .apply(&rt_fpga_sim::app::Action::ConnectInput)
        .unwrap();
    fpga.app.pump(20);
    fpga.app
        .apply(&rt_fpga_sim::app::Action::ConnectOutput)
        .unwrap();
    fpga.app.pump(20);
    fpga.app.immediate().unwrap();
    fpga.app.action(LoopAction::RecordClicked).unwrap();
    fpga.app.pump(24);
    assert_eq!(
        fpga.app.loop_state().mode,
        LoopMode::Recording,
        "{:?}",
        fpga.failure.lock().unwrap()
    );
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
    fpga.app.apply(&rt_fpga_sim::app::Action::Details).unwrap();
    fpga.app.pump(20);
    assert!(fpga.failure.lock().unwrap().is_none());
    let messages = fpga.messages.borrow().join("\n");
    let output = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../work/fpga-app-contract/rtl-recording.txt");
    std::fs::write(output, messages).unwrap();
}

#[shoop_test]
fn common_app_replay_matches_native_audio_and_loop_observations() {
    use rt_fpga_sim::app::{FLOAT_MAX_ERROR, FLOAT_RMS_ERROR, Native, Trace};
    let artifacts =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../work/marlin-app");
    let mut fpga = Fpga::new(artifacts, RunMode::Stepped).unwrap();
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
        eprintln!(
            "step {index}: native={:?}, hardware={:?}",
            (n.mode, n.empty, n.length_frames, n.position),
            (h.mode, h.empty, h.length_frames, h.position)
        );
        assert_eq!(
            (h.mode, h.empty, h.length_frames, h.position),
            (n.mode, n.empty, n.length_frames, n.position),
            "step {index}; failures: {:?}",
            fpga.failure.lock().unwrap()
        );
        assert_eq!(
            (
                h.content_revision,
                h.repeat_sync,
                h.next_mode,
                h.next_transition_delay
            ),
            (
                n.content_revision,
                n.repeat_sync,
                n.next_mode,
                n.next_transition_delay
            ),
            "step {index}: transition/revision observations"
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
            assert!(!n.channels.is_empty());
            for (n, h) in n.channels.iter().zip(&h.channels) {
                assert_eq!(h.samples.len(), n.samples.len());
                assert!(!n.samples.is_empty());
                for (n, h) in n.samples.iter().zip(h.samples.iter()) {
                    assert!((n - h).abs() <= FLOAT_MAX_ERROR);
                }
            }
        }
        let n = native.output.borrow();
        let h = fpga.output.lock().unwrap();
        assert_eq!(h.len(), n.len(), "output frame count, step {index}");
        let errors = n
            .iter()
            .flatten()
            .zip(h.iter().flatten())
            .map(|(n, h)| (n - h).abs())
            .collect::<Vec<_>>();
        let max = errors.iter().copied().fold(0f32, f32::max);
        let rms = (errors.iter().map(|&e| f64::from(e).powi(2)).sum::<f64>() / errors.len() as f64)
            .sqrt();
        eprintln!("step {index}: max={max}, rms={rms}");
        assert!(
            max <= FLOAT_MAX_ERROR && rms <= f64::from(FLOAT_RMS_ERROR),
            "step {index}: max={max}, rms={rms}"
        );
    }
    assert!(fpga.failure.lock().unwrap().is_none());
}

#[shoop_test]
fn unpaced_worker_advances_without_clock_ticks_or_retaining_output() {
    use rt_fpga_sim::app_worker::Worker;
    let artifacts =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../work/marlin-app");
    let prepared = Worker::prepare(artifacts, RunMode::Continuous).unwrap();
    let handle = prepared.execution.handle();
    let first = handle.frame();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while handle.frame() < first + 128 && std::time::Instant::now() < deadline {
        std::thread::yield_now();
    }
    assert!(
        handle.frame() >= first + 128,
        "{:?}",
        prepared.execution.failure.lock().unwrap()
    );
    assert!(prepared.execution.output.lock().unwrap().is_empty());
    assert!(prepared.execution.failure.lock().unwrap().is_none());
}

#[shoop_test]
fn application_store_holds_a_five_second_take_while_recording_another_and_reuses_it() {
    use rt_fpga_sim::app::Action;
    use shoop_app_api::LoopMode;
    let artifacts =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../work/marlin-app");
    let mut fpga = Fpga::new(artifacts, RunMode::Stepped).unwrap();
    fpga.hardware.signal(true).unwrap();
    for action in [
        Action::ConnectInput,
        Action::ConnectOutput,
        Action::GlobalSync { enabled: false },
    ] {
        fpga.app.apply(&action).unwrap();
        fpga.app.pump(20);
    }
    fpga.app.apply(&Action::Record).unwrap();
    fpga.app.pump(4);
    fpga.hardware.run(240_000).unwrap();
    fpga.app.apply(&Action::Stop).unwrap();
    fpga.app.pump(8);
    eprintln!(
        "five-second take finalized: failure={:?}",
        fpga.failure.lock().unwrap()
    );
    let first = fpga.app.loop_state();
    assert_eq!(
        first.mode,
        LoopMode::Stopped,
        "{:?}",
        fpga.failure.lock().unwrap()
    );
    assert!(first.length_frames >= 240_000);
    fpga.app.apply(&Action::Details).unwrap();
    wait_details(&mut fpga);
    eprintln!("first waveform loaded");
    let snapshot = fpga.app.snapshot();
    let details = snapshot.details.as_ref().unwrap();
    assert!(!details.loading);
    assert_eq!(
        details.channels[0].samples.len() as u64,
        first.length_frames
    );
    let samples = details.channels[0].samples.clone();
    assert!(samples.iter().any(|sample| sample.abs() > 0.0001));
    fpga.hardware.signal(false).unwrap();
    fpga.app.apply(&Action::Record).unwrap();
    fpga.app.pump(4);
    fpga.hardware.run(240_000).unwrap();
    fpga.app.pump(4);
    eprintln!("second five-second take captured");
    let peak = fpga.hardware.store_usage();
    eprintln!("retained-take store usage: {peak:?}");
    assert_eq!(peak.slot_limit, 32768);
    assert!(peak.live_slots() >= 30_000);
    assert!(peak.high_water_slots <= peak.slot_limit);
    let recording = fpga.app.loop_state();
    assert_eq!(recording.mode, LoopMode::Recording);
    assert_eq!(
        recording.channel_timings[0].data_length,
        first.length_frames
    );
    assert!(recording.length_frames >= 240_000);
    assert!(fpga.failure.lock().unwrap().is_none());
    fpga.app.apply(&Action::Stop).unwrap();
    fpga.app.pump(8);
    wait_details(&mut fpga);
    let second = fpga.app.loop_state();
    assert!(second.content_revision > first.content_revision);
    assert!(second.length_frames >= 240_000);
    let snapshot = fpga.app.snapshot();
    let details = snapshot.details.as_ref().unwrap();
    assert!(!details.loading);
    assert_eq!(
        details.channels[0].samples.len() as u64,
        second.length_frames
    );
    assert!(
        details.channels[0]
            .samples
            .iter()
            .all(|&sample| sample == 0.)
    );
    assert!(samples.iter().any(|sample| sample.abs() > 0.0001));
    fpga.app.apply(&Action::Clear).unwrap();
    fpga.app.pump(8);
    assert!(fpga.app.loop_state().empty);
    fpga.hardware.signal(true).unwrap();
    fpga.app.apply(&Action::Record).unwrap();
    fpga.app.pump(100);
    fpga.app.apply(&Action::Play).unwrap();
    fpga.app.pump(100);
    assert_eq!(fpga.app.loop_state().mode, LoopMode::Playing);
    assert!(fpga.failure.lock().unwrap().is_none());
    assert!(fpga.hardware.store_usage().high_water_slots <= peak.high_water_slots + 4);
}

#[shoop_test]
fn medium_take_reports_finalization_diagnostics() {
    use rt_fpga_sim::app::Action;
    let artifacts =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../work/marlin-app");
    let mut fpga = Fpga::new(artifacts, RunMode::Stepped).unwrap();
    fpga.hardware.signal(true).unwrap();
    for action in [
        Action::ConnectInput,
        Action::ConnectOutput,
        Action::GlobalSync { enabled: false },
        Action::Record,
    ] {
        fpga.app.apply(&action).unwrap();
        fpga.app.pump(8);
    }
    fpga.hardware.run(16384).unwrap();
    fpga.app.apply(&Action::Stop).unwrap();
    fpga.app.pump(8);
    assert!(
        fpga.failure.lock().unwrap().clone().is_none(),
        "{:?}",
        fpga.failure.lock().unwrap()
    );
    assert_eq!(fpga.app.loop_state().mode, shoop_app_api::LoopMode::Stopped);
}

#[shoop_test]
fn default_sync_settings_support_record_then_play_without_control_shortcuts() {
    use rt_fpga_sim::app::{Action, Native};
    let artifacts =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../work/marlin-app");
    let mut fpga = Fpga::new(artifacts, RunMode::Stepped).unwrap();
    let mut native = Native::new().unwrap();
    for action in [Action::Record, Action::Play, Action::Stop] {
        native.app.apply(&action).unwrap();
        fpga.app.apply(&action).unwrap();
        native.app.pump(24);
        fpga.app.pump(24);
        let n = native.app.loop_state();
        let h = fpga.app.loop_state();
        assert_eq!(
            (
                h.mode,
                h.length_frames,
                h.next_mode,
                h.next_transition_delay
            ),
            (
                n.mode,
                n.length_frames,
                n.next_mode,
                n.next_transition_delay
            )
        );
        eprintln!("{action:?}: {n:?}");
    }
}

#[shoop_test]
fn app_controls_retarget_default_ramps_and_monitoring_without_recompiling() {
    use rt_fpga_sim::app::{Action, FLOAT_MAX_ERROR, Native};
    let artifacts =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../work/marlin-app");
    let mut fpga = Fpga::new(artifacts, RunMode::Stepped).unwrap();
    let mut native = Native::new().unwrap();
    fpga.hardware.signal(true).unwrap();
    *native.signal.borrow_mut() = true;
    let actions = [
        (Action::ConnectInput, 12),
        (Action::ConnectOutput, 12),
        (Action::GlobalSync { enabled: false }, 4),
        (Action::Monitor { enabled: true }, 8),
        (Action::Record, 4),
        (Action::Play, 8),
        (Action::InputGain { value: 0.75 }, 2),
        (Action::InputGain { value: 0.5 }, 3),
        (Action::InputBalance { value: 0.3 }, 3),
        (Action::OutputGain { value: 1.75 }, 3),
        (Action::OutputBalance { value: -0.25 }, 3),
        (Action::MasterGain { value: 0.25 }, 2),
        (Action::MasterGain { value: 1.5 }, 3),
        (Action::MasterBalance { value: 0.5 }, 3),
        (Action::LoopGain { value: 0.625 }, 3),
        (Action::LoopBalance { value: 0.25 }, 3),
        (Action::Mute { enabled: true }, 2),
        (Action::Mute { enabled: false }, 4),
        (Action::MasterMute { enabled: true }, 2),
        (Action::MasterMute { enabled: false }, 4),
        (Action::Monitor { enabled: false }, 8),
        (Action::AutoMonitor { enabled: true }, 4),
        (Action::Stop, 6),
    ];
    let mut installed = 0;
    for (index, (action, ticks)) in actions.into_iter().enumerate() {
        native.app.apply(&action).unwrap();
        fpga.app.apply(&action).unwrap();
        native.app.pump(ticks);
        fpga.app.pump(ticks);
        if index == 1 {
            installed = fpga.hardware.metrics().compilations;
        }
        if index > 1 {
            assert_eq!(fpga.hardware.metrics().compilations, installed);
        }
        let n = native.app.loop_state();
        let h = fpga.app.loop_state();
        assert_eq!(
            (h.mode, h.length_frames, h.position),
            (n.mode, n.length_frames, n.position),
            "{action:?}"
        );
        let n = native.output.borrow();
        let h = fpga.output.lock().unwrap();
        assert_eq!(n.len(), h.len());
        let max = n
            .iter()
            .flatten()
            .zip(h.iter().flatten())
            .map(|(a, b)| (a - b).abs())
            .fold(0f32, f32::max);
        assert!(max <= FLOAT_MAX_ERROR, "{action:?}: max error={max}");
    }
}

fn wait_details(fpga: &mut Fpga) {
    for _ in 0..4000 {
        fpga.app.pump(1);
        if fpga
            .app
            .snapshot()
            .details
            .as_ref()
            .is_some_and(|d| !d.loading && !d.channels.is_empty())
        {
            return;
        }
        if let Some(error) = fpga.failure.lock().unwrap().as_ref() {
            panic!("waveform device fault: {error}");
        }
    }
    panic!("waveform did not complete: {:?}", fpga.app.snapshot());
}
#[shoop_test]
fn app_client_reconnect_keeps_the_graph_and_worker_drop_closes_the_service() {
    use rt_fpga_sim::app::Action;
    let artifacts =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../work/marlin-app");
    let mut fpga = Fpga::new(artifacts, RunMode::Stepped).unwrap();
    for action in [
        Action::GlobalSync { enabled: false },
        Action::Record,
        Action::Play,
    ] {
        fpga.app.apply(&action).unwrap();
        fpga.app.pump(12);
    }
    let before = fpga.app.loop_state();
    let compiled = fpga.hardware.metrics().compilations;
    fpga.app.apply(&Action::Details).unwrap();
    fpga.app.pump(1);
    fpga.app.reconnect().unwrap();
    let after = fpga.app.loop_state();
    assert_eq!(after.id, before.id);
    assert_eq!(
        (after.mode, after.length_frames),
        (before.mode, before.length_frames)
    );
    assert_eq!(fpga.hardware.metrics().compilations, compiled);
    assert!(fpga.failure.lock().unwrap().is_none());
    let worker = fpga.hardware.clone();
    drop(fpga);
    assert!(worker.signal(true).is_err());
}

#[shoop_test]
fn selected_waveform_refreshes_after_a_second_take() {
    use rt_fpga_sim::app::Action;
    let artifacts =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../work/marlin-app");
    let mut fpga = Fpga::new(artifacts, RunMode::Stepped).unwrap();
    fpga.hardware.signal(true).unwrap();
    for action in [
        Action::ConnectInput,
        Action::ConnectOutput,
        Action::GlobalSync { enabled: false },
        Action::Record,
    ] {
        fpga.app.apply(&action).unwrap();
        fpga.app.pump(12);
    }
    fpga.app.apply(&Action::Stop).unwrap();
    fpga.app.pump(8);
    fpga.app.apply(&Action::Details).unwrap();
    wait_details(&mut fpga);
    let first = fpga.app.snapshot().details.as_ref().unwrap().channels[0]
        .samples
        .clone();
    fpga.app.apply(&Action::Record).unwrap();
    fpga.app.pump(16);
    fpga.app.apply(&Action::Stop).unwrap();
    fpga.app.pump(8);
    wait_details(&mut fpga);
    let state = fpga.app.snapshot();
    let second = &state.details.as_ref().unwrap().channels[0].samples;
    assert_ne!(second.len(), first.len());
    assert!(fpga.failure.lock().unwrap().is_none());
}

#[shoop_test]
fn replacing_a_scheduled_stop_cancels_it_before_the_sync_pulse() {
    use rt_fpga_sim::app::{Action, Native};
    use shoop_app_api::LoopMode;
    let artifacts =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../work/marlin-app");
    let mut fpga = Fpga::new(artifacts, RunMode::Stepped).unwrap();
    let mut native = Native::new().unwrap();
    for (action, ticks) in [
        (Action::GlobalSync { enabled: false }, 4),
        (Action::Record, 24),
        (Action::Play, 4),
        (Action::GlobalSync { enabled: true }, 1),
        (Action::Stop, 1),
        (Action::Play, 1),
        (Action::Advance, 40),
    ] {
        native.app.apply(&action).unwrap();
        fpga.app.apply(&action).unwrap();
        native.app.pump(ticks);
        fpga.app.pump(ticks);
        let n = native.app.loop_state();
        let h = fpga.app.loop_state();
        assert_eq!(
            (h.mode, h.position, h.next_mode, h.next_transition_delay),
            (n.mode, n.position, n.next_mode, n.next_transition_delay),
            "{action:?}"
        );
    }
    assert_eq!(fpga.app.loop_state().mode, LoopMode::Playing);
    assert!(fpga.failure.lock().unwrap().is_none());
}
