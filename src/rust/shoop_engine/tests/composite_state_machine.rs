//! Pure composite-plan and state-machine coverage, independent of UI and audio routing.

use shoop_engine::*;

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

fn metadata(identity: LoopIdentity, length_samples: u64) -> LoopTargetMetadata {
    LoopTargetMetadata {
        identity,
        length_samples,
    }
}

fn entry(
    target: LoopIdentity,
    delay: i64,
    n_cycles: Option<i64>,
    mode: Option<LoopMode>,
) -> CompositeEntry {
    CompositeEntry {
        target,
        delay,
        n_cycles,
        mode,
    }
}

fn section(entries: Vec<CompositeEntry>) -> CompositeSection {
    CompositeSection { entries }
}

fn timeline(sections: Vec<CompositeSection>) -> CompositeTimeline {
    CompositeTimeline { sections }
}

fn descriptor(
    source: LoopIdentity,
    sync_length: u64,
    timelines: Vec<CompositeTimeline>,
) -> CompositePlanDescriptor {
    CompositePlanDescriptor {
        source,
        sync_length,
        timelines,
    }
}

fn catalog(source: LoopIdentity, targets: &[(LoopIdentity, u64)]) -> LoopTargetCatalog {
    let mut all = vec![metadata(source, 0)];
    all.extend(
        targets
            .iter()
            .map(|(identity, length)| metadata(*identity, *length)),
    );
    LoopTargetCatalog::new(all).unwrap()
}

fn compile(
    descriptor: &CompositePlanDescriptor,
    catalog: &LoopTargetCatalog,
) -> CompiledCompositePlan {
    compile_composite_plan(descriptor, catalog, &[], CompositePlanLimits::default()).unwrap()
}

fn always_current(_: LoopIdentity) -> bool {
    true
}

fn set_mode(mode: LoopMode, cycle_offset: u32, retrigger: bool) -> CompositeTargetAction {
    CompositeTargetAction::SetMode {
        mode,
        cycle_offset,
        retrigger,
    }
}

#[shoop_wasm_test_support::shoop_test]
fn compiler_flattens_sequential_parallel_delayed_and_repeated_entries() {
    let source = composite(100);
    let a = basic(3);
    let b = basic(1);
    let c = basic(2);
    let desc = descriptor(
        source,
        100,
        vec![
            timeline(vec![
                section(vec![entry(a, 0, Some(2), None), entry(c, 0, Some(1), None)]),
                section(vec![entry(b, 1, Some(1), None)]),
            ]),
            timeline(vec![section(vec![entry(a, 0, Some(1), None)])]),
        ],
    );
    let plan = compile(&desc, &catalog(source, &[(a, 100), (b, 100), (c, 100)]));

    assert_eq!(plan.kind(), CompiledCompositeKind::Regular);
    assert_eq!(plan.n_iterations(), 4);
    assert_eq!(plan.targets(), &[b, c, a]);
    assert!(plan
        .actions()
        .windows(2)
        .all(|pair| pair[0].iteration <= pair[1].iteration));
    assert_eq!(plan.actions_at(4).len(), 1);
}

#[shoop_wasm_test_support::shoop_test]
fn compiler_derives_or_overrides_durations_and_reserves_empty_children() {
    let source = composite(100);
    let derived = basic(1);
    let explicit = basic(2);
    let empty = basic(3);
    let desc = descriptor(
        source,
        100,
        vec![timeline(vec![
            section(vec![entry(derived, 0, None, None)]),
            section(vec![entry(explicit, 0, Some(2), None)]),
            section(vec![entry(empty, 0, None, None)]),
        ])],
    );
    let plan = compile(
        &desc,
        &catalog(source, &[(derived, 201), (explicit, 999), (empty, 0)]),
    );

    assert_eq!(plan.n_iterations(), 6);
    assert_eq!(plan.sync_length(), 100);
}

#[shoop_wasm_test_support::shoop_test]
fn compiler_rejects_invalid_modes_lengths_and_schedule_arithmetic() {
    let source = composite(100);
    let child = basic(1);
    let cat = catalog(source, &[(child, 100)]);
    let one = |entry| descriptor(source, 100, vec![timeline(vec![section(vec![entry])])]);

    assert_eq!(
        compile_composite_plan(
            &one(entry(child, -1, Some(1), None)),
            &cat,
            &[],
            CompositePlanLimits::default()
        ),
        Err(CompositeCompileError::NegativeDelay)
    );
    assert_eq!(
        compile_composite_plan(
            &one(entry(child, 0, Some(0), None)),
            &cat,
            &[],
            CompositePlanLimits::default()
        ),
        Err(CompositeCompileError::NonPositiveCycleCount)
    );
    assert_eq!(
        compile_composite_plan(
            &descriptor(
                source,
                0,
                vec![timeline(vec![section(vec![entry(child, 0, None, None)])])]
            ),
            &cat,
            &[],
            CompositePlanLimits::default()
        ),
        Err(CompositeCompileError::ZeroSyncLength)
    );
    assert_eq!(
        compile_composite_plan(
            &one(entry(child, 0, Some(1), Some(LoopMode::Unknown))),
            &cat,
            &[],
            CompositePlanLimits::default()
        ),
        Err(CompositeCompileError::UnknownMode)
    );
    let mixed = descriptor(
        source,
        100,
        vec![timeline(vec![section(vec![
            entry(child, 0, Some(1), None),
            entry(child, 0, Some(1), Some(LoopMode::Playing)),
        ])])],
    );
    assert_eq!(
        compile_composite_plan(&mixed, &cat, &[], CompositePlanLimits::default()),
        Err(CompositeCompileError::MixedModes)
    );
}

