//! Public adapter operations and confirmed plan identity over the production engine.
mod support;
use shoop_backend::{
    Backend, BackendCompositeConfig, BackendCompositeEntry, BackendCompositeKind,
    BackendCompositeOutcome, BackendCompositeSpan, BackendCompositeTarget, BackendLoopId,
    BackendLoopMode as Mode, BackendPreparedCompositePlan, DirectTrackRequest,
};
use shoop_wasm_test_support::shoop_test;
use support::*;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

fn setup() -> (Stack, Vec<BackendLoopId>) {
    let mut d = description();
    d.max_frames = 1;
    d.ramp_frames = 0;
    let mut backend = stack(&d);
    let track = backend
        .create_direct_track(DirectTrackRequest {
            port_name_base: "composites".into(),
            audio_channels: 1,
            midi: false,
            initial_loops: 5,
        })
        .unwrap();
    backend.wait_idle();
    for (id, length) in [
        (track.loops[0], 64),
        (track.loops[1], 4),
        (track.loops[2], 4),
    ] {
        backend.transition_loop(id, Mode::Recording, None).unwrap();
        backend.wait_idle();
        backend.clock_mut().render(length);
        backend.transition_loop(id, Mode::Stopped, None).unwrap();
        backend.wait_idle();
    }
    (backend, track.loops)
}
fn step(backend: &mut Stack, frames: usize) {
    for _ in 0..frames {
        backend.pump();
        backend.clock_mut().render(1);
    }
    backend.pump();
}
fn plan(
    child: BackendLoopId,
    source: Option<BackendLoopId>,
    iterations: u64,
) -> BackendPreparedCompositePlan {
    BackendPreparedCompositePlan {
        kind: BackendCompositeKind::Regular,
        sync_source: source,
        unit_frames: 2,
        iterations,
        entries: 1,
        spans: vec![BackendCompositeSpan {
            target: BackendCompositeTarget::Loop(child),
            start: 0,
            end: iterations,
            occurrence_start: 0,
            ordinal: 0,
            mode: Mode::Playing,
        }],
    }
}

#[shoop_test]
fn public_composite_creation_configuration_and_play_wait_for_the_first_accepted_plan() {
    let (mut backend, ids) = setup();
    assert!(backend.supports_composite_loops());
    let composite = backend.create_composite_loop().unwrap();
    let version = backend
        .configure_prepared_composite_loop(composite, &plan(ids[1], None, 4))
        .unwrap();
    backend
        .set_composite_play_after_record(composite, true)
        .unwrap();
    backend
        .transition_composite_loop(composite, Mode::Playing, None, None)
        .unwrap();
    backend.wait_idle();
    let state = backend.poll().unwrap();
    assert_eq!(state.composites[&composite].active_plan_version, version);
    assert_eq!(state.composites[&composite].mode, Mode::Playing);
    assert_eq!(state.loops[&ids[1]].mode, Mode::Playing);
    assert!(backend
        .set_composite_play_after_record(
            shoop_backend::BackendCompositeId::from_raw(u64::MAX),
            false
        )
        .is_err());
    assert!(backend
        .transition_composite_loop(composite, Mode::Recording, None, None)
        .is_err());
}

#[shoop_test]
fn confirmed_conversion_retains_public_identity_and_exact_activation_receipt() {
    let (mut backend, ids) = setup();
    let comp = shoop_backend::BackendCompositeId::from_raw(ids[3].raw());
    let version = backend
        .configure_prepared_composite_loop(comp, &plan(ids[1], Some(ids[0]), 4))
        .unwrap();
    let sequence = backend.mutation_cursor();
    assert!(backend.mutations_pending_through(sequence));
    assert!(!backend.poll().unwrap().composites.contains_key(&comp));
    backend.wait_idle();
    let snapshot = backend.poll().unwrap();
    assert!(!snapshot.loops.contains_key(&ids[3]));
    let state = &snapshot.composites[&comp];
    assert_eq!(
        (
            state.active_plan_version,
            state.length,
            state.prepared_unit_frames
        ),
        (version, 8, 2)
    );
    assert!(state.pending_plan_version.is_none());
    assert!(!backend.mutations_pending_through(sequence));
    let receipt = snapshot
        .composite_receipts
        .iter()
        .find(|r| r.version == version)
        .unwrap();
    let BackendCompositeOutcome::Applied { frame } = receipt.outcome else {
        panic!("{receipt:?}");
    };
    assert_eq!(state.active_applied_frame, Some(frame));
    assert!(snapshot
        .mutation_completions
        .iter()
        .any(|r| r.sequence == sequence && r.result.is_ok()));
    backend
        .transition_composite_loop(comp, Mode::Playing, None, None)
        .unwrap();
    step(&mut backend, 3);
    let state = backend.poll().unwrap().composites.remove(&comp).unwrap();
    assert_eq!((state.mode, state.position), (Mode::Playing, 1));
    assert_eq!(state.active_children.len(), 1);
    backend.set_composite_repeat_sync(comp, true).unwrap();
    backend.wait_idle();
    assert_eq!(backend.loop_sync_source(ids[3]), Some(ids[0]));
}

