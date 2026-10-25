# Plan: resolver-driven runtime and portable loop execution

## Status and execution contract

Implementation authorized by confirmed goal `mvnjf70y-cjcfar`. Stages 0–2 are verified; Stage 3 has verified metadata and interval-lifecycle milestones, with typed domain delivery still outstanding; stages 3–10 remain incomplete. Evidence and acceptance mapping are maintained in `docs/loop_layer/VALIDATION.md`.

Governing inputs: `INTER_LOOP_PROCESSING.md`, `PROPOSAL_LOOP_LAYER.md`, and the proposed-evolution sections of `DESIGN_RT_ENGINE.md` and `PROPOSAL_RT_ENGINE.md`. `DONE_INITIAL_CRATE.md` and its evidence remain the historical record of the initial runtime, not acceptance criteria to rewrite retrospectively.

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

For each stage, record changed interfaces, commands/results, evidence paths, limitations, and the milestone commit. Do not mark a stage complete on the strength of proposed tests or unverified claims.

## Goals and scope

1. Evolve `rt_runtime` to support generic planner-led interval execution, typed domain-operation delivery, prepared decision/observation storage, and resource access suitable for sample-exact looping.
2. Implement `src/rust/shoop_loop_engine` as a portable library containing one authoritative loop/composite timeline resolver and separate audio/MIDI render operations.
3. Prove coordinated recording, playback, inter-loop triggering, and prepared composite execution in native and actual non-shared-memory AudioWorklet hosts.
4. Update runtime API documentation, architecture documents, examples, audits, and validation evidence to describe the implemented contracts accurately.

Here, “RT backend changes” means the runtime and its integration with the new loop library. Switching the production `shoop_backend`, `Session`, application protocol, or production worklet to this library is **not** part of this milestone.

**Included loop capabilities:** stopped/playing/recording; immediate and timestamped start/stop/seek; sync-source/cycle-delayed transitions; lengths, positions, cycle counts, and same-frame trigger propagation; stereo and configurable prepared audio channels plus MIDI; exact-frame record-to-play; prepared content-revision replacement; gain and an explicit boundary-smoothing policy; coherent multichannel capture/snapshots; separable playback and recording around a test FX processor. Prepared composites include sequential/parallel entries, delays and repetitions, regular repeating playback, explicit-mode one-pass programs using supported modes, and finite nesting. Editing those programs is outside the render layer.

**Excluded:** complete legacy API/behavior compatibility, in-place overdub/replacing modes, prerecord/preplay and latency compensation, full dry/wet application-mode policy, track/scene/UI integration, persistence/undo/scripting execution, plugin hosting, device adapters, signal feedback graphs, parallel DSP, resampling, MIDI 2.0, unlimited recording, shared-memory acceleration, and guaranteed callback deadlines. An ordinary gain/FX test node is sufficient to prove routing; no production plugin host is required.

## Immutable acceptance criteria