#[shoop_wasm_test_support::shoop_test]
fn compiler_resolves_stable_identities_and_rejects_stale_generations() {
    let source = composite(100);
    let current = basic(1);
    let stale = LoopIdentity {
        generation: 0,
        ..current
    };
    let cat = catalog(source, &[(current, 100)]);
    let stale_desc = descriptor(
        source,
        100,
        vec![timeline(vec![section(vec![entry(
            stale,
            0,
            Some(1),
            None,
        )])])],
    );
    assert_eq!(
        compile_composite_plan(&stale_desc, &cat, &[], CompositePlanLimits::default()),
        Err(CompositeCompileError::StaleTarget)
    );

    let stale_source = CompositePlanDescriptor {
        source: LoopIdentity {
            generation: 0,
            ..source
        },
        ..stale_desc
    };
    assert_eq!(
        compile_composite_plan(&stale_source, &cat, &[], CompositePlanLimits::default()),
        Err(CompositeCompileError::StaleSource)
    );
}

#[shoop_wasm_test_support::shoop_test]
fn compiler_enforces_every_plan_capacity_before_activation() {
    let source = composite(100);
    let a = basic(1);
    let b = basic(2);
    let desc = descriptor(
        source,
        1,
        vec![timeline(vec![section(vec![
            entry(a, 0, Some(3), None),
            entry(b, 0, Some(3), None),
        ])])],
    );
    let cat = catalog(source, &[(a, 1), (b, 1)]);

    let cases = [
        (
            CompositePlanLimits {
                max_entries: 1,
                ..Default::default()
            },
            CompositeCompileError::EntryCapacity,
        ),
        (
            CompositePlanLimits {
                max_targets: 1,
                ..Default::default()
            },
            CompositeCompileError::TargetCapacity,
        ),
        (
            CompositePlanLimits {
                max_iterations: 2,
                ..Default::default()
            },
            CompositeCompileError::IterationCapacity,
        ),
        (
            CompositePlanLimits {
                max_seek_entries: 5,
                ..Default::default()
            },
            CompositeCompileError::SeekCapacity,
        ),
        (
            CompositePlanLimits {
                max_actions: 1,
                ..Default::default()
            },
            CompositeCompileError::ActionCapacity,
        ),
    ];
    for (limits, expected) in cases {
        assert_eq!(
            compile_composite_plan(&desc, &cat, &[], limits),
            Err(expected)
        );
    }
}

#[shoop_wasm_test_support::shoop_test]
fn compiler_rejects_self_transitive_and_candidate_topology_cycles() {
    let a = composite(10);
    let b = composite(20);
    let c = composite(30);
    let cat = LoopTargetCatalog::new(vec![metadata(a, 0), metadata(b, 0), metadata(c, 0)]).unwrap();

    let self_cycle = descriptor(
        a,
        1,
        vec![timeline(vec![section(vec![entry(a, 0, Some(1), None)])])],
    );
    assert_eq!(
        compile_composite_plan(&self_cycle, &cat, &[], Default::default()),
        Err(CompositeCompileError::DependencyCycle)
    );

    let candidate = descriptor(
        a,
        1,
        vec![timeline(vec![section(vec![entry(b, 0, Some(1), None)])])],
    );
    let installed = vec![
        CompositeDependency {
            source: b,
            composite_children: vec![c],
        },
        CompositeDependency {
            source: c,
            composite_children: vec![a],
        },
    ];
    assert_eq!(
        compile_composite_plan(&candidate, &cat, &installed, Default::default()),
        Err(CompositeCompileError::DependencyCycle)
    );
}

