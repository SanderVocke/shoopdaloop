#![cfg(all(feature = "fpga-reference", not(target_arch = "wasm32")))]
#[path = "../examples/fpga_reference_client.rs"]
mod client;
use shoop_wasm_test_support::shoop_test;

#[shoop_test]
fn application_client_live_loads_and_operates_through_rt_backend() {
    let output = tempfile::tempdir().unwrap();
    client::run_with(
        env!("CARGO_BIN_EXE_shoop_host").into(),
        output.path().into(),
    )
    .unwrap_or_else(|error| {
        let log = std::fs::read_to_string(output.path().join("host.log"))
            .unwrap_or_else(|error| format!("could not read host log: {error}"));
        panic!("{error:#}\nhost log:\n{log}");
    });
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output.path().join("report.json")).unwrap()).unwrap();
    assert_eq!(report["result"], "passed");
    assert_eq!(report["live_graph_updates"], 3);
    assert_eq!(report["recycled_logical_slots"], true);
    assert_eq!(report["composite_edit_verified"], true);
    assert!(report["recorded_frames"].as_u64().unwrap() > 0);
}

fn monitoring_layout(
    endpoint: impl shoop_rt_transport::carrier::Endpoint,
    mut renderer: impl rt_runtime::host::Renderer,
) {
    use rt_runtime::buffer::{AudioBlock, MidiBlock};
    use shoop_backend::{
        Backend, BackendLoopMode, BackendPortRole, BackendSnapshot, DirectTrackRequest,
    };
    use shoop_rt_backend::{NoWait, RtBackend};
    use shoop_rt_transport::{
        carrier::{InProcess, Outbox},
        client::ProtocolClient,
    };

    let client = ProtocolClient::new(InProcess::new(endpoint, Outbox::new(8, 8, 0)));
    let mut backend = RtBackend::connect(client, NoWait).unwrap();
    backend.pump();
    backend
        .set_snapshot_delivery(shoop_rt_backend::SnapshotDelivery::Pull)
        .unwrap();
    assert!(backend.supports_automatic_monitoring());
    let track = backend
        .create_direct_track(DirectTrackRequest {
            port_name_base: "Monitor".into(),
            audio_channels: 1,
            midi: false,
            initial_loops: 1,
        })
        .unwrap();
    let input = track
        .ports
        .iter()
        .find(|p| p.role == BackendPortRole::AudioInput)
        .unwrap();
    backend
        .set_port_connected(input.id, "reference_in", true)
        .unwrap();
    let output = track
        .ports
        .iter()
        .find(|p| p.role == BackendPortRole::AudioOutput)
        .unwrap();
    backend
        .set_port_connected(output.id, "reference_left", true)
        .unwrap();
    backend
        .set_track_automatic_monitoring(track.track_id, true)
        .unwrap();
    backend.activate_prepared_graph().unwrap();
    let activation = backend.mutation_cursor();
    let mut wait = |backend: &mut RtBackend<_, _>,
                    predicate: &dyn Fn(&BackendSnapshot, f32) -> bool| {
        for _ in 0..320 {
            let mut left = [0.; 32];
            renderer
                .process(
                    AudioBlock {
                        frame: renderer.next_frame(),
                        frames: 32,
                        inputs: &[&[0.25; 32]],
                        outputs: &mut [&mut left, &mut [0.; 32]],
                    },
                    MidiBlock {
                        inputs: &[],
                        outputs: &mut [],
                    },
                )
                .unwrap();
            let snapshot = backend.poll().unwrap();
            if predicate(&snapshot, left[0]) {
                return snapshot;
            }
        }
        panic!("prepared monitoring change did not settle");
    };
    let track_id = track.track_id;
    let voice = track.loops[0];
    let initial = wait(&mut backend, &|s, sample| {
        sample == 0.0
            && s.mutation_completions
                .iter()
                .any(|receipt| receipt.sequence == activation && receipt.result.is_ok())
    });
    assert!(initial.tracks[&track_id].automatic_monitoring_enabled);
    assert!(!initial.tracks[&track_id].automatic_input_monitoring);
    backend
        .transition_loop(voice, BackendLoopMode::Recording, None)
        .unwrap();
    wait(&mut backend, &|s, sample| {
        s.tracks[&track_id].automatic_input_monitoring && sample == 0.25
    });
    for enabled in [false, true] {
        backend
            .set_track_automatic_monitoring(track_id, enabled)
            .unwrap();
        let expected = if enabled { 0.25 } else { 0.0 };
        let changed = wait(&mut backend, &|s, sample| {
            s.tracks[&track_id].automatic_monitoring_enabled == enabled && sample == expected
        });
        assert_eq!(
            changed.tracks[&track_id].automatic_input_monitoring,
            enabled
        );
        assert!(!changed.tracks[&track_id].input_monitoring);
        assert_eq!(changed.loops[&voice].mode, BackendLoopMode::Recording);
    }
    backend
        .transition_loop(voice, BackendLoopMode::Stopped, None)
        .unwrap();
    let stopped = wait(&mut backend, &|s, sample| {
        s.loops[&voice].mode == BackendLoopMode::Stopped && sample == 0.0
    });
    assert!(!stopped.tracks[&track_id].automatic_input_monitoring);
    assert!(stopped.loops[&voice].length > 0);
}

