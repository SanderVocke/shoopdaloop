# ShoopDaLoop development plan

[Architecture and decisions](ARCHITECTURE.md) · [Verification and evidence](VALIDATION.md) · [Historical documents](docs/history/README.md)

## Authority and execution

This is the single maintained planning/status document for the workstreams below.
It consolidates the plans present at `fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a`.
Superseded plans and their redirect pages have been retired from the root.
[Readable historical views](docs/history/README.md) preserve the original text with
snapshot-aware navigation; the original criteria, checklists and scope-change
records remain byte-for-byte in the archived bundle. They are not parallel live
queues.

This migration changes document organization, not approved outcomes. In particular,
it does not complete the loop goal, resume paused implementation, adopt a proposed
architecture, or turn Milestone 1's smoke-only consolidation into full acceptance.

- Goals, scope and acceptance criteria require explicit user approval to change.
- Implementation methods may change with evidence; design changes need recorded reasons.
- Keep this plan current. Mark work complete only against actual verification.
- When authorized implementation resumes, commit meaningful milestones and maintain
  affected ordinary guides/examples in the same change. Preserve unrelated work.
- Follow `AGENTS.md` and `.agents/`. On this Nix host run Cargo/Python/Trunk/test
  payloads through `nix develop --command ...`; CI uses its provisioned environment.
- The user retired the old loop execution context and will continue elsewhere;
  it will not be resumed as-is. This plan preserves unfinished requirements for a
  separately authorized continuation, not an instruction to restart that goal.
- Keep native lifecycle in normal user/native controls. A document status is not
  a native pause/resume/complete operation; native records remain historical.

## Workstreams