#[shoop_wasm_test_support::shoop_test]
fn dependency_order_and_all_graph_capacities_are_stable_and_bounded() {
    let a = composite(10);
    let b = composite(20);
    let c = composite(30);
    let cat = LoopTargetCatalog::new(vec![metadata(c, 0), metadata(a, 0), metadata(b, 0)]).unwrap();
    let desc = descriptor(
        a,
        1,
        vec![timeline(vec![section(vec![entry(b, 0, Some(1), None)])])],
    );
    let installed = vec![CompositeDependency {
        source: b,
        composite_children: vec![c],
    }];
    let plan = compile_composite_plan(&desc, &cat, &installed, Default::default()).unwrap();
    assert_eq!(plan.dependency_order(), &[a, b, c]);

    for (limits, expected) in [
        (
            CompositePlanLimits {
                max_nesting_depth: 2,
                ..Default::default()
            },
            CompositeCompileError::NestingDepthCapacity,
        ),
        (
            CompositePlanLimits {
                max_dependency_nodes: 2,
                ..Default::default()
            },
            CompositeCompileError::DependencyNodeCapacity,
        ),
        (
            CompositePlanLimits {
                max_dependency_edges: 1,
                ..Default::default()
            },
            CompositeCompileError::DependencyEdgeCapacity,
        ),
    ] {
        assert_eq!(
            compile_composite_plan(&desc, &cat, &installed, limits),
            Err(expected)
        );
    }
    assert_eq!(
        compile_composite_plan(
            &desc,
            &cat,
            &installed,
            CompositePlanLimits {
                max_targets: MAX_COMPOSITE_TARGETS + 1,
                ..Default::default()
            }
        ),
        Err(CompositeCompileError::InvalidCapacity)
    );
}

#[shoop_wasm_test_support::shoop_test]
fn compiled_actions_and_parallel_conflicts_have_canonical_order() {
    let source = composite(100);
    let child = basic(1);
    let make = |modes: [LoopMode; 2]| {
        descriptor(
            source,
            1,
            vec![timeline(vec![section(vec![
                entry(child, 0, Some(2), Some(modes[0])),
                entry(child, 0, Some(2), Some(modes[1])),
            ])])],
        )
    };
    let cat = catalog(source, &[(child, 1)]);
    let left = compile(&make([LoopMode::Playing, LoopMode::Recording]), &cat);
    let right = compile(&make([LoopMode::Recording, LoopMode::Playing]), &cat);
    assert_eq!(left, right);
    assert!(left.actions().windows(2).all(|pair| {
        pair[0].iteration < pair[1].iteration || pair[0].action_ordinal < pair[1].action_ordinal
    }));

    let low = basic(2);
    let high = basic(9);
    let phases = compile(
        &descriptor(
            source,
            1,
            vec![timeline(vec![
                section(vec![entry(high, 0, Some(1), None)]),
                section(vec![entry(low, 0, Some(1), None)]),
            ])],
        ),
        &catalog(source, &[(low, 1), (high, 1)]),
    );
    let boundary = phases.actions_at(1);
    assert!(matches!(boundary[0].kind, CompiledPlanActionKind::Stop));
    assert!(matches!(
        boundary[1].kind,
        CompiledPlanActionKind::SetDesired(_)
    ));
}

#[shoop_wasm_test_support::shoop_test]
fn an_empty_plan_start_is_a_successful_stopped_no_op() {
    let source = composite(100);
    let desc = descriptor(source, 0, vec![]);
    let plan = compile(&desc, &catalog(source, &[]));
    let mut runtime = CompositeRuntime::new(&plan);
    let output = runtime
        .transition_immediate(&plan, LoopMode::Playing, None, always_current)
        .unwrap();
    assert!(output.is_empty());
    assert_eq!(runtime.mode(), LoopMode::Stopped);
    assert_eq!(runtime.iteration(), 0);
    assert_eq!(runtime.cycle_count(), 0);
}

#[shoop_wasm_test_support::shoop_test]
fn explicit_start_establishes_an_authoritative_target_snapshot() {
    let source = composite(100);
    let active = basic(1);
    let delayed = basic(2);
    let desc = descriptor(
        source,
        1,
        vec![timeline(vec![section(vec![
            entry(active, 0, Some(1), None),
            entry(delayed, 1, Some(1), None),
        ])])],
    );
    let plan = compile(&desc, &catalog(source, &[(active, 1), (delayed, 1)]));
    let mut runtime = CompositeRuntime::new(&plan);

    let batch = runtime
        .transition_immediate(&plan, LoopMode::Playing, None, always_current)
        .unwrap();

    assert_eq!(batch.as_slice().len(), 2);
    assert_eq!(batch.as_slice()[0].target, delayed);
    assert_eq!(batch.as_slice()[0].action, CompositeTargetAction::Stop);
    assert_eq!(batch.as_slice()[1].target, active);
    assert_eq!(
        batch.as_slice()[1].action,
        set_mode(LoopMode::Playing, 0, true)
    );
}

#[shoop_wasm_test_support::shoop_test]
fn explicit_seek_reestablishes_the_complete_destination_snapshot() {
    let source = composite(100);
    let active = basic(1);
    let delayed = basic(2);
    let desc = descriptor(
        source,
        1,
        vec![timeline(vec![section(vec![
            entry(active, 0, Some(1), None),
            entry(delayed, 1, Some(1), None),
        ])])],
    );
    let plan = compile(&desc, &catalog(source, &[(active, 1), (delayed, 1)]));
    let mut runtime = CompositeRuntime::new(&plan);
    runtime
        .transition_immediate(&plan, LoopMode::Playing, None, always_current)
        .unwrap();

    let batch = runtime.seek(&plan, 0, always_current).unwrap();

    assert_eq!(batch.as_slice().len(), 2);
    assert_eq!(batch.as_slice()[0].target, delayed);
    assert_eq!(batch.as_slice()[0].action, CompositeTargetAction::Stop);
    assert_eq!(batch.as_slice()[1].target, active);
    assert_eq!(
        batch.as_slice()[1].action,
        set_mode(LoopMode::Playing, 0, true)
    );
}