#[shoop_test]
fn application_can_change_automatic_monitoring_during_recording() {
    use shoop_rt_protocol::{DriverKind, DriverRunState, DriverStatus};
    let driver = DriverStatus {
        kind: DriverKind::Dummy,
        state: DriverRunState::Running,
        sample_rate: 48_000,
        buffer_frames: 32,
    };
    let rust = shoop_rt_host_reference::prepare(32, driver, 1).unwrap();
    monitoring_layout(rust.endpoint, rust.renderer);
    #[cfg(feature = "rtl-sim")]
    {
        let rtl = shoop_rt_host_marlin::prepare(32, driver, 1, 32).unwrap();
        monitoring_layout(rtl.endpoint, rtl.renderer);
    }
}

fn composite_layout(
    endpoint: impl shoop_rt_transport::carrier::Endpoint,
    mut renderer: impl rt_runtime::host::Renderer,
) {
    use rt_runtime::buffer::{AudioBlock, MidiBlock};
    use shoop_backend::{
        Backend, BackendCompositeKind, BackendCompositeOutcome, BackendCompositePending,
        BackendCompositeSpan, BackendCompositeTarget, BackendLoopMode, BackendPortRole,
        BackendPreparedCompositePlan, DirectTrackRequest,
    };
    use shoop_rt_backend::{NoWait, RtBackend};
    use shoop_rt_transport::{
        carrier::{InProcess, Outbox},
        client::ProtocolClient,
    };
    let client = ProtocolClient::new(InProcess::new(endpoint, Outbox::new(8, 8, 0)));
    let mut backend = RtBackend::connect(client, NoWait).unwrap();
    backend.pump();
    backend
        .set_snapshot_delivery(shoop_rt_backend::SnapshotDelivery::Pull)
        .unwrap();
    assert!(backend.is_ready());
    assert!(backend.supports_composite_loops());
    let track = backend
        .create_direct_track(DirectTrackRequest {
            port_name_base: "Arrangement".into(),
            audio_channels: 1,
            midi: false,
            initial_loops: 3,
        })
        .unwrap();
    let input = track
        .ports
        .iter()
        .find(|p| p.role == BackendPortRole::AudioInput)
        .unwrap();
    backend
        .set_port_connected(input.id, "reference_in", true)
        .unwrap();
    backend.set_loop_repeat_sync(track.loops[0], true).unwrap();
    backend.activate_prepared_graph().unwrap();
    let activation = backend.mutation_cursor();
    let render = |renderer: &mut dyn rt_runtime::host::Renderer| {
        let mut left = [0.; 32];
        let mut right = left;
        renderer
            .process(
                AudioBlock {
                    frame: renderer.next_frame(),
                    frames: 32,
                    inputs: &[&[0.25; 32]],
                    outputs: &mut [&mut left, &mut right],
                },
                MidiBlock {
                    inputs: &[],
                    outputs: &mut [],
                },
            )
            .unwrap();
    };
    let mut wait =
        |backend: &mut RtBackend<_, _>,
         predicate: &dyn Fn(&shoop_backend::BackendSnapshot) -> bool| {
            for _ in 0..320 {
                render(&mut renderer);
                let snapshot = backend.poll().unwrap();
                if predicate(&snapshot) {
                    return snapshot;
                }
            }
            panic!("prepared composite mutation did not settle");
        };
    let activated = wait(&mut backend, &|s| {
        s.mutation_completions
            .iter()
            .any(|receipt| receipt.sequence == activation)
    });
    assert_eq!(
        activated
            .mutation_completions
            .iter()
            .find(|receipt| receipt.sequence == activation)
            .unwrap()
            .result,
        Ok(())
    );
    let [parent, child, clock] = track.loops.as_slice() else {
        unreachable!()
    };
    let plan = BackendPreparedCompositePlan {
        kind: BackendCompositeKind::Script,
        sync_source: None,
        unit_frames: 32,
        iterations: 64,
        entries: 2,
        spans: vec![
            BackendCompositeSpan {
                target: BackendCompositeTarget::Loop(*child),
                start: 0,
                end: 2,
                occurrence_start: 0,
                ordinal: 0,
                mode: BackendLoopMode::Recording,
            },
            BackendCompositeSpan {
                target: BackendCompositeTarget::Loop(*child),
                start: 2,
                end: 64,
                occurrence_start: 2,
                ordinal: 1,
                mode: BackendLoopMode::Playing,
            },
        ],
    };
    let (parent, version) = backend
        .convert_empty_loop_to_prepared_composite(*parent, &plan)
        .unwrap();
    backend
        .transition_composite_loop(parent, BackendLoopMode::Playing, None, None)
        .unwrap();
    let applied = wait(&mut backend, &|s| {
        s.composite_receipts
            .iter()
            .any(|receipt| receipt.version == version)
    });
    assert!(
        applied.composite_receipts.iter().any(|receipt| {
            receipt.version == version
                && matches!(receipt.outcome, BackendCompositeOutcome::Applied { .. })
        }),
        "{:?}",
        applied.composite_receipts
    );
    assert_eq!(applied.composites[&parent].active_plan_version, version);
    assert_eq!(applied.composites[&parent].length, 64 * 32);
    wait(&mut backend, &|s| {
        s.composites[&parent].mode == BackendLoopMode::Playing
            && s.loops[child].mode == BackendLoopMode::Playing
    });
    backend
        .transition_composite_loop(parent, BackendLoopMode::Stopped, None, None)
        .unwrap();
    wait(&mut backend, &|s| {
        s.composites[&parent].mode == BackendLoopMode::Stopped
            && s.loops[child].mode == BackendLoopMode::Stopped
    });
    let cancelled = backend
        .configure_prepared_composite_loop(parent, &plan)
        .unwrap();
    backend.cancel_composite_plan(parent, cancelled).unwrap();
    let stopped = wait(&mut backend, &|s| {
        s.composite_receipts.iter().any(|receipt| {
            receipt.version == cancelled && receipt.outcome == BackendCompositeOutcome::Cancelled
        })
    });
    assert_eq!(stopped.composites[&parent].active_plan_version, version);
    let mut repeat = plan;
    repeat.kind = BackendCompositeKind::Regular;
    repeat.entries = 1;
    repeat.iterations = 64;
    repeat.spans = vec![BackendCompositeSpan {
        target: BackendCompositeTarget::Loop(*child),
        start: 0,
        end: 64,
        occurrence_start: 0,
        ordinal: 0,
        mode: BackendLoopMode::Playing,
    }];
    let updated = backend
        .configure_prepared_composite_loop(parent, &repeat)
        .unwrap();
    wait(&mut backend, &|s| {
        s.composites[&parent].active_plan_version == updated
    });
    backend.set_composite_repeat_sync(parent, true).unwrap();
    wait(&mut backend, &|s| s.composites[&parent].repeat_sync);
    backend
        .transition_composite_loop(parent, BackendLoopMode::Playing, None, None)
        .unwrap();
    wait(&mut backend, &|s| {
        s.composites[&parent].mode == BackendLoopMode::Playing
    });
    let mut edit = repeat.clone();
    edit.spans[0].start = 8;
    edit.spans[0].occurrence_start = 8;
    for supersede in [false, true] {
        let candidate = backend
            .configure_prepared_composite_loop(parent, &edit)
            .unwrap();
        wait(&mut backend, &|s| {
            s.composites[&parent].pending_plan_version == Some(candidate)
                && s.composites[&parent].pending_stage
                    == Some(BackendCompositePending::AwaitingRestart)
        });
        backend.set_loop_repeat_sync(*child, !supersede).unwrap();
        let control_sequence = backend.mutation_cursor();
        let active = wait(&mut backend, &|s| {
            s.mutation_completions
                .iter()
                .any(|receipt| receipt.sequence == control_sequence)
        });
        assert!(active
            .mutation_completions
            .iter()
            .any(|receipt| { receipt.sequence == control_sequence && receipt.result.is_ok() }));
        assert_eq!(
            active.composites[&parent].pending_plan_version,
            Some(candidate)
        );
        assert_eq!(active.loops[child].repeat_sync, !supersede);
        let expected = if supersede {
            backend
                .configure_prepared_composite_loop(parent, &repeat)
                .unwrap();
            BackendCompositeOutcome::Superseded
        } else {
            backend.cancel_composite_plan(parent, candidate).unwrap();
            BackendCompositeOutcome::Cancelled
        };
        let cancel_sequence = backend.mutation_cursor();
        let cancelled = wait(&mut backend, &|s| {
            s.composite_receipts
                .iter()
                .any(|receipt| receipt.version == candidate)
        });
        assert_eq!(
            cancelled
                .composite_receipts
                .iter()
                .find(|receipt| receipt.version == candidate)
                .unwrap()
                .outcome,
            expected,
        );
        assert_eq!(cancelled.composites[&parent].active_plan_version, updated);
        assert_eq!(cancelled.loops[child].mode, BackendLoopMode::Playing);
        if !supersede {
            assert!(cancelled
                .mutation_completions
                .iter()
                .any(|receipt| { receipt.sequence == cancel_sequence && receipt.result.is_ok() }));
        }
    }
    wait(&mut backend, &|s| {
        s.composites[&parent].active_plan_version > updated
    });
    let mut resized = edit;
    resized.iterations = 80;
    resized.spans[0].end = 80;
    let resized_version = backend
        .configure_prepared_composite_loop(parent, &resized)
        .unwrap();
    let pending = wait(&mut backend, &|s| {
        s.composites[&parent].pending_plan_version == Some(resized_version)
            && s.composites[&parent].pending_stage == Some(BackendCompositePending::AwaitingRestart)
    });
    assert_eq!(pending.composites[&parent].length, 64 * 32);
    let applied = wait(&mut backend, &|s| {
        s.composites[&parent].active_plan_version == resized_version
    });
    let composite = &applied.composites[&parent];
    assert_eq!(composite.length, 80 * 32);
    assert_eq!(composite.mode, BackendLoopMode::Playing);
    assert!(composite.repeat_sync);

    let target = shoop_backend::BackendLoopId::from_raw(parent.raw());
    resized.spans[0].start = 0;
    resized.spans[0].occurrence_start = 0;
    let candidate = backend
        .configure_prepared_composite_loop(parent, &resized)
        .unwrap();
    let before_sync = wait(&mut backend, &|s| {
        s.composites[&parent].pending_plan_version == Some(candidate)
            && s.composites[&parent].pending_stage == Some(BackendCompositePending::AwaitingRestart)
    });
    backend.set_loop_sync_source(target, Some(*clock)).unwrap();
    let sync_sequence = backend.mutation_cursor();
    let synced = wait(&mut backend, &|s| {
        s.mutation_completions
            .iter()
            .any(|receipt| receipt.sequence == sync_sequence)
    });
    assert_eq!(
        synced
            .mutation_completions
            .iter()
            .find(|receipt| receipt.sequence == sync_sequence)
            .unwrap()
            .result,
        Ok(())
    );
    let composite = &synced.composites[&parent];
    assert_eq!(composite.active_plan_version, resized_version);
    assert_eq!(composite.pending_plan_version, Some(candidate));
    assert!(composite.position >= before_sync.composites[&parent].position);
    assert_eq!(backend.loop_sync_source(target), Some(*clock));
    let applied = wait(&mut backend, &|s| {
        s.composites[&parent].active_plan_version == candidate
    });
    assert!(applied.composite_receipts.iter().any(|receipt| {
        receipt.version == candidate
            && matches!(receipt.outcome, BackendCompositeOutcome::Applied { .. })
    }));
    assert_eq!(backend.loop_sync_source(target), Some(*clock));
    backend.set_loop_sync_source(target, None).unwrap();
    let sync_sequence = backend.mutation_cursor();
    wait(&mut backend, &|s| {
        s.mutation_completions
            .iter()
            .any(|receipt| receipt.sequence == sync_sequence && receipt.result.is_ok())
    });
    backend
        .transition_composite_loop(parent, BackendLoopMode::Stopped, Some(1), None)
        .unwrap();
    let delayed_stop = backend.mutation_cursor();
    let scheduled = wait(&mut backend, &|s| {
        s.mutation_completions
            .iter()
            .any(|receipt| receipt.sequence == delayed_stop)
    });
    assert_eq!(
        scheduled
            .mutation_completions
            .iter()
            .find(|receipt| receipt.sequence == delayed_stop)
            .unwrap()
            .result,
        Ok(())
    );
    wait(&mut backend, &|s| {
        s.composites[&parent].mode == BackendLoopMode::Stopped
    });
    backend
        .transition_composite_loop(parent, BackendLoopMode::Playing, None, None)
        .unwrap();
    let playing = wait(&mut backend, &|s| {
        s.composites[&parent].mode == BackendLoopMode::Playing
            && s.loops[child].mode == BackendLoopMode::Playing
    });
    let child_length = playing.loops[child].length;
    let clear = backend.clear_composite_loop(parent).unwrap();
    let cleared = wait(&mut backend, &|s| s.removed_composites.contains(&parent));
    assert_eq!(cleared.loops[&clear].mode, BackendLoopMode::Stopped);
    assert_eq!(cleared.loops[child].mode, BackendLoopMode::Stopped);
    assert_eq!(cleared.loops[child].length, child_length);

    let standalone = backend.create_composite_loop().unwrap();
    wait(&mut backend, &|s| s.composites.contains_key(&standalone));
    let mut capture = repeat;
    capture.kind = BackendCompositeKind::Script;
    capture.spans[0].mode = BackendLoopMode::Recording;
    let version = backend
        .configure_prepared_composite_loop(standalone, &capture)
        .unwrap();
    wait(&mut backend, &|s| {
        s.composites[&standalone].active_plan_version == version
    });
    backend
        .transition_loop(clear, BackendLoopMode::Recording, None)
        .unwrap();
    backend
        .transition_composite_loop(standalone, BackendLoopMode::Playing, None, None)
        .unwrap();
    wait(&mut backend, &|s| {
        s.loops[child].mode == BackendLoopMode::Recording
            && s.loops[&clear].mode == BackendLoopMode::Recording
    });
    backend.remove_composite_loop(standalone).unwrap();
    let removed = wait(&mut backend, &|s| {
        s.removed_composites.contains(&standalone)
    });
    assert_eq!(removed.loops[child].mode, BackendLoopMode::Stopped);
    assert!(removed.loops[child].length > 0);
    assert_eq!(removed.loops[&clear].mode, BackendLoopMode::Recording);

    backend
        .transition_loop(clear, BackendLoopMode::Stopped, None)
        .unwrap();
    wait(&mut backend, &|s| {
        s.loops[&clear].mode == BackendLoopMode::Stopped && s.loops[&clear].length > 0
    });
    let nested = backend.create_composite_loop().unwrap();
    wait(&mut backend, &|s| s.composites.contains_key(&nested));
    let mut one_pass = capture;
    one_pass.spans[0].mode = BackendLoopMode::Playing;
    let nested_version = backend
        .configure_prepared_composite_loop(nested, &one_pass)
        .unwrap();
    wait(&mut backend, &|s| {
        s.composites[&nested].active_plan_version == nested_version
    });
    let outer = backend.create_composite_loop().unwrap();
    wait(&mut backend, &|s| s.composites.contains_key(&outer));
    let outer_version = backend
        .configure_prepared_composite_loop(
            outer,
            &BackendPreparedCompositePlan {
                kind: BackendCompositeKind::Regular,
                iterations: 128,
                spans: vec![BackendCompositeSpan {
                    target: BackendCompositeTarget::Composite(nested),
                    end: 128,
                    ..one_pass.spans[0].clone()
                }],
                ..one_pass.clone()
            },
        )
        .unwrap();
    wait(&mut backend, &|s| {
        s.composites[&outer].active_plan_version == outer_version
    });
    backend
        .transition_composite_loop(outer, BackendLoopMode::Playing, None, None)
        .unwrap();
    wait(&mut backend, &|s| {
        s.composites[&outer].mode == BackendLoopMode::Playing
            && s.composites[&nested].mode == BackendLoopMode::Playing
            && s.loops[child].mode == BackendLoopMode::Playing
    });

    one_pass.spans[0].target = BackendCompositeTarget::Loop(clear);
    let nested_version = backend
        .configure_prepared_composite_loop(nested, &one_pass)
        .unwrap();
    let switched = wait(&mut backend, &|s| {
        s.composites[&nested].active_plan_version == nested_version
    });
    assert_eq!(
        switched.composites[&outer].active_plan_version,
        outer_version
    );
    assert_eq!(switched.composites[&outer].mode, BackendLoopMode::Playing);
    assert_eq!(switched.composites[&nested].mode, BackendLoopMode::Stopped);
    assert_eq!(switched.loops[child].mode, BackendLoopMode::Stopped);
    wait(&mut backend, &|s| {
        s.composites[&nested].mode == BackendLoopMode::Playing
            && s.loops[&clear].mode == BackendLoopMode::Playing
    });
}

#[shoop_test]
fn application_edits_composite_plans_through_the_prepared_graph() {
    use shoop_rt_protocol::{DriverKind, DriverRunState, DriverStatus};
    let driver = DriverStatus {
        kind: DriverKind::Dummy,
        state: DriverRunState::Running,
        sample_rate: 48_000,
        buffer_frames: 32,
    };
    let rust = shoop_rt_host_reference::prepare(32, driver, 1).unwrap();
    composite_layout(rust.endpoint, rust.renderer);
    #[cfg(feature = "rtl-sim")]
    {
        let rtl = shoop_rt_host_marlin::prepare(32, driver, 1, 32).unwrap();
        composite_layout(rtl.endpoint, rtl.renderer);
    }
}
