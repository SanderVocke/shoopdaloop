# Composite-loop processing architecture

## Status and scope

This document describes the implemented Stage 1 compiled plan/state machine and Stage 2 engine sample-timeline integration, followed by ownership and application boundaries reserved for later stages. The implementation is in:

- `shoop_engine::composite_plan`: off-thread validation and compilation;
- `shoop_engine::composite_runtime`: bounded, allocation-free state transitions;
- `shoop_engine::composite_timeline`: timestamp staging, stable topology, conflict resolution, propagation, trace, and fault state;
- `shoop_engine::session`: authoritative POI/sub-block advancement and basic-loop target commit;
- `tests/composite_state_machine.rs`, `tests/composite_timeline.rs`, and `tests/composite_timing.rs`: pure and sample-timing behavior;
- `tests/no_alloc.rs`: allocator-enforced runtime and integrated timeline tests.

Stage 2 connects compiled composites to `Session::process`, but deliberately does not yet add Stage 3 command queues/plan reclamation, Stage 3 snapshots, or the Stage 5 frontend switch. The existing frontend composite therefore remains the application timing authority until those later integrations are complete; frontend success is not evidence that the Stage 2 engine path is active in the application.

## Stable identities

`LoopIdentity` is the common identity for basic and composite targets:

```text
(slot: u32, generation: u32, kind: Basic | Composite)
```

Slots are globally unique among live loop targets. Reusing a slot requires a changed generation. Stable ordering is slot, generation, then kind; object address and container traversal never participate. `LoopTargetCatalog` sorts identities and rejects duplicate live slots. Compilation distinguishes missing from stale source/target references. It also requires the plan source to be a composite.

Compilation resolves every `CompositeEntry::target` against the catalog. The immutable plan contains only resolved `LoopIdentity` values and numeric metadata. Activation and each emitted action accept an engine-provided generation predicate. A stale activation rejects the candidate atomically; a stale post-activation action increments `stale_targets`, emits nothing, and never retargets the reused slot.

## Descriptor and compilation boundary

`CompositePlanDescriptor` is the non-RT input model. It contains a source identity, synchronization length, and parallel timelines. A timeline contains sequential sections; entries within one section are parallel. An entry contains only:

- a resolved target identity;
- signed input delay, so negative persisted/frontend values can be rejected rather than wrapped;
- optional signed cycle count, so non-positive and oversized values can be rejected;
- optional `LoopMode`.

`compile_composite_plan` performs all allocation, sorting, reference lookup, duration calculation, schedule flattening, mode classification, cycle detection, metadata construction, and capacity validation. This function is not callback-safe and is not intended to run in the RT domain.

Validation rejects:

- missing, stale, duplicate, or wrong-kind identities;
- negative delays, non-positive explicit cycle counts, unknown modes, and mixed explicit/implicit plans;
- zero synchronization length when a duration must be derived;
- schedule/sample arithmetic overflow;
- entry, target, iteration, seek-table, action, dependency-node, dependency-edge, or nesting-depth overflow;
- direct or transitive dependency cycles across the candidate plus unchanged installed topology.

A rejected candidate produces no partial plan and cannot alter runtime state.

## Immutable compiled plan

`CompiledCompositePlan` has private fields and read-only accessors. Its allocation-owning fields are fixed boxed slices built by the compiler:

| Storage | Purpose | Ordering |
|---|---|---|
| target table | Unique basic/composite target identities | Stable identity |
| desired-state tables | Effective scheduled occurrence and regular first-recording occurrence for each `(iteration, target)` | Iteration-major, then stable target |
| action table | Sparse structural changes for timeline/POI integration | Iteration, stop phase before set phase, stable target |
| action ranges | Direct slice range for each populated boundary | Increasing iteration |
| dependency order | Candidate topology order | Parent before child; stable identity tie-break |

A desired-state cell includes inherited/explicit mode, occurrence number, start iteration, duration, and whether the child was empty at compilation. This precomputes the metadata needed for:

- first-occurrence-only recording;
- deterministic active-child tracking and cancellation;
- direct immediate-seek lookup and cycle-offset derivation;
- mode-sensitive empty-child behavior;
- same-sample continuation without schedule replay.

The combined tables are bounded by `max_seek_entries`; immediate seek is therefore a target-table scan, never a replay from iteration zero. A regular recording reads the dedicated occurrence-zero table, so later or overlapping references cannot hide, extend, or restart the first scheduled recording interval.

There are no Qt types, weak pointers, object addresses, strings, runtime lookups, hash containers, locks, or callbacks in a compiled plan. `QObject`, `QVariant`, and frontend IDs must be resolved before compilation.

### Duplicate and overlapping entries

Occurrences are canonically sorted by start iteration, target identity, end iteration, and mode discriminant before occurrence ordinals are assigned. For structurally identical parallel references, this makes compilation independent of descriptor insertion/hash order. If references to one target overlap, the later canonical occurrence is the desired state in the overlap. Runtime output still follows the `SEMANTICS.md` stop-before-set phase order. This is deterministic input normalization, not cross-composite conflict resolution; cross-source precedence remains the Stage 2 boundary resolver's responsibility.

## Dependency graph and termination basis

Compilation substitutes the candidate source's outgoing edges into the installed composite topology, deduplicates edges in ordered storage, and performs a stable Kahn topological sort. Self, two-node, deep, and candidate-to-unchanged-plan cycles are rejected. Longest path is calculated during the sort and checked against `max_nesting_depth`.

This establishes the Stage 2 propagation termination basis:

1. accepted plans form a DAG;
2. parent-before-child delivery has a stable total order;
3. plan target/depth/action bounds are known before activation;
4. Stage 2 will add bounded once-per-source trigger delivery and event-wave storage.

The pure Stage 1 runtime emits a bounded batch but does not recursively deliver it. Therefore it cannot itself recurse or loop over the dependency graph.

## Pure runtime state

`CompositeRuntime` stores:

- installed source and target identities in fixed arrays;
- fixed active-target state (mode and cycle offset);
- current mode;
- optional pending mode and boundaries-to-skip countdown;
- iteration, synchronization position, and monotonic cycle count;
- accepted `play_after_record` value;
- fixed numeric counters and a fixed fault enum.

It owns no heap allocation. Runtime methods borrow an immutable compiled plan. Plan matching verifies source and the full ordered target table before state is read or changed.

### Boundary output

`CompositeTransitionBatch` is a fixed `[CompositeTargetTransition; 128]` plus length. Reconciliation scans at most 64 prevalidated targets in stable order:

1. disappearing active targets emit stops;
2. starts and mode changes emit in stable target order;
3. unchanged contiguous occurrences emit nothing;
4. immediate seek forces active targets to receive their precomputed cycle offset.

Each target can contribute at most one output in an ordinary reconciliation. A running plan replacement can emit at most 64 old-target stops plus 64 candidate-target starts/mode changes, proving that the 128-output batch cannot overflow for an accepted plan. The runtime still has an `OutputCapacity` fault/counter so a broken invariant fails explicitly rather than allocating or dropping an action.

### Mode and pass behavior

- A regular plan resolves `Inherit` to the composite mode.
- A script resolves its explicit modes and stops after its terminal boundary.
- Playing and playing-dry modes reserve an empty child's duration without starting it.
- Recording modes apply to empty children.
- During a regular recording pass, only occurrence zero for each target is active.
- At regular recording completion, `play_after_record` either reconciles iteration zero in the corresponding playback mode or stops all children.
- Regular non-recording completion reconciles terminal and iteration-zero state at one boundary and increments the cycle counter.
- A contiguous same-mode repeated target is not retriggered.
- Stop/clear removes pending state and emits deterministic child cleanup.

### Pending transitions and seek

`request_transition(mode, delay)` stores exactly `delay` boundaries to skip. At a sync boundary, an armed zero countdown replaces the old pass before old due actions; otherwise the countdown decreases and the current pass advances normally.