#[shoop_wasm_test_support::shoop_test]
fn delayed_start_executes_the_authoritative_snapshot() {
    let source = composite(100);
    let active = basic(1);
    let delayed = basic(2);
    let desc = descriptor(
        source,
        1,
        vec![timeline(vec![section(vec![
            entry(active, 0, Some(1), None),
            entry(delayed, 1, Some(1), None),
        ])])],
    );
    let plan = compile(&desc, &catalog(source, &[(active, 1), (delayed, 1)]));
    let mut runtime = CompositeRuntime::new(&plan);
    runtime.request_transition(LoopMode::Playing, 0).unwrap();

    let batch = runtime.sync_boundary(&plan, always_current).unwrap();

    assert_eq!(batch.as_slice().len(), 2);
    assert_eq!(batch.as_slice()[0].target, delayed);
    assert_eq!(batch.as_slice()[0].action, CompositeTargetAction::Stop);
    assert_eq!(batch.as_slice()[1].target, active);
    assert_eq!(
        batch.as_slice()[1].action,
        set_mode(LoopMode::Playing, 0, true)
    );
}

#[shoop_wasm_test_support::shoop_test]
fn regular_runtime_inherits_modes_and_empty_playback_is_duration_only() {
    let source = composite(100);
    let full = basic(1);
    let empty = basic(2);
    let desc = descriptor(
        source,
        1,
        vec![timeline(vec![section(vec![
            entry(full, 0, Some(1), None),
            entry(empty, 0, Some(1), None),
        ])])],
    );
    let plan = compile(&desc, &catalog(source, &[(full, 1), (empty, 0)]));

    for mode in [
        LoopMode::Playing,
        LoopMode::PlayingDryThroughWet,
        LoopMode::Replacing,
    ] {
        let mut runtime = CompositeRuntime::new(&plan);
        let batch = runtime
            .transition_immediate(&plan, mode, None, always_current)
            .unwrap();
        assert_eq!(batch.as_slice().len(), 2);
        if mode == LoopMode::Replacing {
            assert_eq!(batch.as_slice()[0].target, full);
            assert_eq!(
                batch.as_slice()[0].action,
                set_mode(LoopMode::Replacing, 0, true)
            );
            assert_eq!(batch.as_slice()[1].target, empty);
            assert_eq!(batch.as_slice()[1].action, set_mode(mode, 0, true));
        } else {
            assert_eq!(batch.as_slice()[0].target, empty);
            assert_eq!(batch.as_slice()[0].action, CompositeTargetAction::Stop);
            assert_eq!(batch.as_slice()[1].target, full);
            assert_eq!(batch.as_slice()[1].action, set_mode(mode, 0, true));
        }
    }

    for mode in [LoopMode::Recording, LoopMode::RecordingDryIntoWet] {
        let mut runtime = CompositeRuntime::new(&plan);
        let batch = runtime
            .transition_immediate(&plan, mode, None, always_current)
            .unwrap();
        assert_eq!(batch.as_slice().len(), 2);
    }
}

#[shoop_wasm_test_support::shoop_test]
fn script_empty_playback_is_reserved_but_empty_recording_is_applied() {
    let source = composite(100);
    let playing = basic(1);
    let recording = basic(2);
    let desc = descriptor(
        source,
        1,
        vec![timeline(vec![section(vec![
            entry(playing, 0, Some(1), Some(LoopMode::Playing)),
            entry(recording, 0, Some(1), Some(LoopMode::Recording)),
        ])])],
    );
    let plan = compile(&desc, &catalog(source, &[(playing, 0), (recording, 0)]));
    let mut runtime = CompositeRuntime::new(&plan);
    let output = runtime
        .transition_immediate(&plan, LoopMode::Playing, None, always_current)
        .unwrap();
    assert_eq!(output.as_slice().len(), 2);
    assert_eq!(output.as_slice()[0].target, playing);
    assert_eq!(output.as_slice()[0].action, CompositeTargetAction::Stop);
    assert_eq!(output.as_slice()[1].target, recording);
    assert_eq!(
        output.as_slice()[1].action,
        set_mode(LoopMode::Recording, 0, true)
    );
}