#[shoop_test]
fn profiling_and_playing_composite_state_remain_coherent_across_conversion_and_resync() {
    std::thread_local! {
        static TIME: core::cell::Cell<u64> = const { core::cell::Cell::new(0) };
    }
    fn now() -> u64 {
        TIME.with(|time| {
            let at = time.get();
            time.set(at + 10);
            at
        })
    }
    let (mut backend, ids) = setup();
    backend
        .clock_mut()
        .offline
        .render
        .set_profiling_clock(rt_profiling::Clock {
            now,
            resolution_ns: 10,
        });
    backend.set_profiling(true).unwrap();
    step(&mut backend, 32);
    let comp = shoop_backend::BackendCompositeId::from_raw(ids[3].raw());
    let version = backend
        .configure_prepared_composite_loop(comp, &plan(ids[1], None, 1000))
        .unwrap();
    backend.wait_idle();
    backend
        .transition_composite_loop(comp, Mode::Playing, None, None)
        .unwrap();
    step(&mut backend, 32);
    let before = backend.poll().unwrap();
    backend.request_resync().unwrap();
    step(&mut backend, 128);
    let after = backend.poll().unwrap();
    assert!(backend.is_ready());
    for snapshot in [&before, &after] {
        let state = &snapshot.composites[&comp];
        assert_eq!(state.active_plan_version, version);
        assert_eq!(state.mode, Mode::Playing);
        assert_eq!(state.length, 2000);
        assert!(!snapshot.loops.contains_key(&ids[3]));
        assert!(snapshot.profiling.active);
        let topology = snapshot.profiling.topology.as_ref().unwrap();
        assert_eq!(topology.generation, snapshot.profiling.generation);
        assert!(snapshot
            .profiling
            .groups
            .iter()
            .any(|group| group.kind == 2 && group.entity == ids[1].raw()));
    }
    assert!(after.composites[&comp].observed_frame > before.composites[&comp].observed_frame);
}

#[shoop_test]
fn conversion_and_clear_reset_primitive_timing_revisions() {
    let (mut backend, ids) = setup();
    backend.set_loop_length(ids[3], 4).unwrap();
    backend.wait_idle();
    backend.set_loop_length(ids[3], 0).unwrap();
    backend.wait_idle();
    assert!(backend.poll().unwrap().loops[&ids[3]].timing_revision > 0);
    let comp = shoop_backend::BackendCompositeId::from_raw(ids[3].raw());
    backend
        .configure_prepared_composite_loop(comp, &plan(ids[1], None, 4))
        .unwrap();
    backend.wait_idle();
    backend.clear_composite_loop(comp).unwrap();
    backend.wait_idle();
    backend.set_loop_length(ids[3], 8).unwrap();
    backend.wait_idle();
    let state = backend.poll().unwrap();
    assert!(
        state.mutation_failures.is_empty(),
        "{:?}",
        state.mutation_failures
    );
    assert_eq!(state.loops[&ids[3]].length, 8);
}

