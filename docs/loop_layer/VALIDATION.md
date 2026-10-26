# Loop-layer implementation and validation evidence

## Scope and provenance

`PLAN_LOOP_LAYER.md` governs this implementation. Its LL-A1–LL-A12 criteria are immutable without user approval. The initial runtime record in `DONE_INITIAL_CRATE.md` and `docs/rt_runtime/evidence/` is preserved; passing baseline tests is not evidence that new loop-layer behavior exists.

Execution was originally authorized through confirmed goal `mvnjf70y-cjcfar` and carried forward by confirmed goal `mvo6lnim-1xzkd7`. The new tracker carries completed Stages 0–3 forward without restarting those implementations. Stage 4 is now verified; Stage 5 is current and Stages 5–10 remain incomplete. The implementation branch is `feat/loop-execution-layer`, based on `cc6ccb4ac35114d61ef7e1c03ecc7f2fe81bb867`. At branch creation, [runtime PR #878](https://github.com/SanderVocke/shoopdaloop/pull/878) remained open against `master`, with that same head. The delivery PR should initially target `feat/generic-rt-runtime` unless the parent merges before delivery; recheck rather than assuming its future status.

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
| LL-A6 | Debug/release/Wasm allocation/reallocation/deallocation canaries and guarded callback scopes; callback audit | Generic planner/resource and low-level loop audio paths guarded in native/Wasm and selected Miri; integrated loop callback audit pending |
| LL-A7 | Owning return/cancellation/generation/saturation/disconnect/teardown models; selected Miri and concurrency tests | Generic typed receipts/recovery, generated group ownership/content models, lifecycle-cut disconnects and selected Miri verified; integrated loop-domain ownership pending |
| LL-A8 | Stereo/MIDI record-to-play, seek/replacement/cleanup/export; FX capture graph; common-prefix exhaustion | Generic group content/freeze/prefix and low-level audio playback/gain/capture verified; MIDI rendering/restoration and full loop integration pending |
| LL-A9 | Integrated prepared composites, primitive/composite interactions, and rendering-observation boundary tests | Pending |
| LL-A10 | Independent render/maintenance accounting, indexed access, capture reservations, counter/trace exhaustion, pool recovery | Required accounting, scoped indexing, prepared group transactions, synthetic boundary observations and complete generic group ownership/model checks verified; loop integration pending |
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

## Stage 4 — MIDI event/checkpoint indexing

Milestone: `feat(rt-runtime): index MIDI events and checkpoint state` (the commit introducing this section), source base `df25525231d09023b567c65f65ae763f5914c1b5`. Evidence: [`evidence/midi-index-stage4.json`](evidence/midi-index-stage4.json) and [`evidence/midi-index-stage4-provenance.json`](evidence/midi-index-stage4-provenance.json); raw gates and archived Miri logs are under `target/loop-layer-midi-index-stage4/`.

### Interfaces and preparation

`ResourceStore::with_midi_indexing` opts into fixed `MidiIndexConfig` storage: indexed-version slots, a complete checkpoint row per version, and a positive local event-replay span. Legacy stores allocate no heavy MIDI rows. `begin_indexed_midi_upload` and `begin_indexed_midi_capture` attach an explicit initial protocol state; normal upload/capture lifecycle remains in use. Each version pins its row until the final reference is released. Reuse resets source identity/used length without scanning or clearing checkpoint state; data pages still retire incrementally. This deliberately declared version-by-checkpoint storage tradeoff is documented in [`MIDI_INDEX_DESIGN.md`](MIDI_INDEX_DESIGN.md) and the crate's `docs/midi-indexing.md` guide.

Complete-record summaries incrementally maintain state and checkpoints during uploads/capture. A candidate upload parser and checkpoint capacity are checked before publishing any bytes or state from a chunk. Capture preflights metadata capacity before appending. Every packet counts toward replay spacing, including opaque packets. With C checkpoints and span K, a row supports at most C×K−1 packets; byte capacity can exhaust earlier. Valid content may exceed declared checkpoint capacity before its byte reservation fills, producing an explicit refusal rather than an unbounded scan. This is single-resource atomicity, not a coordinated capture transaction.

`RenderIo::{midi_cursor,midi_next,midi_read}` provides scoped event iteration and complete original packet reads, including legacy validated MIDI content. `midi_seek` requires checkpoints, selects the last checkpoint strictly before the target within the immutable snapshot prefix, and replays fewer than the declared span. It returns projected state before the target and leaves all equal-frame events for playback. Header/short-packet lookup uses the page index; reconstruction skips long opaque bodies without removing them from playback. There is no whole-content build at freeze or position change. Output restoration emission remains a renderer responsibility.

Opaque MIDI cursor/event handles validate store scope, lease/version, packet/prefix bounds and offsets. They refer to an existing lease, not a new owner. Old leased prefixes remain readable after capture appends, replacement, and removal. Raw legacy IDs retain their same-store precondition. `MidiIndexWork` reports indexed/replayed records and initial/checkpoint state copies; saturation is diagnostic, and counters reset once per host callback rather than per interval.

### Verification

| Gate | Observed result |
| --- | --- |
| Runtime native debug/release | **155 passed per profile** |
| Loop crate native debug/release | **53 passed per profile** |
| Runtime Node/Chromium | **151 passed per runtime** |
| Loop crate Node/Chromium | **53 passed per runtime** |
| Prescribed full nextest workspace | **2008 passed**, four unchanged opt-in canaries skipped |
| Selected pinned Miri | **95 passing executions**, including eight MIDI-index tests and nine planner-execution tests |
| Production-trait lifetime proof | Positive build/run and deliberate **E0502** refusal |
| Warning-denied build/rustdoc, formatting, policies/audits, examples | Passed |
| Existing native debug/release and actual offline/live worklet; both failure canaries | Passed; historical golden journal unchanged |

The independent seek oracle uses **273 packets**, dense equal-time groups, supplied initial state, and a **257-byte SysEx** spanning pages. It compares positions/original packets and independent ordered-map key/pedal/program state at all **81 target frames** of an 80-frame resource with span 8. Bytewise uploads exercise streaming boundaries. The full workload runs unchanged under Miri. Additional guarded cases cover partial capture snapshots and later appends, no-build freeze, upload/capture checkpoint exhaustion before prefix mutation, malformed-chunk atomicity, foreign/stale leases, old revisions, row reuse and full recovery, empty content, maximum-frame duration, and explicit legacy/unprepared refusal. A unit test separates saturating diagnostics from semantic event-count exhaustion.

The synthetic planner performs 128 MIDI seeks in one callback with span-one checkpoints, checks state around frame 64, and observes exactly 128 checkpoint-state copies with no replay. Required work remains independent of the legacy 64-byte allowance and its 112 refusals. Counters reset at the next callback, not at each interval or resource borrow.

Existing host results retain **2601 offline callbacks / 332928 frames**, **34 MIDI events**, hashes **480331717 / 3308430490 / 487351092**, and fixed **1703936-byte** memory. The actual runtime worklet is regression evidence only: its legacy configuration does not opt into the new MIDI pools and is not the required loop AudioWorklet. Package Wasm tests exercise the new APIs. No failed gate occurred in this broad run; earlier failures remain recorded in their own milestones.

The first two Stage 4 checklist items are now verified. Prepared lifecycle/version/lease tokens, exact-frame reserved freeze-to-play, coordinated audio/MIDI transactions, and truthful integrated failure observations remain incomplete, as do all later loop renderer/host/final-delivery stages. This milestone does not complete Stage 4 or an end-to-end LL-A criterion.

### Carried-forward checkpoint verification

At confirmation of goal `mvo6lnim-1xzkd7`, the Stage 0–3 evidence files were compared byte-for-byte with their introducing commits, all of which are ancestors of the current branch. Recorded Rust hashes matched those milestone commits: 14 for the completed Stage 2 resolver, 50 for planning metadata, 52 for interval execution, and 54 for completed Stage 3 delivery. Current MIDI-index evidence matched all 63 recorded Rust source hashes, all 21 successful gate-log hashes, and all archived Miri case hashes (95 executions). This verifies provenance without presenting historical results as fresh executions or redoing completed implementation. The latest broad MIDI-index suite separately verifies current runtime/loop regressions. The plan and new goal tracker carry Stages 0–3 forward and continue Stage 4; unrelated `.pi/` data remains untracked.

## Stage 4 — Prepared capture lifecycle and staged group transactions

Milestone: `feat(rt-runtime): reserve and stage coherent capture groups` (the commit introducing this section), source base `127290dc`. Evidence: [`evidence/capture-stage4.json`](evidence/capture-stage4.json) and [`evidence/capture-stage4-provenance.json`](evidence/capture-stage4-provenance.json). Raw logs and archived Miri cases are under `target/loop-layer-capture-stage4/`. This is a substantial Stage 4 implementation milestone, not completion of the stage's remaining model/disconnect requirements.

### Interfaces and invariants

- `CaptureGroupConfig` opts into fixed group/member rows and staging regions. `bind_capture_group` validates every member before acquiring exclusive claims. Legacy remove/replace/start/stop/raw append cannot mutate managed resources; existing read leases remain usable. Release requires an idle group and preserves active frozen content until explicitly removed.
- `prepare_capture_take` reserves all version, MIDI-row and final-snapshot slots before changing active content. `CaptureTakeId` carries store/group scope and a checked take sequence. Cancelled or completed takes cannot retarget a subsequent take. Unpublished reserved snapshot slots cannot be consumed by optional snapshots or read as ready leases.
- Exact-frame activation validates the whole group before publishing new revisions. Freeze publishes the already reserved immutable leases without a mutable-tail copy, history scan, or new capacity acquisition. All members share the cut/incomplete reason. Empty freeze is a generic storage operation; empty record-to-play refusal remains a domain rule.
- `CaptureIntervalId` adds checked interval identity. Preflight reserves exact audio bytes and conservative MIDI producer/event/payload/checkpoint bounds for every member before any data is staged. Failed preflight leaves all published prefixes unchanged. Unused reservations return on commit/cancel.
- Consumers copy validated contributions into prepared staging. Missing/duplicate/malformed/over-bound members do not partially publish resource data. One exclusive commit updates all actual bytes, indexes, revisions and the common duration; no graph reader can interleave with intermediate publication or index rotations. Producer-bound violations after successful reservation are interface faults, not normal retroactive exhaustion observations.
- Safe borrowing reuses the existing page append routine via a fixed 64-byte stack chunk rather than moving owning storage or duplicating page/index invariants. Three media-data copies are counted (input→staging→stack→pages); checkpoint copies are reported separately. No zero-copy or performance improvement is claimed.
- Planner contexts now use `PlannerResourceIo`, dereferencing to existing resource methods and adding `CaptureBoundaryIo`. The runtime stamps frame/ceiling authority: activation/freeze at the current boundary, reservation within the current planning window, commit at the reserved end. Ordinary `ResourceIo` cannot acquire this capability; `RenderIo` adds only staged audio/MIDI writes. No capability can reset accounting or reclaim storage.

The design and public guide are `CAPTURE_TRANSACTIONS_DESIGN.md` and `src/rust/rt_runtime/docs/capture-groups.md`. Source remains safe Rust, with no new host/application dependency or callback-side owning allocation.

### Verified gates

| Gate | Observed result |
| --- | --- |
| Runtime native debug/release | **167 passed per profile** |
| Loop crate native debug/release | **53 passed per profile** |
| Runtime Node/Chromium | **163 passed per runtime** |
| Loop crate Node/Chromium | **53 passed per runtime** |
| Prescribed full nextest workspace | **2020 passed**, four unchanged opt-in canaries skipped |
| Selected pinned Miri | **107 passing executions**, including all twelve new capture-group/boundary/planner cases |
| Lifetime proof | Positive build/run and deliberate **E0502** refusal with the new planner resource context |
| Capture authority proof | Planner capability compiles/runs; a consumer `ResourceIo::capture()` attempt fails with **E0599** |
| Warning-denied build/rustdoc, formatting, boundary/policy checks, planned-gain debug/release | Passed |
| Existing native debug/release and actual offline/live worklet; processor and heap canaries | Passed; fixed **1703936-byte** memory and unchanged historical journal |

New tests cover atomic claims, earlier upload refusal, all unrelated version/lease/page-credit saturation after successful preparation, zero remaining background copy allowance, complete cancellation/reuse recovery over 32 cycles, foreign/stale groups and take/interval tokens, occupied freeze outputs, maximum-frame exhaustion, and explicit opt-in/shape limits. Immutable audio/MIDI prefix leases survive staging, later appends, and page-index rotations.

Independent scalar content checks compare complete stereo bytes and original MIDI timestamps/packets across `[8,8]`, sixteen one-frame intervals, and `[3,2,5,6]`; bounded checkpoint seeks agree with the expected last-note state. Conservative checkpoint exhaustion retains the exact 24-frame common prefix. Partial/invalid staging remains unpublished and cancellable; all ordinary test teardowns restore page/version/snapshot/MIDI-row credits.

The actual generic-runtime synthetic planner uses unknown downstream MIDI within its declared bound. At frame **24**, preflight exhaustion changes the dependent output **before sample 24**, freezes the common prefix, and leaves unrelated audio running. Full outputs agree across `[32]`, thirty-two one-frame callbacks, and `[7,9,5,11]`. Reserved interval staging is committed in `finish`; exported prefix/state and eventual credit recovery are checked. This is a generic observation-boundary proof, not loop-domain adaptation or a new loop AudioWorklet.

### Retained development failures and remaining work

Three initial focused attempts failed due to test setup: a missing `unwrap()` on `MidiBuffer::new`, a transfer allowance below the existing minimum page size, and insufficient declared staging space for the test's two MIDI members. The fixes supplied the correct buffer type, increased allowance and deliberate consumption together to 16 bytes (still leaving zero background credit), and provided the 448-byte staging shape needed to reach the intended checkpoint-row refusal. Workloads/assertions were not removed or weakened; original logs and hashes remain in provenance. The later complete broad run passed without retrying a failed gate.

The existing worklet fixture remains legacy/no-planner regression coverage and does not opt into capture groups. New APIs execute in the package Wasm tests and synthetic native/Wasm planner, not yet in the required independent loop host. Complete generated group-state/ownership and disconnect-at-each-phase coverage, Stage 4 acceptance review, loop renderers/restoration, final-head CI and review remain pending. The remaining Stage 4 checklist items stay open until that verification is complete; successful core APIs alone do not close them.

## Stage 4 complete — Generated capture ownership and lifecycle-cut disconnects

Milestone: `test(rt-runtime): verify capture ownership and disconnect cuts` (the commit introducing this section), based on core transaction commit `9aef981c`. Evidence: [`evidence/capture-ownership-stage4.json`](evidence/capture-ownership-stage4.json) and [`evidence/capture-ownership-stage4-provenance.json`](evidence/capture-ownership-stage4-provenance.json). Raw logs and archived Miri cases are under `target/loop-layer-capture-ownership-stage4/`. Earlier milestone evidence and failures remain unchanged.

### Independent ownership/content model

`capture_group_model` runs **1,024 actions each with 64 and 16 physical pages**, seed **`0x7a49c83d502e19b6`**. The complete 2,048-action workload is unchanged on native, Wasm, and Miri. An independent reference ledger tracks prepared/current/pinned versions, mutable-tail private pages, conservative interval-page reservations, reserved final leases, MIDI rows, revisions, capture metadata and currentness. It predicts admission/refusal before calling the store and checks all pool counters and visible metadata after each action. Retirement is drained through bounded maintenance calls rather than treated as immediate callback work.

Original stereo samples and MIDI packets/timestamps are generated independently of the storage encoding. Every 32 actions, all retained snapshots are compared against their original prefixes, including state strictly before each seek frame. Reads use guarded required I/O. Repeated old leases, new takes, partial staging, cancellation and slot reuse must not change those immutable contents. The final teardown returns every page, version, snapshot and MIDI row.

| Action | 64-page model | 16-page model |
| --- | ---: | ---: |
| Snapshot release / acquisition | 115 / 82 | 112 / 73 |
| Take cancellation / activation | 6 / 20 | 9 / 23 |
| Interval cancellation | 43 | 17 |
| Audio / MIDI staging | 95 / 79 | 48 / 39 |
| Commit / freeze | 68 / 20 | 36 / 23 |
| Interval reservation / refusal | 111 / 0 | 53 / 10 |
| Take preparation / refusal | 26 / 310 | 32 / 504 |

The 64-page run does not claim physical interval exhaustion; the additional 16-page run supplies that coverage. Conservative checkpoint exhaustion remains covered by the existing exact-24-frame tests. Neither generator nor assertions are reduced under Miri.

### Disconnect and quiescent teardown

`capture_group_disconnect` exercises fourteen stable lifecycle cuts: unbound, bound, prepared, activated, interval-reserved, audio-staged, all-staged, committed, frozen, first/final lease released, group released, prepared take cancelled, and interval cancelled. It queues a payload-free classified recovery stop, **drops the actual control transport before delivery**, then processes the stop with no callback allocation/reallocation/deallocation. Pending intervals are cancelled before publication; prepared takes are cancelled; committed prefixes freeze at the runtime's actual boundary. Every reserved lease/page credit settles even though the receiver of completion results has disconnected.

This verifies queued recovery plus disconnect, not automatic stop merely because a producer disappears. Group release/resource removal remain explicit owner cleanup rather than structural work smuggled into recovery. A prepared unbound processor is retained during callbacks and destroyed exactly once off-render. New `GraphRuntime::into_resources()` consumes a stopped runtime off-render and returns its store; other owners are destroyed. It does not discover or cancel domain handles lost by a planner: settle those first or retain handles. Explicit cleanup of the returned store restores all pools and group slots at every cut.

### Final Stage 4 gates and checklist review

| Gate | Observed result |
| --- | --- |
| Runtime native debug/release | **169 passed per profile** |
| Loop crate native debug/release | **53 passed per profile** |
| Runtime Node/Chromium | **165 passed per runtime** |
| Loop crate Node/Chromium | **53 passed per runtime** |
| Prescribed full nextest workspace | **2022 passed**, four unchanged opt-in canaries skipped |
| Selected pinned Miri | **109 passing executions**, including full generated model and all disconnect cuts |
| Lifetime / authority proofs | Positive compilation/execution; deliberate **E0502 / E0599** refusals |
| Formatting, warning-denied build/rustdoc, examples, boundary and test/tracing/Wasm policies | Passed |
| Existing native debug/release and actual offline/live worklet, both failure canaries | Passed; unchanged historical journal and fixed **1703936-byte** memory |

All 21 broad gates passed on the first complete run. The preceding focused model/disconnect runs also passed; there is no new failed run being omitted. The ignored Nix SQLite-busy warning did not prevent the Miri run from succeeding.

| Stage 4 checklist obligation | Evidence |
| --- | --- |
| Required work versus background quotas, one callback envelope | `accounting-stage4`; repeated-interval planner tests; independent saturating diagnostics |
| Prepared audio/MIDI indexes, bounded positioning and scoped identities | `index-stage4`, `midi-state-stage4`, `midi-index-stage4`; unchanged full page/event/state oracles and Miri |
| Reserved lifecycle/version/MIDI-row/final-lease capacity; exact activation/freeze and immediate frozen access | `capture-stage4`; lifecycle saturation, stale tokens, empty/nonempty freeze/read, authority proofs |
| Coordinated audio/MIDI preflight/staging/commit; unknown downstream MIDI bounds and common-prefix failure | `capture-stage4`; three content partitions, malformed/partial staging, conservative checkpoint limits; new independent 16-page model |
| Failure observations before affected samples, no rollback | `capture_group_planner`: dependent sample changes at frame **24** under three host partitions, while unrelated audio continues |
| Immutable leases/currentness/old revisions, tail isolation, incremental retirement, cancellation/disconnect/full recovery | Prior index/group cases plus the 2,048-action independent model and fourteen real transport disconnect cuts |

All Stage 4 checklist items are verified. This closes the **generic resource layer**, not loop renderers/restoration, loop-domain control/adaptation, actual loop AudioWorklet execution, final-head CI, or reviewer approval. Stage 5 is current. No hard callback deadline or performance improvement is inferred from these correctness gates.

## Stage 5 — Prepared audio rendering and capture foundation

Milestone: `feat(loop-engine): add prepared audio rendering and capture` (the commit introducing this section), source base `37d13073`. Evidence: [`evidence/audio-stage5.json`](evidence/audio-stage5.json) and [`evidence/audio-stage5-provenance.json`](evidence/audio-stage5-provenance.json); raw logs and archived Miri cases are under `target/loop-layer-audio-stage5/`. This is a partial Stage 5 milestone, not completion of its integrated checklist.

### Interfaces and evidence

- `shoop_loop_engine::audio::{AudioRenderer, AudioCapture, Playback, Smoothing}` allocate/check/pre-touch fixed conversion and per-channel DSP storage at preparation. All channel/frame/aggregate-byte arithmetic uses the common native/Wasm ceiling. The renderer does not own mode, playback position, wrap occurrence, resource leases, or capture lifecycle authority.
- Playback receives a scoped index, authoritative source position, finite gain and explicit discontinuity. The new generic `RenderIo::snapshot(index)` validates store/lease identity and returns pinned-prefix metadata without creating a lease or spending background transfer credits. Format/duration and complete output shape are checked before output/DSP mutation; little-endian interleaved data is read into fixed scratch and converted to planar output. Source intervals cannot cross wraps.
- `None` source is immediate silence and resets smoothing. A zero-frame call has no DSP effect. Default/zero-length smoothing is exact raw gain. Linear restart retains the last actual output as its anchor, advances its ramp only for rendered samples, survives host cuts, and restarts on start/retrigger/seek/wrap/replacement. Refusal leaves output and DSP state unchanged, but does not authorize replaying a failed runtime callback.
- Capture converts planar inputs into fixed interleaved scratch and stages one already reserved group member. It cannot activate/freeze/commit. Invalid shapes, duplicate contributions and stale interval tokens do not publish a prefix. Runtime work counters cover resource storage access, not all conversion/DSP instructions or a deadline guarantee.

Six guarded loop-crate tests establish:

1. A separately implemented scalar source-position/sample/ramp oracle agrees with the production resolver plus audio renderer over **40 frames**, gain 2, smoothing 0/4, start/seek/stop/restart/replacement and natural wraps. Host partitions are `[40]`, forty one-frame callbacks, and `[7,6,4,9,14]`. Required I/O continues after actual public legacy reads exhaust the callback's background allowance; it never refreshes it.
2. Mono/stereo/**five-channel** interleaved regions cross 16-byte pages correctly, with negative gain. Retained leases remain playable after resource removal; metadata remains that of the pinned prefix.
3. Stale/foreign scopes, wrong format, empty content, out-of-range/maximum positions and non-finite gain leave prior output/ramp state intact. Zero frames do not reset a pending ramp; Stop is immediate silence; invalid preparation arithmetic is refused.
4. Planar capture over `[5]`, five one-frame intervals and `[2,3]` freezes and reads immediately. Separate old-playback/gain/new-capture operations double the new take while preserving the immutable old take, without background credit. This is an operation-level proof, not yet the required separate-node runtime graph.
5. **128 one-sample intervals** produce every sample and exactly **127 interior wraps** under `[64,64]`, 128 one-frame callbacks and `[7,53,19,49]`. The wrap at 128 remains pending for the next nonempty interval; the renderer does not invent a clock or impose a subdivision quota.
6. Missing/mismatched/oversized/zero-frame capture inputs, wrong reserved duration, duplicate staging and stale tokens retain an unpublished empty prefix. Cancellation and explicit teardown return all credits.

All six workloads run unchanged in native debug/release, Node/Chromium and pinned Miri. The Miri runner now records the package for each case and includes the loop audio target; it does not claim that the full loop test suite has run under Miri.

### Verified gates and retained failures

| Gate | Observed result |
| --- | --- |
| Loop crate native debug/release | **59 passed per profile** |
| Runtime native debug/release | **169 passed per profile** |
| Loop crate Node/Chromium | **59 passed per runtime** |
| Runtime Node/Chromium | **165 passed per runtime** |
| Prescribed full nextest workspace | **2028 passed**, four unchanged opt-in canaries skipped |
| Selected pinned Miri | **115 passing executions**: 109 runtime plus six complete loop audio cases |
| Lifetime/authority proofs | Positive builds/runs; deliberate **E0502/E0599** refusals |
| Warning-denied build/rustdoc, formatting, examples, boundary/policy gates | Passed |
| Existing native debug/release and actual offline/live worklet, both failure canaries | Passed; unchanged historical journal and fixed **1703936-byte** memory |

Three focused development failures are retained with hashes: using the store method name on `RenderIo` instead of its existing `stage_audio` API; ambiguous `Request` imports/private budget charging in the test (replaced with explicit import and real public legacy reads); and incorrectly treating the resolver's interval journal as cumulative. The last fix accumulates each committed interval's events and preserves the **127-wrap** assertion. No musical workload/assertion was reduced. All 21 subsequent broad gates passed in their first complete run; the ignored Nix SQLite-busy warning did not prevent success.

The user-facing API contract is `src/rust/shoop_loop_engine/docs/audio.md`. The existing runtime worklet remains regression coverage and does not execute the new loop renderer; package Wasm tests do. MIDI playback/capture/restoration, planner/processor adapters, transactional loop/resource binding, coherent mixed-media export, the full separate-node playback→FX→recording graph, the public executable lifecycle example, and independent loop hosts remain pending. Stage 5 stays current; no end-to-end LL-A or final-delivery completion is claimed.

## Stage 5 — Authoritative media discontinuity decisions

Milestone: `feat(loop-engine): publish authoritative media discontinuities` (the commit introducing this section), base `287de2c2`. Evidence: [`evidence/decisions-stage5.json`](evidence/decisions-stage5.json) and [`evidence/decisions-stage5-provenance.json`](evidence/decisions-stage5-provenance.json); logs are under `target/loop-layer-decisions-stage5/`.

The audio foundation exposed a necessary integration distinction: the diagnostic journal may be absent/full, and a successful same-position Seek or identical Replace may not change visible `State`. Neither journal entries nor state inequality can therefore be the source of renderer restart semantics. This milestone implements the already specified discontinuity behavior without changing pulse precedence, request outcomes or timeline authority.

`Decision::playback_discontinuity` is accumulated in prepared per-loop shadow scratch for successful primitive transitions and natural wraps, then gated by the final state's `plays()`. Start/retrigger/seek/replace and newly starting monitored playback set it; idempotent Play, continuing monitored playback and monitored-capture failure do not. Final Stop/non-playing state clears the published flag. Composite operations propagate through ordinary primitive decisions rather than pretending composites render media. Checked storage accounting includes the new scratch byte and decision layout.

Every preview clears/rebuilds these flags before arbitration. Repeated previews are identical; refused/losing/cancelled work cannot leak an earlier candidate restart. Flags are committed with the immutable interval decision and do not depend on external receipt or diagnostic capacity. `SEMANTICS.md`, the crate README and audio guide document the interface; the actual runtime adapter remains pending.

### Fresh scoped verification

- The existing scalar sample/ramp and 128 one-sample audio cases now consume authoritative flags with **journal capacities 0, 1 and 1024**, retaining all prior host partitions and fully provisioned cases. Output is unchanged even with absent diagnostics; every one-sample interval has the required restart flag, while the final wrap at callback end remains pending.
- Four new guarded `media_decisions` tests cover a twenty-frame primitive expected-flag table, same-state Seek/Replace with no transition journal entry, idempotent Play, retrigger, capture monitoring/failure, Stop/empty-content refusal, repeated previews, losing/cancelled/refused intents, recovery of a later natural wrap, reactive final Stop/restart arbitration, and nested authoritative snapshots without a journal.
- **63 loop tests per native debug/release profile; 63 per Node/Chromium runtime.** Fresh prescribed workspace nextest: **2032 passed**, four unchanged opt-in canaries skipped.
- **Ten fresh selected loop Miri executions** (six expanded audio tests and four media-decision tests), with unchanged full workloads. The runner now supports `--package` and keeps filtered reports in separate package directories; default combined execution still selects both packages. No claim that a combined 119-case run occurred in this milestone.
- All **15 scoped gates** passed: formatting, loop native debug/release, warning-denied workspace build/rustdoc, runtime/loop boundary checks and helper tests, test/tracing/Wasm dependency policies, Node/Chromium, pinned nextest version and full workspace tests. The focused tests passed on their first run. The ignored Nix SQLite-busy warning did not prevent success.

No `rt_runtime` implementation changed here. Standalone runtime release/Wasm tests, native/actual-worklet fixtures, lifetime/authority prototypes and generic runtime Miri were **not freshly rerun** for this scoped milestone; their preceding full `audio-stage5` evidence remains historical. Future integrated/final gates still require their prescribed fresh runs. Stage 5 remains current: MIDI emission/restoration, planner/processor adapters, mixed-media capture/export, the separate-node graph, public full-lifecycle example and independent loop hosts remain required.

## Stage 5 — Projected MIDI cleanup/restoration emission

Milestone: `feat(loop-engine): emit ordered MIDI cleanup and restoration` (the commit introducing this section), base `c3526ff1`. Evidence: [`evidence/midi-restore-stage5.json`](evidence/midi-restore-stage5.json) and [`evidence/midi-restore-stage5-provenance.json`](evidence/midi-restore-stage5-provenance.json). Logs and archived Miri cases are under `target/loop-layer-midi-restore-stage5/`. Stage 5 remains incomplete.

### Ordering refinements and interfaces

The first emission implementation exposed information that latest-value controller arrays alone cannot express. A later bank-select packet may be pending for a future program change; using it before restoring the current program selects the wrong bank. Likewise both RPN and NRPN selector arrays may contain values while only the last family governs subsequent Data Entry, and opposed omni/mono modes must not be replayed as if all were current. These are representation refinements needed for the existing restoration contract, not a new timeline or device-state guarantee.

The generic `MidiState` now retains the bank selectors observed at program change, a selector-family ordering hint, and an explicit untracked-parameter-history marker. Only the current member of each CC124/125 and CC126/127 pair remains cached. CC96/97 are momentary relative commands rather than persistent values. CC121 retains loaded/pending banks and untracked parameter history while establishing null selector defaults; System Reset clears the projection. Original recorded packets are unchanged, including parameter and opaque SysEx packets.

`shoop_loop_engine::midi_restore::{cleanup, restore}` borrow state and emit complete packets through a bounded sink, with no allocation or state mutation. Cleanup releases active holds and projected keys rather than indiscriminately issuing normal-operation all-sound-off. Restoration orders reset markers, loaded bank/program, pending banks, controls/modes, selector state, pitch/pressure, holds, correctly latched sostenuto notes, remaining notes, released pedal-held keys, and poly pressure. Data Entry values are emitted only while selectors are temporarily null; known selectors are then restored with the last family last. No relative parameter command is synthesized.

`Emission` exposes attempted/accepted packet counts, first error and untracked parameter history. After a sink refusal it completes its fixed traversal/counting without further sink calls or retries. A prefix is not success: the eventual processor must propagate Full through the runtime's reserved reset path after other required work. The helper itself does not deliver recovery. Conservative protocol bounds are **2096 cleanup events** and **8449 restoration events**, with three bytes/event sufficient; these are capacity bounds, not musical-work quotas.

The existing projection does not reconstruct synthesizer voices/envelopes, vendor SysEx or RPN/NRPN parameter-value histories. The new warning makes parameter history explicit; temporary selector nulling prevents accidental writes rather than pretending to recover missing history. Protective selector nulling can affect previously unknown components; only known values are restored afterwards, and unknown components may remain null. This is not a claim that unknown state was reconstructed. Detailed API/ordering/failure contracts are in `src/rust/shoop_loop_engine/docs/midi-restore.md` and the runtime MIDI-state guide.

### Verified behavior and gates

Two new runtime tests cover program-bank snapshots versus pending selectors, selector family ordering, relative-command treatment, opposed mode replacement, reset retention and immutable copied state. Four loop tests cover:

- Cleanup and sustain/hold/sostenuto restoration against an **independent key/pedal machine**. Only the recorded subset is latched; released pedal-held keys are not turned into held keys.
- Loaded versus pending program bank and RPN/NRPN ordering against an **independent selector/bank machine**, including zero accidental parameter writes, correct reset ordering and omission of obsolete/relative commands.
- Every channel/key/control/pressure path under the conservative fixed bounds, with known-state comparisons; the maximal cleanup emits exactly 2096 packets.
- Zero/tiny-capacity and disconnected sinks, exact accepted/attempted counts, no retry after first failure, unchanged source state, and no false claim that the helper already invoked recovery.

All complete workloads execute under heap guards, native debug/release, Node/Chromium and pinned Miri. The existing indexed-state, generated capture ownership and expanded journal-independent audio cases also rerun with the updated representation.

| Gate | Observed result |
| --- | --- |
| Runtime native debug/release | **171 passed per profile** |
| Loop native debug/release | **67 passed per profile** |
| Runtime Node/Chromium | **167 passed per runtime** |
| Loop Node/Chromium | **67 passed per runtime** |
| Prescribed workspace nextest | **2038 passed**, four unchanged opt-in canaries skipped |
| Fresh combined selected Miri | **125 executions**: 111 runtime plus fourteen loop cases |
| Lifetime/authority prototypes | Positive builds/runs and deliberate **E0502/E0599** refusals |
| Formatting, warning-denied build/rustdoc, examples, boundary/policy checks | Passed |
| Existing native debug/release and actual offline/live worklet; both failure canaries | Passed; historical golden journal unchanged |

The focused tests and all 21 broad gates passed on their first runs; no new failed run is being omitted. This full run supersedes the preceding scoped milestone's lack of fresh generic-runtime/host evidence, without rewriting its historical claims.

These helpers do not yet implement indexed MIDI event playback/rebasing, input tracking/capture, integration with runtime reset delivery, the loop planner/processor facade, mixed-media export, the separate-node graph/example or the independent loop AudioWorklet. Those remained required Stage 5/later work at this milestone. No full Stage 5, external device-state restoration, deadline, final CI or review approval is claimed.

## Stage 5 — Indexed MIDI playback, input tracking and runtime recovery

Milestone: `feat(loop-engine): render and capture indexed MIDI intervals` (the commit introducing this section), source base `f9f041ea`. Evidence: [`evidence/midi-stage5.json`](evidence/midi-stage5.json) and [`evidence/midi-stage5-provenance.json`](evidence/midi-stage5-provenance.json). Logs and twenty archived selected loop Miri executions are under `target/loop-layer-midi-stage5/`. This is a partial Stage 5 milestone.

`midi::{MidiRenderer, MidiCapture, Playback, RenderReport}` now provide:

- Prepared, pre-touched packet scratch and boxed channel projections. Positions/discontinuities remain supplied by the resolver; a scoped cursor and lookahead only validate contiguous media use. Source ownership stays with the caller.
- Cleanup of the previous projection, bounded indexed state strictly before the supplied offset, ordered restoration, then complete original events with stable equal-time ordering and interval-relative offsets. Half-open intervals retain end-boundary events; explicit duration preserves trailing silence.
- Logical media advancement through every original event despite output refusal. Reports distinguish attempted/accepted packets and first failure, with no retry or partial-delivery success claim. The processor must finish other required work and propagate output errors to runtime recovery.
- Continuous input tracking while stopped, capture-start initial state, original-packet staging into an already-reserved group, and recovery-form input independent of ordinary input limits. Invalid input leaves tracking unchanged; staging faults do not retroactively undo valid observed input.

Six guarded tests verify:

1. An independent absolute packet oracle over whole/single/irregular partitions: initial held note, 257-byte SysEx crossing pages, equal-time note/pressure records, original release velocities and note-on-zero, bank/program changes, trailing silence and one-shot stop cleanup.
2. Exact note ordering at seeks, wraps and replacement: cleanup, restoration of state before the target, then original events at that same offset. The replacement has a distinct initial held note and a release at offset zero.
3. Input observed while stopped supplies a captured take's initial state. Frozen state/packets are readable immediately, with the interval's note-off excluded from the initial state but present at offset zero; staging publishes nothing before commit.
4. Position overflow, discontinuity/continuity mismatch, stale lease, malformed interval bounds, capacity and preparation refusals; zero frames preserve renderer/tracker state.
5. A real `GraphRuntime` producer→consumer graph: two original note-ons advance despite output overflow; reserved reset packets clear a seeded downstream held note even with only one ordinary event/three ordinary bytes admitted by the tracker; audio remains exactly 0.25; all 32 host reset packets drain. This is an operation/runtime integration proof, not the full loop facade or an independent loop host.
6. **128 one-sample intervals** driven by the production resolver with journal capacity zero, compared against an independent complete-packet oracle over three host partitions. There is no mandatory timing quantum or journal dependency.

| Fresh scoped gate | Observed result |
| --- | --- |
| Loop native debug/release | **73 passed per profile** |
| Loop Node/Chromium | **73 passed per runtime** |
| Prescribed workspace nextest | **2044 passed**, four unchanged opt-in canaries skipped |
| Selected loop Miri | **20 executions**: audio6, media-decisions4, restoration4, indexed-MIDI6 |
| Formatting, warning-denied workspace build/rustdoc, boundary/test/tracing/Wasm-dependency policies | Passed; all **15 scoped gates** succeeded |

The first focused native run aborted with a stack overflow while processors contained large inline MIDI projections. The revised representation puts those projections in fixed prepared boxes, reducing their footprint in generic processor/command types without raising stack limits, reducing workloads or weakening heap guards. The exact overflowing stack frame was not captured, and the pre-fix source tree was not separately archived; the failure log and change rationale are retained rather than presenting it as a passing run. The subsequent focused run passed; the final complete-packet workload passed in all scoped configurations above.

No generic runtime implementation changed. Standalone runtime release/Wasm, generic Miri, native/actual-worklet fixtures and lifetime/authority prototypes were **not freshly rerun** here; the preceding full `midi-restore-stage5` milestone remains their historical baseline. Twenty fresh loop Miri executions are not a fresh 131-case combined run. The full loop planner/processor adapter, coherent mixed-media binding/capture/export and public outcomes, separate-node playback→FX→recording graph/example, and independent loop AudioWorklet remain required. Stage 5 and final delivery remain open.

## Stage 5 — Reserve a new take's first interval before activation

Milestone: `feat(rt-runtime): reserve capture intervals before activation` (the commit introducing this section), source base `23a1f799`. Evidence: [`evidence/prepared-interval-stage5.json`](evidence/prepared-interval-stage5.json) and [`evidence/prepared-interval-stage5-provenance.json`](evidence/prepared-interval-stage5-provenance.json). Logs and archived Miri cases are under `target/loop-layer-prepared-interval-stage5/`.

Facade integration exposed an ordering gap: the original interval reservation API required an active take, so discovering an initial append refusal could follow publication of new active revisions. `reserve_prepared_capture_interval` and planner `reserve_prepared` now use the existing fixed pools to reserve all first-interval page/checkpoint capacity without publication. `PreparedReserved` rejects staging/commit, preserves current content and revisions, and preflights both activation and append revision increments. Exact-frame activation retains the same interval token/credits and transitions to `Reserved`; cancellation returns to `Prepared` without losing lifecycle reservations. The legacy activate-then-reserve path remains available. Preparing a new take clears obsolete previous-take frame metadata.

Three new guarded tests verify:

- Old indexed audio/MIDI revisions remain current through reservation and rejected staging/commit/wrong-frame activation. Exact activation then staging/commit/freeze produces independent expected bytes, initial MIDI state, packet state, duration and revisions; old leases remain unchanged. Unused conservative reservation credit and every pool recover.
- **32 alternating two-group rounds** with a five-page store and an independent three-page-per-interval byte ledger. Competition refuses atomically; cancellation makes capacity available to the other group; stale tokens cannot cancel replacements; every active resource revision stays unchanged.
- Foreign scope, frame overflow, zero/oversized intervals, planner window and wrong activation frame, plus insufficient MIDI-checkpoint capacity, all refuse without publishing active media or losing credits.

Real transport disconnect coverage grows from fourteen to **seventeen cuts**, retaining the original workload and adding prepared-first-interval reserved/cancelled/activated states. Recovery is queued before disconnect, then delivered to cancel/settle owners; this still does not claim automatic musical stop on disconnect.

| Fresh gate | Observed result |
| --- | --- |
| Runtime native debug/release | **174 passed per profile** |
| Loop native debug/release | **73 passed per profile** |
| Runtime Node/Chromium | **170 passed per runtime** |
| Loop Node/Chromium | **73 passed per runtime** |
| Prescribed workspace nextest | **2047 passed**, four unchanged opt-in canaries skipped |
| Combined selected Miri | **134 fresh executions**: 114 runtime plus twenty loop cases |
| Lifetime/authority prototypes | Positive builds/runs and deliberate **E0502/E0599** refusals |
| Format, warning-denied build/docs, examples and policy/boundary gates | Passed |
| Existing native debug/release and actual offline/live worklet, both failure canaries | Passed; historical golden journal unchanged |

The focused run and all 21 broad gates passed without a new failed run. The three new cases and expanded disconnect workload are unchanged under native/Wasm/Miri. This fresh full run supersedes the preceding scoped milestone's lack of fresh runtime/host gates without rewriting that historical scope. The full loop facade, coherent mixed-media publication/export, separate-node graph/example, independent loop hosts, final CI and review remain pending. Stage 5 remains current.

## Stage 5 — Autonomous capture preflight decisions

Milestone: `feat(loop-engine): refuse autonomous capture starts transactionally` (the commit introducing this section), source base `813f8311`. Evidence: [`evidence/capture-decisions-stage5.json`](evidence/capture-decisions-stage5.json) and [`evidence/capture-decisions-stage5-provenance.json`](evidence/capture-decisions-stage5-provenance.json); logs and archived Miri executions are under `target/loop-layer-capture-decisions-stage5/`.

`Observation::CaptureUnavailable { target, reason }` adds a primitive-target refusal for capture starts/restarts, including autonomous trigger/composite work that has no request ID. It preserves the previous mode/content/capture revision and emits no capture-start pulse. Running-take append exhaustion remains a separate input; both classes can coexist at a boundary. Request-family refusal remains distinct and takes precedence regardless of observation order. Target-only refusal intentionally does not reject a parent composite or siblings. The checked observation bound is now two failure classes per loop plus retained request refusals; no new musical-work quota or external receipt is introduced.

Four guarded tests verify:

- Autonomous capture refusal preserves ongoing playback, suppresses the rejected start's descendant pulse, and does not appropriate a coincident unrelated request.
- **128 one-sample intervals**, all four external request slots occupied/unread, three host partitions and journal capacities **0/1/1024**. Repeated previews preserve states/flags; an independent arithmetic refusal-frame oracle and full retained journals agree across partitions.
- Restart refusal alone continues the old recording; simultaneous running-take exhaustion preserves monitored playback and the old prefix. Failed starts do not consume logical capture revisions. Stop remains usable. Both per-loop failure classes fit even with only one external request slot; request-family error precedence is independent of observation order.
- Whole composite request-family refusal versus targeted child capture refusal, plus stale/composite/duplicate observation rejection.

Fresh scoped gates: **77 loop tests per native profile and per Node/Chromium runtime**, **2051 workspace passes/four existing skips**, and **24 selected loop Miri executions** (audio6, media-decisions4, capture-decisions4, restoration4, indexed-MIDI6). All fifteen scoped gates, including warning-denied workspace build/docs, formatting and policy/boundary checks, passed. Complete new workloads run unchanged under native/Wasm/Miri.

The initial test fixture used integer `Entry.duration` values rather than `Option<u64>`, producing **E0308**; changing them to `Some(2)` corrected the fixture without reducing workload/assertions. Its failed compile log is retained. No generic runtime implementation changed here; standalone runtime release/Wasm, generic Miri, lifetime/authority prototypes and native/actual-worklet fixtures were not freshly rerun. The preceding `prepared-interval-stage5` full run remains their historical baseline. Twenty-four fresh loop Miri executions are not a fresh 138-case combined run. Complete resource-to-resolver wiring, coordinated mixed-media ownership/export, separate-node graph/example and independent loop hosts remain required; Stage 5 stays open.
