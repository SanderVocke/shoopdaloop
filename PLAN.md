# Composite Loops in the Real-Time Engine: Prototype Plan

## Purpose

Integrate composite loops into the engine's real-time processing domain and connect that implementation through the frontend to the QML application. Composite-loop timing and interactions must no longer depend on Qt signal delivery, frontend polling, or update-thread scheduling.

This is a staged prototype plan. Each stage should leave the relevant code buildable and testable, and should establish an explicit verification surface before the next stage begins.

## Plan maintenance

The implementing agent is allowed and expected to update this plan as implementation knowledge improves. It may:

- Add, remove, split, merge, or reorder adaptive tasks and stages.
- Record newly discovered constraints, risks, and decisions.
- Replace a proposed mechanism with a better one.
- Check off completed work and add links or notes about verification.
- Revise test commands to match the test harness available at that point.

The implementing agent must not weaken, remove, reinterpret, or mark an immutable requirement as optional. If an immutable requirement proves incompatible with the codebase or another immutable requirement, stop and present the evidence and required decision instead of silently narrowing scope.

## Canonical investigation and handoff artifacts

Store prototype working documents under `docs/composite_rt/`. These filenames are canonical: later stages should update and link to these files rather than creating unnamed notes or duplicating their content elsewhere.

| Artifact | Canonical filename | Produced initially | Required contents |
|---|---|---|---|
| Current behavior and feature-parity matrix | [`docs/composite_rt/FEATURE_PARITY.md`](docs/composite_rt/FEATURE_PARITY.md) | Stage 0 | Current behaviors, code/test evidence, migration status, and the engine/QML/manual verification assigned to every feature. |
| Deterministic semantic contract | [`docs/composite_rt/SEMANTICS.md`](docs/composite_rt/SEMANTICS.md) | Stage 0 | Sample-boundary semantics, event ordering, conflicts, nesting, cycles, command acceptance, plan activation, stale targets, and overflow behavior. |
| Pre-change automated baseline | [`docs/composite_rt/BASELINE.md`](docs/composite_rt/BASELINE.md) | Stage 0 | Exact commands, environment, passing/failing tests, known pre-existing failures, and relevant timing observations. |
| Implemented processing architecture | [`docs/composite_rt/ARCHITECTURE.md`](docs/composite_rt/ARCHITECTURE.md) | Stage 1 | Prepared-plan format, identities, POI/event processing, termination argument, command/plan ownership, snapshots, frontend integration, and reclamation. |
| RT-safety audit and capacity budget | [`docs/composite_rt/RT_SAFETY.md`](docs/composite_rt/RT_SAFETY.md) | Stage 2, completed in Stage 6 | Callback-path audit, allocation/lock evidence, capacities, overflow responses, event/sub-block bounds, and callback cost measurements. |
| Automated verification record | [`docs/composite_rt/TEST_RESULTS.md`](docs/composite_rt/TEST_RESULTS.md) | Stage 6 | Commands and results for engine/QML tests, environment blockers, repeated timing tests, and final automated gate status. |
| User manual-validation package | [`docs/composite_rt/MANUAL_VALIDATION.md`](docs/composite_rt/MANUAL_VALIDATION.md) | Stage 6, executed in Stage 7 | Setup, scenarios, expected outcomes, diagnostics to capture, and fields for user results. |

When an artifact does not yet exist, the first checklist item that produces it must create it. The adaptive decision log remains in this `PLAN.md`; decisions that define runtime behavior or architecture must also be reflected in `SEMANTICS.md` or `ARCHITECTURE.md` respectively.

## Immutable end requirements