#[shoop_test]
fn standalone_and_legacy_parallel_serial_compilation_use_frozen_units() {
    let (mut backend, ids) = setup();
    let comp = backend.create_prepared_composite().unwrap();
    backend.wait_idle();
    let source_length = backend.poll().unwrap().loops[&ids[0]].length as u64;
    let config = BackendCompositeConfig {
        kind: BackendCompositeKind::Regular,
        sync_source: ids[0],
        timelines: vec![vec![
            vec![BackendCompositeEntry {
                target: BackendCompositeTarget::Loop(ids[1]),
                delay: 0,
                n_cycles: Some(1),
                mode: None,
            }],
            vec![BackendCompositeEntry {
                target: BackendCompositeTarget::Loop(ids[2]),
                delay: 0,
                n_cycles: Some(1),
                mode: None,
            }],
        ]],
    };
    let version = backend.configure_composite_loop(comp, &config).unwrap();
    backend.wait_idle();
    let snapshot = backend.poll().unwrap();
    let state = &snapshot.composites[&comp];
    assert_eq!(
        (
            state.active_plan_version,
            state.prepared_unit_frames,
            state.length
        ),
        (version, source_length, source_length * 2)
    );
    backend.set_composite_repeat_sync(comp, false).unwrap();
    backend.wait_idle();
    assert_eq!(
        backend.loop_sync_source(BackendLoopId::from_raw(comp.raw())),
        Some(ids[0])
    );
    backend.remove_composite_loop(comp).unwrap();
    backend.wait_idle();
    let snapshot = backend.poll().unwrap();
    assert!(!snapshot.composites.contains_key(&comp));
    assert_eq!(snapshot.removed_composites, vec![comp]);
}

#[shoop_test]
fn pending_edit_cancellation_and_supersession_leave_confirmed_plan_coherent() {
    let (mut backend, ids) = setup();
    let comp = shoop_backend::BackendCompositeId::from_raw(ids[3].raw());
    let first = backend
        .configure_prepared_composite_loop(comp, &plan(ids[1], Some(ids[0]), 8))
        .unwrap();
    backend.wait_idle();
    backend.poll().unwrap();
    backend
        .transition_loop(ids[0], Mode::Playing, None)
        .unwrap();
    backend.set_composite_repeat_sync(comp, true).unwrap();
    backend
        .transition_composite_loop(comp, Mode::Playing, None, None)
        .unwrap();
    step(&mut backend, 3);
    let candidate = backend
        .configure_prepared_composite_loop(comp, &plan(ids[2], Some(ids[0]), 9))
        .unwrap();
    step(&mut backend, 8);
    let state = backend.poll().unwrap().composites.remove(&comp).unwrap();
    assert_eq!(state.active_plan_version, first);
    assert_eq!(state.pending_plan_version, Some(candidate));
    backend.cancel_composite_plan(comp, candidate).unwrap();
    step(&mut backend, 20);
    let snapshot = backend.poll().unwrap();
    assert_eq!(snapshot.composites[&comp].active_plan_version, first);
    assert!(snapshot
        .composite_receipts
        .iter()
        .any(|r| r.version == candidate && r.outcome == BackendCompositeOutcome::Cancelled));
    let older = backend
        .configure_prepared_composite_loop(comp, &plan(ids[1], Some(ids[0]), 10))
        .unwrap();
    let newer = backend
        .configure_prepared_composite_loop(comp, &plan(ids[2], Some(ids[0]), 12))
        .unwrap();
    backend
        .transition_composite_loop(comp, Mode::Stopped, None, None)
        .unwrap();
    backend.wait_idle();
    let snapshot = backend.poll().unwrap();
    assert_eq!(snapshot.composites[&comp].active_plan_version, newer);
    assert!(snapshot
        .composite_receipts
        .iter()
        .any(|r| r.version == older && r.outcome == BackendCompositeOutcome::Superseded));
}

#[shoop_test]
fn structural_sync_change_progresses_while_edit_waits_without_retiming_current_pass() {
    let (mut backend, ids) = setup();
    let comp = shoop_backend::BackendCompositeId::from_raw(ids[3].raw());
    let first = backend
        .configure_prepared_composite_loop(comp, &plan(ids[1], Some(ids[0]), 80))
        .unwrap();
    backend.wait_idle();
    backend.poll().unwrap();
    backend
        .transition_composite_loop(comp, Mode::Playing, None, None)
        .unwrap();
    step(&mut backend, 3);
    let candidate = backend
        .configure_prepared_composite_loop(comp, &plan(ids[2], Some(ids[0]), 90))
        .unwrap();
    step(&mut backend, 8);
    backend.set_loop_sync_source(ids[3], Some(ids[4])).unwrap();
    step(&mut backend, 45);
    assert_eq!(backend.loop_sync_source(ids[3]), Some(ids[4]));
    let state = backend.poll().unwrap().composites.remove(&comp).unwrap();
    assert_eq!(
        (
            state.active_plan_version,
            state.length,
            state.pending_plan_version
        ),
        (first, 160, Some(candidate))
    );
    assert!(state.position > 40);
    backend
        .transition_composite_loop(comp, Mode::Stopped, None, None)
        .unwrap();
    backend.wait_idle();
    let snapshot = backend.poll().unwrap();
    assert_eq!(
        snapshot.composites[&comp].active_plan_version, candidate,
        "state={:?} receipts={:?}",
        snapshot.composites[&comp], snapshot.composite_receipts
    );
}

