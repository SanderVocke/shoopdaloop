# Composite-loop deterministic semantic contract

## Status and terminology

This is the normative contract for the engine-backed implementation. Stage 0 decisions are now implemented by the Stage 1 compiled-plan and pure-state-machine layer described in [ARCHITECTURE.md](ARCHITECTURE.md). The state machine is not yet connected to the callback, so this does not claim that the current Qt/update-thread application implementation satisfies the contract. Current behavior and known differences are inventoried in [FEATURE_PARITY.md](FEATURE_PARITY.md).

A **sample boundary** `b` is the instant before sample `b` is processed. An **accepted command** is one the audio thread has removed from a bounded input queue during its defined acceptance phase, not merely one offered by the GUI. A **plan** is an immutable, validated composite schedule. A **target identity** is an engine slot plus generation. A **source ID** below means the stable engine identity ordered lexicographically by slot and generation.

The executable parts of this contract are in `shoop_engine::composite_semantics`; its unit tests are listed under [Contract-test map](#contract-test-map).

## Timeline and sample intervals

State is valid over half-open intervals. If a transition is at boundary `b`, samples in `[a, b)` use the old state and sample `b` and all samples in `[b, c)` use the settled new state. MIDI or control data timestamped exactly `b` belongs to the new interval. No audio sample at or after `b` may be processed until all events at `b` have settled.

Callback partitioning does not alter this rule. A callback `[c, c+n)` is subdivided at every required point of interest. A boundary at `c+n` is the next callback's boundary and does not process a sample in the current callback.

## Command acceptance and latency

At callback start, the audio thread performs one bounded command-queue drain. The cutoff is the queue state observed by that drain:

- An untimestamped command present before the cutoff is accepted at the callback-start boundary. If it requests synchronization, its first eligible sync boundary is the first one at or after acceptance; `delay = n` skips exactly `n` eligible boundaries and executes on the following boundary.
- A command offered after the cutoff is not accepted in that callback even if the producer races with audio processing. It remains pending for the next drain.
- A timestamped command present before the cutoff and targeting a sample in the current callback retains its in-buffer offset and is accepted for that exact boundary.
- A future timestamp remains queued/deferred. A timestamp already in the past when examined is rejected as late, reported through a fixed status/counter, and is never applied late.
- Commands accepted at one drain receive monotonically increasing acceptance sequence numbers in queue order. If several direct commands conflict at one boundary, the later acceptance sequence wins.

An unsynchronized command takes effect at its accepted boundary. A synchronized command is armed there and takes effect at the eligible boundary above. Frontend observation may lag either boundary and has no timing authority.

## Boundary algorithm and coincident events

For each boundary, processing uses this fixed conceptual order. The implementation may combine phases only if it produces the same result.

1. Accept commands eligible at this callback/in-buffer boundary in queue order.
2. Apply source stop/cancel decisions. A composite stopped in this phase is inactive before schedule delivery.
3. Seed natural events, including primitive-loop wraps and accepted timestamped controls.
4. Resolve composite propagation in deterministic dependency order. Each source receives a given primitive trigger at most once per boundary. Iteration-zero actions caused by nested starts are added to this same boundary.
5. Normalize and resolve target intents without mutating targets. Capacity is checked before commit.
6. Apply winning target stops.
7. Apply winning starts and mode changes, then atomically commit composite state and the transition trace before post-boundary audio.

Natural wrap state is therefore calculated before a composite target intent, but any winning composite or direct-control intent supersedes the natural result. The usual primitive mode transition rules still apply: setting a loop already playing to the same playing mode does not restart it; a stopped loop entering a running mode starts at position zero; recording-to-play transitions finalize the recording.

### End/start normalization inside one plan

For one target at one boundary in one compiled plan:

- end with no start becomes `Stop`;
- start with no end becomes `SetMode(new)`;
- end and start with the same effective mode become a continuation and emit no target action;
- end and start with different effective modes become `SetMode(new)`.

Thus contiguous repeated references do not glitch or restart, while an explicit script mode change is not accidentally erased. If non-normalized same-source actions remain, later compiled action ordinal wins.

Parallel duplicate or overlapping entries for one target are canonicalized before ordinals are assigned: start iteration, stable target identity, end iteration, and mode discriminant are the sort keys. The later canonical occurrence supplies desired state in an overlap. Input container/hash order therefore cannot select the winner.

### Conflicting sources targeting one loop

After same-plan normalization, incompatible intents use this total precedence, highest first:

1. accepted direct control;
2. explicit script-composite action;
3. inherited regular-composite action;
4. natural loop event.

For direct controls, later acceptance sequence wins. For composite intents of the same class, the lower stable source ID wins; within the same source, later action ordinal wins. Losing intents are included in a bounded conflict counter/trace. Hash traversal, object address, connection order, and frontend order never participate.

A direct stop targeting a child therefore beats a coincident composite start. A direct stop/cancel targeting the composite source also suppresses that source's due action. If a composite stops naturally because its script completes, its terminal child end/actions at that boundary execute first; natural completion is part of the plan rather than an external pre-delivery stop.

## Starts, stops, cancellation, and cycling

- Starting a non-empty composite sets iteration `0` and executes iteration-zero actions at the start sample. Starting a nested composite does the same at the parent's start sample, recursively, before post-boundary audio.
- Starting an empty (`N = 0`) plan is a successful no-op completion: it remains stopped, emits no child action, and reports no cycle.
- Stopping or cancelling clears the pending transition, stops every child owned as running by that composite unless a higher-priority coincident intent wins, resets displayed iteration/position to zero, and suppresses the source's otherwise due event.
- A regular composite has iterations `[0, N)`. At boundary `N`, terminal ends and iteration-zero starts settle at the same sample, iteration becomes `0`, and cycle count increments once.
- A script executes one pass. At boundary `N`, terminal child actions settle and the script becomes stopped. A parent may explicitly restart or continue it; otherwise there is no implicit script cycle.
- Cycle count is monotonic while the plan remains installed and resets on clear or replacement while stopped. Immediate seek does not increment it.

## Schedule duration and child modes

A playlist contains parallel timelines. Within one timeline, sections are sequential; entries in one section are parallel. An entry starts at the section origin plus its non-negative delay. A section's duration is the maximum of each entry's `delay + duration`. Empty sections have duration zero. The plan length is the maximum timeline end.

Entry duration is an explicit positive `n_cycles` when supplied. Otherwise it is `max(1, ceil(child_length / sync_length))`; an absent/zero sync length makes plan compilation invalid. Negative delays, zero explicit cycle counts, overflow, and unknown modes are validation errors rather than implicit coercions.

A regular composite has no explicit child modes and inherits its current mode. During playback, a valid zero-length child reserves its calculated duration but emits no playback action. During regular recording it is not ignored: it records for that duration. A script requires every entry to carry a valid explicit mode; mixed explicit/implicit plans are rejected rather than filled from traversal order. A zero-length child with an explicit recording mode records, while a zero-length child with a playing mode reserves time and emits no playback action.

For regular recording, only the first scheduled occurrence of a target records during one composite pass. Later occurrences emit a stop/idle action and reserve their scheduled duration. At pass end, all children are stopped, then the composite either starts regular playback at iteration zero when `play_after_record` is true or becomes stopped when false.

## Immediate synchronization and seek

An immediate-sync command carries an iteration `i` and takes effect at its accepted unsynchronized sample boundary. It is valid only when `0 <= i < N`; otherwise it is rejected with no state change.

The engine derives, without unbounded replay, the last effective action for every target at or before `i`, applies only targets active at `i`, stops targets made inactive by the seek, and derives each active child's cycle offset from its scheduled start. Playing offsets wrap by the child's duration; recording offsets do not wrap. The composite position is `i * sync_length + current_sync_position`. Iteration-zero actions execute when `i = 0`. Seeking while already running is one atomic boundary transaction. Recording seek still honors first-occurrence recording and `play_after_record` at the eventual pass end.

## Nesting and dependency cycles

Plans form a directed parent-to-child graph for composite targets. Compilation performs transitive cycle detection over the candidate topology, including self edges and edges spanning unchanged installed plans. Any cycle rejects the whole candidate transaction before RT activation and leaves the previous plan active.

Accepted DAGs use a deterministic parent-before-child topological order with stable ID as the tie-break. Same-sample propagation continues until settled, but each source/primitive trigger pair is delivered once and all queues/waves are bounded. A nested start executes the child's iteration-zero actions at the parent's start boundary. Regular-to-regular, regular-to-script, script-to-regular, and script-to-script combinations all follow these rules; the child plan's kind determines inherited versus explicit child behavior.

## Plan installation and replacement

Validation, allocation, reference resolution, sorting, cycle checking, capacity checking, and metadata derivation occur off the audio thread. Installation is atomic.

- **Stopped:** a valid plan accepted at a command boundary activates there. Runtime remains stopped at iteration/position zero.
- **Pending:** activation also occurs at the command boundary and preserves the pending mode/countdown. Any pending immediate-seek iteration must be valid for the candidate or the replacement is rejected.
- **Running:** the candidate becomes the pending replacement. The newest accepted valid candidate supersedes an older pending replacement. The old plan remains authoritative through its current pass; replacement activates at the next iteration-zero boundary, where children absent from the new plan are stopped and new iteration-zero actions are resolved in one transaction. Same-target/same-mode children continue without a stop/start pair. A one-shot script has no natural next iteration zero, so its candidate activates in stopped state when the script completes and starts only in response to a later control/parent action.
- If a running composite is stopped before that boundary, cancellation settles first and the pending replacement activates in stopped state at that stop boundary.

A rejected candidate never partly installs and never disturbs the active plan.

## Stale and missing targets

Compilation requires every referenced target to resolve to a stable slot plus generation. A missing target rejects the candidate plan. At activation, every identity is checked again; a mismatch rejects activation and leaves the old plan active.

If a target is deleted or its generation changes after activation, an action for that identity is reported as `stale_target` and skipped. It is never redirected to a newer object in the same slot. The composite timeline continues; only the stale action is omitted. Deletion/topology handling may separately install a prepared replacement plan. Running-child publication removes stale identities on the next snapshot.

## Capacity and overflow behavior

All capacities are explicit plan/engine configuration and are validated before installation.

| Site | Required behavior |
|---|---|
| Command queue full | Producer receives rejection; command was not accepted. |
| Plan-install queue full | Producer receives rejection; plan was not accepted and active plan is unchanged. |
| Plan actions/targets/depth exceed capacity | Candidate is rejected during off-thread validation. |
| Boundary event queue or propagation-wave capacity exceeded | Abort before target commit, enter a latched RT fault at that boundary, silence the callback remainder, freeze musical state, and publish a fixed fault record. No partial or late event is allowed. |
| Required sub-block count exceeded | Stop before processing past the unserviceable boundary, enter the same latched RT fault, and silence the callback remainder. |
| Snapshot queue full | Drop/overwrite stale observation according to snapshot design; authoritative processing continues and a dropped-snapshot counter advances. |

The RT fault remains latched until a non-RT recovery/reset command is accepted. Recovery does not replay missed events. This fail-closed behavior is intentionally audible but deterministic and preferable to a late musical transition.

## Observation

Snapshots are observational. They include composite mode, pending mode/countdown, iteration, cycle count, length, position, and a deterministically ordered active/running-child list plus conflict, stale-target, rejection, overflow, and RT-fault counters. Snapshot latency or loss cannot feed back into timing.

## Contract-test map

All tests are unit tests under `src/rust/shoop_engine/src/composite_semantics.rs` and run with `cargo test -p shoop_engine composite_semantics --features app_backend`.

| Decision | Test evidence |
|---|---|
| Half-open intervals | `sample_intervals_are_half_open_at_a_boundary` |
| Boundary order, natural events, stop/start order | `boundary_phase_order_is_fixed` |
| Coincident end/start and mode changes | `coincident_end_and_start_continue_or_change_mode` |
| Incompatible multi-source modes | `incompatible_intents_have_total_precedence` |
| Stop suppresses due event | `stop_before_delivery_suppresses_a_due_action` |
| Nested iteration zero at parent sample | `nested_iteration_zero_occurs_at_the_parent_start_sample` |
| Stable DAG order | `dependency_order_is_stable_and_parent_before_child` |
| Direct/transitive cycle rejection | `direct_and_transitive_dependency_cycles_are_rejected` |
| Invalid dependency identity | `invalid_dependency_identity_is_rejected` |
| Stopped/pending/running plan activation and empty plans | `plan_activation_depends_only_on_runtime_status` |
| Countdown boundary meaning | `countdown_delay_counts_boundaries_to_skip` |
| Explicit/default duration and sync-length validation | `schedule_duration_is_explicit_or_length_derived` |
| Regular/script mode classification | `regular_and_script_modes_are_all_or_nothing` |
| Empty-child, first-recording, pass-end, and cycling rules | `empty_child_and_recording_pass_rules_are_explicit` |
| Immediate-seek bounds and child offsets | `immediate_seek_is_bounded_and_derives_cycle_offsets` |
| Missing/stale generation behavior | `stale_or_missing_targets_are_never_retargeted` |
| Queue/plan/event/wave/sub-block/snapshot overflow classes | `overflow_never_turns_into_a_late_event` |
| Callback acceptance cutoff | `callback_cutoff_defers_commands_that_missed_the_drain` |
| Timestamp retention and late rejection | `timestamped_commands_keep_exact_in_buffer_timing` |

Stage 1 full-effect state-machine tests and their requirement mapping are recorded in [ARCHITECTURE.md](ARCHITECTURE.md#stage-1-verification-map). Callback timing and application integration still require later integration tests; the decision helpers alone are not sufficient evidence for those stages.