- [ ] **Feature parity:** Engine-backed composite loops provide at least the complete user-visible feature set of the current composite-loop implementation.
- [ ] **RT authority:** Once a composite configuration has been accepted by the engine, every timing decision that processes that configuration—including iteration advancement, starts, stops, mode changes, recording behavior, cycling, and nested-composite propagation—is made on the real-time audio thread.
- [ ] **Determinism:** Given the same accepted configuration, initial engine state, timestamped/accepted control inputs, and audio timeline, composite processing produces the same transitions at the same sample positions regardless of GUI load, update-thread timing, hash iteration order, or audio-buffer partitioning.
- [ ] **Sample-correct interaction:** Composite-to-basic-loop and composite-to-composite actions take effect at their defined sample boundary before post-boundary audio is processed.
- [ ] **No RT allocation:** The audio thread performs no allocation, reallocation, or deallocation. This includes exceptional, topology-change, command, publication, and teardown paths reached from the callback.
- [ ] **No RT mutexes:** The audio thread does not acquire a mutex or any other potentially blocking lock. RT-owned state, bounded lock-free queues, immutable prepared data, or equivalent non-blocking mechanisms must be used instead.
- [ ] **Bounded RT work:** Same-sample event propagation, nesting, command application, and POI subdivision have explicit capacities or otherwise defensible finite worst-case bounds. Capacity failure is reported and never silently converted into a late musical event.
- [ ] **Good automated coverage:** Engine tests cover state-machine semantics, exact timing, nesting, conflicting/coincident events, buffer-partition independence, configuration changes, and RT constraints. QML tests cover end-to-end application integration and current composite-loop behavior.
- [ ] **Top-level integration:** The QML application creates, configures, controls, observes, saves, and loads engine-backed composite loops without using the old Qt/update-thread implementation as the timing authority.
- [ ] **Manual-validation handoff:** Manual live-performance scenarios are documented for the user. The implementing agent may report them as pending user validation but may not claim they were manually verified.

### Control-input latency boundary

The determinism requirement begins when a command or prepared configuration is accepted into the RT domain. GUI scheduling can delay when that acceptance happens and can therefore change which future quantization boundary is eligible. That unavoidable UI-to-engine latency is not permission for configured composite processing itself to occur outside the audio thread or to become timing-dependent.

The implementation must document a precise acceptance rule in [`docs/composite_rt/SEMANTICS.md`](docs/composite_rt/SEMANTICS.md), such as "the first eligible sync boundary after the audio thread accepts the command at a callback boundary." Latency-critical inputs with audio-clock timestamps or in-buffer sample offsets should retain that timing information rather than being reduced to Qt delivery time.

## Target processing model

This is the initial design direction, not an immutable implementation prescription:

- A composite loop is an RT-owned temporal state machine and does not produce audio channels itself.
- Frontend playlist data is validated and compiled off the audio thread into an immutable, allocation-complete RT plan using stable engine IDs.
- A prepared plan crosses into the RT domain through a bounded non-blocking command mechanism and is activated at a documented boundary.
- Basic loops and composite loops share one sample timeline. The earliest POI bounds a sub-block.
- At a boundary, a bounded deterministic event resolver processes primitive loop triggers, composite state changes, transition intents, nested composites, and resulting trigger propagation before post-boundary samples are processed.
- State is published to the frontend through non-blocking snapshots. Published state is observational and is never the source of composite timing decisions.
- Replaced plans, commands, snapshots, and other owned allocations are returned to a non-RT thread for destruction.

## Stage 0 — Baseline, behavior inventory, and semantic contract

### Current behavior inventory

- [x] Create [`docs/composite_rt/FEATURE_PARITY.md`](docs/composite_rt/FEATURE_PARITY.md) and enumerate all composite-loop behavior represented by QML, frontend Rust, session persistence, documentation, and tests.
- [x] Build the parity matrix in `FEATURE_PARITY.md`, covering at least:
  - [x] Sequential playlist elements.
  - [x] Parallel playlist timelines.
  - [x] Delays and repeated references to the same child.
  - [x] Explicit `n_cycles` overrides and length-derived durations.
  - [x] Empty or ignored children.
  - [x] Regular composites with inherited modes.
  - [x] Script composites with explicit modes and one-shot completion.
  - [x] Composite playback, stop, cancellation, and cycling.
  - [x] Composite recording, record-only-first-occurrence behavior, and play-after-record on/off.
  - [x] Delayed transitions/countdowns.
  - [x] Immediate synchronization/seeking to an arbitrary composite iteration.
  - [x] Composite-to-composite nesting in both regular and script combinations.
  - [x] Running-child reporting and displayed mode, length, iteration, position, and cycle count.
  - [x] Composite ringbuffer grab behavior, including synced/unsynced and fixed-length cases.
  - [x] Schedule changes caused by child creation, deletion, conversion, or length changes.
  - [x] Circular-reference rejection.
  - [x] Session save/load compatibility.