| ID | Required result |
| --- | --- |
| LL-A1 — boundaries | `rt_runtime` remains generic, with no production/build dependency on `shoop_*`, UI, device, or browser crates. `shoop_loop_engine` depends on the runtime, not the old engine/backend/application or browser/device facilities. Both libraries build and execute on native and `wasm32-unknown-unknown`; host bindings remain in fixtures. |
| LL-A2 — timing authority | One render-owned resolver owns authoritative primitive/composite timing. Renderers consume coherent decisions and return observations; they do not discover sync by reading partially updated peer states. Explicit planner-to-consumer dependencies are validated independently of audio/MIDI routes. |
| LL-A3 — musical execution | Every required finite transition of an admitted supported program executes at its intended sample, including A → B → C at the same frame and one-sample-loop wraps. No fixed interaction quantum, mandatory block/per-hop delay, or arbitrary per-callback wrap, propagation-round, or subdivision cap may alter musical results. |
| LL-A4 — termination | The supported trigger/program model has documented termination semantics and a progress argument. Same-frame infinite behavior is rejected/reported through that model, not a work counter. Valid cyclic sync relationships remain possible; they are not confused with instantaneous causal recursion or signal feedback. |
| LL-A5 — interval invariance | With identical initial musical state, timestamped inputs admitted before their deadlines, and sufficient prepared resources, sample output, absolute MIDI events, and semantic transition journals agree across host callback partitions and permitted declaration/traversal orders. Callback-end events execute once, never late or twice. |
| LL-A6 — callback safety | All callback-reachable planning, delivery, rendering, observation, recycling, and failure paths are allocation/reallocation/deallocation-free, nonblocking, and free of locks, waits, external I/O, and unbounded producer-chasing retries. Storage is prepared and fixed. Work is finite and application-dependent; no hard-real-time deadline claim is made. Guards remain active in debug, release, and the actual worklet. |
| LL-A7 — ownership/admission | External ingestion remains a finite callback snapshot/batch with reliable owning returns and explicit admission versus execution outcomes. Internal musical events do not consume external command/completion credits merely to progress. Stalls, cancellation, stale identities, disconnect, plan replacement, and teardown cannot leak, retarget, double-release, or destroy owners on the callback. |
| LL-A8 — media and capture | A stereo-plus-MIDI loop records, finishes at an exact frame, immediately plays the recorded content without an unintended silent block, wraps, seeks, replaces prepared content, stops with defined MIDI cleanup, and exports a coherent immutable snapshot. Playback → FX → recording works without an artificial signal cycle. Multichannel exhaustion leaves a truthful common captured prefix/incomplete status and does not abort unrelated audio. |
| LL-A9 — composites and outcomes | The resolver executes the included prepared composite features and primitive/composite interactions under one ordering/termination model. Capture/resource outcomes capable of changing timing reach the resolver before affected execution crosses their semantic boundary; speculative whole-block planning cannot conceal them. |
| LL-A10 — resource/accounting separation | Fixed storage limits and background-transfer/admission quotas remain explicit. Required rendering and internal musical progression cannot fail merely because a background CPU-work quota or diagnostic counter ran out. Indexing, capture activation, coherent publication, and reclamation have documented preparation, ownership, and exhaustion contracts. Optional planning/trace storage exhaustion does not change music. |
| LL-A11 — evidence | Deterministic reference/property tests, heap/destructor guards, selected Miri/concurrency checks, native offline/two-thread replay, Node/Chromium tests, and a real non-isolated AudioWorklet prove the new paths. Actual Web Audio output and transition/MIDI journals agree with native reference execution; ordinary Wasm memory does not grow during rendering. Performance reports expose workload, raw timings, transitions, memory/storage peaks, and failures without hiding expensive workloads. |
| LL-A12 — docs and delivery | Runtime and loop-library documentation match shipped APIs and limitations; old evidence remains labeled historical. Existing applicable runtime/application tests remain green. Milestone commits are pushed in a PR; required final-head CI is green and available automated reviews are checked and addressed through approval, or absence/unavailability is reported explicitly without inventing approval. |

## Investigation baseline and concrete change map

Inspected starting commit: `cc6ccb4ac35114d61ef7e1c03ecc7f2fe81bb867` on `feat/generic-rt-runtime`, with pending edits to the three architecture documents and unrelated `.pi/`. Recheck status/base before implementation; this is not a claim that the inspected tree is clean or that the earlier PR is merged.

| Area | Current constraint | Planned change |
| --- | --- | --- |
| `rt_runtime/src/executor.rs` | `process` begins clock/admission/resource accounting once; `run_scheduled` traverses at most `budgets.subranges`; `run_graph_range` only invokes processors | Preserve the outer callback lifecycle; introduce boundary planning and observation phases inside it, and run the signal DAG over selected intervals |
| `graph.rs` | `GraphDescription`, `PlanDescriptor`, compiler and incremental validator describe only signal connections | Add explicit planning/consumer dependencies and prepared view bindings; validate generations, ordering, capacities, and atomic activation on both compiler and receiver paths |
| `processor.rs`, `schedule.rs`, `executor.rs` | `ProcessBlock` exposes media and `ResourceIo`; actions are parameter/MIDI only; event/result capacity is coupled to split allowance | Add typed operation delivery and typed decision/observation access; distinguish external scheduler credits from internally selected intervals |
| `config.rs`, resource accounting | Shared byte/validation quotas cover control and processor work; counter bounds assume event-limited subdivisions | Separate maintenance/admission accounting from required rendering; retain storage validation and safe counters without imposing a musical-work quota |
| `resource/{mod,capture,request}.rs` | Sequential linked-page cursors; capture lifecycle commands at callback boundaries; per-resource atomic append; no renderer start/stop/group reservation API | Prepared indexed access, exact-frame capture preparation/activation/freeze, coherent group reservations/commit, and render-safe lease/resource operations |
| `shoop_engine/src/{basic_loop,audio_midi_loop,composite_plan,composite_runtime,composite_semantics,composite_timeline}.rs` and corresponding tests | Useful timing/media behavior, but old coupling, snapshot sync, event-wave/sub-block caps, and broad sync-cycle rejection | Extract intended behavior into explicit new contracts and reference cases; do not port forbidden limits or make the new crate depend on the old engine |
| `tests/rt_runtime_host`, scripts and CI | Existing independent runtime/worklet evidence and failure gates | Keep these regression fixtures; add a loop-library fixture with shared native/Wasm implementation and new evidence |

## Design rules

