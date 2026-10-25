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
| LL-A2 | Generic synthetic planner dependency/view tests; resolver ownership tests; graph receiver validation | Pending |
| LL-A3 | Same-frame A → B → C and nested cascades; one-sample loops exceeding old subdivision allowances | Pending |
| LL-A4 | Semantic progress proof; valid mutual sync; rejected instantaneous recursion; generated valid/invalid programs | Pending |
| LL-A5 | Independent reference journals/audio/MIDI across callback partitions and declaration orders; exact end-boundary cases | Pending |
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

Milestone: `feat(loop-layer): add portable crate and prepared timing compiler` (the commit introducing this section). Evidence: [`evidence/compiler-stage2.json`](evidence/compiler-stage2.json). Source hashes identify the tested implementation on top of the Stage 1 commit; raw logs are under `target/loop-layer-compiler/`.

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

These tests validate compilation and lookup only. They do **not** establish resolver interval/partition invariance, callback cascade execution, reliable pending requests, resource transactions, or loop AudioWorklet behavior. Compiler preparation is allowed to allocate; the guard does not falsely impose callback constraints on it. Stage 2 remains open for the resolver, fixed scratch/state, synthetic observations, independent musical simulator, and guarded rejection/replacement paths.