#[shoop_test]
fn invalid_composite_admission_is_atomic_and_does_not_issue_mutation_ticket() {
    let (mut backend, ids) = setup();
    let comp = shoop_backend::BackendCompositeId::from_raw(ids[3].raw());
    let cursor = backend.mutation_cursor();
    let mut invalid = plan(ids[1], Some(ids[0]), 4);
    invalid.spans[0].target = BackendCompositeTarget::Loop(ids[3]);
    assert!(backend
        .configure_prepared_composite_loop(comp, &invalid)
        .is_err());
    invalid.spans[0].target = BackendCompositeTarget::Loop(ids[1]);
    invalid.spans[0].mode = Mode::Recording;
    assert!(backend
        .configure_prepared_composite_loop(comp, &invalid)
        .is_err());
    assert_eq!(backend.mutation_cursor(), cursor);
    assert!(backend.poll().unwrap().loops.contains_key(&ids[3]));
}

#[shoop_test]
fn clearing_a_playing_composite_stops_authored_children_and_restores_same_public_slot() {
    let (mut backend, ids) = setup();
    let comp = shoop_backend::BackendCompositeId::from_raw(ids[3].raw());
    backend
        .configure_prepared_composite_loop(comp, &plan(ids[1], Some(ids[0]), 80))
        .unwrap();
    backend.wait_idle();
    backend
        .transition_composite_loop(comp, Mode::Playing, None, None)
        .unwrap();
    step(&mut backend, 4);
    assert_eq!(backend.poll().unwrap().loops[&ids[1]].mode, Mode::Playing);
    assert_eq!(backend.clear_composite_loop(comp).unwrap(), ids[3]);
    backend.wait_idle();
    let snapshot = backend.poll().unwrap();
    assert!(
        !snapshot.composites.contains_key(&comp),
        "failures={:?} completions={:?}",
        snapshot.mutation_failures,
        snapshot.mutation_completions
    );
    assert_eq!(
        (snapshot.loops[&ids[3]].mode, snapshot.loops[&ids[3]].length),
        (Mode::Stopped, 0)
    );
    assert_eq!(snapshot.loops[&ids[1]].mode, Mode::Stopped);
    assert!(backend
        .transition_composite_loop(comp, Mode::Playing, None, None)
        .is_err());
    backend
        .transition_loop(ids[3], Mode::Recording, None)
        .unwrap();
    backend.wait_idle();
    step(&mut backend, 3);
    backend
        .transition_loop(ids[3], Mode::Stopped, None)
        .unwrap();
    backend.wait_idle();
    assert!(backend.poll().unwrap().loops[&ids[3]].length > 0);
}

#[shoop_test]
fn resync_of_active_and_waiting_versions_preserves_identity_and_frozen_spans() {
    let (mut backend, ids) = setup();
    let comp = shoop_backend::BackendCompositeId::from_raw(ids[3].raw());
    let first = backend
        .configure_prepared_composite_loop(comp, &plan(ids[1], Some(ids[0]), 80))
        .unwrap();
    backend.wait_idle();
    backend
        .transition_composite_loop(comp, Mode::Playing, None, None)
        .unwrap();
    step(&mut backend, 3);
    let next = backend
        .configure_prepared_composite_loop(comp, &plan(ids[2], Some(ids[0]), 90))
        .unwrap();
    step(&mut backend, 8);
    backend.request_resync().unwrap();
    step(&mut backend, 10);
    assert!(backend.is_ready());
    let snapshot = backend.poll().unwrap();
    let state = &snapshot.composites[&comp];
    assert_eq!(
        (
            state.active_plan_version,
            state.pending_plan_version,
            state.length
        ),
        (first, Some(next), 160)
    );
    assert_eq!(
        state.active_children[0].target,
        BackendCompositeTarget::Loop(ids[1])
    );
    backend.cancel_composite_plan(comp, next).unwrap();
    step(&mut backend, 20);
    assert_eq!(
        backend.poll().unwrap().composites[&comp].active_plan_version,
        first
    );
}