- One runtime and signal graph, one authoritative loop resolver, serial render ownership. Start with one loop crate; proposed modules are `ids`, `command`, `semantics`, `program`, `resolver`, `decision`, `audio`, `midi`, `runtime`, and `state`.
- Use safe Rust in both production libraries; retain `rt_runtime`'s `forbid(unsafe_code)`. Any existing test/host allocator shim remains narrowly audited, not a production aliasing workaround.
- The runtime supplies a generic planner lifecycle and lifetime-safe access to prepared views. No Shoop mode enums, musical conflict rules, dummy MIDI ordering edges, shared mutable peer access, or callback-side owning allocations.
- A plan selects nonempty intervals; same-frame settlement happens before them. Begin admission, resource maintenance accounting, result recycling, and host MIDI recovery allowances once per host callback, never per musical interval.
- Constrain trigger semantics instead of accepting arbitrary event-producing closures. Derive scratch requirements from admitted topology/program storage, not an arbitrary number of propagation rounds. Retained event capacity is legitimate; a disguised musical-work cutoff is not.
- Reuse transport, reliable returns, resource pools, immutable leases, generation checks, and retirement. Musical resolution is necessary domain scheduling, not another generic executor or transport.
- Required media reads/writes and internal activation must not draw from a throttled background-work allowance. Keep per-call work finite, storage-bound, and accounted; background upload/export/validation/reclamation remains budgeted. MIDI output storage and recovery remain explicit limits, not promises of unlimited output.
- Observation timing is part of the contract. Use preparation/preflight/reservation or a genuinely earlier boundary when needed; do not render dependent samples and then retroactively claim an earlier capture failure. Preserve the runtime's distinction between normal domain failure and programming faults; a propagated processor error is not rollback.
- Keep signal playback/capture operations separable and give all channels a common logical capture cut. Index recorded data incrementally as it arrives so record-to-play does not require an unbounded scan or later control-thread task.
- Distinguish semantic state/journals from callback-shaped diagnostics and opaque handle values. Do not demand identical callback counts or handle allocation histories across partitions; do demand identical musical results under LL-A5.
- Longer preplanned batches and narrower traversal are optional later optimizations. The incremental path is the baseline and must remain correct when optional batch/trace storage fills.

## Staged implementation

### Stage 0 — Branch, baseline, and evidence layout

Depends on: none. Establishes the implementation baseline for all criteria.

- [x] Inspect current changes and upstream/PR state; create `feat/loop-execution-layer` from the required runtime base. Use an explicit stacked PR if the runtime base is not merged. Preserve unrelated work and carry the intended architecture-document edits deliberately.
- [x] Read project mandates/build/test/CI guidance; record toolchain and browser pins. Use `nix develop --command ...` for local Cargo/Python work on this host; use the provisioned environment in CI.
- [x] Record baseline runtime debug/release tests, workspace build/test gates, dependency audits, and existing native/worklet fixture results. Record failures as evidence rather than silently redefining baseline success.
- [x] Create `docs/loop_layer/VALIDATION.md` and an evidence directory, with LL-A1–LL-A12 mapped to planned tests/artifacts. Keep existing runtime evidence and golden journals intact.
- [x] Commit the planning/design baseline and evidence setup without unrelated files.

**Verification:** baseline commands and source SHA are recorded; branch/base and dirty-file handling are explicit. A missing browser is a recorded blocker for browser evidence, not grounds to substitute replay.

### Stage 1 — Specify semantics and generic integration contracts

Depends on: Stage 0. Must finish before implementation choices are treated as settled.

- [x] Write `docs/loop_layer/SEMANTICS.md`: primitive modes, zero-length behavior, positions/cycles, start/retrigger/stop/seek, sync-source idle/missing behavior, countdowns, recording completion, prepared revision replacement, smoothing, and MIDI state cleanup/restoration. State precisely which legacy tests are adopted, superseded by the governing decision, or outside scope.
- [x] Specify composite sequence/parallel/delay/repetition/nesting semantics, regular versus explicit-mode one-pass execution, source-stop suppression, empty children, seek, conflict precedence, and active-program replacement. Use existing composite tests as evidence, not automatic compatibility requirements.
- [x] Settle occurrence identity/creation/delivery, simultaneous-action precedence, and the termination proof. Include a valid mutually synchronized example and an invalid instantaneous recursion example. A candidate mechanism may relay an occurrence once per recipient, but must also constrain creation of fresh occurrences; document why all admitted same-frame work terminates.
- [x] Define half-open sample/event intervals, callback-end treatment, external action ordering relative to natural boundaries/observations, and exactly-once settlement. Define reliable terminal outcomes for externally requested delayed transitions separately from automatic wraps/telemetry; specify cancellation after runtime admission separately from ticket cancellation.
- [x] Write `docs/loop_layer/RUNTIME_INTEGRATION.md` and generic API sketches: planner preparation/lifecycle, typed operation targets, immutable decision views, observation writers, capacities, default no-planner behavior, and activation/retirement ownership. Compile a small type/lifetime prototype with a non-musical synthetic planner; finalize signatures before extending the executor. No unsafe/shared-mutable escape hatch.
- [x] Specify a failure/observation table for capture group reservation, append, stop/freeze, new playback lease/index readiness, MIDI refusal, stale content, and plan activation. Each entry gives the decision boundary, committed prefix, affected loops, reliable outcome, and unrelated processing behavior.
- [x] Document the revised budget classes and counter-overflow policy in `DESIGN_RT_ENGINE.md` and `docs/rt_runtime/CONTRACTS.md`, clearly labeled proposed until implemented. Preparation may reject insufficient storage; CPU cost alone cannot justify truncating a valid admitted program.

