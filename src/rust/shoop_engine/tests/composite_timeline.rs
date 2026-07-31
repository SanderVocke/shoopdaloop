use shoop_engine::{
    compile_composite_plan, AcceptedTimelineControl, BoundaryIntent, BoundaryIntentOrigin,
    BoundaryTargetAction, CompiledCompositePlan, CompositeBoundaryTimeline, CompositeEntry,
    CompositePlanDescriptor, CompositePlanLimits, CompositeSection, CompositeTimeline,
    CompositeTimelineBuildError, CompositeTimelineControlError, CompositeTimelineFault,
    CompositeTimelineLimits, CompositeTimelineNode, LoopIdentity, LoopMode, LoopTargetCatalog,
    LoopTargetKind, LoopTargetMetadata,
};

fn basic(slot: u32) -> LoopIdentity {
    LoopIdentity {
        slot,
        generation: 1,
        kind: LoopTargetKind::Basic,
    }
}

fn composite(slot: u32) -> LoopIdentity {
    LoopIdentity {
        slot,
        generation: 1,
        kind: LoopTargetKind::Composite,
    }
}

fn plan(
    source: LoopIdentity,
    sync_length: u64,
    entries: &[(LoopIdentity, i64, Option<LoopMode>)],
    catalog: &LoopTargetCatalog,
) -> CompiledCompositePlan {
    let descriptor = CompositePlanDescriptor {
        source,
        sync_length,
        timelines: vec![CompositeTimeline {
            sections: vec![CompositeSection {
                entries: entries
                    .iter()
                    .map(|(target, delay, mode)| CompositeEntry {
                        target: *target,
                        delay: *delay,
                        n_cycles: Some(1),
                        mode: *mode,
                    })
                    .collect(),
            }],
        }],
    };
    compile_composite_plan(&descriptor, catalog, &[], CompositePlanLimits::default()).unwrap()
}

fn catalog(identities: &[LoopIdentity]) -> LoopTargetCatalog {
    LoopTargetCatalog::new(
        identities
            .iter()
            .copied()
            .map(|identity| LoopTargetMetadata {
                identity,
                length_samples: 4,
            })
            .collect(),
    )
    .unwrap()
}

fn start(
    at_sample: u64,
    sequence: u64,
    target: LoopIdentity,
    mode: LoopMode,
) -> AcceptedTimelineControl {
    AcceptedTimelineControl {
        at_sample,
        target,
        action: BoundaryTargetAction::SetMode {
            mode,
            offset_samples: 0,
            retrigger: true,
        },
        acceptance_sequence: sequence,
    }
}

#[test]
fn script_regular_natural_and_direct_conflicts_use_total_precedence() {
    let child = basic(1);
    let script = composite(10);
    let regular = composite(20);
    let targets = catalog(&[child, script, regular]);
    let mut timeline = CompositeBoundaryTimeline::new(
        vec![
            CompositeTimelineNode {
                plan: plan(regular, 4, &[(child, 0, None)], &targets),
                sync_source: basic(2),
            },
            CompositeTimelineNode {
                plan: plan(script, 4, &[(child, 0, Some(LoopMode::Playing))], &targets),
                sync_source: basic(2),
            },
        ],
        CompositeTimelineLimits::default(),
    )
    .unwrap();

    timeline
        .queue_control(start(0, 1, regular, LoopMode::Recording))
        .unwrap();
    timeline
        .queue_control(start(0, 2, script, LoopMode::Playing))
        .unwrap();
    let natural = BoundaryIntent {
        target: child,
        action: BoundaryTargetAction::Stop,
        origin: BoundaryIntentOrigin::Natural { source: child },
    };
    let trace = timeline
        .resolve_boundary(&[], &[natural], |identity| identity == child)
        .unwrap();
    let child_action = trace.iter().find(|entry| entry.target == child).unwrap();
    assert!(matches!(
        child_action.winner,
        BoundaryIntentOrigin::ScriptComposite { source, .. } if source == script
    ));
    assert!(matches!(
        child_action.action,
        BoundaryTargetAction::SetMode {
            mode: LoopMode::Playing,
            ..
        }
    ));
    assert_eq!(child_action.n_losing_conflicts, 2);

    timeline
        .queue_control(AcceptedTimelineControl {
            at_sample: 0,
            target: child,
            action: BoundaryTargetAction::Stop,
            acceptance_sequence: 3,
        })
        .unwrap();
    let trace = timeline
        .resolve_boundary(&[], &[], |identity| identity == child)
        .unwrap();
    assert!(matches!(
        trace
            .iter()
            .find(|entry| entry.target == child)
            .unwrap()
            .winner,
        BoundaryIntentOrigin::Direct {
            acceptance_sequence: 3
        }
    ));
}

