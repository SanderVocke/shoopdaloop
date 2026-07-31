# Composite-loop real-time safety and capacity budget

## Status and scope

This is the initial Stage 2 budget for the engine-authoritative composite timeline. It covers the implemented `CompositeBoundaryTimeline`, its integration into `Session::process_loop_group`, and the already compiled Stage 1 plans. It is not the complete callback-path audit required in Stage 3 and Stage 6: command transport, plan ownership/reclamation, snapshots, driver I/O, FX, and every existing session allocation exception still require those later audits.

The application frontend does not submit composite plans to this path yet. Stage 2 proves the engine timing and boundary mechanism; Stages 3–5 provide non-blocking ownership, publication, and frontend switching.

## Implemented capacity table

| Resource | Stage 2 bound | Storage/validation point | Overflow behavior |
|---|---:|---|---|
| Compiled entries per plan | 256 | Off-thread `CompositePlanLimits` | Reject candidate plan |
| Targets per plan | 64 | Off-thread compiler and fixed runtime arrays | Reject candidate plan |
| Compiled actions per plan | 512 | Off-thread compiler | Reject candidate plan |
| Iterations per plan | 16,384 | Off-thread compiler | Reject candidate plan |
| Precomputed seek cells per plan | 65,536 | Off-thread compiler | Reject candidate plan |
| Installed composites per timeline | 64 | `CompositeTimelineLimits` during construction | Reject prepared timeline |
| Primitive events at one boundary | 256 | Preallocated event scratch | Latch `PrimitiveEventCapacity` before resolver commit |
| Intents at one boundary | 16,384 | Preallocated intent scratch | Latch `IntentCapacity` before target commit |
| Composite/event propagation depth | 32 waves | Prepared topology validation and bounded primitive propagation | Reject prepared topology or latch `EventWaveCapacity` |
| Accepted timestamped controls | 128 | Fixed `[Option<AcceptedTimelineControl>; 128]` | Producer receives `QueueFull`; command is not accepted |
| Transition trace entries per callback | 16,384 | Preallocated trace storage | Drop excess diagnostics and increment `trace_overflows`; authoritative processing continues |
| Runtime output per composite transition | 128 | Fixed `CompositeTransitionBatch` | Latch runtime fault; no target commit |
| Session sub-blocks per callback | 16 | Existing `MAX_SUB_BLOCKS` | Latch `SubBlockCapacity`, stop at the unserviceable boundary, process no later samples |

The intent-construction check reserves for every installed composite's maximum 128 outputs, all accepted controls, and all primitive natural intents. Construction fails if configured intent storage cannot hold that one-boundary maximum. Scratch vectors are allocated and reserved when the timeline or graph schedule is built; processing only clears, indexes, sorts within capacity, and swaps fixed runtime state.

## Initial later-stage queue budgets

These are explicit design inputs, not claims of completed Stage 3 transport:

| Resource | Initial target | Stage 2 status |
|---|---:|---|
| Callback command queue | 128 accepted commands per drain | Stage 2 has fixed accepted-control staging; the non-blocking producer/consumer queue is Stage 3 |
| Prepared plan-install queue | 16 plans | Not implemented until Stage 3 ownership/reclamation |
| Snapshot publication | 3 preallocated latest-state slots | Not implemented until Stage 3; a full publisher must overwrite/drop stale observation and count it |
| Displaced-plan reclamation queue | 16 plans | Not implemented until Stage 3; callback-side destruction remains forbidden |

A later stage may change these values only with updated memory/cost evidence and matching validation/tests. It may not replace a bounded rejection with allocation, blocking, or late delivery.

## Points of interest and sub-blocks

Each basic loop contributes one dominant next POI; coincident loop-end, trigger, and channel reasons share it. All basic loops are co-processed to the earliest POI before any boundary is resolved. Composite iteration changes reuse the configured sync source trigger and therefore add no POI or sub-block. A composite-only POI is currently needed only for an accepted timestamped control that falls before the next source/basic POI.

The callback interval is processed as half-open sample spans. At a boundary:

1. all basic loops advance to the same sample;
2. primitive/sync propagation settles in bounded waves;
3. natural and accepted direct intents seed the composite resolver;
4. composites run once in stable parent-before-child topology order;
5. target winners are selected using the Stage 0 precedence;
6. basic and composite target state is committed before the next non-empty sub-block.

Consequently, a source-aligned composite action reuses the source POI. `iteration_aligned_composite_events_reuse_the_source_poi` verifies that enabling the composite does not increase the steady-state sub-block count.

## Transaction and termination argument

