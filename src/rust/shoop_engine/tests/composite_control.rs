use shoop_engine::{
    compile_composite_plan, split, BoundaryTargetAction, CompositeBoundaryTimeline,
    CompositeDependency, CompositeEntry, CompositePlanDescriptor, CompositePlanLimits,
    CompositeSection, CompositeTimeline, CompositeTimelineLimits, CompositeTimelineNode,
    LoopIdentity, LoopMode, LoopTargetCatalog, LoopTargetKind, LoopTargetMetadata, Session,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

struct Fixture {
    session: Session,
    timeline: CompositeBoundaryTimeline,
    source: LoopIdentity,
    child: LoopIdentity,
}

fn fixture() -> Fixture {
    fixture_with_cycles(1)
}

fn fixture_with_cycles(n_cycles: i64) -> Fixture {
    let mut session = Session::default();
    let sync = session.create_loop();
    let child = session.create_loop();
    session.loop_mut(sync).unwrap().set_length(4);
    session.loop_mut(child).unwrap().set_length(4);
    session.set_loop_mode(sync, LoopMode::Playing).unwrap();
    session.apply_graph_changes().unwrap();

    let source = LoopIdentity {
        slot: 10,
        generation: 1,
        kind: LoopTargetKind::Composite,
    };
    let sync_identity = session.loop_identity(sync).unwrap();
    let child_identity = session.loop_identity(child).unwrap();
    let catalog = LoopTargetCatalog::new(vec![
        LoopTargetMetadata {
            identity: source,
            length_samples: 0,
        },
        LoopTargetMetadata {
            identity: sync_identity,
            length_samples: 4,
        },
        LoopTargetMetadata {
            identity: child_identity,
            length_samples: 4,
        },
    ])
    .unwrap();
    let plan = compile_composite_plan(
        &CompositePlanDescriptor {
            source,
            sync_length: 4,
            timelines: vec![CompositeTimeline {
                sections: vec![CompositeSection {
                    entries: vec![CompositeEntry {
                        target: child_identity,
                        delay: 0,
                        n_cycles: Some(n_cycles),
                        mode: None,
                    }],
                }],
            }],
        },
        &catalog,
        &[],
        CompositePlanLimits::default(),
    )
    .unwrap();
    let mut timeline = CompositeBoundaryTimeline::new(
        vec![CompositeTimelineNode {
            plan,
            sync_source: sync_identity,
        }],
        CompositeTimelineLimits::default(),
    )
    .unwrap();
    timeline.prepare_install(1, &[None, None]).unwrap();

    Fixture {
        session,
        timeline,
        source,
        child: child_identity,
    }
}

#[tracy_nextest_capture::tracy_capture_test]
fn prepared_timeline_and_control_cross_at_callback_boundaries_and_publish_state() {
    let Fixture {
        session,
        timeline,
        source,
        child,
    } = fixture();
    let state = Arc::clone(timeline.state_mirror(source).unwrap());
    let (mut engine, mut handle) = split(session, 16);

    let mut install = handle.send_composite_timeline(timeline).unwrap();
    engine.process(1);
    let displaced = install.pop().unwrap().unwrap();
    assert_eq!(displaced.n_composites(), 0);
    assert_eq!(handle.reclaim(), 1);

    let mut accepted = handle
        .send_composite_control(
            source,
            BoundaryTargetAction::SetMode {
                mode: LoopMode::Playing,
                offset_samples: 0,
                retrigger: true,
            },
            None,
        )
        .unwrap();
    engine.process(1);
    assert_eq!(accepted.pop(), Ok(Ok(0)));
    assert_eq!(handle.reclaim(), 1);

    let composite = state.read();
    assert!(composite.installed);
    assert_eq!(composite.identity, source);
    assert_eq!(composite.active_plan_version, 1);
    assert_eq!(composite.pending_plan_version, None);
    assert_eq!(composite.mode, LoopMode::Playing);
    assert_eq!(composite.iteration, 0);
    assert_eq!(composite.length, 4);
    assert_eq!(composite.position, 2);
    assert_eq!(
        composite.active_children().collect::<Vec<_>>(),
        vec![shoop_engine::ActiveCompositeChild {
            identity: child,
            mode: LoopMode::Playing,
            cycle_offset: 0,
        }]
    );
}

#[tracy_nextest_capture::tracy_capture_test]
fn latched_fault_only_recovers_through_an_accepted_reset_command() {
    let Fixture {
        mut session,
        timeline,
        ..
    } = fixture();
    session.install_composite_timeline(timeline).unwrap();
    session.composite_timeline_mut().latch_sub_block_overflow();
    let (mut engine, mut handle) = split(session, 8);
    engine.process(1);
    assert_eq!(engine.session().composite_timeline().sample_clock(), 0);

    let mut reset = handle.send_composite_fault_reset().unwrap();
    engine.process(1);

    assert_eq!(reset.pop(), Ok(0));
    assert_eq!(engine.session().composite_timeline().sample_clock(), 1);
    assert_eq!(
        engine.session().composite_timeline().fault().fault,
        shoop_engine::CompositeTimelineFault::None
    );
}

#[tracy_nextest_capture::tracy_capture_test]
fn immediate_transition_validates_seek_before_acceptance() {
    let Fixture {
        session,
        timeline,
        source,
        ..
    } = fixture();
    let (mut engine, mut handle) = split(session, 8);
    let mut install = handle.send_composite_timeline(timeline).unwrap();
    engine.process(1);
    assert!(install.pop().unwrap().is_ok());

    let mut invalid = handle
        .send_composite_immediate_transition(source, LoopMode::Playing, 1)
        .unwrap();
    engine.process(1);
    assert_eq!(
        invalid.pop(),
        Ok(Err(
            shoop_engine::CompositeTimelineControlError::InvalidSeek
        ))
    );
    assert_eq!(
        engine
            .session()
            .composite_timeline()
            .runtime(source)
            .unwrap()
            .mode(),
        LoopMode::Stopped
    );
    assert_eq!(
        engine.session().composite_timeline().fault().fault,
        shoop_engine::CompositeTimelineFault::None
    );

    let mut valid = handle
        .send_composite_immediate_transition(source, LoopMode::Playing, 0)
        .unwrap();
    engine.process(1);
    assert_eq!(valid.pop(), Ok(Ok(0)));
    assert_eq!(
        engine
            .session()
            .composite_timeline()
            .runtime(source)
            .unwrap()
            .mode(),
        LoopMode::Playing
    );
}

#[tracy_nextest_capture::tracy_capture_test]
fn synchronized_transition_countdown_and_record_option_are_engine_owned() {
    let Fixture {
        session,
        timeline,
        source,
        ..
    } = fixture();
    let (mut engine, mut handle) = split(session, 8);
    let mut install = handle.send_composite_timeline(timeline).unwrap();
    engine.process(1);
    assert!(install.pop().unwrap().is_ok());

    let state = Arc::clone(
        engine
            .session()
            .composite_timeline()
            .state_mirror(source)
            .unwrap(),
    );
    let mut transition = handle
        .send_composite_transition(source, LoopMode::Recording, 1)
        .unwrap();
    let mut record_option = handle
        .send_composite_play_after_record(source, true)
        .unwrap();
    engine.process(3);
    assert_eq!(transition.pop(), Ok(Ok(0)));
    assert_eq!(record_option.pop(), Ok(Ok(1)));
    let first = state.read();
    assert_eq!(first.mode, LoopMode::Stopped);
    assert_eq!(first.next_mode, Some(LoopMode::Recording));
    assert_eq!(first.next_mode_delay, Some(0));
    assert!(first.play_after_record);

    engine.process(4);
    let second = state.read();
    assert_eq!(second.mode, LoopMode::Recording);
    assert_eq!(second.next_mode, None);
    assert_eq!(second.iteration, 0);
}

#[tracy_nextest_capture::tracy_capture_test]
fn primitive_loop_mirror_publishes_composite_anticipated_transitions() {
    let Fixture {
        mut session,
        timeline,
        source,
        child,
    } = fixture();
    session.install_composite_timeline(timeline).unwrap();
    let child_state = Arc::clone(session.loop_(child.slot as usize).unwrap().state_mirror());

    session
        .accept_composite_transition(source, LoopMode::Playing, 2)
        .unwrap();
    let state = child_state.read();
    assert_eq!(state.maybe_next_mode, Some(LoopMode::Playing));
    assert_eq!(state.maybe_next_mode_delay, Some(2));

    session.process(4);
    let state = child_state.read();
    assert_eq!(state.maybe_next_mode, Some(LoopMode::Playing));
    assert!(state.maybe_next_mode_delay.unwrap() < 2);

    session
        .accept_composite_immediate_transition(source, LoopMode::Stopped, 0)
        .unwrap();
    assert_eq!(child_state.read().maybe_next_mode, None);
}

#[tracy_nextest_capture::tracy_capture_test]
fn a_timestamp_that_is_past_at_callback_acceptance_is_rejected_not_applied_late() {
    let Fixture {
        session,
        timeline,
        source,
        ..
    } = fixture();
    let (mut engine, mut handle) = split(session, 8);
    let mut install = handle.send_composite_timeline(timeline).unwrap();
    engine.process(4);
    assert_eq!(install.pop().unwrap().unwrap().n_composites(), 0);

    let mut accepted = handle
        .send_composite_control(
            source,
            BoundaryTargetAction::SetMode {
                mode: LoopMode::Playing,
                offset_samples: 0,
                retrigger: true,
            },
            Some(3),
        )
        .unwrap();
    engine.process(4);

    assert_eq!(
        accepted.pop(),
        Ok(Err(shoop_engine::CompositeTimelineControlError::Late))
    );
    assert_eq!(
        engine
            .session()
            .composite_timeline()
            .runtime(source)
            .unwrap()
            .mode(),
        LoopMode::Stopped
    );
}

#[tracy_nextest_capture::tracy_capture_test]
fn prepared_timeline_is_rejected_if_primitive_topology_changed_before_acceptance() {
    let Fixture {
        mut session,
        timeline,
        ..
    } = fixture();
    session.set_loop_sync_source(1, Some(0)).unwrap();
    let (mut engine, mut handle) = split(session, 8);

    let mut install = handle.send_composite_timeline(timeline).unwrap();
    engine.process(1);

    let rejected = install.pop().unwrap().unwrap_err();
    assert_eq!(
        rejected.error,
        shoop_engine::SessionError::StaleCompositeTopology
    );
    assert!(engine.session().composite_timeline().is_empty());
}

#[tracy_nextest_capture::tracy_capture_test]
fn running_timeline_replacement_activates_at_iteration_zero_and_reclaims_off_rt() {
    let Fixture {
        session,
        timeline,
        source,
        ..
    } = fixture();
    let mut replacement = fixture_with_cycles(2).timeline;
    replacement.prepare_install(2, &[None, None]).unwrap();
    let (mut engine, mut handle) = split(session, 8);
    let mut install = handle.send_composite_timeline(timeline).unwrap();
    engine.process(1);
    assert!(install.pop().unwrap().is_ok());
    let mut start = handle
        .send_composite_immediate_transition(source, LoopMode::Playing, 0)
        .unwrap();
    engine.process(1);
    assert!(start.pop().unwrap().is_ok());

    let mut replace = handle.send_composite_timeline(replacement).unwrap();
    engine.process(1);
    assert!(replace.pop().unwrap().is_ok());
    assert_eq!(engine.session().composite_timeline_version(), 2);
    let before_boundary = engine.session().composite_timeline().node_state(0).unwrap();
    assert_eq!(
        before_boundary.runtime.length_samples(before_boundary.plan),
        Ok(4)
    );
    assert_eq!(before_boundary.active_version, 1);
    assert_eq!(before_boundary.pending_version, Some(2));
    let observed_pending = engine
        .session()
        .composite_timeline()
        .state_mirror(source)
        .unwrap()
        .read();
    assert_eq!(observed_pending.active_plan_version, 1);
    assert_eq!(observed_pending.pending_plan_version, Some(2));

    engine.process(1);
    let after_boundary = engine.session().composite_timeline().node_state(0).unwrap();
    assert_eq!(
        after_boundary.runtime.length_samples(after_boundary.plan),
        Ok(8)
    );
    assert_eq!(after_boundary.active_version, 2);
    assert_eq!(after_boundary.pending_version, None);
    assert_eq!(after_boundary.runtime.mode(), LoopMode::Playing);
    assert_eq!(after_boundary.runtime.iteration(), 0);
    assert_eq!(engine.session().composite_timeline().n_retired_plans(), 1);
    let observed_active = engine
        .session()
        .composite_timeline()
        .state_mirror(source)
        .unwrap()
        .read();
    assert_eq!(observed_active.active_plan_version, 2);
    assert_eq!(observed_active.pending_plan_version, None);
    assert_eq!(observed_active.active_children().count(), 1);

    let mut reclaimed = handle.send_composite_plan_reclamation(64).unwrap();
    engine.process(1);
    assert_eq!(reclaimed.pop().unwrap().len(), 1);
    assert_eq!(engine.session().composite_timeline().n_retired_plans(), 0);
}

#[tracy_nextest_capture::tracy_capture_test]
fn pending_replacement_activates_immediately_and_preserves_countdown() {
    let Fixture {
        session,
        timeline,
        source,
        ..
    } = fixture();
    let mut replacement = fixture_with_cycles(2).timeline;
    replacement.prepare_install(2, &[None, None]).unwrap();
    let (mut engine, mut handle) = split(session, 8);
    let mut install = handle.send_composite_timeline(timeline).unwrap();
    engine.process(1);
    assert!(install.pop().unwrap().is_ok());
    let mut transition = handle
        .send_composite_transition(source, LoopMode::Playing, 2)
        .unwrap();
    engine.process(1);
    assert!(transition.pop().unwrap().is_ok());

    let mut replace = handle.send_composite_timeline(replacement).unwrap();
    engine.process(1);

    assert!(replace.pop().unwrap().is_ok());
    let node = engine.session().composite_timeline().node_state(0).unwrap();
    assert_eq!(node.runtime.length_samples(node.plan), Ok(8));
    assert_eq!(node.runtime.mode(), LoopMode::Stopped);
    assert_eq!(node.runtime.pending().unwrap().boundaries_to_skip, 2);
}

#[tracy_nextest_capture::tracy_capture_test]
fn stop_before_iteration_zero_activates_pending_plan_stopped() {
    let Fixture {
        session,
        timeline,
        source,
        ..
    } = fixture_with_cycles(2);
    let mut replacement = fixture_with_cycles(3).timeline;
    replacement.prepare_install(2, &[None, None]).unwrap();
    let (mut engine, mut handle) = split(session, 8);
    let mut install = handle.send_composite_timeline(timeline).unwrap();
    engine.process(1);
    assert!(install.pop().unwrap().is_ok());
    let mut start = handle
        .send_composite_immediate_transition(source, LoopMode::Playing, 0)
        .unwrap();
    engine.process(1);
    assert!(start.pop().unwrap().is_ok());
    let mut replace = handle.send_composite_timeline(replacement).unwrap();
    engine.pump();
    assert!(replace.pop().unwrap().is_ok());

    let mut stop = handle
        .send_composite_immediate_transition(source, LoopMode::Stopped, 0)
        .unwrap();
    engine.process(1);

    assert!(stop.pop().unwrap().is_ok());
    let node = engine.session().composite_timeline().node_state(0).unwrap();
    assert_eq!(node.runtime.length_samples(node.plan), Ok(12));
    assert_eq!(node.runtime.mode(), LoopMode::Stopped);
    assert_eq!(engine.session().composite_timeline().n_retired_plans(), 1);
}

#[tracy_nextest_capture::tracy_capture_test]
fn newest_running_replacement_supersedes_older_candidate() {
    let Fixture {
        session,
        timeline,
        source,
        ..
    } = fixture();
    let mut version_two = fixture_with_cycles(2).timeline;
    version_two.prepare_install(2, &[None, None]).unwrap();
    let mut version_three = fixture_with_cycles(3).timeline;
    version_three.prepare_install(3, &[None, None]).unwrap();
    let (mut engine, mut handle) = split(session, 8);
    let mut install = handle.send_composite_timeline(timeline).unwrap();
    engine.process(1);
    assert!(install.pop().unwrap().is_ok());
    let mut start = handle
        .send_composite_immediate_transition(source, LoopMode::Playing, 0)
        .unwrap();
    engine.process(1);
    assert!(start.pop().unwrap().is_ok());

    let mut second = handle.send_composite_timeline(version_two).unwrap();
    engine.pump();
    assert!(second.pop().unwrap().is_ok());
    let mut third = handle.send_composite_timeline(version_three).unwrap();
    engine.pump();
    assert!(third.pop().unwrap().is_ok());
    engine.process(2);

    let node = engine.session().composite_timeline().node_state(0).unwrap();
    assert_eq!(node.runtime.length_samples(node.plan), Ok(12));
    assert_eq!(node.runtime.iteration(), 0);
    assert_eq!(engine.session().composite_timeline_version(), 3);
}

#[tracy_nextest_capture::tracy_capture_test]
fn running_dependency_topology_change_restarts_at_the_install_boundary() {
    let Fixture {
        session,
        timeline,
        source,
        child,
    } = fixture();
    let mut incompatible =
        CompositeBoundaryTimeline::new(Vec::new(), CompositeTimelineLimits::default()).unwrap();
    incompatible.prepare_install(2, &[None, None]).unwrap();
    let (mut engine, mut handle) = split(session, 8);
    let mut install = handle.send_composite_timeline(timeline).unwrap();
    engine.process(1);
    assert!(install.pop().unwrap().is_ok());
    let mut start = handle
        .send_composite_immediate_transition(source, LoopMode::Playing, 0)
        .unwrap();
    engine.process(1);
    assert!(start.pop().unwrap().is_ok());

    let mut replace = handle.send_composite_timeline(incompatible).unwrap();
    engine.process(1);

    assert!(replace.pop().unwrap().is_ok());
    assert_eq!(engine.session().composite_timeline_version(), 2);
    assert_eq!(engine.session().composite_timeline().n_composites(), 0);
    assert_eq!(
        engine.session().loop_(child.slot as usize).unwrap().mode(),
        LoopMode::Stopped
    );
}

#[tracy_nextest_capture::tracy_capture_test]
fn running_dependency_addition_restarts_retained_sources_and_nested_children() {
    let Fixture {
        session,
        timeline,
        source,
        child,
    } = fixture();
    let nested = LoopIdentity {
        slot: source.slot + 1,
        generation: 1,
        kind: LoopTargetKind::Composite,
    };
    let sync = session.loop_identity(0).unwrap();
    let catalog = LoopTargetCatalog::new(vec![
        LoopTargetMetadata {
            identity: source,
            length_samples: 4,
        },
        LoopTargetMetadata {
            identity: nested,
            length_samples: 4,
        },
        LoopTargetMetadata {
            identity: sync,
            length_samples: 4,
        },
        LoopTargetMetadata {
            identity: child,
            length_samples: 4,
        },
    ])
    .unwrap();
    let dependencies = vec![
        CompositeDependency {
            source,
            composite_children: vec![nested],
        },
        CompositeDependency {
            source: nested,
            composite_children: Vec::new(),
        },
    ];
    let descriptor = |plan_source, target| CompositePlanDescriptor {
        source: plan_source,
        sync_length: 4,
        timelines: vec![CompositeTimeline {
            sections: vec![CompositeSection {
                entries: vec![CompositeEntry {
                    target,
                    delay: 0,
                    n_cycles: Some(1),
                    mode: None,
                }],
            }],
        }],
    };
    let parent = compile_composite_plan(
        &descriptor(source, nested),
        &catalog,
        &dependencies,
        CompositePlanLimits::default(),
    )
    .unwrap();
    let nested_plan = compile_composite_plan(
        &descriptor(nested, child),
        &catalog,
        &dependencies,
        CompositePlanLimits::default(),
    )
    .unwrap();
    let mut replacement = CompositeBoundaryTimeline::new(
        vec![
            CompositeTimelineNode {
                plan: parent,
                sync_source: sync,
            },
            CompositeTimelineNode {
                plan: nested_plan,
                sync_source: sync,
            },
        ],
        CompositeTimelineLimits::default(),
    )
    .unwrap();
    replacement.prepare_install(2, &[None, None]).unwrap();

    let (mut engine, mut handle) = split(session, 8);
    let mut install = handle.send_composite_timeline(timeline).unwrap();
    engine.process(1);
    assert!(install.pop().unwrap().is_ok());
    let mut start = handle
        .send_composite_immediate_transition(source, LoopMode::Playing, 0)
        .unwrap();
    engine.process(1);
    assert!(start.pop().unwrap().is_ok());
    let mut pending = handle
        .send_composite_transition(source, LoopMode::Recording, 2)
        .unwrap();
    engine.pump();
    assert!(pending.pop().unwrap().is_ok());

    let mut replace = handle.send_composite_timeline(replacement).unwrap();
    engine.process(1);

    assert!(replace.pop().unwrap().is_ok());
    let parent_runtime = engine
        .session()
        .composite_timeline()
        .runtime(source)
        .unwrap();
    assert_eq!(parent_runtime.mode(), LoopMode::Playing);
    assert_eq!(parent_runtime.pending(), None);
    assert_eq!(
        engine
            .session()
            .composite_timeline()
            .runtime(nested)
            .unwrap()
            .mode(),
        LoopMode::Playing
    );
    assert_eq!(
        engine.session().loop_(child.slot as usize).unwrap().mode(),
        LoopMode::Playing
    );
}

#[tracy_nextest_capture::tracy_capture_test]
fn older_prepared_version_is_rejected_even_if_compilers_finish_out_of_order() {
    let Fixture {
        session,
        timeline: version_one,
        ..
    } = fixture();
    let mut version_two = fixture().timeline;
    version_two.prepare_install(2, &[None, None]).unwrap();
    let (mut engine, mut handle) = split(session, 8);

    let mut newer = handle.send_composite_timeline(version_two).unwrap();
    let mut older = handle.send_composite_timeline(version_one).unwrap();
    engine.process(1);

    assert!(newer.pop().unwrap().is_ok());
    let rejected = older.pop().unwrap().unwrap_err();
    assert_eq!(
        rejected.error,
        shoop_engine::SessionError::StaleCompositeVersion(2)
    );
    assert_eq!(engine.session().composite_timeline_version(), 2);
    assert_eq!(handle.poll_trace().unwrap().composite_timeline_version, 2);
}

#[tracy_nextest_capture::tracy_capture_test]
fn transition_history_survives_frontend_polling_stall() {
    let Fixture {
        session,
        timeline,
        source,
        ..
    } = fixture();
    let (mut engine, mut handle) = split(session, 8);
    let mut install = handle.send_composite_timeline(timeline).unwrap();
    engine.process(1);
    assert!(install.pop().unwrap().is_ok());
    let mut start = handle
        .send_composite_immediate_transition(source, LoopMode::Playing, 0)
        .unwrap();
    engine.process(1);
    assert!(start.pop().unwrap().is_ok());

    for _ in 0..8 {
        engine.process(4);
    }
    handle.poll_trace();
    engine.process(1);
    let snapshot = handle.poll_trace().unwrap();

    assert!(!snapshot.composite_trace.is_empty());
    assert_eq!(
        snapshot.n_composite_trace_entries,
        snapshot.composite_trace.len()
    );
    assert!(snapshot
        .composite_trace
        .iter()
        .any(|entry| entry.at_sample >= 32));
}

#[tracy_nextest_capture::tracy_capture_test]
fn stale_trace_publication_is_dropped_without_stalling_processing() {
    let (mut engine, _handle) = split(Session::default(), 8);

    for _ in 0..5 {
        engine.process(1);
    }

    assert_eq!(engine.stats().cycles.load(Ordering::Relaxed), 5);
    assert_eq!(
        engine
            .stats()
            .trace_snapshots_dropped
            .load(Ordering::Relaxed),
        2
    );
}

#[tracy_nextest_capture::tracy_capture_test]
fn callback_drain_has_a_fixed_cutoff() {
    let mut session = Session::default();
    let loop_idx = session.create_loop();
    session.apply_graph_changes().unwrap();
    let (mut engine, mut handle) = split(session, 8);
    let entered = Arc::new(AtomicBool::new(false));
    let release = Arc::new(AtomicBool::new(false));
    let entered_command = Arc::clone(&entered);
    let release_command = Arc::clone(&release);

    handle
        .send(Box::new(move |session| {
            entered_command.store(true, Ordering::Release);
            while !release_command.load(Ordering::Acquire) {
                std::hint::spin_loop();
            }
            session.loop_mut(loop_idx).unwrap().set_length(1);
        }))
        .unwrap();

    let process = std::thread::spawn(move || {
        engine.process(1);
        engine
    });
    while !entered.load(Ordering::Acquire) {
        std::hint::spin_loop();
    }
    handle
        .send(Box::new(move |session| {
            session.loop_mut(loop_idx).unwrap().set_length(2);
        }))
        .unwrap();
    release.store(true, Ordering::Release);

    let mut engine = process.join().unwrap();
    assert_eq!(engine.session().loop_(loop_idx).unwrap().length(), 1);
    assert_eq!(engine.stats().commands_applied.load(Ordering::Relaxed), 1);

    engine.process(1);
    assert_eq!(engine.session().loop_(loop_idx).unwrap().length(), 2);
    assert_eq!(engine.stats().commands_applied.load(Ordering::Relaxed), 2);
}