#[shoop_wasm_test_support::shoop_test]
fn script_uses_explicit_modes_and_stops_after_one_pass() {
    let source = composite(100);
    let child = basic(1);
    let desc = descriptor(
        source,
        1,
        vec![timeline(vec![
            section(vec![entry(child, 0, Some(1), Some(LoopMode::Playing))]),
            section(vec![entry(child, 0, Some(1), Some(LoopMode::Recording))]),
        ])],
    );
    let plan = compile(&desc, &catalog(source, &[(child, 1)]));
    let mut runtime = CompositeRuntime::new(&plan);

    let start = runtime
        .transition_immediate(&plan, LoopMode::Playing, None, always_current)
        .unwrap();
    assert_eq!(
        start.as_slice()[0].action,
        set_mode(LoopMode::Playing, 0, true)
    );
    let change = runtime.sync_boundary(&plan, always_current).unwrap();
    assert_eq!(
        change.as_slice()[0].action,
        set_mode(LoopMode::Recording, 0, false)
    );
    let finish = runtime.sync_boundary(&plan, always_current).unwrap();
    assert_eq!(finish.as_slice()[0].action, CompositeTargetAction::Stop);
    assert_eq!(runtime.mode(), LoopMode::Stopped);
    assert_eq!(runtime.iteration(), 0);
}

#[shoop_wasm_test_support::shoop_test]
fn regular_playback_cycles_without_retriggering_contiguous_repeats() {
    let source = composite(100);
    let child = basic(1);
    let desc = descriptor(
        source,
        1,
        vec![timeline(vec![
            section(vec![entry(child, 0, Some(1), None)]),
            section(vec![entry(child, 0, Some(1), None)]),
        ])],
    );
    let plan = compile(&desc, &catalog(source, &[(child, 1)]));
    let mut runtime = CompositeRuntime::new(&plan);
    assert_eq!(
        runtime
            .transition_immediate(&plan, LoopMode::Playing, None, always_current)
            .unwrap()
            .as_slice()
            .len(),
        1
    );
    assert!(runtime
        .sync_boundary(&plan, always_current)
        .unwrap()
        .is_empty());
    assert!(runtime
        .sync_boundary(&plan, always_current)
        .unwrap()
        .is_empty());
    assert_eq!(runtime.iteration(), 0);
    assert_eq!(runtime.cycle_count(), 1);
}

#[shoop_wasm_test_support::shoop_test]
fn stop_and_clear_cancel_pending_state_and_clean_children_in_stable_order() {
    let source = composite(100);
    let high = basic(9);
    let low = basic(1);
    let desc = descriptor(
        source,
        1,
        vec![timeline(vec![section(vec![
            entry(high, 0, Some(2), None),
            entry(low, 0, Some(2), None),
        ])])],
    );
    let plan = compile(&desc, &catalog(source, &[(high, 1), (low, 1)]));
    let mut runtime = CompositeRuntime::new(&plan);
    runtime
        .transition_immediate(&plan, LoopMode::Playing, None, always_current)
        .unwrap();
    runtime.request_transition(LoopMode::Recording, 3).unwrap();

    let stopped = runtime.stop(&plan, always_current).unwrap();
    assert_eq!(
        stopped
            .as_slice()
            .iter()
            .map(|transition| transition.target)
            .collect::<Vec<_>>(),
        vec![low, high]
    );
    assert_eq!(runtime.pending(), None);
    assert_eq!(runtime.mode(), LoopMode::Stopped);

    let stopped_again = runtime.stop(&plan, always_current).unwrap();
    assert_eq!(stopped_again.as_slice().len(), 2);
    assert!(stopped_again
        .as_slice()
        .iter()
        .all(|transition| transition.action == CompositeTargetAction::Stop));

    let cleared = runtime.clear(&plan, always_current).unwrap();
    assert_eq!(cleared.as_slice().len(), 2);
    assert_eq!(runtime.active_children().count(), 0);
    assert_eq!(runtime.cycle_count(), 0);
}

#[shoop_wasm_test_support::shoop_test]
fn countdown_skips_exactly_the_requested_boundaries_while_current_pass_advances() {
    let source = composite(100);
    let child = basic(1);
    let desc = descriptor(
        source,
        1,
        vec![timeline(vec![section(vec![entry(
            child,
            0,
            Some(3),
            None,
        )])])],
    );
    let plan = compile(&desc, &catalog(source, &[(child, 1)]));
    let mut runtime = CompositeRuntime::new(&plan);
    runtime
        .transition_immediate(&plan, LoopMode::Playing, None, always_current)
        .unwrap();
    runtime.request_transition(LoopMode::Recording, 1).unwrap();

    assert!(runtime
        .sync_boundary(&plan, always_current)
        .unwrap()
        .is_empty());
    assert_eq!(runtime.iteration(), 1);
    assert_eq!(runtime.pending().unwrap().boundaries_to_skip, 0);
    let due = runtime.sync_boundary(&plan, always_current).unwrap();
    assert_eq!(runtime.iteration(), 0);
    assert_eq!(runtime.mode(), LoopMode::Recording);
    assert_eq!(
        due.as_slice()[0].action,
        set_mode(LoopMode::Recording, 0, true)
    );
    assert_eq!(
        runtime.request_transition(LoopMode::Unknown, 0),
        Err(CompositeRuntimeError::UnknownMode)
    );
    assert_eq!(runtime.counters().rejected_modes, 1);

    runtime.request_transition(LoopMode::Stopped, 0).unwrap();
    let stopped = runtime.sync_boundary(&plan, always_current).unwrap();
    assert_eq!(stopped.as_slice()[0].action, CompositeTargetAction::Stop);
    assert_eq!(runtime.mode(), LoopMode::Stopped);
    assert_eq!(runtime.iteration(), 0);
}