Immediate transition/seek validates `0 <= iteration < N`, directly indexes the desired-state table, stops no-longer-active children, and emits active targets with derived cycle offsets. It does not allocate or replay earlier boundaries. Invalid seek leaves musical state unchanged and increments `invalid_seeks`.

### Plan replacement

Stage 1 models the activation decision but intentionally does not own queued candidate plans:

- stopped and pending runtimes activate immediately, recheck all candidate generations, preserve a pending mode/countdown, and reset replacement state;
- running runtimes return `DeferredUntilIterationZero` without changing either plan or runtime;
- `activate_deferred_at_iteration_zero` verifies the old terminal boundary, rechecks generations, preserves same-target continuations, emits old stops before candidate starts, installs the new target table, and begins candidate iteration zero atomically.

Stage 3 will own and reclaim the deferred candidate and call this activation transaction at iteration zero. No plan `Box` or other allocation is currently transferred to or dropped by `CompositeRuntime`.

## Failure publication

Callback-relevant failures use `CompositeRuntimeError`, `CompositeRuntimeFault`, and `CompositeRuntimeCounters`. These are fixed enums/integers; runtime methods do not log or format. Counters include stale targets, invalid seeks, rejected modes, mismatched plans, output overflow, and arithmetic overflow. Counter increment saturates. Cycle-count overflow also saturates and records arithmetic overflow.

Compiler errors may be formatted because compilation is explicitly non-RT.

## Capacity budget implemented in Stage 1

The defaults are policy inputs to off-thread compilation, not hidden growth points:

| Capacity | Default | Runtime consequence |
|---|---:|---|
| entries | 256 | rejected before plan construction |
| targets | 64 | fixed runtime target/active arrays |
| compiled actions | 512 | rejected before activation |
| iterations | 16,384 | bounds schedule coordinates |
| seek cells | 65,536 | bounds precomputed desired-state storage |
| dependency nodes | 256 | bounds candidate topology work |
| dependency edges | 1,024 | bounds candidate topology work |
| nesting depth | 32 | bounds later propagation depth |
| boundary outputs | 128 hard maximum | fixed batch for up to 64 old stops plus 64 replacement starts |

Stage 2's [RT_SAFETY.md](RT_SAFETY.md) adds the implemented event, intent, wave, accepted-control, trace, POI, and sub-block capacities. Stage 1's limits remain the per-plan portion of that combined budget.

## Engine sample timeline implemented in Stage 2

`Session::process_loop_group` co-advances every basic loop to the earliest basic/channel POI or accepted timestamped-control boundary. It settles primitive sync propagation before calling `CompositeBoundaryTimeline`, then applies the resolver's basic-loop winners before any non-empty post-boundary sub-block. Composite iteration events use their sync source trigger and add no separate POI. Only an accepted timestamp between existing POIs introduces a composite-related split.

The timeline owns installed plans/runtimes in stable topological order. Its prepared graph includes parent-to-composite-target, composite-sync-source, primitive sync-source/follower, and producer-to-basic-sync-follower edges. It copies runtime state into preallocated working storage, seeds natural/direct intents, processes every composite once parent-before-child, resolves targets with the Stage 0 precedence, and swaps working runtimes into authority only after successful resolution. Composite target transitions execute inside that transaction, so nested iteration-zero actions reach primitive targets at the same sample through several levels.

Natural primitive wraps are already reflected in basic-loop mechanics when resolution starts. A higher-priority winning intent supersedes that state before post-boundary audio. Direct source stops are resolved before source triggers are delivered, suppressing coincident due composite work. Basic sync followers settle in bounded repeated waves rather than the previous single snapshot pass.

The resolver records a deterministic fixed-capacity transition trace and a latched fixed fault record. Event/intention failures abort before target commit; excess trace diagnostics are dropped and counted without affecting musical state. A sub-block overflow stops at the unserviceable boundary and prevents later processing. Exact capacities and allocator evidence are in [RT_SAFETY.md](RT_SAFETY.md).

## Command ownership and reclamation reserved for Stage 3