**Verification:** table-driven expected journals cover A → B → C, simultaneous start/stop/wrap, composite → primitive → composite, idle sync sources, callback ends, and capture failure. Review every acceptance criterion against the contracts. Escalate any necessary goal/scope change for user approval; do not resolve it by weakening LL-A3/LL-A4.

### Stage 2 — Pure loop/composite resolver and reference model

Depends on: Stage 1. Covers LL-A1–LL-A5 and the logical portion of LL-A9.

- [x] Create `src/rust/shoop_loop_engine` with workspace lint/test conventions and Wasm-test metadata; the existing `src/rust/*` workspace glob includes it. Update the root lockfile. Production dependencies start with `rt_runtime`; shared test support remains dev-only. Add a separate loop dependency-boundary audit and tests without weakening `check_rt_runtime_boundary.py`.
- [x] Implement generation-checked logical IDs, typed loop requests, prepared topology/program compilation, and fixed-capacity resolver state/scratch. Compile structural data off-render; validate arithmetic, targets, program storage, supported nesting, and termination conditions before activation.
- [x] Implement primitive timing, sync/countdown transitions, same-frame propagation/conflict resolution, and prepared composite execution in one resolver. Return incremental interval decisions and consume synthetic execution observations without retaining all callback transitions.
- [x] Implement declared/derived scratch sizing and transactional rejection. Remove reliance on legacy `max_event_waves`, sub-block caps, and blanket sync-cycle rejection; do not replace them with differently named work caps.
- [x] Build an independent scalar/reference simulator from the semantic tables, plus seeded valid/invalid program generators. The reference may allocate off-render; it must not call the production resolver as its oracle.
- [x] Add tests for declaration-order independence, deep finite cascades beyond old wave limits, one-sample loops beyond old subdivision limits, finite nested programs, valid cyclic sync, invalid instantaneous behavior, stale IDs, zero/maximum values, and scratch exhaustion at preparation.

**Verification:** native debug/release and Node/Chromium semantic tests; compare full absolute-frame journals across partitions such as `[128]`, `[1 × 128]`, and seeded irregular partitions. Run production resolver paths under heap/destructor guards, including rejection and replacement. Document the implemented progress argument alongside its tests.

### Stage 3 — Generic planner lifecycle, graph dependencies, and typed delivery

Depends on: Stages 1–2; synthetic planners isolate runtime correctness from loop implementation.

Progress: the planning-metadata/compiler/receiver-validation milestone is verified in `docs/loop_layer/evidence/graph-stage3.json`, with failure investigation and host provenance in `graph-stage3-provenance.json`. Separate planning descriptors preserve signal-only descriptor/wire compatibility. The interval-lifecycle milestone adds production planner views/observations, internal intervals, separate external credits, and saturating diagnostics; see `interval-stage3.json` and `interval-stage3-provenance.json`. Typed domain delivery/pending runtime receipts and their recovery/ownership tests remain outstanding, so this stage remains incomplete.