- [x] Create [`docs/composite_rt/BASELINE.md`](docs/composite_rt/BASELINE.md), then run and record the existing relevant engine and QML test baseline before replacing behavior.
- [x] Record in `FEATURE_PARITY.md` behavior that currently depends on accidental Qt, signal-connection, or hash traversal order.

### Define deterministic semantics

- [x] Create [`docs/composite_rt/SEMANTICS.md`](docs/composite_rt/SEMANTICS.md) as the semantic contract.
- [x] Specify there the exact half-open sample interval semantics around a transition boundary.
- [x] Specify the order or conflict policy for natural wraps, stops, starts, and mode changes at the same sample.
- [x] Specify what happens when multiple active composites target the same loop with incompatible modes at the same sample.
- [x] Specify whether a composite stopped at a boundary executes an event that would otherwise occur at that boundary.
- [x] Specify nested start behavior, including whether iteration-zero child actions happen at the parent's start sample.
- [x] Specify cycle handling. At minimum, composite dependency cycles must be detected transitively and rejected before RT activation.
- [x] Specify plan activation while stopped, pending, and running.
- [x] Specify behavior when a referenced target no longer exists or its generation no longer matches.
- [x] Specify bounded-overflow behavior for commands, plans, event queues, and sub-block limits.
- [x] Add engine-level tests that pin each decision in `SEMANTICS.md` before relying on it in integration code.

### Stage 0 exit gate

- [x] Every current feature has an entry in `FEATURE_PARITY.md`.
- [x] Every coincident-event case needed by the prototype has deterministic semantics in `SEMANTICS.md`.
- [x] Existing test status and known pre-existing failures are recorded in `BASELINE.md`.

## Stage 1 — Engine composite data model and pure state machine

### RT-safe identity and compiled plan

- [x] Create [`docs/composite_rt/ARCHITECTURE.md`](docs/composite_rt/ARCHITECTURE.md) and keep its implemented design current throughout the prototype.
- [x] Introduce stable engine-side identities with generation checking for basic and composite loop targets.
- [x] Define an immutable compiled composite plan with sorted iteration/action storage.
- [x] Ensure the plan contains no `QObject`, `QVariant`, weak Qt pointer, runtime string lookup, or hash-order dependency.
- [x] Resolve playlist references and compute schedule metadata before RT installation.
- [x] Precompute or bound metadata needed for recording-first-occurrence, active-child tracking, immediate sync, and cancellation.
- [x] Validate all indices, modes, lengths, dependency edges, and capacities before installation.
- [x] Detect transitive composite cycles during compilation.

### Pure state machine

- [x] Implement composite mode, pending transition/countdown, iteration, cycle count, position derivation, and active-child state.
- [x] Implement regular-loop mode inheritance.
- [x] Implement script explicit-mode behavior and completion.
- [x] Implement stop/cancel behavior and child cleanup.
- [x] Implement recording and play-after-record behavior.
- [x] Implement immediate-sync/seek state calculation without RT allocation or unbounded replay.
- [x] Make transition output deterministic and ordered.
- [x] Represent failures as explicit status/counters suitable for publication without RT logging or formatting.

### Tests

- [x] Unit-test the state machine independently of audio channels and Qt.
- [x] Cover every applicable row of [`docs/composite_rt/FEATURE_PARITY.md`](docs/composite_rt/FEATURE_PARITY.md) that does not require audio routing.
- [x] Test invalid plans, stale identities, cycles, capacity limits, and conflicting events.
- [x] Test long-running iteration/cycle behavior and integer-boundary cases.

### Stage 1 exit gate

- [x] The pure engine state machine matches [`docs/composite_rt/SEMANTICS.md`](docs/composite_rt/SEMANTICS.md).
- [x] It can execute from fully prepared storage without allocating, locking, string lookup, or nondeterministic iteration.