#[shoop_wasm_test_support::shoop_test]
fn overlapping_references_do_not_hide_the_first_recording_occurrence() {
    let source = composite(100);
    let child = basic(1);
    let desc = descriptor(
        source,
        1,
        vec![timeline(vec![section(vec![
            entry(child, 0, Some(2), None),
            entry(child, 1, Some(2), None),
        ])])],
    );
    let plan = compile(&desc, &catalog(source, &[(child, 1)]));
    let mut runtime = CompositeRuntime::new(&plan);
    runtime
        .transition_immediate(&plan, LoopMode::Recording, None, always_current)
        .unwrap();
    assert!(runtime
        .sync_boundary(&plan, always_current)
        .unwrap()
        .is_empty());
    let first_end = runtime.sync_boundary(&plan, always_current).unwrap();
    assert_eq!(first_end.as_slice()[0].action, CompositeTargetAction::Stop);
}

#[shoop_wasm_test_support::shoop_test]
fn recording_only_uses_first_occurrence_and_honors_both_pass_end_options() {
    let source = composite(100);
    let child = basic(1);
    let desc = descriptor(
        source,
        1,
        vec![timeline(vec![
            section(vec![entry(child, 0, Some(1), None)]),
            section(vec![entry(child, 0, Some(1), None)]),
        ])],
    );
    let plan = compile(&desc, &catalog(source, &[(child, 1)]));

    let mut stop_after = CompositeRuntime::new(&plan);
    stop_after
        .transition_immediate(&plan, LoopMode::Recording, None, always_current)
        .unwrap();
    let repeated = stop_after.sync_boundary(&plan, always_current).unwrap();
    assert_eq!(repeated.as_slice()[0].action, CompositeTargetAction::Stop);
    assert!(stop_after
        .sync_boundary(&plan, always_current)
        .unwrap()
        .is_empty());
    assert_eq!(stop_after.mode(), LoopMode::Stopped);

    let mut play_after = CompositeRuntime::new(&plan);
    play_after.set_play_after_record(true);
    play_after
        .transition_immediate(&plan, LoopMode::Recording, None, always_current)
        .unwrap();
    play_after.sync_boundary(&plan, always_current).unwrap();
    let playback = play_after.sync_boundary(&plan, always_current).unwrap();
    assert_eq!(play_after.mode(), LoopMode::Playing);
    assert_eq!(play_after.iteration(), 0);
    assert_eq!(
        playback.as_slice()[0].action,
        set_mode(LoopMode::Playing, 0, true)
    );
}

#[shoop_wasm_test_support::shoop_test]
fn immediate_seek_uses_precomputed_state_offsets_without_replay() {
    let source = composite(100);
    let a = basic(1);
    let b = basic(2);
    let desc = descriptor(
        source,
        100,
        vec![timeline(vec![section(vec![
            entry(a, 0, Some(4), None),
            entry(b, 1, Some(2), None),
        ])])],
    );
    let plan = compile(&desc, &catalog(source, &[(a, 100), (b, 100)]));
    let mut runtime = CompositeRuntime::new(&plan);

    let middle = runtime
        .transition_immediate(&plan, LoopMode::Playing, Some(2), always_current)
        .unwrap();
    assert_eq!(middle.as_slice().len(), 2);
    assert_eq!(
        middle.as_slice()[0].action,
        set_mode(LoopMode::Playing, 2, true)
    );
    assert_eq!(
        middle.as_slice()[1].action,
        set_mode(LoopMode::Playing, 1, true)
    );
    runtime.set_sync_position(&plan, 7).unwrap();
    assert_eq!(runtime.position_samples(&plan).unwrap(), 207);

    let changed = runtime.seek(&plan, 3, always_current).unwrap();
    assert_eq!(changed.as_slice().len(), 2);
    assert_eq!(changed.as_slice()[0].target, b);
    assert_eq!(changed.as_slice()[0].action, CompositeTargetAction::Stop);
    assert_eq!(changed.as_slice()[1].target, a);
    assert_eq!(
        changed.as_slice()[1].action,
        set_mode(LoopMode::Playing, 3, true)
    );

    let before = (runtime.mode(), runtime.iteration());
    assert_eq!(
        runtime.seek(&plan, 4, always_current),
        Err(CompositeRuntimeError::InvalidSeek)
    );
    assert_eq!((runtime.mode(), runtime.iteration()), before);
    assert_eq!(runtime.counters().invalid_seeks, 1);
    assert_eq!(
        runtime.transition_immediate(&plan, LoopMode::Playing, Some(-1), always_current),
        Err(CompositeRuntimeError::InvalidSeek)
    );
    assert_eq!(runtime.counters().invalid_seeks, 2);
}