#[test]
fn same_class_conflicts_use_lower_source_identity_not_install_order() {
    let child = basic(1);
    let lower = composite(10);
    let higher = composite(20);
    let targets = catalog(&[child, lower, higher]);
    let make = |reverse: bool| {
        let mut nodes = vec![
            CompositeTimelineNode {
                plan: plan(lower, 4, &[(child, 0, None)], &targets),
                sync_source: basic(2),
            },
            CompositeTimelineNode {
                plan: plan(higher, 4, &[(child, 0, None)], &targets),
                sync_source: basic(2),
            },
        ];
        if reverse {
            nodes.reverse();
        }
        CompositeBoundaryTimeline::new(nodes, CompositeTimelineLimits::default()).unwrap()
    };

    for reverse in [false, true] {
        let mut timeline = make(reverse);
        timeline
            .queue_control(start(0, 1, higher, LoopMode::Recording))
            .unwrap();
        timeline
            .queue_control(start(0, 2, lower, LoopMode::Playing))
            .unwrap();
        let trace = timeline
            .resolve_boundary(&[], &[], |identity| identity == child)
            .unwrap();
        let winner = trace.iter().find(|entry| entry.target == child).unwrap();
        assert!(matches!(
            winner.winner,
            BoundaryIntentOrigin::RegularComposite { source, .. } if source == lower
        ));
    }
}

#[test]
fn nested_iteration_zero_propagates_through_several_levels_at_one_sample() {
    let child = basic(1);
    let leaf = composite(10);
    let middle = composite(20);
    let root = composite(30);
    let targets = catalog(&[child, leaf, middle, root]);
    let mut timeline = CompositeBoundaryTimeline::new(
        vec![
            CompositeTimelineNode {
                plan: plan(leaf, 4, &[(child, 0, None)], &targets),
                sync_source: basic(2),
            },
            CompositeTimelineNode {
                plan: plan(middle, 4, &[(leaf, 0, None)], &targets),
                sync_source: basic(2),
            },
            CompositeTimelineNode {
                plan: plan(root, 4, &[(middle, 0, None)], &targets),
                sync_source: basic(2),
            },
        ],
        CompositeTimelineLimits::default(),
    )
    .unwrap();
    timeline.advance_clock(7);
    timeline
        .queue_control(start(7, 1, root, LoopMode::Playing))
        .unwrap();

    let trace = timeline
        .resolve_boundary(&[], &[], |identity| identity == child)
        .unwrap();
    assert!(trace.iter().any(|entry| {
        entry.at_sample == 7
            && entry.target == child
            && matches!(
                entry.action,
                BoundaryTargetAction::SetMode {
                    mode: LoopMode::Playing,
                    ..
                }
            )
    }));
    for source in [root, middle, leaf] {
        assert_eq!(timeline.runtime(source).unwrap().mode(), LoopMode::Playing);
    }
}

#[test]
fn a_source_trigger_advances_the_schedule_at_the_exact_sample() {
    let child = basic(1);
    let source = basic(2);
    let composite = composite(10);
    let targets = catalog(&[child, source, composite]);
    let mut timeline = CompositeBoundaryTimeline::new(
        vec![CompositeTimelineNode {
            plan: plan(composite, 4, &[(child, 1, None)], &targets),
            sync_source: source,
        }],
        CompositeTimelineLimits::default(),
    )
    .unwrap();
    timeline
        .queue_control(start(0, 1, composite, LoopMode::Playing))
        .unwrap();
    assert!(!timeline
        .resolve_boundary(&[], &[], |identity| identity == child || identity == source)
        .unwrap()
        .iter()
        .any(|entry| entry.target == child));

    timeline.advance_clock(4);
    let trace = timeline
        .resolve_boundary(&[source], &[], |identity| {
            identity == child || identity == source
        })
        .unwrap();
    assert_eq!(
        trace
            .iter()
            .find(|entry| entry.target == child)
            .unwrap()
            .at_sample,
        4
    );
}