## Stage 2 — POI integration and deterministic boundary-event resolution

### Timeline integration

- [x] Create the initial [`docs/composite_rt/RT_SAFETY.md`](docs/composite_rt/RT_SAFETY.md) capacity budget for POIs, sub-blocks, actions, event waves, commands, plans, and snapshots.
- [x] Add composites to the engine's authoritative sample timeline.
- [x] Reuse a sync source's POI for iteration-aligned events where possible so composites do not introduce redundant sub-blocks.
- [x] Define a composite POI only where an event is not already guaranteed by a source POI.
- [x] Ensure every relevant node advances to the same sample before boundary events are resolved.
- [x] Ensure transitions are committed before processing any post-boundary audio sample.

### Boundary-event resolver

- [x] Add a preallocated, bounded event/intention mechanism for same-sample propagation.
- [x] Seed it from primitive events such as basic-loop wraps and accepted timestamped controls.
- [x] Deliver sync triggers to composites in a deterministic order.
- [x] Gather and resolve composite transition intents using the Stage 0 conflict policy.
- [x] Apply actions to basic loops and composite loops at the same boundary.
- [x] Propagate newly caused triggers through nested composites until the boundary is settled.
- [x] Guarantee termination through DAG ordering, once-per-boundary delivery, bounded waves, or another proof documented in [`docs/composite_rt/ARCHITECTURE.md`](docs/composite_rt/ARCHITECTURE.md).
- [x] Remove dependence on the current one-pass snapshot behavior for transitive propagation.
- [x] Report queue/wave/sub-block overflow explicitly without processing the event late.

### Timing tests

- [x] Assert exact output samples when a composite event falls in the middle of an audio callback.
- [x] Run equivalent timelines with different callback sizes and arbitrary buffer partitions; compare transition traces and audio output.
- [x] Test several nested composite levels at one sample boundary.
- [x] Test parallel and coincident actions.
- [x] Test source loops that wrap multiple times in one callback.
- [x] Test stopped, recording, replacing, and playing child modes at boundaries.
- [x] Test that grid-aligned composite events do not add unnecessary sub-blocks.

### Stage 2 exit gate

- [x] Engine-only composite processing is sample-correct and buffer-partition independent.
- [x] Nested propagation is deterministic and bounded.

## Stage 3 — Non-blocking control boundary and RT ownership

### Engine ownership and commands

- [x] Make the active audio callback the sole mutable owner of session and composite runtime state.
- [x] Route control changes through bounded non-blocking command queues.
- [x] Define callback and in-buffer command acceptance cutoffs in [`docs/composite_rt/SEMANTICS.md`](docs/composite_rt/SEMANTICS.md).
- [x] Build and allocate commands and plans off the RT thread.
- [x] Return executed commands and displaced plans to a non-RT reclamation queue; never drop them on the audio thread.
- [x] Version plans and topology so stale prepared configurations are rejected or handled deterministically.
- [x] Make plan replacement atomic from the processing model's perspective.
- [x] Decide and test whether running-plan changes activate at callback boundaries, sync boundaries, or another explicit point.

### Remove RT locks and allocation

- [ ] Audit the complete active callback path in [`docs/composite_rt/RT_SAFETY.md`](docs/composite_rt/RT_SAFETY.md), including driver I/O, session processing, graph installation, ports, channels, MIDI, FX, commands, snapshots, error paths, and teardown.
- [x] Remove the session mutex from the callback/control interaction.
- [ ] Remove callback-side mutexes protecting registered ports, capture/playback rings, MIDI queues, or hosted processing state.
- [ ] Replace mutable callback-visible collections with RT-owned or immutably swapped prepared collections.
- [ ] Pre-size all callback scratch and event storage for declared capacities and supported buffer sizes.
- [ ] Remove callback-reachable exceptional allocation allowances rather than treating them as successful RT behavior.
- [ ] Ensure topology or capacity changes are prepared off-thread and installed without allocation or destruction on RT.
- [ ] Ensure error reporting uses atomics, fixed records, or snapshots rather than RT formatting/logging.
- [ ] Exercise allocation guards over command application, composite events, graph changes, and normal processing.