#[shoop_test]
fn resync_preserves_a_playing_plan_with_more_than_seven_sparse_spans() {
    let (mut backend, ids) = setup();
    let comp = shoop_backend::BackendCompositeId::from_raw(ids[3].raw());
    let mut prepared = plan(ids[1], None, 39);
    prepared.entries = 20;
    prepared.spans = (0..20)
        .map(|i| BackendCompositeSpan {
            target: BackendCompositeTarget::Loop(ids[1]),
            start: i * 2,
            end: i * 2 + 1,
            occurrence_start: i * 2,
            ordinal: i as u32,
            mode: Mode::Playing,
        })
        .collect();
    let version = backend
        .configure_prepared_composite_loop(comp, &prepared)
        .unwrap();
    backend.wait_idle();
    backend
        .transition_composite_loop(comp, Mode::Playing, None, None)
        .unwrap();
    step(&mut backend, 3);
    let before = backend.poll().unwrap();
    backend.request_resync().unwrap();
    step(&mut backend, 128);
    assert!(
        backend.is_ready(),
        "large composite resync did not complete"
    );
    let after = backend.poll().unwrap();
    let state = &after.composites[&comp];
    assert_eq!(state.active_plan_version, version);
    assert_eq!(state.mode, Mode::Playing);
    assert_eq!(state.length, 78);
    assert_eq!(state.prepared_unit_frames, 2);
    assert!(state.observed_frame > before.composites[&comp].observed_frame);
    assert!(after.mutation_failures.is_empty());
}

#[shoop_test]
fn at_limit_sparse_plan_streams_beyond_mutation_tracking_capacity_as_one_logical_edit() {
    let mut d = description();
    d.composite_entries = 8192;
    d.composite_segments = 8192;
    d.reliable_credits = 4;
    let mut backend = stack(&d);
    let track = backend
        .create_direct_track(DirectTrackRequest {
            port_name_base: "large".into(),
            audio_channels: 1,
            midi: false,
            initial_loops: 2,
        })
        .unwrap();
    backend.wait_idle();
    backend.poll().unwrap();
    let comp = shoop_backend::BackendCompositeId::from_raw(track.loops[0].raw());
    let mut prepared = plan(track.loops[1], None, 16383);
    prepared.entries = 8192;
    prepared.spans = (0..8192)
        .map(|i| BackendCompositeSpan {
            target: BackendCompositeTarget::Loop(track.loops[1]),
            start: i * 2,
            end: i * 2 + 1,
            occurrence_start: i * 2,
            ordinal: i as u32,
            mode: Mode::Playing,
        })
        .collect();
    let before = backend.mutation_cursor();
    let version = backend
        .configure_prepared_composite_loop(comp, &prepared)
        .unwrap();
    assert_eq!(backend.mutation_cursor(), before + 1);
    backend.wait_idle();
    let snapshot = backend.poll().unwrap();
    assert_eq!(snapshot.composites[&comp].active_plan_version, version);
    assert_eq!(snapshot.composite_receipts.len(), 1);
    assert_eq!(snapshot.mutation_completions.len(), 1);
    prepared.entries += 1;
    assert!(backend
        .configure_prepared_composite_loop(comp, &prepared)
        .is_err());
    assert_eq!(backend.mutation_cursor(), before + 1);
}

struct Encoded<C> {
    inner: C,
    bytes: Vec<u8>,
}
impl<C: shoop_rt_transport::carrier::Carrier> shoop_rt_transport::carrier::Carrier for Encoded<C> {
    fn send(
        &mut self,
        message: shoop_rt_protocol::ClientMessage,
    ) -> Result<(), shoop_rt_transport::carrier::CarrierError> {
        let count = shoop_rt_protocol::codec::encode_client(&message, &mut self.bytes).unwrap();
        self.inner
            .send(shoop_rt_protocol::codec::decode_client(&self.bytes[..count]).unwrap())
    }
    fn receive(&mut self) -> Option<shoop_rt_protocol::EngineMessage> {
        let message = self.inner.receive()?;
        let count = shoop_rt_protocol::codec::encode_engine(&message, &mut self.bytes).unwrap();
        Some(shoop_rt_protocol::codec::decode_engine(&self.bytes[..count]).unwrap())
    }
}