- [x] Extend `graph.rs` descriptions, reusable descriptors, compiler, membership handling, and incremental receiver validation with explicit planner/consumer bindings and ordering dependencies. Validate missing/stale targets, duplicate bindings, invalid dependency order/cycles, and descriptor capacities without using signal connections as control edges.
- [x] Extend `processor.rs` and the executor with the finalized planner/view/observation interface. Retain a straightforward no-planner path for existing processors/examples. Prepare and pre-touch decision and observation storage; enforce view lifetimes and clear/reuse observations without aliasing or stale-frame access.
- [x] Refactor `process`/`run_scheduled` into one host-callback envelope and an internal boundary/interval loop. Select the next boundary from external due actions, planner decisions, and block end; preserve absolute/relative MIDI offsets and signal DAG ordering. Observe outcomes before settling the next affected boundary.
- [ ] Add typed planner/processor operation delivery through the existing admission/ownership machinery. Support explicit target generations, payload limits, malformed-operation refusal, late reporting, and reliable execution results; do not encode loop operations in parameter IDs/floats.
- [x] Separate external event/result reservations from internal subdivision counts in `schedule.rs`/`config.rs`. Preserve external admission guarantees and due-action semantics without charging each resolver boundary a scheduler slot. Make diagnostics overflow-safe without using diagnostic bounds to reject legitimate musical work; diagnostic saturation/reporting must not alter execution, while frame/identity exhaustion remains an explicit semantic error.
- [ ] Test queued ingress arriving during many internal intervals: it waits for the next host callback. Verify that command snapshots, result recycling, control/maintenance quotas, telemetry publication, and MIDI recovery allowances are not replenished per interval.
- [ ] Update all affected generic runtime examples, tests, and independent host descriptor/wire code. Keep old-format support or deliberately version the fixture format; preserve historical journals and evidence rather than silently overwriting them.

**Verification:** generic synthetic planner tests prove ordering and view safety, dense interval execution with tiny external budgets, exact-frame scheduled parameter/MIDI actions interleaved with planning, callback-end behavior, invalid intervals, stale graph activation, cancellation, and owning retirement. Existing runtime suites and native host replay remain green; selected new view/ownership cases run under Miri.

### Stage 4 — Render-safe indexed media and exact-frame capture transactions

Depends on: Stage 3 and Stage 1 resource/failure contracts. Required before loop media integration.

- [ ] Split `ResourceStore`/`ResourceIo` accounting into background maintenance/transfer work and required rendering access. Keep background quotas and reservations effective across the entire host callback. Preserve finite per-operation work and fixed pool capacities; report render work independently rather than returning `Budget` solely for exceeding background allowances.
- [ ] Add prepared page/index access for audio seek/wrap and MIDI event/state positioning. Validate all offsets, generations, leases, and index ownership. Build indexes during upload/capture or incremental preparation; no whole-resource scan at seek, wrap, record-stop, or exact-frame activation.
- [ ] Add capture preparation/reservation tokens and restricted render-side activation/freeze operations. Reserve lifecycle/version/lease capacity before promises are made; support exact-frame record-to-play with immediately usable frozen content/indexes.
- [ ] Add generic group preflight/reservation/commit for coordinated audio/MIDI capture. Account for unknown downstream MIDI sizes using the specified reservation/staging strategy. Make all channels retain a common logical prefix on failure; release unused reservations without callback destruction. Never commit one channel and then silently fail another.
- [ ] Integrate resource observations with interval planning: insufficient preparation or a failed reservation changes decisions before affected samples are rendered. Verify data-dependent outcomes against the Stage 1 boundary contract; do not disguise an already crossed boundary as recoverable rollback.
- [ ] Preserve immutable snapshots/currentness, old revision playback, active-capture tail isolation, deferred reclamation, and store-scoped handles. Test cancellation/disconnect between every new preparation/activation/release step.

**Verification:** extend runtime resource/capture/property tests with indexed seeks across page boundaries, empty/large MIDI records, stale indices, immediate freeze/read, multichannel partial-capacity refusal, old leases during replacement, and full pool recovery. Under heap guards and Miri, tiny maintenance budgets must not interrupt otherwise provisioned rendering or be refreshed by interval iteration.

### Stage 5 — Audio/MIDI renderers and coordinated loop facade

Depends on: Stages 2–4. Covers LL-A6–LL-A8 and actual execution observations.

- [ ] Implement audio playback and capture operations consuming resolver decisions: planar/interleaved conversion in prepared scratch, gain/smoothing per contract, indexed region access, wrap/seek, and prepared revision activation.
- [ ] Implement MIDI playback/capture with sample rebasing, prepared event/state indexes, deterministic equal-time ordering, note/controller tracking, discontinuity cleanup/restoration, bounded SysEx, and the runtime's overflow/reset protocol. No state-restoration scan through the full recorded history.
- [ ] Implement `runtime` integration connecting the resolver, typed operations, renderers, prepared resources, and signal graph. Keep timeline state in the resolver; renderer cursors and media observations must not become a second clock.
- [ ] Implement coherent group recording/freeze/playback and snapshot/export. Preserve previous playable content when a new prepared activation is refused; make capture exhaustion/incomplete state visible without aborting unrelated audio or freezing the resolver.
- [ ] Prove separate playback → gain/FX → recording nodes for the same logical loop, including simultaneous playback/capture of distinct revisions. Prevent writes to leased immutable playback content.
- [ ] Add an executable public-API example that records stereo plus MIDI, finishes at an interior frame, plays immediately, wraps repeatedly, seeks, replaces content, stops, exports, and drains/releases every owner.

