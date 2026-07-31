# Composite-loop real-time safety and capacity budget

## Status and scope

This budget covers the engine-authoritative composite timeline, its integration into `Session::process_loop_group`, compiled plans, Stage 3 command/install/reclamation/snapshot/lifecycle paths, grab, and the Stage 4 application adapter. The active callback audit below is complete for the plan's composite RT-safety scope; unrelated legacy driver, hosted-FX, generic graph, and teardown mechanisms are explicitly classified rather than represented as globally remediated.

The application frontend translates and submits composite plans off RT through `BackendSession`; engine timing, bounded ownership transfer, same-topology running-plan replacement, command controls, and observational publication are active once a composite QObject receives its engine handle. The adapter's registry mutex, topology mirror, descriptor compilation, QObject resolution, lifecycle dependency scan, and grab bookkeeping remain strictly on the control/update thread. Its update-thread wrap handler, recursive dependency notification, and fallback composite state machine have been removed. The focused composite QML suite passes 26/26, including frontend/file-I/O stalls and nested transactional grab, and nested save/load/session replacement passes separately. Full callback-path proof and live-backend/manual verification remain open.

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
| Rolling transition history | 16,384 newest entries | Preallocated `VecDeque`, overwrite oldest before push | Observation loses oldest diagnostics only; authoritative processing continues |
| Runtime output per composite transition | 128 | Fixed `CompositeTransitionBatch` | Latch runtime fault; no target commit |
| Session sub-blocks per callback | 16 | Existing `MAX_SUB_BLOCKS` | Latch `SubBlockCapacity`, stop at the unserviceable boundary, process no later samples |
| Primitive loop ringbuffer adoptions per transaction | 64 loops / 256 audio channels | Fixed shape query; complete replacement chunk storage allocated on the control side | Reject the complete transaction before mutation when request count, duplicate targets, channel count, shape, or destination capacity is invalid |

The intent-construction check reserves for every installed composite's maximum 128 outputs, all accepted controls, and all primitive natural intents. Construction fails if configured intent storage cannot hold that one-boundary maximum. Scratch vectors are allocated and reserved when the timeline or graph schedule is built; processing only clears, indexes, sorts within capacity, and swaps fixed runtime state.

## Implemented cross-thread queue budgets

| Resource | Bound | Storage and overflow behavior |
|---|---:|---|
| Application callback command queue | 4,096 commands | Bounded `rtrb`; producer receives `Full`. The callback snapshots readable slots and drains only that fixed cutoff. |
| Executed-command reclamation | 4,096 commands | Equal-sized return ring; command boxes and captures are destroyed by `EngineHandle::reclaim`. |
| Accepted timeline controls | 128 controls | Fixed timeline staging; the accepted command receives `QueueFull` if full. |
| Composite timeline installation | Shares command queue; one result slot per request | Timeline and plan allocations are built before submission. Exact prepared primitive topology, generations, and monotonic version are rechecked at installation. Displaced, rejected, or superseded ownership returns through the result slot. |
| Deferred plan retirement | One pending slot per node; retired vector sized to `max_composites` | Activation commits before the old plan moves to retired storage. Control-provided empty storage is swapped through a reclamation command for non-RT destruction; incompatible/full replacement rejects. |
| Snapshot publication | 3 reusable boxes | Callback fills within existing vector/fixed-child/trace capacity. If all boxes are in use, it increments `snapshots_dropped` and skips publication. The control side grows short trace vectors. |
| Composite grab | Up to 64 primitive loops / 256 audio channels in one command | Frontend recursively flattens nested schedules, queries a fixed shape, and allocates replacement chunks off RT. The callback revalidates every target/capacity, copies fixed rolling-buffer chunks into prepared storage, then swaps and commits all child data/mode changes. Requests and displaced storage return for non-RT destruction. |

The application command capacity is intentionally larger than the initial 128 estimate because one queue carries all backend mutations, not only composite inputs. Work per callback is still finite and queue-full rejection is explicit. Runtime-preserving same-topology replacement uses one pending plan per node and preallocated retired-plan storage. A changed-topology candidate transfers fixed controls/diagnostics into its preallocated timeline, stops old primitive children, and stages at most one iteration-zero restart per retained source. It rejects before mutation if existing controls plus restarts exceed the fixed control capacity.

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