#[shoop_wasm_test_support::shoop_test]
fn stale_actions_are_skipped_and_reported_without_retargeting() {
    let source = composite(100);
    let child = basic(1);
    let desc = descriptor(
        source,
        1,
        vec![timeline(vec![section(vec![entry(
            child,
            0,
            Some(1),
            None,
        )])])],
    );
    let plan = compile(&desc, &catalog(source, &[(child, 1)]));
    let mut runtime = CompositeRuntime::new(&plan);
    let batch = runtime
        .transition_immediate(&plan, LoopMode::Playing, None, |_| false)
        .unwrap();
    assert!(batch.is_empty());
    assert_eq!(runtime.active_children().count(), 0);
    assert_eq!(runtime.counters().stale_targets, 1);
}

#[shoop_wasm_test_support::shoop_test]
fn state_reporting_has_deterministic_children_length_position_and_cycles() {
    let source = composite(100);
    let high = basic(8);
    let low = basic(2);
    let desc = descriptor(
        source,
        32,
        vec![timeline(vec![section(vec![
            entry(high, 0, Some(2), None),
            entry(low, 0, Some(2), None),
        ])])],
    );
    let plan = compile(&desc, &catalog(source, &[(high, 32), (low, 32)]));
    let mut runtime = CompositeRuntime::new(&plan);
    runtime
        .transition_immediate(&plan, LoopMode::Playing, None, always_current)
        .unwrap();
    runtime.set_sync_position(&plan, 31).unwrap();

    assert_eq!(runtime.length_samples(&plan).unwrap(), 64);
    assert_eq!(runtime.position_samples(&plan).unwrap(), 31);
    assert_eq!(
        runtime
            .active_children()
            .map(|child| child.identity)
            .collect::<Vec<_>>(),
        vec![low, high]
    );
    runtime.sync_boundary(&plan, always_current).unwrap();
    runtime.sync_boundary(&plan, always_current).unwrap();
    assert_eq!(runtime.cycle_count(), 1);
}

#[shoop_wasm_test_support::shoop_test]
fn stopped_and_pending_plan_replacements_activate_but_running_replacements_defer() {
    let source = composite(100);
    let a = basic(1);
    let b = basic(2);
    let cat = catalog(source, &[(a, 1), (b, 1)]);
    let old = compile(
        &descriptor(
            source,
            1,
            vec![timeline(vec![section(vec![entry(a, 0, Some(1), None)])])],
        ),
        &cat,
    );
    let new = compile(
        &descriptor(
            source,
            1,
            vec![timeline(vec![section(vec![entry(b, 0, Some(1), None)])])],
        ),
        &cat,
    );

    let mut stopped = CompositeRuntime::new(&old);
    let (result, _) = stopped.activate_plan(&old, &new, always_current).unwrap();
    assert_eq!(result, CompositePlanReplacement::Activated);
    stopped.request_transition(LoopMode::Playing, 2).unwrap();
    let (result, _) = stopped.activate_plan(&new, &old, always_current).unwrap();
    assert_eq!(result, CompositePlanReplacement::Activated);
    assert_eq!(stopped.pending().unwrap().boundaries_to_skip, 2);

    stopped
        .transition_immediate(&old, LoopMode::Playing, None, always_current)
        .unwrap();
    let (result, batch) = stopped.activate_plan(&old, &new, always_current).unwrap();
    assert_eq!(result, CompositePlanReplacement::DeferredUntilIterationZero);
    assert!(batch.is_empty());

    let activated = stopped
        .activate_deferred_at_iteration_zero(&old, &new, always_current)
        .unwrap();
    assert_eq!(activated.as_slice().len(), 2);
    assert_eq!(activated.as_slice()[0].target, a);
    assert_eq!(activated.as_slice()[0].action, CompositeTargetAction::Stop);
    assert_eq!(activated.as_slice()[1].target, b);
    assert_eq!(
        activated.as_slice()[1].action,
        set_mode(LoopMode::Playing, 0, true)
    );
    assert_eq!(stopped.active_children().next().unwrap().identity, b);
}

#[shoop_wasm_test_support::shoop_test]
fn deferred_replacement_preserves_continuations_and_script_completion_stays_stopped() {
    let source = composite(100);
    let a = basic(1);
    let b = basic(2);
    let cat = catalog(source, &[(a, 1), (b, 1)]);
    let old = compile(
        &descriptor(
            source,
            1,
            vec![timeline(vec![section(vec![entry(a, 0, Some(1), None)])])],
        ),
        &cat,
    );
    let continuation = compile(
        &descriptor(
            source,
            1,
            vec![timeline(vec![section(vec![entry(a, 0, Some(2), None)])])],
        ),
        &cat,
    );
    let mut runtime = CompositeRuntime::new(&old);
    runtime
        .transition_immediate(&old, LoopMode::Playing, None, always_current)
        .unwrap();
    assert!(runtime
        .activate_deferred_at_iteration_zero(&old, &continuation, always_current)
        .unwrap()
        .is_empty());
    assert_eq!(runtime.active_children().next().unwrap().identity, a);

    let script = compile(
        &descriptor(
            source,
            1,
            vec![timeline(vec![section(vec![entry(
                a,
                0,
                Some(1),
                Some(LoopMode::Playing),
            )])])],
        ),
        &cat,
    );
    let script_candidate = compile(
        &descriptor(
            source,
            1,
            vec![timeline(vec![section(vec![entry(
                b,
                0,
                Some(1),
                Some(LoopMode::Playing),
            )])])],
        ),
        &cat,
    );
    let mut script_runtime = CompositeRuntime::new(&script);
    script_runtime
        .transition_immediate(&script, LoopMode::Playing, None, always_current)
        .unwrap();
    let output = script_runtime
        .activate_deferred_at_iteration_zero(&script, &script_candidate, always_current)
        .unwrap();
    assert_eq!(output.as_slice().len(), 1);
    assert_eq!(output.as_slice()[0].target, a);
    assert_eq!(output.as_slice()[0].action, CompositeTargetAction::Stop);
    assert_eq!(script_runtime.mode(), LoopMode::Stopped);
    assert_eq!(script_runtime.active_children().count(), 0);
}

