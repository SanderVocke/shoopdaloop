# Loop-layer implementation and validation evidence

## Scope and provenance

`PLAN_LOOP_LAYER.md` governs this implementation. Its LL-A1–LL-A12 criteria are immutable without user approval. The initial runtime record in `DONE_INITIAL_CRATE.md` and `docs/rt_runtime/evidence/` is preserved; passing baseline tests is not evidence that new loop-layer behavior exists.

Execution was authorized through confirmed goal `mvnjf70y-cjcfar`. The implementation branch is `feat/loop-execution-layer`, based on `cc6ccb4ac35114d61ef7e1c03ecc7f2fe81bb867`. At branch creation, [runtime PR #878](https://github.com/SanderVocke/shoopdaloop/pull/878) remained open against `master`, with that same head. The delivery PR should initially target `feat/generic-rt-runtime` unless the parent merges before delivery; recheck rather than assuming its future status.

The pre-existing edits to `DESIGN_RT_ENGINE.md`, `PROPOSAL_RT_ENGINE.md`, and `PROPOSAL_LOOP_LAYER.md`, plus the new plan, are the intended architecture baseline. Unrelated `.pi/` state is left untouched and untracked. No production application integration is included.

## Acceptance-to-evidence map

These are required test/evidence groups, not claims of implementation. Add concrete test names and artifacts as stages complete.

| Criterion | Planned implementation/evidence | Status |
| --- | --- | --- |
| LL-A1 | Four-target dependency audits; both crate builds; native and Node/Chromium execution; host-only browser bindings | Compiler milestone: both boundary audits pass; new crate executes natively and in Node/Chromium. Full integration/CI pending |
| LL-A2 | Generic synthetic planner dependency/view tests; resolver ownership tests; graph receiver validation | Pure resolver and generic runtime binding/view/interval/typed-delivery lifecycle verified; loop integration pending |
| LL-A3 | Same-frame A → B → C and nested cascades; one-sample loops exceeding old subdivision allowances | Pure primitive/composite execution verified; media/runtime/host proof pending |
| LL-A4 | Semantic progress proof; valid mutual sync; rejected instantaneous recursion; generated valid/invalid programs | Stage 2 compiler/resolver proof and tests pass; integrated audit pending |
| LL-A5 | Independent reference journals/audio/MIDI across callback partitions and declaration orders; exact end-boundary cases | Pure semantic journals/logical decisions match; audio/MIDI/host comparisons pending |
| LL-A6 | Debug/release/Wasm allocation/reallocation/deallocation canaries and guarded callback scopes; callback audit | Pending new paths; original runtime baseline passes |
| LL-A7 | Owning return/cancellation/generation/saturation/disconnect/teardown models; selected Miri and concurrency tests | Generic typed receipts/recovery and selected Miri verified; integrated domain ownership pending |
| LL-A8 | Stereo/MIDI record-to-play, seek/replacement/cleanup/export; FX capture graph; common-prefix exhaustion | Pending |
| LL-A9 | Integrated prepared composites, primitive/composite interactions, and rendering-observation boundary tests | Pending |
| LL-A10 | Independent render/maintenance accounting, indexed access, capture reservations, counter/trace exhaustion, pool recovery | Pending |
| LL-A11 | New independent native/worklet fixture journals, actual Web Audio output, fixed-memory and failure gates; raw release measurements | Pending new fixture |
| LL-A12 | Updated runtime/loop guides and executable examples, acceptance audit, milestone commits, final-head CI and review record | Pending |

## Stage 0 — Verified pre-implementation baseline

Milestone: `6ad71196b63e61956610ec2a71ecc48369d7a040` — `docs(loop-layer): establish execution plan and verified baseline`. No runtime, processor, resource, browser-host, or test behavior was changed in this milestone.

### Environment

Local Linux x86_64, repository `nix develop` environment:

- Rust/Cargo 1.97.1; installed targets `x86_64-unknown-linux-gnu` and `wasm32-unknown-unknown`.
- Prescribed cargo-nextest **0.9.116**, from `$HOME/.cache/rt-runtime-nextest-0.9.116`; the shell default 0.9.143 was not used for the suite.
- Node **22.22.2**, wasm-pack **0.15.0**, wasm-bindgen **0.2.127**, wasm-bindgen-test **0.3.77**.
- Chromium and ChromeDriver **147.0.7727.137**, with `CHROME_BIN` explicitly set.
- `RUSTFLAGS="-D warnings"` set for the baseline runner and child builds.

Exact environment overrides, commands, durations, exit statuses, log hashes, and package counts are in [`evidence/baseline-stage0.json`](evidence/baseline-stage0.json). Raw logs remain under `target/loop-layer-baseline/`. The runner was invoked as `nix develop --command python3 /tmp/loop-layer-baseline.py`; its individual commands are recorded in the report so the temporary orchestration file is not required for reproduction.

### Results

| Gate | Observed result |
| --- | --- |
| Main workspace and independent-host formatting | Passed |
| `cargo test --locked -p rt_runtime` | 93 passed, 0 failed/ignored |
| Same runtime tests, release | 93 passed, 0 failed/ignored |
| Warning-denied locked workspace build | Passed |
| Prescribed nextest full workspace, `shoop_engine/app_backend`, CI profile | 1893 passed, 4 existing ignored tests |
| Shared test attribute policy | Passed |
| Runtime production/build boundary, Linux/Windows/macOS/Wasm target trees | Passed |
| Boundary-audit Python tests | 2 passed |
| Runtime Wasm-test dependency isolation | Passed |
| Closed tracing inventory | Passed, 169 modules classified |
| Native host offline and two-thread replay, debug and release | Passed; exact response/sample checks |
| Fresh non-isolated offline/live AudioWorklet plus native replay | Passed |
| Deliberate processor and Rust heap worklet failure gates | Both detected, each child exited 1 as required |
| Fresh runtime Node and Chromium package suites | 89 passed each, 0 failed/ignored |

The four full-workspace ignored tests were enumerated from the nextest JSON test list: `intentional_failure_publishes_trace`, `native_result_failure_canary`, `shared_failure_canary_is_ignored_by_default`, and `shared_result_failure_canary_is_ignored_by_default`. They are existing opt-in failure canaries, not newly skipped loop/runtime coverage. Their binary IDs are retained in the report.

The Wasm package suites reused the runner's input-hash-validated application asset bundle; its metadata still records its earlier build SHA. The package tests were freshly executed, but this stage does not claim a fresh production-application browser smoke. The independent runtime worklet was separately built and executed.

### Actual worklet and native agreement

[`evidence/hosts-stage0.json`](evidence/hosts-stage0.json) records a fresh actual browser run:

- Offline: **2601 callbacks / 332928 frames**; sample hash **480331717**, MIDI hash **3308430490**, graph hash **487351092**, **34 MIDI events**. Both native offline and two-thread replays agree.
- Live: **13444 callbacks / 1720832 frames**, with acknowledged-time checks. Live message timing is not equated with deterministic offline timing.
- Independent instances and `cross_origin_isolated: false`; memory starts and ends at **1507328 bytes** with per-callback growth checks.
- Observed core work maxima: **1042 copied bytes / 512 validation steps** under the current runtime contract.
- Five withheld transfer credits, continued rendering for 2048 frames, duplicate/oversized refusal, delayed delivery, successful resource/ownership disposal, and both intentional failure sessions were exercised by the fixture.

The fresh uncompressed journal SHA-256 is `fb8ecf1fae2cb7ac7246bbb836d2ed7670f9d1341b8c005ab3d2565f07ee6237`, matching the initial golden journal. Neither the golden artifact nor historical evidence was replaced. [`native-debug-stage0.json`](evidence/native-debug-stage0.json) and [`native-release-stage0.json`](evidence/native-release-stage0.json) separately identify native-only replay as replay, not browser execution.

### Limits of this evidence

This establishes a clean local regression baseline for the inspected source, not the new loop-layer acceptance criteria. Windows/macOS execution, new loop-library behavior, new Miri cases, updated documentation/examples, and final PR CI remain future stage requirements. No browser/device hard-real-time or performance-improvement claim follows from these results.

## Stage 1 — Semantics and integration contract

Milestone: `7dfd657c7b908d79d6a95e6cb9e2c66c6a5bcb3b` — `docs(loop-layer): specify timing semantics and planner ownership contract`.

- [`SEMANTICS.md`](SEMANTICS.md) specifies primitive/composite state, exact boundary ownership, typed request outcomes/cancellation, ready-versus-clock-reactive arbitration, pulse coalescing, source-stop behavior, seeking/replacement, media/MIDI discontinuities, and legacy behavior mapping.
- Its termination model accepts pure sync strongly connected components, rejects internal state-changing action feedback/recursive containment, and walks the resulting component DAG. Clock delivery and pending eligibility have finite prepared identities, not a wave quota. Pre-clock and reactive phases distinguish suppression of an as-yet uncreated clock edge from attempted retraction of a delivered one.
- The normative boundary table states expected results for record-to-play, cascades, callback ends, dense wraps, valid/idle mutual sync, rejected instantaneous feedback, source stop, direct conflicts, reactive stops, countdowns, nesting, capture failure, source stalls, and MIDI seeks. These expectations were reviewed against the stated phase/progress rules; executable production/reference conformance is still required in Stage 2 onward.
- [`RUNTIME_INTEGRATION.md`](RUNTIME_INTEGRATION.md) specifies generic planner types and borrow phases, explicit graph bindings, one callback admission lifecycle, retained pending receipts, recovery-safe cancellation, prepared program staging, separate rendering/maintenance accounting, indexed resources, exclusive capture claims, group reservation/preflight, coherent state, and the native/browser adapter boundary.
- Resource preflight uses unpublished shadow state and provisional reservations. A failed preview must latch a new finite group/activation failure before retrying; no externally visible action or generation is consumed twice. The failure table distinguishes ordinary refusal, incomplete capture, invariant/programming faults, and consumer stalls.
- A recording may explicitly monitor a retained immutable take while capturing a distinct revision, allowing playback → FX → recording without an artificial signal cycle or in-place overdub. Both playback and capture positions are resolver-owned and tied to the same absolute interval.
- `DESIGN_RT_ENGINE.md` and `docs/rt_runtime/CONTRACTS.md` now describe these as specified-but-unimplemented extensions. Initial runtime contracts/evidence remain labeled historical. No LL-A goal or acceptance criterion was weakened.

### Compiled type/lifetime experiment

[`prototypes/interval_lifecycle.rs`](prototypes/interval_lifecycle.rs) is a safe-Rust non-musical prototype, not production runtime code. [`evidence/contracts-stage1.json`](evidence/contracts-stage1.json) records source hash, commands, outcomes, log hashes, and limitations.

Native debug and optimized runs each pass six assertion groups: a single callback with a decision boundary; irregular callbacks with reversed consumer bindings; 128 one-frame callbacks; 128 internal intervals in one callback; invalid binding refusal; and observation writer exhaustion preserving the already written value. Samples and consumed-observation totals match the independent scalar expected values. Callback admission counters advance only on nonempty host callbacks, not per interval.

The deliberately invalid live-view-reuse configuration fails with `E0502` (`cannot borrow planner as mutable because it is also borrowed as immutable`). Ordinary native and Wasm builds pass with warnings denied. The Wasm binary was compiled, not executed; no AudioWorklet claim is made for this prototype. It has no allocator instrumentation and does not prove real resource transactions, graph validation, scheduler receipts, or musical behavior.

Workspace formatting, the warning-denied locked workspace build, and the shared test-attribute policy pass. Full production/reference/heap/native-browser integration evidence remains pending as shown in the acceptance map; Stage 1 is a verified contract/prototype milestone, not completion of any end-to-end loop feature.

## Stage 2 — Compiler milestone (stage remains in progress)

Milestone: `3a764206bf7f72609fabefb6e257f2c559e6d88b` — `feat(loop-layer): add portable crate and prepared timing compiler`. Evidence: [`evidence/compiler-stage2.json`](evidence/compiler-stage2.json). Source hashes identify the tested implementation on top of the Stage 1 commit; raw logs are under `target/loop-layer-compiler/`.

Implemented interfaces in `src/rust/shoop_loop_engine`:

- Distinct generation-checked `LoopId`/`RequestId`, compact errors, checked `Limits`, workspace lint/test conventions, and Wasm-test metadata. Production dependencies contain only `rt_runtime` and its generic transitive dependency; shared test support is dev-only.
- `TopologyDescription` → `PreparedTopology`: stable identities, target validation, canonical dependency ordering, iterative strongly connected component analysis, accepted pure synchronization cycles, rejected action-feedback components, and rejected recursive composite containment.
- `ProgramDescription` → sparse `CompiledProgram`: sequential sections, parallel/delayed/repeated entries, child-first nested duration derivation, separate regular/explicit-mode classification, same-mode continuation normalization, half-open ranges, deterministic overlap selection, and binary-search lookup without scanning event history or allocating per tick.
- A separate loop dependency audit and Python tests. The original runtime audit is unchanged and still rejects the loop crate as a runtime dependency. Five new source modules have explicit tracing-inventory classifications; no application tracing dependency was added.

Verification in the repository Nix environment:

| Gate | Observed result |
| --- | --- |
| New crate, native debug/release | 19 tests passed per profile; no failures/ignored tests |
| New crate, Node/Chromium | 19 tests passed per runtime; no failures/ignored tests |
| Independent graph reachability oracle | 4096 generated cases; accepted component membership/order and invalid action feedback checked |
| Independent dense program table | 1024 generated programs; sparse lookup matches every represented tick/target |
| Deep topology | 2048-node action chain; iterative compilation, not a wave-limited executor |
| Long sparse program | One segment represents one billion ticks; guarded immutable lookup |
| Shared heap guard | Positive allocation/deallocation canaries pass; value validation and immutable lookup are heap-free |
| Full prescribed nextest workspace suite | 1912 passed; same four existing opt-in failure canaries skipped |
| Both four-target production/build boundary audits | Passed; target trees are metadata checks, not Windows/macOS execution |
| Python boundary-policy tests | 5 passed |
| Wasm test-dependency policy, Rust test policy, formatting, warning-denied workspace build | Passed |
| Closed tracing inventory | Passed; 174 modules classified |

These tests validate compilation and lookup only. They do **not** establish resolver interval/partition invariance, callback cascade execution, reliable pending requests, resource transactions, or loop AudioWorklet behavior. Compiler preparation is allowed to allocate; the guard does not falsely impose callback constraints on it. At this compiler milestone, Stage 2 remained open for the resolver, fixed scratch/state, synthetic observations, independent musical simulator, and guarded rejection/replacement paths.

## Stage 2 — Primitive resolver milestone (stage remains in progress)

Milestone: `9f8c60f5d7896a155a45dff70da154434ddd9267` — `feat(loop-layer): resolve primitive timelines with transactional previews`. Evidence: [`evidence/primitive-stage2.json`](evidence/primitive-stage2.json). Exact commands, source/log hashes, counts, runtime summaries, and environment overrides are retained there; raw logs are under `target/loop-layer-primitive/`.

### Implemented interfaces and ownership

- `command`, `state`, `decision`, and `resolver` provide typed primitive operations, fixed pending/terminal records, immutable interval decisions, synthetic boundary observations, and authoritative logical positions.
- `Resolver::preview` copies into prepared shadow storage and settles the boundary without publishing results, state, or pulses. `commit` swaps the prepared storage; `finish` advances only the selected nonempty interval. Invalidated/failed previews cannot be committed or expose valid decision views. Resource preflight integration is still pending; no retry work quota was introduced.
- Prepared sync components are visited in canonical order. Each seeded member delivers its clock once; pending countdowns consume that edge once. Direct intent precedence, pre-clock stop masks, reactive starts/stops, automatic wraps, and same-frame trigger cascades share this algorithm. Extra host cuts do not seed clocks.
- Pending and unread terminal records retain their slots. Cancellation and result recycling check generations; ordinary result stalls do not stop automatic progression. These are domain bookkeeping records, not a second owning transport. Runtime receipt/recovery-lane integration remains pending.
- Topology replacement checks the expected epoch, preserves retained logical states, rejects active removal, revalidates pending targets/sources, and returns the old owner. Per-slot generation high-water marks prevent resurrection after removal. Returned/rejected descriptors are destroyed outside the guarded scope.
- Storage sizing includes shadow states, receipts, decisions, pulse/eligibility marks, and both journals. Preparation initializes/touches storage; processing does not resize it. Journal overflow is saturating diagnostic loss, not a musical-work refusal.

Three primitive edge cases are now explicit in `SEMANTICS.md`: supplied prepared initial metadata may establish a stopped loop's length; a fresh recording emits one start edge but no periodic recording clock; an empty finish-to-play fails transactionally without freezing, so the caller can continue recording or explicitly stop. The latter preserves atomic precondition failure and avoids fabricating a zero-duration playing state. Logical capture serials retain a high-water mark when an older content revision is selected. These clarify the specified behavior without changing scope or LL-A criteria.

### Verification and limits

| Gate | Observed result |
| --- | --- |
| Loop crate native debug/release | 36 tests per profile: 19 compiler + 17 resolver; no failures/ignored tests |
| Existing runtime native debug/release | 93 tests per profile; no failures/ignored tests |
| Loop crate Node/Chromium | 36 tests per runtime; no failures/ignored tests |
| Independent single-sample primitive model | 256 generated scripts × 32 requests × 64 frames; complete journals, per-sample logical decisions, every terminal outcome, and final state compared across three partition/order profiles |
| Same-frame execution | 256-node cascade under heap guards; direct/reactive conflict, valid/idle mutual sync, and source-stop tests |
| Dense progression | One-sample loops over 128 frames; exact callback-end treatment and irregular/one-frame partition agreement |
| Failure/ownership paths | Repeated unpublished preview/refusal, duplicate-observation rejection, cancelled/stalled receipts, stale identities, owner-returning replacement, empty finish, maximum countdown, frame/revision exhaustion |
| Optional diagnostic storage | Disabled journal gives the same logical samples/state; diagnostic loss accounts for omitted events |
| Full prescribed nextest workspace suite | 1929 passed; four unchanged existing opt-in canaries skipped |
| Formatting, warning-denied workspace build/rustdoc, both dependency audits, five Python policy tests, Wasm/test/tracing policies | Passed; tracing inventory now covers 178 modules |

Allocator guards cover each production resolver operation, including submission, preview, commit, finish, cancellation, recycling, and replacement/refusal, in native debug/release and Wasm. Preparation and the allocating test oracle/journal collection are deliberately outside those scopes. This is not yet a whole AudioWorklet callback safety claim.

The independent reference advances one sample at a time and does not call the production resolver, component compiler, or interval selector. Its generated execution coverage is currently **single-primitive**; multi-loop timing has explicit expected-case and partition tests, not a completed generated multi-loop/composite simulator.

Logical recording tests prove positions and observation timing, **not captured audio/MIDI or actual resource transactions**. At this primitive milestone, composite descriptions compiled but were explicitly rejected by the interim resolver. Composite execution/reference tests, required render effects and media binding, generic planner/receipt integration, Miri, actual hosts, and final CI were still pending. No further Stage 2 checklist item or end-to-end LL-A criterion was marked complete on that partial milestone.

## Stage 2 — Complete pure primitive/composite resolver

Milestone: `feat(loop-layer): execute prepared composites and verify pure resolver` (the commit introducing this section). Evidence: [`evidence/resolver-stage2.json`](evidence/resolver-stage2.json); raw logs: `target/loop-layer-stage2/`. The report identifies the Stage 2 primitive base commit and hashes every tested Rust source, including both independent simulation implementations.

### Implemented composite contract

`State::composite` contains iteration, pass count, and iteration-entry frame. Actual clock delivery remains separately recorded in `last_pulse`; composites never advance a synthetic sample playhead or mint periodic clocks from nominal units. `PlayIteration` and `SeekIteration` select prepared snapshots without replaying earlier events. Non-retriggering seeks reposition already-playing descendants without inventing new clock edges; fresh activations retain their normal start edge.

The same component walk now executes regular repeating and explicit-mode one-pass programs, sequential/parallel/delayed/repeated entries, finite nesting, authoritative start/seek/stop, empty children, and primitive/composite interactions. Snapshot intents, explicit actions, inherited playback, direct requests, and natural progression use one precedence model. Program/cleanup/OnPulse ordinal ranges are disjoint; declaration order is irrelevant. Adjacent desired-mode continuations do not restart media at each clock tick or regular pass boundary.

Nested authoritative snapshots carry the originating request for atomic preview refusal. Ordinary OnPulse actions and natural program advancement do not incorrectly claim a coincident request as their transaction origin. Actual automatic resource refusal remains a Stage 4 integration requirement, not something these logical tests pretend to execute.

The contract now explicitly distinguishes iteration entry from actual clock time, and clarifies that stopped loops may relay incoming clocks while a genuinely idle source stalls its followers. This reconciles the separate clock/media rules rather than inventing a nominal fallback timer.

Stopped program replacement resets prepared nominal metadata/cursors and returns the displaced owner. A changed running program is safely refused without state/ownership loss. **Stage 6 still must stage running replacements for the next iteration-zero boundary**; that integrated lifecycle requirement is not marked complete here.

### Progress and storage argument

Preparation bounds node slots, relations, program entries/segments, pending records, decision state, and scratch. The compiler rejects action edges inside strongly connected timing components and recursive containment, but accepts pure-sync components containing primitives or composites. Inter-component action order is acyclic.

At a boundary, each seeded member's delivered bit changes at most once; each pending request consumes its single prepared source edge at most once. Composite target snapshots/differences are streamed once to downstream components. Preview retries publish nothing until committed. Every selected interval is nonempty and advances toward its finite ceiling. Thus dense work is finite and application-dependent, without an arbitrary wave/subrange counter or required callback-sized transition history. Optional journal loss cannot influence arbitration or state.

### Verified gates

| Gate | Observed result |
| --- | --- |
| Loop crate native debug/release | **53 passed per profile**: 19 compiler, 17 primitive, 17 composite tests; no failures/ignored tests |
| Existing runtime native debug/release | **93 passed per profile** |
| Loop crate Node/Chromium | **53 passed per runtime**; no failures/ignored tests |
| Independent primitive single-sample simulation | 256 generated scripts × 32 requests × 64 frames; three partition/order profiles |
| Independent composite dense simulation | 256 generated regular/explicit programs × 64 frames; full absolute journals, every logical media sample, and iteration/pass/entry-frame state match across three partition/order profiles |
| Compiler references retained | 4096 independent reachability cases and 1024 sparse-versus-dense lookup cases |
| Dense/deep actual execution | 65 nested program levels × 128 one-sample ticks, reversed declarations, `[128]` versus `[1 × 128]`; existing 256-node trigger cascade also passes |
| Indexed extremes | A `u64::MAX`-tick sparse program starts near its end, seeks its child offset, and wraps without overflow or history replay |
| Failure/replacement guards | Atomic descendant preview refusal; stale/removed identities and epochs; stopped replacement; running replacement refusal returns its owner |
| Diagnostic saturation | Nested execution with one retained journal event per boundary preserves complete logical output/state |
| Full prescribed nextest workspace suite | **1946 passed**, four unchanged existing opt-in canaries skipped |
| Formatting, warning-denied build/rustdoc, dependency/test/tracing policies | Passed; both four-target boundary audits and five Python policy tests pass |

The independent composite oracle interprets raw sections into a dense desired-state table and advances all clock/media state one sample at a time; it calls no production compiler, resolver, or sparse lookup. Generated references cover flat regular/explicit programs. Nested seeks, source stalls, valid cyclic sync, conflict priorities, and mixed composite/primitive cascades additionally have explicit expected cases and full-journal partition/declaration comparisons. Integrated randomized resource/media/host cases remain required in later stages.

All Stage 2 checklist items are now verified. This completes the **pure logical layer only**: no generic runtime planner API, real media renderer, resource transaction, Miri extension, actual loop AudioWorklet fixture, production migration, or final-head CI is claimed. The initial runtime and historical evidence remain unchanged.

## Stage 3 — Planning metadata milestone (stage remains in progress)

Milestone: `feat(rt-runtime): validate explicit planner dependencies` (the commit introducing this section). Evidence: [`evidence/graph-stage3.json`](evidence/graph-stage3.json), with the initial workspace failure, investigation, and fresh host reports in [`evidence/graph-stage3-provenance.json`](evidence/graph-stage3-provenance.json). Source hashes identify the tested implementation on top of `e66a00b7124de4b90e201aa6ec01925ffe7787c7`; raw logs are under `target/loop-layer-graph-stage3/`.

### Implemented structure

- Generic `PlannerId` and planner/consumer signatures declare decision slots, observation storage, and payload limits without any loop-domain dependency.
- `Catalog::with_planner` and `compile_planned` handle explicit bindings independently of the signal DAG. Several consumers may share one decision slot; required consumers need exactly one binding. Bindings are sorted by consumer identity, not incidental signal traversal order.
- `PreparedPlan` owns a separate `PlanningDescriptor`. Keeping `PlanDescriptor` signal-only avoids an unnecessary legacy fixture wire-format change. Reusable staging preallocates/touches binding storage and clears it without allocation or deallocation.
- Incremental receiver validation checks identities/generations, complete planner signatures, graph membership, unique/canonical bindings, slot bounds, and aggregate observation reservations. The single planning step is required before signal step zero; backward planning dependencies are invalid rather than simulated with signal edges. Existing signal DAG cycle/order validation remains independent.
- Signal-only catalogs reject unsolicited planning metadata. No-planner compilation keeps its existing path; the existing host journal and golden evidence are unchanged.

Eight new shared `planning_graph` tests cover canonicalization, validation-budget partition invariance, malformed/stale/missing/duplicate bindings, prefix/signature mismatch, aggregate observation storage, absent graph members, empty clock-only metadata, legacy compatibility, and guarded staged reuse/refusal. Validation and staged edits run under allocation/reallocation/deallocation guards; allocating control-side compilation is intentionally outside them.

### Verification and retained failure

| Gate | Observed result |
| --- | --- |
| Runtime native debug/release | **101 passed per profile**, including eight new planning-metadata tests |
| Loop crate native debug/release | **53 passed per profile** |
| Runtime Node/Chromium | **97 passed per runtime** |
| Loop crate Node/Chromium | **53 passed per runtime** |
| Full prescribed nextest workspace rerun | **1954 passed**, four unchanged opt-in canaries skipped |
| Warning-denied workspace build/rustdoc, workspace/host formatting | Passed |
| Both four-target boundary audits, five Python boundary tests, shared test/Wasm/tracing policies | Passed |
| Existing native fixture, debug/release offline and two-thread replay | Passed |
| Fresh existing-runtime offline/live non-isolated AudioWorklet | Passed; fixed **1507328-byte** memory, native/browser offline agreement |
| Processor and Rust heap worklet failure canaries | Both detected with expected child exit 1 |

The **first** full workspace attempt had 1953 passes and one failure: unchanged JACK test `external_midi_fanout_respects_each_tracks_passthrough_mute` failed connecting `monitored_dry_midi_send` to `monitored` with error 22. The error message says a source or destination may not belong to an active client. Twenty isolated reruns passed without code changes or a missing-backend allowance; the subsequent unchanged full suite passed. The engine normal/build dependency tree contains neither new library. These facts do **not** establish the failure's cause or claim a fix. The original failure and all repeat log hashes remain in the provenance report; no test, timeout, or gate was weakened.

Fresh host regression evidence retains **2601 offline callbacks / 332928 frames**, sample/MIDI/graph hashes **480331717 / 3308430490 / 487351092**, and **34 MIDI events**. The raw journal matches the historical golden byte-for-byte. This proves compatibility with the **existing runtime fixture**, not a new loop AudioWorklet. Package Wasm tests also remain distinct from actual worklet execution.

The current executor still constructs signal-only catalogs. Planner views/observations, typed operations and pending runtime receipts, callback-wide accounting with internal intervals, integrated activation/retirement tests, and selected new Miri cases remain outstanding. **Stage 3 is not complete**, and no end-to-end LL-A criterion is claimed from this structural milestone.

## Stage 3 — Planner views and interval lifecycle milestone (stage remains in progress)

Milestone: `feat(rt-runtime): execute prepared planner intervals` (the commit introducing this section). Evidence: [`evidence/interval-stage3.json`](evidence/interval-stage3.json) and [`evidence/interval-stage3-provenance.json`](evidence/interval-stage3-provenance.json); raw logs: `target/loop-layer-interval-stage3/`. The report hashes tested Rust sources on top of `f26fd217` and records exact commands, profiles, counts, and environment overrides.

### Implemented lifecycle and compatibility

- `Planner`, `NoPlanner`, `PlanContext`, `FinishContext`, `DecisionView`, and `ObservationWriter` implement the production safe-Rust borrow phases. `GraphRuntime`, `Processor`, `PrepareProcessor`, `PreparedNode`, `Command`, and `Control` carry a defaulted planner type; existing ordinary clients retain the no-planner path.
- Preparation checks fixed decision dimensions and associated element-size arithmetic, initializes/touches observation storage, and captures processor planning signatures. Replacement must preserve both ports and planning access. The planner owns its prepared decision/state storage; consumers borrow only their validated immutable slot.
- One callback snapshots ingress/recycles results and resets accounting. Its internal loop delivers due external actions, asks the planner for a positive interval no later than the next action/host end, traverses the signal DAG, and finishes observations. Empty signal graphs can still advance a configured planner. Zero-frame callbacks do nothing; end-boundary actions settle in the next nonempty callback.
- Observation writers stamp producer identity and enforce each producer's declared allowance. Refusal preserves prior observations and another producer's reserved space. Each interval resets only the written length; stale values are not exposed or freed.
- Invalid intervals and changed decision-slice length are explicit render errors before consumers run. Stale planner candidates return their owner while the prior graph/bindings keep rendering. Programming errors retain the existing output-clearing/advanced-outer-clock contract, not rollback.
- External event/result capacity is now `min(scheduled_events, budgets.events)`. `Budgets::subranges` remains source-compatible but is ignored, including zero. This intentional migration removes the old coupling between external admission and musical interval counts. Existing event/result reservations and half-open timing remain intact.
- Traversal/event/MIDI-overflow diagnostics saturate with `counter_overflow`. Configuration no longer refuses otherwise representable storage based only on a worst-case diagnostic work estimate. Frame/identity arithmetic remains checked and semantic.

The new standalone crate guide `src/rust/rt_runtime/docs/planning.md` and `planned_gain` example describe the actual API and remaining limits. Historical initial scheduler evidence remains labeled historical; live scheduling/capacity/integration guides describe the new behavior.

### Verification

| Gate | Observed result |
| --- | --- |
| Runtime native debug/release | **109 passed per profile**: seven new executor tests and one diagnostic-overflow unit test |
| Loop crate native debug/release | **53 passed per profile** |
| Runtime Node/Chromium | **105 passed per runtime** |
| Loop crate Node/Chromium | **53 passed per runtime** |
| Full prescribed nextest workspace | **1962 passed**, four unchanged existing opt-in canaries skipped |
| Selected pinned Miri suite | **53 passing executions**, including all eight planning-metadata and seven planner-executor tests; existing lifecycle stress seeds 0/1/2 retained |
| Production-trait lifetime proof | Positive executable compiles/runs; retained decision borrow across planner mutation fails with **E0502** |
| Planned-gain example | Native debug/release checks exact samples, five intervals, and frame-13 parameter execution |
| Formatting, warning-denied workspace build/rustdoc, dependency/test/tracing policies | Passed; both four-target audits and five Python policy tests retained |
| Existing native and actual non-isolated offline/live AudioWorklet fixture | Passed in fresh runs; both failure canaries detected; historical journal unchanged |

The synthetic pipeline binds source and capture-side consumers to the same decision around an ordinary FX node. Full samples and absolute MIDI events match across `[64]`, `[1 × 64]`, irregular partitions, reversed declarations, adaptive intervals, and one-sample intervals. A **128-interval callback with one external event credit and `subranges = 0`** processes all 384 consumer/FX calls under heap guards. Tests exercise per-consumer overflow isolation, stale graph refusal, malformed intervals, callback-end ownership, and zero callbacks.

A prepared test producer enqueues work from inside traversal without allocating or locking. Its requests and returned event credits wait until the following host callback, never the next internal interval; telemetry is also published only once per callback. A separate planner borrows resource I/O on every interval: the 64-byte callback allowance is consumed once, not refreshed 128 times. This verifies preservation of the **current** resource quota, not completion of the required-render accounting change planned for Stage 4.

The first Miri tool invocation timed out after 900 seconds while preparing/executing the synthetic cases with unnecessarily large default media pools. The report retains that partial attempt. Synthetic tests now use smaller prepared media capacities on **all** platforms; interval counts, assertions, and selections are unchanged. Full native/Wasm/host gates and all selected Miri cases subsequently passed. The Miri runner removes stale success reports at startup and retains partial output for per-case timeouts.

Fresh existing-host evidence again has 2601 offline callbacks / 332928 frames, fixed 1507328-byte worklet memory, native/browser sample/MIDI/graph agreement, and a byte-identical historical golden. This is runtime compatibility evidence, **not a loop-library worklet**.

### Outstanding Stage 3 work

Typed domain operation delivery, retained pending runtime receipts, recovery-safe domain cancellation/stop under normal saturation, and their malformed/stale/ownership tests remain unimplemented. `operation_bytes` currently validates signatures only. Planner/consumer resource access still uses the initial shared `ResourceIo` quotas; required-render indexing and capture transactions remain Stage 4. The production loop resolver is not yet attached to this generic lifecycle. No Stage 3 completion or end-to-end LL-A claim follows from this milestone.

## Stage 3 — Complete generic planning and typed delivery

Milestone: `feat(rt-runtime): retain typed receipts with independent recovery` (the commit introducing this section). Evidence: [`evidence/delivery-stage3.json`](evidence/delivery-stage3.json) and [`evidence/delivery-stage3-provenance.json`](evidence/delivery-stage3-provenance.json). The source base is `6028f8f8`; raw logs are under `target/loop-layer-delivery-stage3/`. Earlier Stage 3 reports remain unchanged historical milestones.

### Typed operations, receipts, and recovery

`Planner::Operation` and `Planner::Result` define compact Copy/Send data. `Action::Planner` and `Action::Processor` use distinct generational target identities; ordinary parameter/MIDI actions remain available. Prepared signatures bound payload bytes, scheduler storage checks account for concrete operation/result sizes, and a larger typed-payload stride does not increase MIDI packet limits. `Action::target()` exposes the target kind; `node()` now returns an optional processor identity rather than inventing a node identity for a planner.

Processor operations return immediate typed results. Planner operations receive a borrowed payload/resource/receipt context and return `Complete` or `Pending`. Delivery removes the event from the due queue while retaining its terminal reservation. A token combines runtime-instance scope, slot, and per-lane monotonic sequence; stale, foreign, duplicate, and premature completion attempts are explicit errors. Disjoint payload and receipt borrows allow safe completion during delivery without unchecked pointers or aliasing. Scope allocation is preparation-only; tokens are not portable capabilities between independent Wasm instances.

Execution records distinguish requested, optional delivered, and terminal frames. Delivery lateness does not count legitimate pending time. Generic runtime success is separate from the domain terminal value, including refusal/cancellation/supersession. Queued typed cancellation produces no delivery and prevents subsequent execution; delivered pending cancellation completes the retained token. Ticket cancellation retains its original pre-dequeue cut. Original owning payloads return through command completions; Copy terminal records release their credits only through receipt and callback-boundary recycling.

One additional recovery receipt is independent of all normal pending/unread results. Only classified payload-free, nonstructural planner recovery actions at or before the current frame are admitted. They must finish immediately or in planning at that frame; an unresolved recovery receipt faults before interval execution. Domain adapters must also reserve their own recovery state independently. Per-lane admission sequence checks permit priority recovery to precede older normal dequeue without incorrectly rejecting that normal command. Structural edits remain outside this lane, and finite held validation is not preempted.

### Verification

| Gate | Observed result |
| --- | --- |
| Runtime native debug/release | **127 passed per profile** |
| Loop crate native debug/release | **53 passed per profile** |
| Runtime Node/Chromium | **123 passed per runtime** |
| Loop crate Node/Chromium | **53 passed per runtime** |
| Prescribed full nextest workspace | **1980 passed**, four unchanged opt-in canaries skipped |
| Selected pinned Miri | **71 passing executions**, including seven receipt-model and eleven typed-executor tests; existing stress seeds retained |
| Production-trait lifetime proof | Positive build/run and deliberate **E0502** refusal still pass with receipt contexts |
| Planned-gain example, debug/release | Exact 32 samples, five intervals, typed processor result at frame 13 |
| Warning-denied workspace build/rustdoc, formatting, boundary/test/Wasm/tracing policies | Passed; both four-target audits and five Python policy tests retained |
| Existing native host debug/release and fresh offline/live AudioWorklet | Passed; both intentional failure canaries detected; unchanged golden journal |

The independent receipt model runs **8192 generated steps** natively and in Wasm (64 explicitly reduced steps under Miri), with seed `0xa36b20472f5881d9`. It models three slots, queued/delivered/terminal/read states, normal/recovery reservations, and a terminal FIFO independently of scheduler internals. Every accepted action/result, requested/delivery/completion frame, cancellation, credit snapshot, and final return to full capacity is checked. Oracle allocation stays outside guarded production calls.

Typed executor tests compare complete samples and terminal records across whole, one-sample, and irregular host partitions. They cover delayed completion, callback ends/zero callbacks, malformed payloads, domain refusal versus runtime error, exact-frame planner/processor/parameter ordering, queued and pending cancellation, stale planner IDs, processor retirement/reinstallation, and owning payload return. Recovery is exercised with every normal receipt pending or unread, including a full domain pending array and completion during planning before the first affected sample. Unsafe/future/payload-bearing recovery is refused; deliberately nonterminating recovery faults explicitly. Runtime `Full` from a typed operation does not masquerade as MIDI overflow.

The prior interval tests continue to verify frozen ingress and credit snapshots, one telemetry publication per callback, shared resource allowances across 128 internal intervals, observation isolation, and stale-plan preservation. Source review confirms command/maintenance and host MIDI recovery allowances are reset only in the outer callback; delivery and receipt writers cannot replenish them. Native/Wasm guards cover success, refusal, cancellation, stale receipts, and owner retention; Miri exercises the corresponding borrow and ownership paths.

### Compatibility and completion boundary

The independent runtime fixture deliberately retains `NoPlanner` and its `RTR1` wire format; no new operation is disguised as a parameter or claimed to cross that legacy wire. Fresh native/browser runs still agree on **2601 offline callbacks / 332928 frames**, **34 MIDI events**, sample/MIDI/graph hashes **480331717 / 3308430490 / 487351092**, fixed **1507328-byte** memory, and a byte-identical historical journal. The fixture documentation now explicitly states that boundary.

All Stage 3 checklist items are verified. This completes the **generic coordination layer**, not loop/media integration or any end-to-end LL-A acceptance. Required-render resource accounting/indexes/capture transactions remain Stage 4; the loop adapter, program staging, coherent domain state, and new actual loop AudioWorklet remain later stages. No hard callback deadline, production migration, or final-head CI is claimed.

## Stage 4 — Required resource accounting foundation

Milestone: `feat(rt-runtime): separate required render work from background quotas`, source base `c8414b68`. Evidence: `evidence/accounting-stage4.json` and `evidence/accounting-stage4-provenance.json`; raw gate logs and archived Miri case logs are under `target/loop-layer-accounting-stage4/`.

### Implemented boundary

`ResourceIo::render()` and `ResourceStore::render_io()` lend restricted `RenderIo` access for sequential reads and active single-resource audio/MIDI capture. A private explicit work class selects accounting in the same checked implementation, rather than inflating or temporarily resetting quotas. Required data operations cannot return `Budget` from background copy/validation exhaustion. They still validate shapes, frames, IDs/leases, semantic arithmetic and fixed physical capacity. This capability cannot upload, allocate a lease, change structural capture lifecycle, reclaim, or replenish allowances.

`RenderWork` reports attempted data operations, copied bytes, and charged MIDI validation units; counters saturate and latch overflow. The executor publishes separate render counters and merges overflow into its non-semantic diagnostic flag. Both work classes reset only once per host callback. Direct legacy `ResourceIo` methods keep their quota/refusal behavior, headroom reservations, owning returns, and immutable snapshot semantics. Optional snapshot tails, transfers, validation and reclamation remain budgeted.

### Verification

| Gate | Observed result |
| --- | --- |
| Runtime native debug/release | **133 passed per profile** |
| Loop crate native debug/release | **53 passed per profile** |
| Runtime Node/Chromium | **129 passed per runtime** |
| Loop crate Node/Chromium | **53 passed per runtime** |
| Prescribed full nextest workspace | **1986 passed**, four unchanged opt-in canaries skipped |
| Selected pinned Miri | **76 passing executions**, including all four required-resource tests and eight planner-execution tests |
| Production-trait lifetime proof | Positive build/run; deliberate retained-borrow mutation rejected with **E0502** |
| Warning-denied workspace build/rustdoc; formatting; boundary, test, tracing and Wasm policies | Passed |
| Planned-gain example debug/release | Passed |
| Existing native debug/release and actual offline/live AudioWorklet | Passed; processor/heap failure canaries detected; historical journal unchanged |

Four new heap-guarded tests cover required audio and MIDI work beyond exhausted background credits, unchanged legacy refusal, separate validation accounting, stale released cursors, immutable partial snapshot tails across continued capture/removal, bounded reclamation back to full pools, invalid shapes/frames, and explicit physical-capacity exhaustion retaining the complete prefix. A unit test verifies saturating diagnostic arithmetic never causes a budget refusal.

The additional synthetic planner test executes **128 one-sample intervals per callback**, across two callbacks. Required reads complete **512 bytes per callback** while legacy reads retain exactly **64 bytes** of allowance and **112 refusals**. Render operation counts remain 128, with no allowance/counter refresh at interval or capability reborrow boundaries. The original legacy-only test remains intact through the shared fixture.

Fresh existing-host replay retains **2601 offline callbacks / 332928 frames**, **34 MIDI events**, sample/MIDI/graph hashes **480331717 / 3308430490 / 487351092**, and fixed **1507328-byte** Wasm memory. Native and browser journals remain byte-identical to the historical golden.

### Remaining work and limits

Only the first Stage 4 checklist item is complete. Reads still use existing sequential cursors; captures are active single-resource appends, not coordinated group transactions. Indexed audio/MIDI positioning, checked new index ownership, prepared lifecycle/lease reservations, exact-frame freeze-to-play, group preflight/commit, and interval failure observations remain required. Existing IDs/cursors still have the documented same-store precondition; this milestone does not add cross-store identity enforcement.

The existing worklet fixture still uses legacy access, so it is regression evidence, not actual-worklet execution of required `RenderIo` or the loop library. Package Node/Chromium tests exercise the new path in Wasm. No loop integration, group atomicity, deadline guarantee, full Stage 4 completion, final CI, or reviewer approval is inferred.

## Stage 4 — Append-maintained page indexes and scoped reads

Milestone: `feat(rt-runtime): index resource pages for scoped positional reads`, source base `effc7ba4`. Evidence: `evidence/index-stage4.json` and `evidence/index-stage4-provenance.json`; logs under `target/loop-layer-index-stage4/`, with the first broad failure preserved in `target/loop-layer-index-stage4-first-run/`.

### Implementation and design choice

Each data-page slot now embeds a balanced append-only index node; each version owns a fixed root/count. Increasing logical page ordinals permit single right-heavy rotations. Metadata storage is O(total prepared pages), avoiding a version-by-page cross product. Lookup and insertion are logarithmic in version pages; existing sequential links remain available. No index scan/build occurs at lease preparation, seek, wrap, capture stop, or frozen-content access. Pages and their index nodes recycle together under existing incremental reclamation.

`read_index(snapshot_id)` creates a Copy handle referring to an existing lease in constant metadata work; it does not acquire ownership. `RenderIo::cursor_at` / `ResourceStore::cursor_at` accept checked byte offsets, including EOF, and create scoped cursors. Index and cursor scopes reject cross-store reuse even when raw slot/generation pairs coincide. Released/reused leases fail generation checks. Legacy raw resource/snapshot IDs still require their same-store precondition; opaque handles are not serialized capabilities between independent Wasm instances.

`IndexWork` counts background/render metadata steps and saturating overflow; the executor exposes it in diagnostics. Background index insertion is bounded overhead of already admitted page materialization, not an additional content scan or work quota. Required lookup/capture indexing consumes no background allowance. Instance-scope allocation is preparation-only and refuses exhaustion. The representation choice and rationale are recorded in `RUNTIME_INTEGRATION.md`.

### Verification

| Gate | Observed result |
| --- | --- |
| Runtime native debug/release | **140 passed per profile** |
| Loop crate native debug/release | **53 passed per profile** |
| Runtime Node/Chromium | **136 passed per runtime** |
| Loop crate Node/Chromium | **53 passed per runtime** |
| Prescribed full nextest workspace | **1993 passed**, four unchanged opt-in canaries skipped |
| Selected pinned Miri | **81 passing executions**, including all five indexed-resource tests with their full workloads |
| Lifetime proof | Positive build/run and intentional **E0502** refusal |
| Warning-denied build/rustdoc, formatting, policies/boundary audits, examples | Passed |
| Existing native and actual offline/live worklet regression; intentional failure canaries | Final broad rerun passed; earlier failures retained below |

The flat-byte oracle compares every one of **4097 byte offsets**, then **4096 generated wrap/seek positions** using seed `0x0b29778f3a19fbe5`, identically on native, Wasm, and Miri. Reads respect page/prefix/output limits and at most 21 index visits. Guarded cases cover empty/audio/raw-MIDI data, partial immutable tails across further appends/tree rotations, immediate frozen-content access without index work, old leases after replacement/removal, stale/foreign handles, cancelled partial uploads, slot reuse, and full pool recovery. A separate native/Wasm unit test checks parent links, in-order ordinals, stored heights, AVL balance, and equivalence to the independent sequential page chain after every one of **1024 insertions**. Scope/counter exhaustion never wraps.

The synthetic planner now performs an indexed seek and read at each of 128 one-sample intervals: 256 required operations and 512 copied bytes per callback, while legacy reads still receive only 64 bytes and report 112 refusals. Index counters reset once per callback and remain within the tree-height bound.

### Failed runs and investigation

The initial focused test compile passed a `Frame` to the existing resource-relative `u64` MIDI encoder; the test argument was corrected without changing production API, workload, or assertions. A reconstructed diagnostic note is retained.

The first broad run passed all earlier gates but Chromium disconnected during the actual-worklet check. One isolated unchanged-artifact repeat reproduced `invalid session id`. Driver failures at **17:47:57.044** and **17:50:02.780** coincide with host suspend events. This is temporal evidence, not a proven cause or fix. No power settings or artificial resource constraints were changed. Ten subsequent isolated unchanged-artifact runs passed; the complete broad gate was then rerun successfully with both intentional failure canaries. Failed logs, driver diagnostics, kernel-event evidence and repeat records/hashes are retained. No timeout, flag, assertion, or test selection was weakened.

The failed run also exposed stale success reports from a previous fixture run. The runner now deletes the selected result and browser summaries before starting; the failed isolated repeat confirmed these remained absent. This fixes report hygiene, not the browser disconnect.

### Compatibility and limits

The golden journal remains byte-identical: **2601 offline callbacks / 332928 frames**, **34 MIDI events**, sample/MIDI/graph hashes **480331717 / 3308430490 / 487351092**. Additional preallocated index/handle metadata increases fixture Wasm startup memory from the preceding **1507328** to **1703936 bytes**; it stays fixed throughout offline/live rendering. This is preparation growth, not callback allocation or a memory-growth exemption.

The second Stage 4 checklist item remains incomplete: byte positioning is not MIDI event/time indexing or bounded note/controller restoration. Prepared capture lifecycle/lease reservations, group preflight/commit, and failure observations also remain pending. The existing actual-worklet fixture exercises index maintenance through legacy resource operations, not the new indexed-seek API or a loop-library worklet. No full Stage 4 or end-to-end LL-A completion is claimed.

## Stage 4 — Fixed MIDI checkpoint-state foundation

Milestone: `feat(rt-runtime): add fixed MIDI channel and key state`, source base `bf94631f`. Evidence: `evidence/midi-state-stage4.json` and `evidence/midi-state-stage4-provenance.json`; raw gates and archived Miri logs under `target/loop-layer-midi-state-stage4/`.

`midi_state::MidiState` adds fixed Copy metadata for sixteen channels and 128 keys/controller slots per channel. It distinguishes held and pedal-latched keys, retains latest velocities and supported pressure/program/pitch/controller values, treats velocity-zero note-ons as note-offs, and records reset baselines without inventing unknown device-specific defaults. Repeated note-ons remain separate media packets; the state projection is per key, not per synthesizer voice. No clock, loop, renderer, device, or host dependency is introduced. Packet validation precedes mutation; already validated messages avoid redundant validation. State copying and all updates are heap-free and bounded by protocol shape.

The state/reset policy and MIDI Association references are recorded in `src/rust/rt_runtime/docs/midi-state.md`. It includes sustain/hold/sostenuto interactions, immediate all-sound-off versus pedal-aware all-notes-off, retained bank/program/volume/pan/sound/effects values on controller reset, known reset values, unknown defaults, and system-reset markers. Opaque SysEx/device effects and RPN/NRPN parameter histories are not synthesized by this protocol projection; original packets are not discarded from media. This is a state foundation, not completed capture/checkpoint or restoration behavior.

| Gate | Observed result |
| --- | --- |
| Runtime native debug/release | **145 passed per profile** |
| Loop crate native debug/release | **53 passed per profile** |
| Runtime Node/Chromium | **141 passed per runtime** |
| Loop crate Node/Chromium | **53 passed per runtime** |
| Prescribed full nextest workspace | **1998 passed**, four unchanged opt-in canaries skipped |
| Selected pinned Miri | **86 passing executions**, including all five new state tests at full workload |
| Lifetime proof | Positive build/run and intentional **E0502** refusal |
| Warning-denied build/rustdoc, formatting, policies/audits, examples | Passed |
| Existing native debug/release and actual offline/live worklet; failure canaries | Passed; golden journal unchanged |

The independent ordered-map oracle runs **4096 operations**, seed `0xad473b29385cc291`, with separate pedal/pitch arrays and full channel/key comparisons after every 64 operations. The same workload runs natively, in both Wasm runtimes, and under Miri. Targeted tests cover pedal interactions and velocities, channel isolation, reset/mode behavior, pressures/program/pitch, Copy snapshots, invalid indices, malformed/capacity refusal without mutation, and non-channel packets. All production updates/copies are heap guarded; reference allocation is outside the guard.

The existing host remains a regression fixture, not an execution test of this new state type or a loop AudioWorklet. Offline results retain **2601 callbacks / 332928 frames**, **34 MIDI events**, hashes **480331717 / 3308430490 / 487351092**, fixed **1703936-byte** memory, and the byte-identical golden journal. There were no gate failures in this milestone; earlier index-milestone browser failures remain recorded above, not rewritten.

Stage 4 remains incomplete. Checkpoint storage and its reservations, incrementally built event/state positioning with a declared local replay bound, capture group lifecycle/transactions, and truthful failure observations are still pending. Actual restoration emission belongs to the later renderer work. No additional Stage 4 checklist item or end-to-end LL-A acceptance is marked complete.