`composite_timeline_processing_does_not_allocate_or_free` queues prepared timelines and controls outside `assert_no_alloc`, then runs callback installation, displaced ownership transfer, control acceptance, repeated source wraps, composite commits, snapshot publication, deterministic stop cleanup, and final empty-timeline lifecycle removal inside the guard. `transactional_audio_ringbuffer_adoption_does_not_allocate_or_partially_apply` additionally covers multi-child rolling-buffer copying, state commit, and rejected duplicate-target failure under the allocator guard. The allocator guard covers both reserve-reuse and off-thread-prepared storage-swap paths; the callback never snapshots, flattens, allocates, or destroys a rolling buffer. Displaced lifecycle ownership is inspected and destroyed only after leaving the guard. Command execution has no exceptional allocation allowance. High-level Session process allowances were removed so composite loop/audio/MIDI dispatch is guarded directly. Localized legacy capacity and optional driver/FX mechanisms that are not used to implement composite timing, control, publication, or lifecycle are classified below rather than masked.

## Composite callback cost measurement

Command:

```sh
cargo run --release -p shoop_engine --example composite_callback_bench
```

Measured on the Linux x86_64 PREEMPT_RT host recorded in [TEST_RESULTS.md](TEST_RESULTS.md), using 64-frame callback spans:

| Case | Schedule exercised | Iterations | Mean callback cost |
|---|---|---:|---:|
| Ordinary | 1 script composite × 4 primitive targets | 20,000 | 1.516 µs |
| Maximum configured timeline | 64 script composites × 64 primitive targets, direct starts and terminal cleanup each callback | 500 | 832.104 µs |

At 48 kHz, a 64-frame callback budget is 1.333 ms, so this resolver-heavy maximum consumed about 62.4% of one callback period on this host before representative audio/MIDI/FX/driver work. It is a capacity stress measurement, not evidence that the complete application callback meets its deadline. The current 64×64 policy therefore needs whole-callback benchmark evidence before final support claims; reducing exposed capacities is preferable if the integrated callback cannot retain margin.

