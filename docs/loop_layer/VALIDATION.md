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
| LL-A2 | Generic synthetic planner dependency/view tests; resolver ownership tests; graph receiver validation | Pure resolver and standalone generic binding validation verified; runtime lifecycle/integration pending |
| LL-A3 | Same-frame A → B → C and nested cascades; one-sample loops exceeding old subdivision allowances | Pure primitive/composite execution verified; media/runtime/host proof pending |
| LL-A4 | Semantic progress proof; valid mutual sync; rejected instantaneous recursion; generated valid/invalid programs | Stage 2 compiler/resolver proof and tests pass; integrated audit pending |
| LL-A5 | Independent reference journals/audio/MIDI across callback partitions and declaration orders; exact end-boundary cases | Pure semantic journals/logical decisions match; audio/MIDI/host comparisons pending |
| LL-A6 | Debug/release/Wasm allocation/reallocation/deallocation canaries and guarded callback scopes; callback audit | Pending new paths; original runtime baseline passes |
| LL-A7 | Owning return/cancellation/generation/saturation/disconnect/teardown models; selected Miri and concurrency tests | Pending new paths |
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