#[test]
fn composite_sync_triggers_propagate_transitively_without_snapshot_order() {
    let first_child = basic(1);
    let second_child = basic(2);
    let primitive_source = basic(3);
    let follower = composite(10);
    let root = composite(20);
    let targets = catalog(&[first_child, second_child, primitive_source, follower, root]);
    let mut timeline = CompositeBoundaryTimeline::new(
        vec![
            CompositeTimelineNode {
                plan: plan(follower, 4, &[(second_child, 1, None)], &targets),
                sync_source: root,
            },
            CompositeTimelineNode {
                plan: plan(root, 4, &[(first_child, 1, None)], &targets),
                sync_source: primitive_source,
            },
        ],
        CompositeTimelineLimits::default(),
    )
    .unwrap();
    timeline
        .queue_control(start(0, 1, root, LoopMode::Playing))
        .unwrap();
    timeline
        .queue_control(start(0, 2, follower, LoopMode::Playing))
        .unwrap();
    timeline.resolve_boundary(&[], &[], |_| true).unwrap();

    timeline.advance_clock(4);
    let trace = timeline
        .resolve_boundary(&[primitive_source], &[], |_| true)
        .unwrap();
    assert!(trace
        .iter()
        .any(|entry| entry.target == first_child && entry.at_sample == 4));
    assert!(trace
        .iter()
        .any(|entry| entry.target == second_child && entry.at_sample == 4));
}

#[test]
fn a_composite_started_primitive_source_triggers_its_follower_in_the_same_boundary() {
    let child = basic(1);
    let primitive_source = basic(2);
    let follower = composite(10);
    let producer = composite(20);
    let targets = catalog(&[child, primitive_source, follower, producer]);
    let mut timeline = CompositeBoundaryTimeline::new(
        vec![
            CompositeTimelineNode {
                plan: plan(follower, 4, &[(child, 1, None)], &targets),
                sync_source: primitive_source,
            },
            CompositeTimelineNode {
                plan: plan(producer, 4, &[(primitive_source, 0, None)], &targets),
                sync_source: basic(3),
            },
        ],
        CompositeTimelineLimits::default(),
    )
    .unwrap();
    timeline
        .queue_control(start(0, 1, follower, LoopMode::Playing))
        .unwrap();
    timeline.resolve_boundary(&[], &[], |_| true).unwrap();

    timeline.advance_clock(1);
    timeline
        .queue_control(start(1, 2, producer, LoopMode::Playing))
        .unwrap();
    let trace = timeline.resolve_boundary(&[], &[], |_| true).unwrap();
    assert!(trace
        .iter()
        .any(|entry| entry.target == child && entry.at_sample == 1));
}

#[test]
fn one_composite_is_not_delivered_twice_when_a_trigger_appears_in_a_later_same_sample_wave() {
    let child = basic(1);
    let sync = basic(2);
    let source = composite(10);
    let targets = catalog(&[child, sync, source]);
    let mut timeline = CompositeBoundaryTimeline::new(
        vec![CompositeTimelineNode {
            plan: plan(source, 4, &[(child, 1, None)], &targets),
            sync_source: sync,
        }],
        CompositeTimelineLimits::default(),
    )
    .unwrap();
    timeline
        .queue_control(start(0, 1, source, LoopMode::Playing))
        .unwrap();
    timeline.resolve_boundary(&[], &[], |_| true).unwrap();
    let trace = timeline.resolve_boundary(&[sync], &[], |_| true).unwrap();
    assert!(!trace.iter().any(|entry| entry.target == child));
    assert_eq!(timeline.runtime(source).unwrap().iteration(), 0);
}

#[test]
fn direct_source_stop_suppresses_the_coincident_natural_trigger() {
    let child = basic(1);
    let source = basic(2);
    let composite = composite(10);
    let targets = catalog(&[child, source, composite]);
    let mut timeline = CompositeBoundaryTimeline::new(
        vec![CompositeTimelineNode {
            plan: plan(composite, 4, &[(child, 1, None)], &targets),
            sync_source: source,
        }],
        CompositeTimelineLimits::default(),
    )
    .unwrap();
    timeline
        .queue_control(start(0, 1, composite, LoopMode::Playing))
        .unwrap();
    timeline
        .resolve_boundary(&[], &[], |identity| identity == child || identity == source)
        .unwrap();
    timeline.advance_clock(4);
    timeline
        .queue_control(AcceptedTimelineControl {
            at_sample: 4,
            target: source,
            action: BoundaryTargetAction::Stop,
            acceptance_sequence: 2,
        })
        .unwrap();
    let natural = BoundaryIntent {
        target: source,
        action: BoundaryTargetAction::SetMode {
            mode: LoopMode::Playing,
            offset_samples: 0,
            retrigger: false,
        },
        origin: BoundaryIntentOrigin::Natural { source },
    };
    let trace = timeline
        .resolve_boundary(&[source], &[natural], |identity| {
            identity == child || identity == source
        })
        .unwrap();
    assert!(!trace.iter().any(|entry| entry.target == child));
    assert_eq!(timeline.runtime(composite).unwrap().iteration(), 0);
}