The benchmark builds all plans/storage before timing and repeatedly exercises accepted direct starts, 4,096 scheduled target intents, conflict collapse, script terminal cleanup, trace/history, and session POI processing. Exact output is linked in [TEST_RESULTS.md](TEST_RESULTS.md#composite-callback-benchmark).

## Active callback-path audit and scope classification

This audit distinguishes the delivered composite callback surface from unrelated legacy facilities. The strict gate covers compiled plans/runtimes, timeline installation and restart, command acceptance/reclamation, boundary propagation, primitive loop actions, snapshots, lifecycle, grab, and the ordinary dummy audio/MIDI processing exercised with those actions. Optional driver, hosted-plugin, test-FX, and generic primitive-topology mechanisms are background findings unless a composite-specific path invokes them to implement timing, control, publication, lifecycle, or failure handling.

| Callback area | Classification | Evidence or remaining limitation |
|---|---|---|
| `Engine` command drain and reclamation | In scope; verified | Fixed-cutoff `rtrb` plus equal return ring. Composite install/control/grab/lifecycle commands execute under allocation guards; command captures and displaced ownership return for non-RT destruction. |
| Composite plan/timeline installation | In scope; verified | Plans and topology are compiled off RT. Installation, same-topology deferral, changed-topology restart, rejection, removal, and ownership return are move/swap/fixed-control operations covered by `composite_timeline_processing_does_not_allocate_or_free`. |
| Composite runtime/boundary state | In scope; verified | RT-owned fixed arrays and capacity-reserved vectors. Structural assertions reject `Mutex`, `RwLock`, or `.lock(` in plan/runtime/timeline sources; dense success and fail-closed overflow run under the allocator guard. |
| Ordinary Session loop/audio/MIDI dispatch reached by composite target actions | In scope; verified on supported automated scenarios | `playing_audio_does_not_allocate`, `recording_audio_does_not_allocate`, `a_full_audio_chain_does_not_allocate`, `midi_routing_does_not_allocate`, queued-input tests, and multi-loop/channel tests cover the direct processing reached after composite mode commits. Existing one-time diagnostic wrappers do not disable `assert_no_alloc`, so these runs still fail on a real allocation/deallocation. |
| Composite grab | In scope; verified | At most 64 recursively flattened primitive loops and 256 audio channels; fixed shape/capacity validates before mutation. Fixed rolling buffers copy into off-thread-prepared chunks that swap atomically. Success, rejection, reserve reuse, and storage swap are allocator-guarded. |
| Composite snapshots/errors/teardown | In scope; verified | Three reusable snapshot boxes, fixed fault/counter records, no callback formatting/logging, idempotent control-side removal, and allocator-guarded stop/removal/displaced return. |
| Generic primitive graph description/build | Background legacy mechanism | `describe_topology` allocates when a generic graph rebuild is requested; composite registry configuration does not use it. Prepared graph installation is move-only. Removing generic graph-description allocation is not a composite prototype gate under the stated scope boundary. |
| External/JACK/CPAL registration and transport | Background legacy mechanism; manual backend validation pending | These pre-existing bridges contain lock-bearing registration/ring/endpoint code. Composite timing/control does not call those locks and remains Session-owned, but live backend/xrun behavior is assigned to `MANUAL_VALIDATION.md`. No claim is made that every unrelated driver callback is globally lock-free. |
| Test FX and optional Carla/LV2 hosting | Background legacy mechanism | Name-based test routing and hosted-plugin code contain dynamic storage and locks. The composite implementation neither uses them as timing authority nor changes them; plugin-wide RT remediation is outside this prototype. Dry/wet composite modes themselves are ordinary fixed loop/channel mode actions and are covered independently. |
| Generic driver/FX teardown and poison reporting | Background legacy mechanism | May format, lock, or destroy allocations on driver-owned threads. Composite plans, commands, snapshots, and lifecycle ownership use their dedicated non-RT return path and do not depend on those mechanisms. |

Within the plan's RT-safety scope boundary, every composite-related callback mechanism is classified and has source, allocator, bounded-capacity, or ownership evidence. The background rows remain architectural debt and manual integration risk, not hidden evidence of a green whole-engine RT audit.

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
| No timeline/lifecycle/grab-path allocation or deallocation | `composite_timeline_processing_does_not_allocate_or_free` covers installation, replacement, stop cleanup, empty-timeline removal, and displaced ownership return; `transactional_audio_ringbuffer_adoption_does_not_allocate_or_partially_apply` covers grab commit and rejection |
| Running dependency-topology restart and cleanup | `running_dependency_topology_change_restarts_at_the_install_boundary`, `running_dependency_addition_restarts_retained_sources_and_nested_children`; allocator guard covers running removal |
| Fixed command acceptance cutoff | `callback_drain_has_a_fixed_cutoff` |
| Prepared topology/version race rejection | `prepared_timeline_is_rejected_if_primitive_topology_changed_before_acceptance`, `older_prepared_version_is_rejected_even_if_compilers_finish_out_of_order` |
| No rejected-candidate destruction on RT | `composite_timeline_processing_does_not_allocate_or_free` exercises duplicate-version rejection inside `assert_no_alloc` |
| Bounded snapshot publication and explicit stale drop | `prepared_timeline_and_control_cross_at_callback_boundaries_and_publish_state`, `stale_snapshot_publication_is_dropped_without_stalling_processing` |
| Rolling trace survives observation stalls | `transition_history_survives_frontend_polling_stall` |

The targeted commands are:

```sh
cargo test -p shoop_engine --test composite_timeline
cargo test -p shoop_engine --test composite_timing
cargo test -p shoop_engine --test no_alloc composite_timeline_processing_does_not_allocate_or_free
```

Full-suite and warning-denied results are recorded in `ARCHITECTURE.md` after the Stage 2 completion run.