Prepared plans will cross into RT ownership through a bounded non-blocking queue. The callback will not construct, clone, drop, or deallocate them. Executed commands and displaced plans will move to a bounded non-RT reclamation queue. A running replacement will remain externally owned until the runtime reports its iteration-zero activation point. Exact ownership wrappers and queue capacities are intentionally deferred until the callback ownership refactor is implemented and audited.

## Snapshots and frontend integration reserved for Stages 3–5

A snapshot can be built from current getters without making observation authoritative: mode, pending state, iteration, cycle count, length, position, deterministic active children, counters, and fault. Snapshot transport is not yet implemented. It will use bounded non-blocking publication and stale-observation dropping as specified in `SEMANTICS.md`.

The frontend adapter will resolve persisted/QML loop IDs into `LoopIdentity`, build descriptors, compile plans off-thread, submit commands, and mirror snapshots. QML editing, persistence, grab transactions, and top-level application switching remain unchanged in Stage 1.

## Stage 1 verification map

All state-machine tests are engine-only and do not construct audio channels or Qt objects.

| Requirement | Evidence |
|---|---|
| Sequential/parallel/delay/repeat flattening | `compiler_flattens_sequential_parallel_delayed_and_repeated_entries` |
| Explicit/default duration and empty reservation | `compiler_derives_or_overrides_durations_and_reserves_empty_children` |
| Mode, length, delay, and arithmetic validation | `compiler_rejects_invalid_modes_lengths_and_schedule_arithmetic` |
| Stable identity and generation validation | `compiler_resolves_stable_identities_and_rejects_stale_generations` |
| Entry/target/iteration/seek/action capacities | `compiler_enforces_every_plan_capacity_before_activation` |
| Transitive/candidate cycle rejection | `compiler_rejects_self_transitive_and_candidate_topology_cycles` |
| Stable DAG order and graph/depth capacities | `dependency_order_and_all_graph_capacities_are_stable_and_bounded` |
| Canonical conflict/action order | `compiled_actions_and_parallel_conflicts_have_canonical_order` |
| Empty-plan no-op and regular inheritance/empty modes | `an_empty_plan_start_is_a_successful_stopped_no_op`, `regular_runtime_inherits_modes_and_empty_playback_is_duration_only` |
| Explicit empty/script behavior and completion | `script_empty_playback_is_reserved_but_empty_recording_is_applied`, `script_uses_explicit_modes_and_stops_after_one_pass` |
| Cycling and same-mode continuation | `regular_playback_cycles_without_retriggering_contiguous_repeats` |
| Stop, clear, cancellation, active-child order | `stop_and_clear_cancel_pending_state_and_clean_children_in_stable_order` |
| Countdown/retrigger and invalid mode status | `countdown_skips_exactly_the_requested_boundaries_while_current_pass_advances` |
| First recording occurrence, overlaps, and play-after-record | `overlapping_references_do_not_hide_the_first_recording_occurrence`, `recording_only_uses_first_occurrence_and_honors_both_pass_end_options` |
| Immediate seek/position/offset and invalid seek | `immediate_seek_uses_precomputed_state_offsets_without_replay` |
| Post-activation stale target | `stale_actions_are_skipped_and_reported_without_retargeting` |
| Snapshot-source fields and deterministic children | `state_reporting_has_deterministic_children_length_position_and_cycles` |
| Stopped/pending/running activation and atomic iteration-zero replacement | `stopped_and_pending_plan_replacements_activate_but_running_replacements_defer` |
| Replacement continuation and script-terminal behavior | `deferred_replacement_preserves_continuations_and_script_completion_stays_stopped` |
| Activation generation recheck | `activation_rechecks_candidate_generations_and_rejects_stale_targets_atomically` |
| Four regular/script nesting combinations | `all_regular_and_script_nesting_combinations_compile_to_composite_targets` |
| Long-running and integer edges | `long_running_cycle_counts_and_integer_boundaries_remain_defined`, `cycle_counter_saturates_and_reports_integer_overflow` |
| No runtime allocation/deallocation | `composite_state_machine_does_not_allocate_or_free` under `assert_no_alloc` |