#[shoop_wasm_test_support::shoop_test]
fn activation_rechecks_candidate_generations_and_rejects_stale_targets_atomically() {
    let source = composite(100);
    let a = basic(1);
    let b = basic(2);
    let cat = catalog(source, &[(a, 1), (b, 1)]);
    let old = compile(
        &descriptor(
            source,
            1,
            vec![timeline(vec![section(vec![entry(a, 0, Some(1), None)])])],
        ),
        &cat,
    );
    let new = compile(
        &descriptor(
            source,
            1,
            vec![timeline(vec![section(vec![entry(b, 0, Some(1), None)])])],
        ),
        &cat,
    );
    let mut runtime = CompositeRuntime::new(&old);
    assert_eq!(
        runtime.activate_plan(&old, &new, |identity| identity != b),
        Err(CompositeRuntimeError::StalePlanTarget)
    );
    runtime
        .transition_immediate(&old, LoopMode::Playing, None, always_current)
        .unwrap();
    assert_eq!(runtime.active_children().next().unwrap().identity, a);
}

#[shoop_wasm_test_support::shoop_test]
fn all_regular_and_script_nesting_combinations_compile_to_composite_targets() {
    let parent = composite(10);
    let child = composite(20);
    let primitive = basic(30);
    let cat = LoopTargetCatalog::new(vec![
        metadata(parent, 0),
        metadata(child, 1),
        metadata(primitive, 1),
    ])
    .unwrap();

    for child_mode in [None, Some(LoopMode::Playing)] {
        let child_plan = compile(
            &descriptor(
                child,
                1,
                vec![timeline(vec![section(vec![entry(
                    primitive,
                    0,
                    Some(1),
                    child_mode,
                )])])],
            ),
            &cat,
        );
        assert_eq!(
            child_plan.kind(),
            if child_mode.is_some() {
                CompiledCompositeKind::Script
            } else {
                CompiledCompositeKind::Regular
            }
        );

        for parent_mode in [None, Some(LoopMode::Playing)] {
            let parent_plan = compile(
                &descriptor(
                    parent,
                    1,
                    vec![timeline(vec![section(vec![entry(
                        child,
                        0,
                        Some(1),
                        parent_mode,
                    )])])],
                ),
                &cat,
            );
            let mut runtime = CompositeRuntime::new(&parent_plan);
            let output = runtime
                .transition_immediate(&parent_plan, LoopMode::Playing, None, always_current)
                .unwrap();
            assert_eq!(output.as_slice()[0].target, child);
            assert_eq!(
                parent_plan.kind(),
                if parent_mode.is_some() {
                    CompiledCompositeKind::Script
                } else {
                    CompiledCompositeKind::Regular
                }
            );
        }
    }
}

#[shoop_wasm_test_support::shoop_test]
fn long_running_cycle_counts_and_integer_boundaries_remain_defined() {
    let source = composite(100);
    let child = basic(1);
    let desc = descriptor(
        source,
        1,
        vec![timeline(vec![section(vec![entry(
            child,
            0,
            Some(1),
            None,
        )])])],
    );
    let plan = compile(&desc, &catalog(source, &[(child, 1)]));
    let mut runtime = CompositeRuntime::new(&plan);
    runtime
        .transition_immediate(&plan, LoopMode::Playing, None, always_current)
        .unwrap();
    for _ in 0..100_000 {
        runtime.sync_boundary(&plan, always_current).unwrap();
    }
    assert_eq!(runtime.cycle_count(), 100_000);
    assert_eq!(runtime.iteration(), 0);

    let overflowing = descriptor(
        source,
        u64::MAX,
        vec![timeline(vec![section(vec![entry(
            child,
            0,
            Some(2),
            None,
        )])])],
    );
    assert_eq!(
        compile_composite_plan(
            &overflowing,
            &catalog(source, &[(child, 1)]),
            &[],
            Default::default()
        ),
        Err(CompositeCompileError::ArithmeticOverflow)
    );
}
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
