# Composite-loop real-time safety and capacity budget

## Status and scope

This budget covers the engine-authoritative composite timeline, its integration into `Session::process_loop_group`, compiled plans, the Stage 3 command-install/snapshot slice, and the Stage 4 application adapter. It is not the complete callback-path audit required in Stage 3 and Stage 6: driver I/O, FX, every application command, and remaining session allocation exceptions still require audit and removal.

The application frontend translates and submits composite plans off RT through `BackendSession`; engine timing, bounded ownership transfer, same-topology running-plan replacement, command controls, and observational publication are active once a composite QObject receives its engine handle. The adapter's registry mutex, topology mirror, descriptor compilation, QObject resolution, lifecycle dependency scan, and grab bookkeeping remain strictly on the control/update thread. Its update-thread wrap handler, recursive dependency notification, and fallback composite state machine have been removed. The focused composite QML suite passes 24/24, including frontend/file-I/O stalls, and nested save/load/session replacement passes separately. Running dependency-topology replacement, full callback-path proof, and live-backend/manual verification remain open.

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
| Primitive loop ringbuffer adoptions per transaction | 64 | Fixed request limit plus channel chunk reserves allocated at channel creation | Reject the complete transaction before mutation when request count, duplicate targets, or destination capacity is invalid |

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
| Composite primitive-child grab | Up to 64 loop adoptions in one command | Frontend builds the request vector off RT. The callback validates every target and destination reserve, visits fixed rolling-buffer chunks directly, then commits all primitive child data/mode changes in one command. The request vector returns with its command for non-RT destruction. |

The application command capacity is intentionally larger than the initial 128 estimate because one queue carries all backend mutations, not only composite inputs. Work per callback is still finite and queue-full rejection is explicit. Runtime-preserving replacement uses one pending plan per node and preallocated retired-plan storage; dependency-topology-changing running candidates reject before acceptance.

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

`composite_timeline_processing_does_not_allocate_or_free` queues prepared timelines and controls outside `assert_no_alloc`, then runs callback installation, displaced ownership transfer, control acceptance, repeated source wraps, composite commits, snapshot publication, deterministic stop cleanup, and final empty-timeline lifecycle removal inside the guard. `transactional_audio_ringbuffer_adoption_does_not_allocate_or_partially_apply` additionally covers multi-child rolling-buffer copying, state commit, and rejected duplicate-target failure under the allocator guard. Channel chunk reserves are prepared at channel creation; the callback never snapshots or flattens a rolling buffer. Displaced lifecycle ownership is inspected and destroyed only after leaving the guard. Command execution has no exceptional allocation allowance. Existing `Session` code still contains separately annotated allocation exceptions and lock-bearing application/driver paths; removing and auditing them is Stage 3 and Stage 6 work, not something this document masks.

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

## Active callback-path audit (open findings)

The following source audit is deliberately a blocker list, not a safety claim. A green composite allocator test covers the new path but cannot hide callback branches it does not exercise.

| Callback area | Current ownership/storage | Open RT violation or remaining proof |
|---|---|---|
| `Engine` command drain | Fixed-cutoff `rtrb`; equal return ring | Composite commands are guarded. Every generic application command still needs classification because topology description, object creation, and several mutations allocate when their closure executes. |
| Schedule installation | Prepared schedule swaps vectors and returns displaced storage | The expensive build is off-thread, but `describe_topology` is currently a queued callback command and allocates its topology description on RT. |
| Session graph dispatch | RT-owned schedule and scratch | High-level process phases remain wrapped in `realtime_allow_alloc_once!`; those allowances must be removed after the branches below are prepared. |
| Internal/dummy/external audio ports | Mutable RT-owned vectors | Buffer/staging growth is still callback-reachable and explicitly permitted. Supported callback sizes must be pre-sized; size mismatch needs a fixed fault instead of resize. External output capture can extend/drain a `Vec` on RT. |
| Session test FX routing | Mutable maps plus name-based discovery | Per-callback `String`, `Vec`, `format!`, and name searches allocate. Routes and scratch must be compiled into the prepared graph. |
| Carla/LV2 processing | `Arc<Mutex<CarlaLv2Host>>` and dynamic MIDI/audio staging | Session processing locks the host and constructs vectors/strings. Host ownership and prepared port/event buffers must move into RT-owned state. |
| JACK registered ports | `Arc<Mutex<Vec<JackRegisteredPort>>>` | Callback locks the registry. Decoupled MIDI endpoints lock queues and clone MIDI bytes into new vectors. A bounded ownership/registration command path is required. |
| CPAL capture/playback bridge | Several `Arc<Mutex<...>>` rings and endpoint vectors | Input/output callbacks lock audio rings, external connection state, and MIDI endpoint collections. MIDI conversion allocates byte vectors. Bounded SPSC audio/MIDI transport is required. |
| Dummy backend bridge | Engine is thread-owned | Mock external-connection and test capture structures still use mutexes outside or adjacent to processing; callback reachability and replacement with bounded queues remain to be proven. |
| MIDI session routing | Pre-sized per-channel scratch on the normal path | FX and decoupled-driver branches still clone/allocate messages. Maximum event capacities and explicit overflow need one common policy. |
| Snapshot publication | Three reusable boxes; fixed composite child arrays | Allocation-free in tests. Filled-ring push is capacity-invariant but should gain a structural assertion or non-destructive fallback rather than relying only on accounting. |
| Error and teardown | Composite errors are fixed records/counters | Driver/FX poison recovery and teardown can format, lock, or destroy owned allocations on callback threads; return-to-owner teardown must be audited per backend. |

The source search surface includes callback entry points in `app_backend`, all `Session::process` callees, port/channel/MIDI modules, hosted FX, command application, snapshot publication, and driver teardown. Closing this table requires removing each open mechanism and adding exercised allocation/lock evidence; merely deleting an allowance macro is insufficient.

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