### State publication

- [x] Extend non-blocking engine snapshots with composite mode, next mode/countdown, iteration, cycle count, length, position, and active/running child information required by the UI.
- [x] Ensure snapshot publication is bounded and may drop stale observations rather than blocking audio.
- [x] Grow or replace snapshot storage only on a non-RT thread.
- [x] Make clear that frontend observations may lag while authoritative processing continues.

### Stage 3 exit gate

- [ ] No active audio callback path allocates, deallocates, or locks.
- [x] Commands, plans, and snapshots cross the thread boundary without blocking RT.
- [x] Command acceptance and plan activation semantics are tested and documented in `SEMANTICS.md`, with their mechanism documented in `ARCHITECTURE.md`.

## Stage 4 — Frontend and QML integration

### Engine-facing frontend API

- [x] Expose creation, deletion/tombstoning, configuration, transition, immediate sync, clear, and state observation for engine composite loops.
- [x] Translate frontend/QML loop references into stable engine identities before plan installation.
- [x] Return explicit validation and capacity errors to the frontend.
- [x] Keep schedule preparation and UI bookkeeping off RT while keeping all runtime timing decisions on RT.
- [x] Ensure basic and composite loops can be targeted together through one deterministic engine command path.

### Replace timing authority

- [x] Change QML composite creation to create or bind an engine composite object.
- [x] Send compiled schedule/configuration updates to the engine.
- [x] Drive displayed state from engine snapshots.
- [x] Remove or disable update-thread cycle polling as a composite timing input.
- [x] Remove Qt `cycled`/`dependent_will_handle_sync_loop_cycle` recursion from composite execution.
- [x] Ensure GUI or backend-update thread stalls cannot stop an already configured composite timeline.
- [ ] Ensure old frontend objects, if temporarily retained for API compatibility, are passive adapters only.

### Persistence and lifecycle

- [x] Preserve existing session format compatibility unless an intentional migration is documented and tested.
- [x] Restore references only after stable engine identities are available.
- [ ] Handle child deletion, replacement, and regular-to-composite conversion without dangling targets.
- [ ] Test teardown and session replacement without RT destruction or stale-event delivery.

### Stage 4 exit gate

- [x] The normal QML application path uses engine composites as its sole timing authority.
- [x] Existing composite sessions load and expose the expected UI state.

## Stage 5 — Complete feature parity

Work through [`docs/composite_rt/FEATURE_PARITY.md`](docs/composite_rt/FEATURE_PARITY.md) and close every remaining row there.

- [x] Sequential and parallel scheduling.
- [x] Delays, repeats, explicit lengths, and ignored/empty children.
- [x] Regular inherited modes.
- [x] Script explicit modes and completion.
- [x] Nested regular/script combinations.
- [x] Countdown transitions and cancellation.
- [x] Recording-first-occurrence semantics.
- [x] Play-after-record enabled and disabled.
- [x] Immediate sync to first, middle, last, and changed iterations.
- [x] Running-child reporting and UI-derived properties.
- [x] Composite ringbuffer grab in all currently supported synced/unsynced, fixed/default length, stop/play outcomes.
- [x] Runtime schedule recalculation and activation.
- [x] Circular-reference and invalid-reference handling.
- [ ] Save/load and lifecycle behavior.

For heavy operations such as ringbuffer adoption:

- [ ] Prepare destination storage and metadata off RT.
- [ ] Perform only bounded, allocation-free work on RT.
- [ ] Commit all child state changes as one documented RT transaction or boundary sequence.
- [x] Do not reintroduce frontend timing decisions as a shortcut.

### Stage 5 exit gate

- [ ] Every row in `FEATURE_PARITY.md` identifies an engine test, QML test, justified manual-only item, or a documented combination of those.
- [ ] No current user-visible composite feature is knowingly missing.

## Stage 6 — Automated verification and RT hardening