#[shoop_test]
fn encoded_carrier_installs_and_runs_explicit_recording_and_reconciles_complete_state() {
    let mut d = description();
    d.ramp_frames = 0;
    d.max_frames = 1;
    let (clock, client) = parts(&d);
    let carrier = Encoded {
        inner: client.into_carrier(),
        bytes: vec![0; shoop_rt_protocol::MAX_RECORD_BYTES],
    };
    let mut backend = shoop_rt_backend::RtBackend::connect(
        shoop_rt_transport::client::ProtocolClient::new(carrier),
        clock,
    )
    .unwrap();
    let track = backend
        .create_direct_track(DirectTrackRequest {
            port_name_base: "encoded".into(),
            audio_channels: 1,
            midi: false,
            initial_loops: 2,
        })
        .unwrap();
    backend.wait_idle();
    let comp = shoop_backend::BackendCompositeId::from_raw(track.loops[0].raw());
    let mut prepared = plan(track.loops[1], None, 4);
    prepared.kind = BackendCompositeKind::Script;
    prepared.spans[0].mode = Mode::Recording;
    let version = backend
        .configure_prepared_composite_loop(comp, &prepared)
        .unwrap();
    backend.wait_idle();
    backend
        .transition_composite_loop(comp, Mode::Playing, None, None)
        .unwrap();
    for _ in 0..20 {
        backend.pump();
        backend.clock_mut().render(1);
    }
    backend.request_resync().unwrap();
    for _ in 0..10 {
        backend.pump();
        backend.clock_mut().render(1);
    }
    let snapshot = backend.poll().unwrap();
    assert_eq!(snapshot.composites[&comp].active_plan_version, version);
    assert_eq!(snapshot.composites[&comp].mode, Mode::Stopped);
    assert_eq!(
        (
            snapshot.loops[&track.loops[1]].mode,
            snapshot.loops[&track.loops[1]].length
        ),
        (Mode::Stopped, 8)
    );
    assert!(snapshot
        .composite_receipts
        .iter()
        .any(|receipt| matches!(receipt.outcome, BackendCompositeOutcome::Applied { .. })));
}

#[shoop_test]
fn clear_then_record_in_one_control_turn_binds_the_restored_primitive_generation() {
    let (mut backend, ids) = setup();
    let comp = shoop_backend::BackendCompositeId::from_raw(ids[3].raw());
    backend
        .configure_prepared_composite_loop(comp, &plan(ids[1], None, 4))
        .unwrap();
    backend.wait_idle();
    backend.clear_loop(ids[3]).unwrap();
    let clear = backend.mutation_cursor();
    backend
        .transition_loop(ids[3], Mode::Recording, None)
        .unwrap();
    let record = backend.mutation_cursor();
    backend.wait_idle();
    let snapshot = backend.poll().unwrap();
    for sequence in [clear, record] {
        assert!(snapshot
            .mutation_completions
            .iter()
            .any(|completion| completion.sequence == sequence && completion.result.is_ok()));
    }
    assert!(!snapshot.composites.contains_key(&comp));
    assert_eq!(snapshot.loops[&ids[3]].mode, Mode::Recording);
    backend.clock_mut().render(8);
    backend
        .transition_loop(ids[3], Mode::Stopped, None)
        .unwrap();
    backend.wait_idle();
    assert!(backend.poll().unwrap().loops[&ids[3]].length >= 8);
}

#[shoop_test]
fn automatic_monitoring_policy_and_actual_demand_survive_active_plan_resync() {
    let (mut backend, ids) = setup();
    let track = *backend.poll().unwrap().tracks.keys().next().unwrap();
    backend.set_track_automatic_monitoring(track, true).unwrap();
    let comp = shoop_backend::BackendCompositeId::from_raw(ids[3].raw());
    let mut recording = plan(ids[1], None, 1000);
    recording.kind = BackendCompositeKind::Script;
    recording.spans[0].mode = Mode::Recording;
    backend
        .configure_prepared_composite_loop(comp, &recording)
        .unwrap();
    backend.wait_idle();
    backend
        .transition_composite_loop(comp, Mode::Playing, None, None)
        .unwrap();
    step(&mut backend, 4);
    let before = backend.poll().unwrap();
    assert!(before.tracks[&track].automatic_monitoring_enabled);
    assert!(before.tracks[&track].automatic_input_monitoring);
    assert!(!before.tracks[&track].input_monitoring);
    backend.request_resync().unwrap();
    step(&mut backend, 10);
    assert!(backend.is_ready());
    let after = backend.poll().unwrap();
    assert!(after.tracks[&track].automatic_monitoring_enabled);
    assert!(after.tracks[&track].automatic_input_monitoring);
    assert!(!after.tracks[&track].input_monitoring);
    backend
        .transition_composite_loop(comp, Mode::Stopped, None, None)
        .unwrap();
    step(&mut backend, 4);
    assert!(!backend.poll().unwrap().tracks[&track].automatic_input_monitoring);
}
