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

fn composite_layout(
    endpoint: impl shoop_rt_transport::carrier::Endpoint,
    mut renderer: impl rt_runtime::host::Renderer,
) {
    use rt_runtime::buffer::{AudioBlock, MidiBlock};
    use shoop_backend::{
        Backend, BackendCompositeKind, BackendCompositeOutcome, BackendCompositeSpan,
        BackendCompositeTarget, BackendLoopMode, BackendPortRole, BackendPreparedCompositePlan,
        DirectTrackRequest,
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
            initial_loops: 2,
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
    let [parent, child] = track.loops.as_slice() else {
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
    repeat.iterations = 4;
    repeat.spans = vec![BackendCompositeSpan {
        target: BackendCompositeTarget::Loop(*child),
        start: 0,
        end: 4,
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
    let clear = backend.clear_composite_loop(parent).unwrap();
    let cleared = wait(&mut backend, &|s| s.removed_composites.contains(&parent));
    assert_eq!(cleared.loops[&clear].mode, BackendLoopMode::Stopped);
    let standalone = backend.create_composite_loop().unwrap();
    wait(&mut backend, &|s| s.composites.contains_key(&standalone));
    backend.remove_composite_loop(standalone).unwrap();
    wait(&mut backend, &|s| {
        s.removed_composites.contains(&standalone)
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