The implementing agent's runtime test obligation may be limited to `shoop_engine` tests and QML tests. Broader platform and manual live-audio validation is assigned to the user, but this does not reduce the required automated coverage within those suites.

### Engine tests

- [x] Create [`docs/composite_rt/TEST_RESULTS.md`](docs/composite_rt/TEST_RESULTS.md) and record all Stage 6 commands, environments, and results there.
- [x] Run targeted tests frequently while implementing.
- [x] Run the complete `shoop_engine` suite with the application backend feature (attempted; environment-blocked before project compilation and recorded in `TEST_RESULTS.md`):

  ```sh
  cargo test -p shoop_engine --features app_backend
  ```

- [x] Add deterministic transition-trace tests independent of frontend polling.
- [x] Add buffer-size and buffer-partition property/table tests.
- [x] Add dense-event and maximum-capacity tests.
- [x] Add command-cutoff and plan-version race tests using controlled scheduling.
- [ ] Add RT allocation tests for normal, event-heavy, command, plan-swap, and failure paths.
- [ ] Add tests or structural assertions demonstrating that callback state access is lock-free.
- [ ] Repeat timing-sensitive tests enough to expose accidental ordering dependencies.

### Frontend/QML tests

- [x] Build before running QML tests:

  ```sh
  cargo build
  ```

- [x] Run the frontend/QML self-test suite:

  ```sh
  target/debug/shoopdaloop_dev.sh --self-test
  ```

- [x] Keep or migrate all existing composite-loop QML scenarios.
- [ ] Add a test that stalls frontend/update processing while engine audio continues and verifies the composite transition trace afterward.
- [ ] Add end-to-end tests for configuration acceptance errors and delayed state observation.
- [ ] Add save/load coverage for nested composites and scripts.

### Quality gates

- [x] Run `cargo fmt --all` after Rust changes.
- [x] Build Rust changes with warnings denied using `RUSTFLAGS="-D warnings"`.
- [x] Record in `TEST_RESULTS.md` any test that cannot run because of environment/dependency limitations, including the command and error.
- [x] Document measured callback cost for ordinary and worst supported composite schedules in `RT_SAFETY.md`, with benchmark commands/results linked from `TEST_RESULTS.md`.
- [x] Create [`docs/composite_rt/MANUAL_VALIDATION.md`](docs/composite_rt/MANUAL_VALIDATION.md) from the Stage 7 checklist, adding required setup, session files, logging counters, expected outcomes, result fields, and reproduction instructions.
- [ ] Confirm in `RT_SAFETY.md` that overload is explicit and deterministic rather than late, blocked, or silently dropped.

### Stage 6 exit gate

- [ ] `TEST_RESULTS.md` shows that engine and QML automated gates pass, except for clearly recorded environment failures or pre-existing failures.
- [ ] `RT_SAFETY.md` contains evidence that RT constraints are verified on all exercised callback paths.
- [ ] Remaining manual checks are listed in `MANUAL_VALIDATION.md` without being represented as completed.

## Stage 7 — User-owned manual validation

The user follows [`docs/composite_rt/MANUAL_VALIDATION.md`](docs/composite_rt/MANUAL_VALIDATION.md), executes its scenarios in representative live setups, and records results there. If this checklist changes, the implementing agent first updates that canonical package to match.

- [ ] Low-buffer JACK live session with sequential and parallel composites.
- [ ] Representative CPAL live session where supported.
- [ ] Start, stop, cancel, and retrigger composites close to sync boundaries.
- [ ] Record a regular composite with play-after-record both enabled and disabled.
- [ ] Use a repeated child and confirm it is recorded only on its first scheduled occurrence.
- [ ] Run nested regular and script composites as scenes/song sections.
- [ ] Exercise immediate sync/seek to several positions while audio is running.
- [ ] Exercise all composite grab variants with real or representative input.
- [ ] Edit a composite configuration while stopped, pending, and running; confirm documented activation behavior.
- [ ] Freeze or heavily load the GUI and frontend update thread while a configured composite continues.
- [ ] Use keyboard/MIDI controls and evaluate perceived command latency around quantization boundaries.
- [ ] Load existing sessions containing composites, save them again, and reload.
- [ ] Stress a large but supported schedule and inspect xruns, overload counters, and transition diagnostics.
- [ ] Confirm no unexplained extra sync-cycle delays or audible boundary glitches.