**Verification:** sample/event reference comparisons for short and non-block-aligned content, page boundaries, MIDI notes across wraps, callback-end transitions, and configured smoothing. Guard all successful/failure paths in debug/release and Wasm. Check exact first playback sample, captured bytes/duration, note cleanup, common incomplete prefix, and recovery of all resource/lease credits after teardown.

### Stage 6 — Integrated inter-loop/composite lifecycle and coherent state

Depends on: Stage 5. Completes the supported musical feature set and control surface.

- [ ] Connect sync/countdown, trigger, and composite requests to the typed runtime path. Keep external request admission, pending musical state, cancellation, actual transition outcome, and automatic recurring events distinct; reserve reliable result storage for externally requested terminal outcomes.
- [ ] Implement prepared topology/program activation and replacement with runtime graph/loop generation checks. Define and enforce a consistent activation boundary across view bindings, resources, and program state. Failed/stale candidates leave the current working configuration intact; displaced owners retire off-render.
- [ ] Implement primitive/composite stop, seek, nested activation, and program replacement using the Stage 1 precedence rules. Exercise active/pending/stopped states and same-frame primitive → composite → primitive interactions, not just standalone composite simulation.
- [ ] Publish coherent loop/composite state and capture metadata tagged with frame/program identity. Use bounded snapshot/telemetry mechanisms; no UI locks, mixed-time field reads, or retry-until-consistent loops. Snapshot refusal must not block musical execution.
- [ ] Add the full coordinated scenario: A records and starts playback at an exact frame; A triggers B and B triggers C at that frame; a nested prepared composite joins the timeline; playback/FX/recording continues while capture failure changes a later decision at its specified boundary.
- [ ] Add mixed control stress: normal-lane saturation, stalled result/telemetry/export consumers, stale generations, pending cancellation, topology replacement, disconnect, recovery, stop/restart, and eventual off-render disposal.

**Verification:** compare integrated semantic journals, audio, MIDI, state cuts, and captured content against the independent model. Vary host partitions and graph declaration order with sufficient resources; run explicit exhaustion tests separately. Trace/telemetry buffers filling must not change audio or transition results. No external credit stall may stop already admitted automatic loop progression.

### Stage 7 — Independent native and real AudioWorklet hosts

Depends on: Stage 6. Covers LL-A11 portability and host evidence.

- [ ] Add a test-only `tests/loop_engine_host` workspace with its own committed lockfile, shared native/Wasm controller, relocatable versioned wire codec, prepared bridge, and the **actual loop library**. Add `scripts/run_loop_engine_hosts.py`; reuse existing harness utilities where useful without making the independent `rt_runtime` fixture depend on Shoop semantics.
- [ ] Prepare fixed ingress/response buffers, typed-operation payloads, planner/graph descriptors, program staging, and cancellation/return credits before rendering. Validate wire sizes, offsets, versions, sequences, target generations, and memory ownership incrementally; no transferred Rust pointers or callback graph/program construction.
- [ ] Run native offline and bounded two-thread hosts. Record timestamped admissions/executions, semantic transitions, MIDI, actual audio sample checks, exported content, ownership counts, and fixed-memory usage. Keep thread coordination outside the callback.
- [ ] Run real non-isolated OfflineAudioContext and live AudioContext sessions with independent ordinary Wasm memories, no SharedArrayBuffer/threads/COOP/COEP. Check actual Web Audio output, not only a Rust-produced self-hash; compare the offline journal and precisely representable sample fixture with native replay/reference results.
- [ ] Include dense one-sample wraps, same-frame cascades, composites, record-to-play, seeks/replacement, capture failure, transfer/result stalls, malformed/stale messages, and graph changes. Live tests use acknowledged timing, not an assumption that message delivery matches offline timing.
- [ ] Guard ingress, planner/render, observation, and completion recycling; assert no Wasm memory growth. Add real failure canaries for processor/planner errors and Rust heap violations, with specific expected diagnostics/nonzero runner exits. Prove successful stop/dispose and complete credit/resource recovery separately from intentionally failed sessions.
- [ ] Preserve reproducible journals, hashes, tool/browser versions, commands, and structured reports under `docs/loop_layer/evidence/` and CI artifacts. Label native-only replay as replay, never a fresh worklet run.

**Verification:** both native profiles, Node and Chromium unit tests, and actual offline/live worklet sessions pass. Browser absence, unrelated exceptions, missing bindings, timeouts, or merely building Wasm cannot satisfy the host/failure gates.

### Stage 8 — Adversarial validation, performance, and CI wiring

Depends on: Stages 3–7. No optimization may weaken LL-A3–LL-A10.