The test command is:

```sh
cargo test -p shoop_engine --test composite_state_machine
cargo test -p shoop_engine --test no_alloc composite_state_machine_does_not_allocate_or_free
```

## Stage 1 completion verification

On 2026-07-31:

- `cargo test -p shoop_engine --test composite_state_machine`: **25 passed, 0 failed**;
- targeted allocator test above: **1 passed, 0 failed**;
- `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo test -p shoop_engine --features app_backend`: **718 passed, 0 failed** across all test binaries;
- `RUSTFLAGS="-D warnings" cargo build`: **passed**;
- `cargo fmt --all --check` and `git diff --check`: **passed**.

The missing-backend opt-out is the same documented host qualification as [BASELINE.md](BASELINE.md): JACK is unavailable on this Windows host. Stage 1 changes no frontend/QML behavior, so the Stage 0 QML baseline remains the applicable frontend evidence; it is not used as proof of the new engine state machine.

## Stage 2 verification map

The detailed capacity and overflow map is in [RT_SAFETY.md](RT_SAFETY.md#stage-2-verification-evidence). Stage 2 adds these engine-authoritative guarantees:

| Requirement | Evidence |
|---|---|
| Session POI integration and pre-audio commit | `mid_callback_composite_transition_changes_the_exact_first_output_sample` |
| Identical output/trace under callback partitioning | `callback_size_and_arbitrary_partitions_do_not_change_audio_or_transition_trace` |
| Source POI reuse | `iteration_aligned_composite_events_reuse_the_source_poi` |
| Primitive wrap seeding, including multiple wraps | `a_source_that_wraps_multiple_times_advances_every_composite_boundary` |
| Timestamp POI and exact accepted boundary | `timestamped_controls_keep_their_boundary_and_late_controls_are_rejected`, `timestamped_script_modes_commit_before_post_boundary_samples` |
| Deterministic direct/script/regular/natural conflict resolution | `script_regular_natural_and_direct_conflicts_use_total_precedence`, `same_class_conflicts_use_lower_source_identity_not_install_order` |
| Multi-level same-sample propagation | `nested_iteration_zero_propagates_through_several_levels_at_one_sample`, `composite_sync_triggers_propagate_transitively_without_snapshot_order`, `a_composite_started_primitive_source_triggers_its_follower_in_the_same_boundary`, `composite_to_primitive_to_composite_propagation_settles_before_audio_advances` |
| Source stop before due delivery | `direct_source_stop_suppresses_the_coincident_natural_trigger` |
| Session installation generation/combined-topology recheck | `session_rechecks_primitive_generations_before_timeline_installation`, `session_rejects_cycles_spanning_composite_and_primitive_sync_edges` |
| Queue, topology, event, and sub-block bounds | `control_queue_and_dependency_wave_capacities_are_enforced_before_processing`, `event_overflow_latches_before_runtime_or_target_commit`, `sub_block_overflow_latches_and_never_processes_the_remainder_late` |
| Allocation-free integrated timeline | `composite_timeline_processing_does_not_allocate_or_free` |

## Stage 2 completion verification

On 2026-07-31:

- `cargo test -p shoop_engine --test composite_timeline`: **12 passed, 0 failed**;
- `cargo test -p shoop_engine --test composite_timing`: **10 passed, 0 failed**;
- targeted integrated allocator test: **1 passed, 0 failed**;
- `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo test --workspace --features shoop_engine/app_backend`: **859 passed, 0 failed** across the workspace, including **741 engine tests**;
- `RUSTFLAGS="-D warnings" cargo build`: **passed**;
- Windows launcher equivalent `target/debug/shoopdaloop_dev.bat --self-test`: **187 passed, 0 failed, 0 skipped**;
- `cargo fmt --all --check`, `git diff --check`, and the Stage 2 artifact audit: **passed**.

The JACK opt-out and Windows `.bat` launcher qualification remain as documented in [BASELINE.md](BASELINE.md). The QML run protects existing behavior from the shared `Session` changes; it is not represented as evidence that the frontend now uses engine composites.