User findings recorded in `MANUAL_VALIDATION.md` that reveal a violation of an immutable requirement reopen the relevant implementation stage. Usability adjustments that do not violate immutable requirements may update this plan and `SEMANTICS.md`.

## Stage 8 — Remove obsolete execution paths and finish documentation

- [ ] Remove dead update-thread composite scheduling code after parity and integration are established.
- [ ] Remove compatibility signals/slots that can no longer affect behavior.
- [ ] Ensure there is only one authoritative composite state machine.
- [ ] Finalize RT command acceptance, plan activation, conflict handling, and observable state latency in `SEMANTICS.md`.
- [ ] Finalize implemented processing, ownership, and frontend integration in `ARCHITECTURE.md`; finalize capacity limits and callback evidence in `RT_SAFETY.md`.
- [ ] Update permanent developer architecture documentation and user documentation where behavior is newly defined, linking back to the canonical prototype artifacts where useful.
- [ ] Record final automated results in `TEST_RESULTS.md` and pending/completed user manual results in `MANUAL_VALIDATION.md`.
- [ ] Review the immutable requirement checklist and attach evidence for every item.

## Adaptive decision log

The implementing agent should maintain this table as discoveries are made.

| Date/commit | Decision or discovery | Reason/evidence | Plan stages affected | Immutable requirements checked |
|---|---|---|---|---|
| 2026-07-31 / Stage 0 | Use half-open sample intervals and a transactional boundary resolver with direct control > explicit script > inherited regular > natural-event precedence; stable identity breaks same-class ties. | [`SEMANTICS.md`](docs/composite_rt/SEMANTICS.md) and executable `composite_semantics` contract tests. | 0–6 | Determinism, sample-correct interaction, bounded RT work |
| 2026-07-31 / Stage 0 | Activate stopped/pending plan replacements at command acceptance; defer running replacements to the next iteration-zero boundary. | Avoids changing the meaning of the current pass or orphaning children; exact stop-before-activation behavior is specified in `SEMANTICS.md`. | 0, 1, 3, 5 | RT authority, determinism, feature parity |
| 2026-07-31 / Stage 0 | Reject producer/plan capacity overflow before acceptance; event/wave/sub-block overflow enters a latched fail-closed RT fault rather than applying an event late. | [`SEMANTICS.md`](docs/composite_rt/SEMANTICS.md) capacity policy and `overflow_never_turns_into_a_late_event`. | 0, 2, 3, 6 | Bounded RT work, no RT allocation, determinism |
| 2026-07-31 / Stage 0 | Preserve `loop.1` playlist persistence as the compatibility surface; compile missing IDs as errors and use generation-checked engine targets after acceptance. | Current schema/application inventory in [`FEATURE_PARITY.md`](docs/composite_rt/FEATURE_PARITY.md); stale-target contract tests. | 0, 1, 4, 5 | Feature parity, top-level integration, determinism |
| 2026-07-31 / Stage 1 | Compile descriptors off-thread into immutable stable-ID, iteration-major desired-state and sparse ordered-action tables; cap precomputed seek storage instead of replaying schedules in RT. | [`ARCHITECTURE.md`](docs/composite_rt/ARCHITECTURE.md), `composite_plan`, and compiler/state-machine tests. | 1–4 | No RT allocation, determinism, bounded RT work |
| 2026-07-31 / Stage 1 | Keep `CompositeRuntime` allocation-free by borrowing plans and using fixed target/output arrays; defer plan ownership and non-RT reclamation to Stage 3. | Allocator-enforced `composite_state_machine_does_not_allocate_or_free`; source audit finds no locks, strings, or hash containers in runtime/plan storage. | 1, 3 | No RT allocation, no RT mutexes, bounded RT work |
| 2026-07-31 / Stage 1 | Model running replacement as an externally owned deferred candidate and expose an atomic iteration-zero activation transaction; a completing script activates its candidate stopped. | Replacement tests and the plan-replacement sections of [`SEMANTICS.md`](docs/composite_rt/SEMANTICS.md) and [`ARCHITECTURE.md`](docs/composite_rt/ARCHITECTURE.md). | 1, 3 | Determinism, RT authority, feature parity |
| 2026-07-31 / Stage 2 | Reuse primitive sync-source POIs for all iteration-aligned composite work; only accepted timestamps between existing POIs add a split. | Exact-output, arbitrary-partition, and source-POI sub-block tests in `composite_timing`. | 2, 3, 6 | Sample correctness, RT authority, bounded work |
| 2026-07-31 / Stage 2 | Resolve a boundary transaction over preallocated working runtimes in a stable graph containing both composite-target and composite-sync edges. | `composite_timeline` conflict, deep-nesting, and transitive-sync tests; termination proof in [`ARCHITECTURE.md`](docs/composite_rt/ARCHITECTURE.md). | 2–4, 6 | Determinism, nested propagation, no RT allocation |
| 2026-07-31 / Stage 2 | Use fixed accepted-control staging and preallocated event/intent/trace storage; reject producer/topology overflow and latch event/sub-block failures without late delivery. | [`RT_SAFETY.md`](docs/composite_rt/RT_SAFETY.md), overflow tests, and allocator-enforced integrated processing. | 2, 3, 6 | Bounded RT work, explicit overload, no RT allocation |
| 2026-08-01 / Stage 3 partial | Reuse the engine's bounded SPSC command/return rings for timeline ownership; take one fixed callback-start drain cutoff, assign control sequence on RT acceptance, and return displaced or rejected timelines through preallocated result slots. Exact primitive topology plus monotonically increasing global timeline versions reject stale/out-of-order work. | `composite_control` cutoff/install/version/race tests and allocator-enforced success/rejection processing; [`ARCHITECTURE.md`](docs/composite_rt/ARCHITECTURE.md). | 3–4, 6 | RT authority, determinism, no RT allocation, bounded work |
| 2026-08-01 / Stage 3 partial | Publish composite state in three reusable latest-state boxes with fixed active-child arrays; skip and count publication when all boxes are in use, and resize only when the control side recycles them. | `prepared_timeline_and_control_cross_at_callback_boundaries_and_publish_state`, `stale_snapshot_publication_is_dropped_without_stalling_processing`, and allocator guard. | 3–6 | No RT allocation, no RT mutexes, top-level integration |
| 2026-08-01 / Stage 3 partial | Store one candidate plan per runtime when dependency topology is unchanged; commit at old iteration zero (or stopped command boundary), move old plans into preallocated retirement storage, and swap them into control-provided reclamation storage. Reject running dependency-topology changes before version/authority changes. | Integrated stopped/pending/running/supersession/stop tests and allocator-enforced activation/reclamation; [`ARCHITECTURE.md`](docs/composite_rt/ARCHITECTURE.md). | 3–6 | RT authority, determinism, no RT allocation, feature parity |
| 2026-08-01 / Stages 4–5 partial | Keep the existing QML playlist/persistence surface as an off-RT adapter, resolve QObject references to stable identities, install a transactional application registry, route execution through engine commands, and mirror snapshots. Preserve UI-only anticipated-transition reporting without making it timing authority. | `composite_app_backend` passes 2/2; `tst_CompositeLoop_running.qml` passes 24/24; full QML suite passes 188/189 with only the no-CPAL-device host failure. | 4–6 | Top-level RT authority, feature parity, determinism, session compatibility |

## Completion definition

The prototype is implementation-complete when:

- Every immutable requirement has evidence.
- Engine composites are the timing authority used by the top-level application.
- `FEATURE_PARITY.md` is complete.
- `SEMANTICS.md`, `ARCHITECTURE.md`, and `RT_SAFETY.md` describe and support the implemented result.
- `TEST_RESULTS.md` shows that the engine and QML automated gates meet Stage 6.
- `MANUAL_VALIDATION.md` has been handed to the user.

Final live-performance acceptance remains pending until the user completes the Stage 7 scenarios and reports the results.