- [ ] Extend seeded model/property testing for programs, occurrence delivery, descriptors, typed operations, resource indexes, capture transactions, and partition invariance. Include adversarial arithmetic, counter/generation exhaustion, reused slots, saturated retained-event storage, and malformed wire records; assert rejected operations preserve prior valid state/ownership.
- [ ] Extend the pinned `scripts/run_rt_runtime_miri.py` runner with explicit package/target/filter/minimum-count records for the new runtime and loop cases, retaining its existing checks and isolation. Run selected lifetime/lease/decision-view/capture tests and multiple concurrency seeds; retain full-length native stress when Miri uses documented shorter loops.
- [ ] Add `scripts/benchmark_loop_engine.py` and a guarded release example. Measure idle/no-planner overhead, multiple audio/MIDI loops, short-loop/dense cascades, composite nesting, indexed seek/replacement, playback/FX/capture, and simultaneous uploads/exports. Preserve scalar/media correctness checks and preallocated timing records.
- [ ] Report raw callback timing distributions/maxima/deadline misses, frames/intervals/transitions, graph visits, render versus maintenance copies/steps, storage high-water marks, ownership credits, and source/toolchain/hardware/configuration. Vary loop counts and lengths to show workload scaling. Timing is evidence, not an arbitrary xrun-free CI requirement or an unsupported speedup claim.
- [ ] Audit every new callback-reachable path for heap destruction, locks, I/O, retries, hidden full-content scans, and accidentally refreshed quotas. Exercise deliberate allocation/reallocation/deallocation violations in debug/release and Wasm so guards cannot silently disappear.
- [ ] Update `.github/workflows/build_and_test.yml`: both crates/examples/native fixture replays in Linux/Windows/macOS debug/release cells, loop Wasm discovery/dependency checks, browser path filters, actual worklet/failure gates, selected Miri, release measurements, and uploaded reports. Keep existing runtime and production application gates.
- [ ] Update tracing coverage inventory for new modules according to project policy, without adding application tracing dependencies to the generic runtime or exempting callback code from validation.

**Verification:** CI definitions and helper-script tests pass locally where applicable; new jobs fail on injected semantic/guard failures and missing tests/artifacts. Review dense-workload tails with recorded evidence rather than reducing required work. Identify any unavailable platform checks as pending until remote CI runs them.

### Stage 9 — Runtime documentation, examples, and final contract audit

Depends on: implemented Stages 3–8; update affected API docs during those stages as well.

- [ ] Update `src/rust/rt_runtime/README.md`, crate/module rustdoc, and all crate-local guides: `docs/getting-started.md`, `integration.md`, `scheduling-midi.md`, `resources.md`, and `performance.md`. Describe planner/no-planner paths, dependency/view lifetimes, typed delivery, callback versus interval lifecycle, changed capacity formulas, media indexing/capture groups, separate work classes, failure/termination obligations, and shutdown. Keep these guides self-contained within the crate.
- [ ] Update repository runtime documents: `docs/rt_runtime/{CONTRACTS,RESOURCES,GUIDE,AUDIT,HOST_FIXTURE,PERFORMANCE,VALIDATION}.md`. Clearly distinguish current contracts from dated initial evidence; add new audit/evidence sections rather than falsifying earlier measurements or test runs. Retain `DONE_INITIAL_CRATE.md` unchanged.
- [ ] Update `DESIGN_RT_ENGINE.md`, `PROPOSAL_RT_ENGINE.md`, and `PROPOSAL_LOOP_LAYER.md` from proposed capability to implemented status only where demonstrated; preserve rationale and list remaining limitations. Cross-check consistency with `INTER_LOOP_PROCESSING.md` without silently changing its decision.
- [ ] Write the loop crate README and user-facing preparation, commands/composites, recording/MIDI, state/export, and host-integration guides. Finalize `docs/loop_layer/{SEMANTICS,RUNTIME_INTEGRATION,VALIDATION}.md` and add callback/ownership audit and performance reports with acceptance/evidence links.
- [ ] Add/run executable generic planner and coordinated-loop examples; update both standalone fixture READMEs and any changed wire/journal version descriptions. Check Markdown links, rustdoc/doctests, code snippets, advertised commands, and capacity arithmetic against the final code.
- [ ] Audit LL-A1–LL-A12 item by item. Record exact test names/artifacts, intentional legacy semantic differences, unsupported features, and evidence limits. No claim of compatibility, lossless unbounded MIDI/recording, browser hard real time, or measured performance outside the tested configuration.

**Verification:** warning-denied rustdoc and examples pass; documentation describes no stale blanket subdivision/render-I/O CPU limit and no unimplemented API as shipped. A reader can prepare, run, observe, and dispose the new layer from the public docs without studying the old backend.