The prepared topology includes parent-to-composite-target edges, composite-sync-source edges, primitive sync-source/follower edges, and producer-to-basic-sync-follower edges. Timeline construction and Session installation perform stable topological validation and reject combined cycles or depth above the wave bound. At a boundary:

- active composite runtimes are copied into preallocated working runtimes;
- direct and natural inputs are bounded before propagation;
- each installed composite is visited exactly once in parent-before-child order;
- each source identity is inserted into the trigger set at most once;
- each target is resolved exactly once after all eligible producers precede it;
- active runtimes are swapped with working runtimes only after successful resolution.

This replaces one-pass snapshot recursion with bounded transitive propagation. Nested iteration-zero output is available at the same sample even through several composite levels.

A primitive-event, intent, wave, or runtime failure leaves the active composite runtimes and basic target actions uncommitted. Trace capacity is observational: excess entries are dropped with a counter only after a successful transaction, so diagnostics cannot fault or alter musical state. The fixed fault record latches the first failure sample. A sub-block failure occurs after all serviceable earlier spans, freezes loop/composite advancement at that point, and prevents processing the callback remainder or later callbacks until reset. Accepted events are never retained for later musical delivery after a fault.

## Allocation and locking evidence

The Stage 2 hot structures contain fixed arrays or vectors whose capacities are reserved before processing. The resolver uses stable identities, ordered prepared topology, linear/binary bounded lookup, and no hash traversal. It does not log, format strings, lock, allocate, deallocate, compile plans, or install/drop plans during boundary processing.

`composite_timeline_processing_does_not_allocate_or_free` runs repeated source wraps and composite commits inside `assert_no_alloc`. This evidence is deliberately scoped to the new timeline path. Existing `Session` code still contains separately annotated allocation exceptions and lock-bearing application/driver paths; removing and auditing them is Stage 3 and Stage 6 work, not something this document masks.

## Stage 2 verification evidence

| Requirement | Automated evidence |
|---|---|
| Exact mid-callback transition and post-boundary output | `mid_callback_composite_transition_changes_the_exact_first_output_sample` |
| Buffer-size/partition independence | `callback_size_and_arbitrary_partitions_do_not_change_audio_or_transition_trace` |
| Deep same-sample nesting and transitive sync propagation | `nested_iteration_zero_propagates_through_several_levels_at_one_sample`, `composite_sync_triggers_propagate_transitively_without_snapshot_order`, `a_composite_started_primitive_source_triggers_its_follower_in_the_same_boundary`, `composite_to_primitive_to_composite_propagation_settles_before_audio_advances`, `one_composite_is_not_delivered_twice_when_a_trigger_appears_in_a_later_same_sample_wave` |
| Parallel/coincident conflict precedence | `script_regular_natural_and_direct_conflicts_use_total_precedence`, `same_class_conflicts_use_lower_source_identity_not_install_order` |
| Multiple source wraps per callback | `a_source_that_wraps_multiple_times_advances_every_composite_boundary` |
| Stopped/recording/replacing/playing boundary modes | `an_explicit_script_stop_commits_at_its_source_boundary`, `timestamped_script_modes_commit_before_post_boundary_samples` |
| No redundant source-aligned sub-block | `iteration_aligned_composite_events_reuse_the_source_poi` |
| Direct source stop suppresses due natural delivery | `direct_source_stop_suppresses_the_coincident_natural_trigger` |
| Timestamp retention and late rejection | `timestamped_controls_keep_their_boundary_and_late_controls_are_rejected` |
| Activation-time generation and combined topology recheck | `session_rechecks_primitive_generations_before_timeline_installation`, `session_rejects_cycles_spanning_composite_and_primitive_sync_edges` |
| Queue/topology/event capacities and bounded diagnostic trace | `control_queue_and_dependency_wave_capacities_are_enforced_before_processing`, `event_overflow_latches_before_runtime_or_target_commit`, `trace_overflow_drops_diagnostics_without_affecting_the_runtime_transaction` |
| Sub-block fail-closed behavior | `sub_block_overflow_latches_and_never_processes_the_remainder_late` |
| No timeline-path allocation/deallocation | `composite_timeline_processing_does_not_allocate_or_free` |

The targeted commands are:

```sh
cargo test -p shoop_engine --test composite_timeline
cargo test -p shoop_engine --test composite_timing
cargo test -p shoop_engine --test no_alloc composite_timeline_processing_does_not_allocate_or_free
```

Full-suite and warning-denied results are recorded in `ARCHITECTURE.md` after the Stage 2 completion run.