| Workstream | Recorded state | Remaining work / next decision |
| --- | --- | --- |
| Initial generic runtime | Delivered at its historical accepted source | Maintain through applicable regressions; do not reopen its completed initial checklist |
| [Portable loop execution](#portable-loop-execution) | **Unfinished handoff; old execution context retired by user**. Stages 0–6 and 9 verified, Stage 7 **7/8**, Stage 8 **6/8**, Stage 10 open | A separately scoped continuation must recover full current Miri coverage, verify remote artifacts and final integrated gates, then PR/CI/review/auditor; do not resume the old goal |
| [Native composition / Milestone 1](#native-composition--milestone-1) | Native DSP/service implemented; narrowed consolidation delivered | Original full final acceptance is unclaimed; further full validation is distinct from the completed merge/smoke/push request |
| Runtime book and existing loop guides | Documentation completion recorded; accepted guides remain authoritative | Maintain only affected material with future changes; no retrospective book audit or handoff queue |
| [Replacement loop book](#replacement-loop-book) | Draft opening only | User decision on continuation/adoption; not a replacement for existing guides yet |
| [Composable loop boundary](#composable-loop-boundary) | **Delivered**: `shoop_loops` offers parts for an application-owned runtime; the example DSP uses scheduled processor operations; A1–A12 met and verified locally, including a CI replay | Remote CI/review need a PR; composing several planners in one runtime remains future work |
| [Common transport / application integration](#proposed-next-work) | Proposed, not delivered | Scope a next milestone before implementation; no IPC/network or production parity claim |
| Native bus-FX switching | Bugfix delivery recorded, PR #873 | Historical result, not an open implementation queue |
| Plan-linked execution tooling | Architectural review complete; replacement executor only proposed | No extension implementation authorized by the review or this migration |

## Portable loop execution

### Goals and scope

1. Evolve `rt_runtime` to support generic planner-led interval execution, typed domain-operation delivery, prepared decision/observation storage, and resource access suitable for sample-exact looping.
2. Implement `src/rust/shoop_loops` as a portable library containing one authoritative loop/composite timeline resolver and separate audio/MIDI render operations.
3. Prove coordinated recording, playback, inter-loop triggering, and prepared composite execution in native and actual non-shared-memory AudioWorklet hosts.
4. Keep the existing authoritative documentation and examples up to date as implementation changes. Extend explanations or add chapters when new features need them, following the current documentation style. Keep implementation audits and validation evidence accurate.

Here, “RT backend changes” means the runtime and its integration with the new loop library. Switching the production `shoop_backend`, `Session`, application protocol, or production worklet to this library is **not** part of this milestone.

**Included loop capabilities:** stopped/playing/recording; immediate and timestamped start/stop/seek; sync-source/cycle-delayed transitions; lengths, positions, cycle counts, and same-frame trigger propagation; stereo and configurable prepared audio channels plus MIDI; exact-frame record-to-play; prepared content-revision replacement; gain and an explicit boundary-smoothing policy; coherent multichannel capture/snapshots; separable playback and recording around a test FX processor. Prepared composites include sequential/parallel entries, delays and repetitions, regular repeating playback, explicit-mode one-pass programs using supported modes, and finite nesting. Editing those programs is outside the render layer.

**Excluded:** complete legacy API/behavior compatibility, in-place overdub/replacing modes, prerecord/preplay and latency compensation, full dry/wet application-mode policy, track/scene/UI integration, persistence/undo/scripting execution, plugin hosting, device adapters, signal feedback graphs, parallel DSP, resampling, MIDI 2.0, unlimited recording, shared-memory acceleration, and guaranteed callback deadlines. An ordinary gain/FX test node is sufficient to prove routing; no production plugin host is required.

### Immutable loop acceptance criteria

The LL-A1–LL-A12 wording below is retained from the approved plan, not revised by consolidation.

| ID | Required result |
| --- | --- |
| LL-A1 — boundaries | `rt_runtime` remains generic, with no production/build dependency on `shoop_*`, UI, device, or browser crates. `shoop_loops` depends on the runtime, not the old engine/backend/application or browser/device facilities. Both libraries build and execute on native and `wasm32-unknown-unknown`; host bindings remain in fixtures. |
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

### Verified stages

| Original stage | State and evidence boundary |
| --- | --- |
| 0 — Branch/baseline | Verified; original baseline and branch history retained |
| 1 — Semantics/integration contract | Verified; governing behavior is consolidated in `ARCHITECTURE.md` |
| 2 — Pure resolver/reference | Verified |
| 3 — Generic planning/typed delivery | Verified |
| 4 — Indexed media/capture transactions | Verified |
| 5 — Media renderers/coordinated facade | Verified |
| 6 — Integrated lifecycle/publication | Verified, including collected **81 commands / 228 Miri executions** at its frozen source |
| 7 — Independent native/worklet hosts | **7/8**, local hosts implemented/verified; checked remote acceptance still open |
| 8 — Models/performance/CI | **6/8**, models, benchmark/reporting, callback audit, tracing and job infrastructure verified; current Miri and remote CI still open |
| 9 — Examples/implementation acceptance map | Verified; not final acceptance of every LL-A criterion |
| 10 — Final validation/delivery | **Open**; local checkpoints do not close final-source, remote, review or audit obligations |

Chronological completed checklists and their verification contracts remain in the
archived original plan. Evidence references and exact source boundaries are in
[VALIDATION.md](VALIDATION.md#loop-evidence).

### Remaining stage checks

These are the original unchecked rows, retained as an **unfinished handoff** with
stable references. Their wording and unchecked state are preserved, not issued as
instructions to resume the retired executor. A new continuation needs its own user
authorization. Dependency order remains: complete Stage 7/8 verification before
claiming Stage 10 acceptance. Some native/browser portions already pass; a row is
not closed until its entire contract is verified.

**CI scope (user decision, 2026-10-29).** CI lacks the capacity for Miri and Chrome,
so on this branch it runs neither. The Miri inventory, the Chromium Wasm suite, the
browser/AudioWorklet host runs and the Chrome application smokes remain for manual
runs. Where the rows below ask for these in CI or as CI artifacts, manual runs
recorded in [VALIDATION.md](VALIDATION.md) take their place.

**Stage 7 — independent native and real AudioWorklet hosts**

- [ ] **LL-S7-EVIDENCE:** Preserve reproducible journals, hashes, tool/browser versions, commands, and structured reports under `docs/loop_layer/evidence/` and CI artifacts. Label native-only replay as replay, never a fresh worklet run.

Verification: both native profiles, Node/Chromium and actual non-isolated
independent-memory offline/live sessions, full downstream media/journals/receipts,
fixed memory, specific failure witnesses and successful ownership recovery.
Missing browsers/artifacts, timeouts or package-Wasm compilation are not passes.

**Stage 8 — adversarial validation, performance and CI wiring**

- [ ] **LL-S8-MIRI:** Extend the pinned `scripts/run_rt_runtime_miri.py` runner with explicit package/target/filter/minimum-count records for the new runtime and loop cases, retaining its existing checks and isolation. Run selected lifetime/lease/decision-view/capture tests and multiple concurrency seeds as the consolidated applicable stage-level background selection under the execution contract above, not once per smaller step; retain full-length native stress when Miri uses documented shorter loops.
- [ ] **LL-S8-CI:** Update `.github/workflows/build_and_test.yml`: both crates/examples/native fixture replays in Linux/Windows/macOS debug/release cells, loop Wasm discovery/dependency checks, browser path filters, actual worklet/failure gates, selected Miri, release measurements, and uploaded reports. Keep existing runtime and production application gates.

Registration/wiring and local helpers exist. These rows remain unchecked because
full current interpretation and checked source-matched remote acceptance do not.
Preserve all **151 commands / 304 executions**, prior filters/seeds/order and the
**900-second per-command watchdog**; additional cases are additive, not replacements.
The final sync-component model timed out after 303 verified partial executions.
A proposed full-identity-checked topology lookup fast path has not been implemented
or accepted by this document. Any separately authorized recovery must test an
evidence-based cost fix without shortening a model, then obtain supported complete
recovery and collection.

**Stage 10 — final end-to-end validation and delivery**

- [ ] **LL-S10-GATES:** Run the complete final gate set below from the final implementation tree, including existing application/browser smoke checks. Record actual discovered/passed/skipped counts and reasons; do not copy initial-crate totals or treat unexpected skips as success.
- [ ] **LL-S10-REPLAY:** Rerun the coordinated reference/partition suite, mixed saturation/exhaustion teardown, actual offline/live worklet, failure canaries, selected Miri, and release benchmarks. Launch final Miri in the background against the frozen final implementation snapshot while other gates run; collect and verify its complete result before final acceptance. Reconcile the Miri job ledger and refresh evidence invalidated by subsequent implementation changes. Verify artifacts contain real results and full logical resource/owner recovery, not just successful command exits.
- [ ] **LL-S10-PR:** Commit each remaining meaningful milestone, affected documentation, plan progress, and implementation evidence. Push the dedicated branch and open/update the PR with scope, architectural/API changes, semantic decisions, tests, limitations, and LL-A acceptance mapping. Do not merge without a separate request.
- [ ] **LL-S10-CI:** Watch required CI to green on the final PR head, including all native matrix cells, full Node/Chromium/application checks, actual worklet, ownership checks, and documentation/policy gates. Inspect/download representative artifacts; verify head SHA/run attempt and fixture hashes/counts. Fix failures, rerun behavioral gates, commit, and push as needed; never weaken a gate to obtain green status.
- [ ] **LL-S10-REVIEW:** Check PR reviews, inline comments, and check-run feedback for automated review. Repeatedly address applicable feedback, explain justified disagreements, and request reruns/re-review until the reviewer approves. If no automated reviewer is configured or available, record that explicitly; passing CI is not reviewer approval.
- [ ] **LL-S10-CLOSE:** Close the acceptance/evidence checklist and update this plan. After any final commit, verify CI for that new head; record final run/review links in the PR so evidence bookkeeping does not falsely attribute an older green run to newer code. Report the delivered PR/head, remaining limitations, and whether it is unmerged.

Final acceptance also requires the independent completion audit against LL-A1–LL-A12
and actual final-source evidence. Recheck parent PR #878/base and branch PR state
before delivery; historical remote snapshots are not current authority. Do not push
the parent or merge either PR without separate authorization.

### Miri execution contract

Miri is an asynchronous verification lane, not a foreground editing barrier.
Default to one consolidated applicable invocation per numbered-stage milestone;
accumulate smaller edits rather than rerunning per helper/prose commit. Additional
focused runs require a concrete interpreter failure, safety regression or relevant
source/freshness change, and use the smallest adequate affected selection.

Use immutable committed snapshots, copied supervisor, isolated target/report paths,
full source/dependency/configuration/toolchain/flag and ordered inventory identity;
exclude `.pi/`, credentials and unrelated state. Keep durable process identities,
atomic terminal state and process-group cleanup. Normally one job runs at a time;
check supported status at natural checkpoints, not tight polling.

Keep full workloads, assertions, allocator/destructor guards and seeds. The new
adversarial models have no Miri-only shortening. Existing historical typed lifecycle
8192-native/64-Miri workloads remain unchanged. Do not raise the 900s watchdog,
relax safety flags, replace exact filters (`-- --exact`) or reinterpret a failed/
interrupted/diagnostic campaign as passing. Analyze profiles on copies; censored
inclusive costs are not additive/full-run/native timings.

Collection requires terminal supervisor success, every case/seed/count, passing
raw summaries, source/artifact identity and stopped recorded processes. A complete
stage run can satisfy final Miri only with exact relevant final-input equivalence;
it cannot certify remote CI or review. After relevant changes, refresh affected
coverage at the applicable milestone/final barrier. Record current evidence once
in [VALIDATION.md](VALIDATION.md#miri-evidence), not a second mutable job ledger.
Runner commands remain in the [operational reference](docs/loop_layer/MIRI_BACKGROUND.md).

### Final gate set

This is the retained execution recipe, **not a claim these commands ran during
migration**. Use the prescribed nextest **0.9.116**. The verified cache executable
is directly under `$HOME/.cache/rt-runtime-nextest-0.9.116`, not a `bin/` child.
Keep `RUSTFLAGS` and `RUSTDOCFLAGS` at `-D warnings`.

```sh
cargo fmt --all -- --check
cargo fmt --manifest-path tests/rt_runtime_host/Cargo.toml -- --check
cargo fmt --manifest-path tests/loops_host/Cargo.toml -- --check
RUSTFLAGS="-D warnings" cargo build --locked --workspace
RUSTFLAGS="-D warnings" cargo test --locked -p rt_runtime -p shoop_loops
RUSTFLAGS="-D warnings" cargo test --locked -p rt_runtime -p shoop_loops --release
RUSTDOCFLAGS="-D warnings" cargo doc --locked -p rt_runtime -p shoop_loops --no-deps
SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --locked --workspace --features shoop_engine/app_backend --profile ci
python3 scripts/check_shoop_test_usage.py
python3 scripts/check_rt_runtime_boundary.py
python3 scripts/check_loops_boundary.py
python3 -m unittest scripts.tests.test_rt_runtime_boundary scripts.tests.test_loops_boundary
python3 scripts/check_wasm_test_dependencies.py --package rt_runtime --package shoop_loops
python3 scripts/check_tracing_coverage.py --require-closed
python3 scripts/run_wasm_tests.py --runtime node --package rt_runtime --package shoop_loops
python3 scripts/run_wasm_tests.py --runtime chrome --package rt_runtime --package shoop_loops
python3 scripts/run_rt_runtime_hosts.py --verify-failures
python3 scripts/run_loops_hosts.py --verify-failures
# In an authorized supervised immutable background snapshot, then collect/verify:
python3 scripts/run_rt_runtime_miri.py
python3 scripts/benchmark_rt_runtime.py
python3 scripts/benchmark_loops.py
```

Additionally execute applicable public examples and doctests, both native-only
fixtures and both actual-host/failure profiles, helper tests, the complete Cargo-
discovered repository Node/Chromium suites (including the added DSP/service), and
application/worklet Wasm builds and hosted/direct-file smoke procedures in the
[application README](src/rust/shoopdaloop/README.md). Retain dependency/wire/
application policies and the merged service's applicable regressions. Omit the
missing-backend allowance for checks needing real facilities. Generic/loop/
package-Wasm/production application scopes remain distinct. Benchmark correctness,
guards, bounds and recovery gate success, not timing: retain all misses/outliers.
Generic/loop measurement watchdogs remain 300s/600s, separate from Miri's 900s.

### Documentation maintenance

The accepted runtime book and existing loop guides are complete for their shipped
APIs; do not reopen them as a baseline, coverage-ledger, reconciliation or
publication-signoff project. With subsequent implementation, update only affected
public examples, snippets, API prose, explanations and navigation, and run changed
examples/applicable rustdoc. New host/benchmark features need ordinary usage docs.
No handoff notes, documentation hash baselines, coverage ledgers or new management
scripts are required. `docs-previous/` is historical, not another set to synchronize.

## Native composition / Milestone 1

### Delivered scope and limits

The native `shoop_example_dsp` and `shoop_engine_service` are implemented, and the
[composable loop boundary](#composable-loop-boundary) work rebuilt them on the loop parts and
scheduled processor operations with unchanged scenario oracles. The later
user request to merge, smoke test, commit and push is delivered at source merge
`30d09bbdf4bbfd8f63b4eb58cc3e111b8e7a7a8c`. It preserved the newer runtime/loop
implementation. The old dependency wait is not an active blocker for using this
consolidated code.

Stages 0–6 were verified on the original dependency pin; focused integrated checks
passed on consolidation. The original full Stage 7 reruns, complete merged-head
CI/review and full M1-A10 acceptance were **not performed or claimed**. The later
narrow request superseded that execution workflow, not the meaning of its original
criteria. If full acceptance is requested later, scope and execute the original
broader reruns against an agreed source; do not substitute the smoke results.

The slice contains one logical stereo/MIDI loop, independent prepared DSP, native
typed request/result endpoints, fixed node-pair route changes, meters, coherent state,
startup imports, retained media selection and live immutable exports. Its D=3
probe identifies missing history/postroll/playback-window mechanisms; it does not
implement latency compensation. Excluded: production backend/UI/devices, full
session/parity, plugins, persistence/undo/scripting, general module loading/wire
protocol, new service AudioWorklet, IPC/network/shared-memory acceleration and
arbitrary live media adoption. Old backend behavior remains unchanged.

### Retained original M1 acceptance criteria

These original criteria are preserved verbatim for interpretation of historical
results and any separately authorized full acceptance work. Their presence does
not restart the superseded workflow or mark them all satisfied on the merged head.

| ID | Required result |
| --- | --- |
| M1-A1 — compatibility direction | A source/test-grounded parity map identifies major current capabilities, intended owners, reusable modules, gaps and intentional semantic differences. Detailed contracts cover this slice; architecture remains explicit about undescribed parts. |
| M1-A2 — independent composition | One runtime graph/render owner executes loop playback/capture plus DSP supplied by a separate crate. The DSP crate has no production dependency on the loop, old engine/backend, UI or browser/device modules. Loop timing remains authoritative in the existing resolver. |
| M1-A3 — typed DSP control | A non-parameter-only operation such as `SelectPreparedPreset` has a component-defined request/result, executes at an admitted requested engine frame, and reports actual terminal timing/revision. It is not encoded as magic float parameters or added to the loop command vocabulary. |
| M1-A4 — musical/media correctness | A native public-API executable imports initial content, records stereo and MIDI, finishes at an interior frame, immediately plays without an extra silent block, wraps, seeks/selects retained media, and stops with appropriate MIDI cleanup. Full samples, packets, exports and outcomes match independent expectations across valid callback partitions. |
| M1-A5 — configuration cuts | Initial graph/roles are valid before frame zero without a warm-up callback. At least one supported prepared fixed-role route change activates at a known boundary with coherent state and continued capture/playback; preset changes use their declared boundary semantics. No unsupported cross-domain atomicity is claimed. |
| M1-A6 — control-side visibility | Native meters accumulate peaks with specified read/reset semantics without reliable meter-query commands. Loop queries name a coherent resolver cut. A live prefix export retains unchanged stereo/MIDI content and initial MIDI metadata while capture continues; reads/releases retain correct ownership. |
| M1-A7 — safety and lifecycle | All new callback-reachable ingress, control application, processing, telemetry publication, results and recycling are prepared, heap/free guarded, nonblocking and free of locks/waits/I/O. Fixed capacities, reserved reliable outcomes and an explicit recovery/disconnect policy preserve progression and recover owners after stalls/refusals/faulted teardown. |
| M1-A8 — parity-risk probe | An executable fixed-delay dry/wet experiment compares physical capture samples and proposed logical-window expectations with an independent oracle and current-engine references. Report supported mappings and precisely located missing mechanisms. Findings are required; full production compensation is not. |
| M1-A9 — measurement/portability | Record raw native release callback tails, command/meter costs and live-export throughput with a comparable current-engine baseline for a declared common workload. New libraries compile for native and Wasm; portable new tests run in Node. No unmeasured parity/deadline claim is made. |
| M1-A10 — collaboration/delivery | No unapproved protected-path/API changes or borrowed fixes to the loop executor's work. Final integration is checked against its completed agreed revision, relevant regressions and CI pass, runnable usage/docs are updated, and PR review is addressed or its unavailability is explicitly reported. |

Use public loop/runtime seams, one graph and render owner, no copied resolver or
second scheduler/clock. Do not split host callbacks to renew allowances. DSP-owned
prepared requests preserve actual frame/revision and reliable ownership; meters
are independent telemetry, not multi-domain transactions. Future shared-path/API
changes need coordination with the owner; do not undo upstream work or weaken tests
to repair integration. Historical isolation/pause rules and original full rerun
recipes are retained in the archived plan, not a new requirement to wait again
before using the already consolidated source.

## Replacement loop book

`src/rust/shoop_loops/docs-new/` has a draft index, three opening chapters
and a separately manifested `first_loop` example. It is not the adopted public
reading path. Existing `src/rust/shoop_loops/README.md` and `docs/` guides
remain authoritative. There is no approved full replacement-book implementation
plan in this migration; continuation/adoption requires a user decision.

## Composable loop boundary

Commissioned by the user on 2026-10-29 and delivered on this branch. It resolved the
earlier composability question in favour of application-owned runtimes. The boundary
and its rules are in [ARCHITECTURE.md](ARCHITECTURE.md#composable-loop-boundary); the
baseline, stage evidence, CI replay and acceptance check are in
[VALIDATION.md](VALIDATION.md#composable-loop-refactor-evidence).

### Problem and goals

`shoop_loop_engine::runtime::Engine` owned the whole `GraphRuntime`, its
`ResourceStore` and its lifecycle. Every other processor had to be a single type
wrapped in the loop crate's `Node<P>`, and processor operations were typed by the
planner's operation type, so `shoop_example_dsp` carried its own private transport,
mailbox and `begin_callback`/`finish_callback` hooks around every engine call. The
loop's coordination guarantees were sound; the problem was that the loop crate owned
the runtime they ran in.

1. The loop crate is renamed `shoop_loops` and offers **parts only**: `LoopPlanner`,
   preparable on its own; `LoopNodes`, the playback/capture processors and their
   decision bindings; and `LoopClient`, typed loop commands and results for a control
   endpoint owned by the application.
2. The application owns the `GraphRuntime`, the `ResourceStore`, its node type, the
   control transport and the lifecycle.
3. Loop invariants are enforced by runtime hooks the loop planner implements, not by
   owning the runtime.
4. Processor operations are typed independently of the planner, so independent DSP
   is controlled through the runtime's own scheduler.
5. The demonstrator (`shoop_example_dsp` + `shoop_engine_service`) is rebuilt on the
   parts, and the DSP's private control bridge is removed.
6. Code docs, guides, architecture, plan and validation documents describe the new
   structure. The Milestone 1 composition spike is retired.

**In scope:** `rt_runtime` changes for goals 3–4; the rename and restructuring of
the loop crate; a dev-only test-support crate; porting every user of the loop crate
(tests, examples, benchmarks, native/Wasm/browser test hosts, service, scripts, CI);
and documentation.

**Out of scope:** composing several planners in one runtime; atomic transactions
across domains (for example a DSP preset change and a loop change as one
transaction); any change to musical or media semantics; new loop features, latency
compensation, plugin hosting, production backend or UI integration; continuing the
replacement loop book beyond keeping its existing draft compiling; and the open
checks of [portable loop execution](#portable-loop-execution), which remain open and
apply to the refactored code under its new names.

### Acceptance criteria

All met; see the [acceptance check](VALIDATION.md#composable-loop-acceptance-check).
These must not change without explicit user approval.

| ID | Required result |
| --- | --- |
| A1 — parts only | The workspace has no `shoop_loop_engine` crate. `shoop_loops` exposes no type that owns or wraps `GraphRuntime`, `executor::Control` or `ResourceStore`: `Engine`, `Controller`, `Stopped` and `Node<P>` are gone. |
| A2 — application-owned assembly | An application builds its own `GraphRuntime<AppNode, LoopPlanner>`: it creates the `ResourceStore`; `LoopPlanner` preparation borrows that store and returns `LoopNodes`; the application's own node enum holds the loop nodes next to independent processors. The service demonstrates this. |
| A3 — invariants kept | The five guarantees listed under the [composable loop boundary](ARCHITECTURE.md#composable-loop-boundary) still hold, each with a test that fails if its guard is removed. These include: a generic `Commit` of a graph missing a loop node or decision binding is refused; a generic `Install`/`Remove` that would replace or drop a loop node is refused; paired program/graph publication; node-pair reassignment; capture preflight before affected samples; recovery, cancellation and stopped-state reclamation. |
| A4 — independent processor operations | `Processor` typed operations and results no longer use the planner's operation type. A processor crate can define its own operation/result types while depending only on `rt_runtime`. |
| A5 — DSP through the scheduler | `shoop_example_dsp` has no private transport, no mailbox and no `begin_callback`/`finish_callback`. Preset selection is a scheduled runtime processor operation. Its observable behaviour is kept: it applies at the exact sample; its result reports the actual frame and revision; stale revisions, unknown presets and late requests are refused; it can be cancelled before delivery. |
| A6 — demonstrator unchanged in behaviour | `shoop_engine_service` makes exactly one runtime `process` call per nonempty callback, with no per-component hooks. Its tests and examples pass with unchanged oracles: `one_track` 42 stereo frames / 487 MIDI packets / 378 bytes in every partition run; `latency_probe` all 18 cases; stall, disconnect, fault and two-thread tests. |
| A7 — vocabulary | "Role" no longer appears in `shoop_loops`' public API or docs. `Role*` becomes `NodePair*`, and the node factory is `LoopNodes` producing `PlaybackNode`/`CaptureNode`. |
| A8 — nothing lost | Every existing loop, runtime, service and host test passes after porting, as do the examples, including native debug and release, Wasm builds, Node tests and browser/AudioWorklet hosts with `--verify-failures`. Oracles change only mechanically. Any removed test is replaced by one that covers the same behaviour. |
| A9 — dev-only assembly | The only turnkey assembly lives in `shoop_loops_test_support` (`publish = false`). It is used only as a dev-dependency, which `cargo tree` confirms. Teaching examples and guides assemble from the parts. |
| A10 — boundaries and safety | `shoop_loops` and `shoop_example_dsp` keep their dependency boundary (the boundary script passes); `forbid(unsafe_code)` is kept; all new or changed render paths are heap/free guarded in tests; the test-usage and tracing-coverage checks pass. |
| A11 — documentation | `ARCHITECTURE.md` describes the new boundary, and the open decision is closed. `PLAN.md` describes this work and updates the affected workstreams. `VALIDATION.md` records the evidence. All current READMEs, guides and runtime book chapters use the new API, and their code samples compile and run. No current (non-historical) document or script refers to `shoop_loop_engine`, `Engine::prepare` or roles. |
| A12 — measurements recorded | `measure_service` (idle/steady) and the loop benchmark are run before and after on the same machine, and the results are recorded in `VALIDATION.md`. Regressions are reported with an explanation; they do not block completion. |

### Delivery

Delivered in stages 0–8: baseline (0); spike retirement, crate rename and
`NodePair` vocabulary (1); independent processor operations (2) and planner admission
hooks (3) in `rt_runtime`; `shoop_loops` as parts, with the dev-only test-support
crate (4); the test hosts (5); the rebuilt demonstrator (6); documents (7); and
end-to-end validation (8), repeated with CI's own tool pins. Revisions made during
execution:

- A planner's own activations are checked by that planner, which holds the
  activation permit; the runtime cannot call back into a planner mid-operation.
  `Install`/`Remove` are admitted per node.
- The service and the loop test host were ported together with the loop parts so
  that the tree kept building.
- The loop test host is a library and cannot take a dev-dependency, so it compiles
  the support crate's assembly source by path.

### Follow-ups outside this work

None of these is part of the plan as approved; each needs its own decision.

- Pull request, remote CI and review: the user chose no pull request stage. The
  macOS and Windows jobs need GitHub's runners and were not run locally.
- CI runs neither Miri nor Chrome; both are manual (see the
  [CI scope decision](#remaining-stage-checks)). Both ran in the CI replay. With the
  unmodified runner, six Miri commands exceed the 900 s per-command watchdog on the
  reference machine, equally on the baseline.
- Planner composition and cross-domain transactions remain
  [future work](ARCHITECTURE.md#composable-loop-boundary).
- The draft loop book's chapters were updated to the new API; continuing the book is
  a separate decision.

## Proposed next work

These are design directions, not authorized implementation checkboxes:

1. Resolved by the [composable loop boundary](#composable-loop-boundary) work: see [ARCHITECTURE.md](ARCHITECTURE.md#composable-loop-boundary). When a second planning domain is needed, compose planners explicitly rather than nesting runtimes.
2. Define a bounded next service/protocol slice: one logical scenario through typed native calls, independent-memory AudioWorklet and native IPC. No three unrelated demos or premature universal protocol.
3. After that foundation, scope an opt-in real application backend using existing UI/modules and the old backend as a selectable fallback/reference. Backend selection belongs at a lifecycle boundary, not per-operation delegation between two live engines.
4. Close parity gaps driven by real workflows, then consider broader deployment. Full dry/wet, prerecord/preplay/latency windows, production plugins/devices, persistence/undo/scripting and network operation remain future work. The [loop features assessment](LOOP_FEATURES_ASSESSMENT.md) estimates the loop-library gaps and the decisions they need.
5. A plan-linked execution extension is only a reviewed tooling proposal. Do not implement/configure one or create a second task authority under this documentation migration.

## Native execution records

The former goals are [historical execution context](docs/history/README.md#retired-execution-context),
not another current checklist or a dependency on resuming the old executor.
Their root plan paths are intentionally removed. Native `.pi/` storage and
lifecycle remain managed by normal user/native controls; no raw records are
manually edited or declared complete by this documentation cleanup.