### Stage 10 — Final end-to-end validation and delivery

Depends on: all prior stages. Completes LL-A12; do not stop at local success.

- [ ] Run the complete final gate set below from the final implementation tree, including existing application/browser smoke checks. Record actual discovered/passed/skipped counts and reasons; do not copy initial-crate totals or treat unexpected skips as success.
- [ ] Rerun the coordinated reference/partition suite, mixed saturation/exhaustion teardown, actual offline/live worklet, failure canaries, selected Miri, and release benchmarks. Verify artifacts contain real results and full logical resource/owner recovery, not just successful command exits.
- [ ] Commit each remaining meaningful milestone, documentation, plan progress, and evidence. Push the dedicated branch and open/update the PR with scope, architectural/API changes, semantic decisions, tests, limitations, and LL-A acceptance mapping. Do not merge without a separate request.
- [ ] Watch required CI to green on the final PR head, including all native matrix cells, full Node/Chromium/application checks, actual worklet, ownership checks, and documentation/policy gates. Inspect/download representative artifacts; verify head SHA/run attempt and fixture hashes/counts. Fix failures, rerun behavioral gates, commit, and push as needed; never weaken a gate to obtain green status.
- [ ] Check PR reviews, inline comments, and check-run feedback for automated review. Repeatedly address applicable feedback, explain justified disagreements, and request reruns/re-review until the reviewer approves. If no automated reviewer is configured or available, record that explicitly; passing CI is not reviewer approval.
- [ ] Close the acceptance/evidence checklist and update this plan. After any final commit, verify CI for that new head; record final run/review links in the PR so evidence bookkeeping does not falsely attribute an older green run to newer code. Report the delivered PR/head, remaining limitations, and whether it is unmerged.

**Verification:** LL-A1–LL-A12 have implementation/evidence references, all stages are checked only after verification, required final-head CI is green, and automated review status is explicitly recorded.

## Final gate set

Run Cargo/Python commands inside the repository Nix environment locally on this host. In CI use its provisioned environment. Use the repository-prescribed `cargo-nextest 0.9.116`; do not silently substitute a different version. Commands for the new scripts/examples become executable deliverables of their stages, not claims that they exist now.

```sh
cargo fmt --all -- --check
cargo fmt --manifest-path tests/rt_runtime_host/Cargo.toml -- --check
cargo fmt --manifest-path tests/loop_engine_host/Cargo.toml -- --check
RUSTFLAGS="-D warnings" cargo build --locked --workspace
RUSTFLAGS="-D warnings" cargo test --locked -p rt_runtime -p shoop_loop_engine
RUSTFLAGS="-D warnings" cargo test --locked -p rt_runtime -p shoop_loop_engine --release
RUSTDOCFLAGS="-D warnings" cargo doc --locked -p rt_runtime -p shoop_loop_engine --no-deps
SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --locked --workspace --features shoop_engine/app_backend --profile ci
python3 scripts/check_shoop_test_usage.py
python3 scripts/check_rt_runtime_boundary.py
python3 scripts/check_loop_engine_boundary.py
python3 -m unittest scripts.tests.test_rt_runtime_boundary scripts.tests.test_loop_engine_boundary
python3 scripts/check_wasm_test_dependencies.py --package rt_runtime --package shoop_loop_engine
python3 scripts/check_tracing_coverage.py --require-closed
python3 scripts/run_wasm_tests.py --runtime node --package rt_runtime --package shoop_loop_engine
python3 scripts/run_wasm_tests.py --runtime chrome --package rt_runtime --package shoop_loop_engine
python3 scripts/run_rt_runtime_hosts.py --verify-failures
python3 scripts/run_loop_engine_hosts.py --verify-failures
python3 scripts/run_rt_runtime_miri.py
python3 scripts/benchmark_rt_runtime.py
python3 scripts/benchmark_loop_engine.py
```

Additionally run all new public examples in debug/release, both native-only host fixtures in both profiles, helper-script tests, the complete repository Node/Chromium suites, and the existing `shoopdaloop`/`shoop_audio_worklet` Wasm builds and hosted smoke procedures documented in `src/rust/shoopdaloop/README.md`. Format standalone fixtures separately; retain applicable existing dependency/wire/application policies. Omit `SHOOP_ALLOW_MISSING_BACKENDS=1` for tests explicitly requiring real host facilities.

Evidence must identify command, source SHA, configuration, toolchain/browser versions, test counts, exit/result status, artifact provenance, and limitations. Keep tokens, credentials, and signed artifact URLs out of committed reports. Consult `.agents/info/ci-debug.md` and `.agents/info/ci-repro.md` when CI fails; read the Perfetto skill before investigating `.pftrace` evidence.