#[test]
fn timestamped_controls_keep_their_boundary_and_late_controls_are_rejected() {
    let mut timeline = CompositeBoundaryTimeline::default();
    let child = basic(1);
    timeline
        .queue_control(start(3, 1, child, LoopMode::Playing))
        .unwrap();
    assert_eq!(timeline.next_control_poi(8), Some(3));
    timeline.advance_clock(2);
    assert_eq!(timeline.next_control_poi(6), Some(1));
    timeline.advance_clock(1);
    let trace = timeline
        .resolve_boundary(&[], &[], |identity| identity == child)
        .unwrap();
    assert_eq!(trace[0].at_sample, 3);
    assert!(timeline
        .queue_control(start(2, 2, child, LoopMode::Playing))
        .is_err());
    assert_eq!(timeline.counters().late_controls, 1);
}

#[test]
fn control_queue_and_dependency_wave_capacities_are_enforced_before_processing() {
    let mut controls = CompositeBoundaryTimeline::new(
        vec![],
        CompositeTimelineLimits {
            max_controls: 1,
            ..CompositeTimelineLimits::default()
        },
    )
    .unwrap();
    controls
        .queue_control(start(0, 1, basic(1), LoopMode::Playing))
        .unwrap();
    assert_eq!(
        controls.queue_control(start(0, 2, basic(2), LoopMode::Playing)),
        Err(CompositeTimelineControlError::QueueFull)
    );

    let child = basic(1);
    let leaf = composite(10);
    let root = composite(20);
    let targets = catalog(&[child, leaf, root]);
    let result = CompositeBoundaryTimeline::new(
        vec![
            CompositeTimelineNode {
                plan: plan(leaf, 4, &[(child, 0, None)], &targets),
                sync_source: basic(2),
            },
            CompositeTimelineNode {
                plan: plan(root, 4, &[(leaf, 0, None)], &targets),
                sync_source: basic(2),
            },
        ],
        CompositeTimelineLimits {
            max_event_waves: 1,
            ..CompositeTimelineLimits::default()
        },
    );
    assert!(matches!(
        result,
        Err(CompositeTimelineBuildError::EventWaveCapacity)
    ));
}

#[test]
fn trace_overflow_drops_diagnostics_without_affecting_the_runtime_transaction() {
    let child = basic(1);
    let source = composite(10);
    let targets = catalog(&[child, source]);
    let mut timeline = CompositeBoundaryTimeline::new(
        vec![CompositeTimelineNode {
            plan: plan(source, 4, &[(child, 0, None)], &targets),
            sync_source: basic(2),
        }],
        CompositeTimelineLimits {
            max_trace_entries: 1,
            ..CompositeTimelineLimits::default()
        },
    )
    .unwrap();
    timeline
        .queue_control(start(0, 1, source, LoopMode::Playing))
        .unwrap();
    timeline.resolve_boundary(&[], &[], |_| true).unwrap();
    assert_eq!(timeline.trace().len(), 1);
    assert_eq!(timeline.counters().trace_overflows, 1);
    assert_eq!(timeline.runtime(source).unwrap().mode(), LoopMode::Playing);
}

#[test]
fn event_overflow_latches_before_runtime_or_target_commit() {
    let child = basic(1);
    let source = basic(2);
    let composite = composite(10);
    let targets = catalog(&[child, source, composite]);
    let limits = CompositeTimelineLimits {
        max_primitive_events: 1,
        ..CompositeTimelineLimits::default()
    };
    let mut timeline = CompositeBoundaryTimeline::new(
        vec![CompositeTimelineNode {
            plan: plan(composite, 4, &[(child, 0, None)], &targets),
            sync_source: source,
        }],
        limits,
    )
    .unwrap();

    let error = timeline
        .resolve_boundary(&[source, basic(3)], &[], |_| true)
        .unwrap_err();
    assert_eq!(error.fault, CompositeTimelineFault::PrimitiveEventCapacity);
    assert_eq!(
        timeline.runtime(composite).unwrap().mode(),
        LoopMode::Stopped
    );
    assert!(timeline.trace().is_empty());
    assert_eq!(timeline.counters().primitive_event_overflows, 1);
}
