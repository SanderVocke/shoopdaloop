# Plan: composable loop library

[Main plan](PLAN.md) · [Architecture](ARCHITECTURE.md#open-composability-decision) · [Validation](VALIDATION.md)

Direction agreed with the user on 2026-10-29. This resolves the
[open composability decision](ARCHITECTURE.md#open-composability-decision) in
favour of application-owned runtimes.

## Problem

`shoop_loop_engine::runtime::Engine` owns the whole `GraphRuntime`, its
`ResourceStore` and its lifecycle. All non-loop processors must be a single type
`P`, wrapped in the loop crate's `Node<P>`. Processor operations are typed by the
planner's operation type, and `Node<P>` does not forward them. As a result,
`shoop_example_dsp` carries its own private transport, mailbox and
`begin_callback`/`finish_callback` hooks around every engine call. A DAW-like
application with many components cannot scale that pattern. The loop's
coordination guarantees are sound; the problem is that the loop crate owns the
runtime they run in.

## Goals

1. The loop crate is renamed `shoop_loops` and offers **parts only**:
   - `LoopPlanner`: preparable on its own;
   - `LoopNodes`: the playback/capture processors and their decision bindings;
   - `LoopClient`: typed loop commands and results for a control endpoint owned
     by the application.
2. The application owns the `GraphRuntime`, the `ResourceStore`, its node type,
   the control transport and the lifecycle.
3. Loop invariants are enforced by runtime hooks the loop planner implements,
   not by owning the runtime.
4. Processor operations are typed independently of the planner, so independent
   DSP is controlled through the runtime's own scheduler.
5. The demonstrator (`shoop_example_dsp` + `shoop_engine_service`) is rebuilt on
   the parts, and the DSP's private control bridge is removed.
6. Code docs, guides, architecture, plan and validation documents describe the
   new structure. The Milestone 1 composition spike is retired.

## Scope

**In scope:** `rt_runtime` changes for goals 3–4; the rename and restructuring of
the loop crate; a dev-only test-support crate; porting every user of the loop
crate (tests, examples, benchmarks, native/Wasm/browser test hosts, service,
scripts, CI); and documentation.

**Out of scope:**
- composing several planners in one runtime;
- atomic transactions across domains (e.g. a DSP preset change and a loop change
  as one transaction);
- any change to musical or media semantics;
- new loop features, latency compensation, plugin hosting, production backend
  or UI integration;
- continuing the replacement loop book beyond keeping its existing draft
  compiling;
- the open checks of the [portable loop execution](PLAN.md#portable-loop-execution)
  workstream (full Miri recovery, Stages 7/8/10). Those remain open and will
  apply to the refactored code under its new names.

## Acceptance criteria

These must not change without explicit user approval.

- **A1 — parts only.** The workspace has no `shoop_loop_engine` crate.
  `shoop_loops` exposes no type that owns or wraps `GraphRuntime`,
  `executor::Control` or `ResourceStore`: `Engine`, `Controller`, `Stopped` and
  `Node<P>` are gone.
- **A2 — application-owned assembly.** An application builds its own
  `GraphRuntime<AppNode, LoopPlanner>`:
  - it creates the `ResourceStore`;
  - `LoopPlanner` preparation borrows that store and returns `LoopNodes`;
  - the application's own node enum holds the loop nodes next to independent
    processors.

  The service demonstrates this.
- **A3 — invariants kept.** The five invariants listed under
  [open composability decision](ARCHITECTURE.md#open-composability-decision)
  still hold. Each has a test that fails if its guard is removed. These include:
  - a generic `Commit` of a graph missing a loop node or decision binding is
    refused;
  - a generic `Install`/`Remove` that would replace or drop a loop node is
    refused;
  - paired program/graph publication;
  - node-pair reassignment;
  - capture preflight before affected samples;
  - recovery, cancellation and stopped-state reclamation.
- **A4 — independent processor operations.** `Processor` typed operations and
  results no longer use the planner's operation type. A processor crate can
  define its own operation/result types while depending only on `rt_runtime`.
- **A5 — DSP through the scheduler.** `shoop_example_dsp` has no private
  transport, no mailbox and no `begin_callback`/`finish_callback`. Preset
  selection is a scheduled runtime processor operation. Its observable behaviour
  is kept:
  - it applies at the exact sample;
  - its result reports the actual frame and revision;
  - stale revisions, unknown presets and late requests are refused;
  - it can be cancelled before delivery.
- **A6 — demonstrator unchanged in behaviour.** `shoop_engine_service` makes
  exactly one runtime `process` call per nonempty callback, with no
  per-component hooks. Its tests and examples pass with unchanged oracles:
  - `one_track`: 42 stereo frames / 487 MIDI packets / 378 bytes in every
    partition run;
  - `latency_probe`: all 18 cases;
  - stall, disconnect, fault and two-thread tests.
- **A7 — vocabulary.** "Role" no longer appears in `shoop_loops`' public API or
  docs. `Role*` becomes `NodePair*`, and the node factory is `LoopNodes`
  producing `PlaybackNode`/`CaptureNode`.
- **A8 — nothing lost.** Every existing loop, runtime, service and host test
  passes after porting, as do the examples, including:
  - native debug and release;
  - Wasm builds;
  - Node tests;
  - browser/AudioWorklet hosts with `--verify-failures`.

  Oracles change only mechanically. Any removed test is replaced by one that
  covers the same behaviour.
- **A9 — dev-only assembly.** The only turnkey assembly lives in
  `shoop_loops_test_support` (`publish = false`). It is used only as a
  dev-dependency, which `cargo tree` confirms. Teaching examples and guides
  assemble from the parts.
- **A10 — boundaries and safety.**
  - `shoop_loops` and `shoop_example_dsp` keep their dependency boundary (the
    boundary script passes).
  - `forbid(unsafe_code)` is kept.
  - All new or changed render paths are heap/free guarded in tests.
  - The test-usage and tracing-coverage checks pass.
- **A11 — documentation.**
  - `ARCHITECTURE.md` describes the new boundary, and the open decision is
    closed.
  - `PLAN.md` links this plan and updates the affected workstreams.
  - `VALIDATION.md` records the evidence.
  - All current READMEs, guides and runtime book chapters use the new API, and
    their code samples compile and run.
  - No current (non-historical) document or script refers to
    `shoop_loop_engine`, `Engine::prepare` or roles.
- **A12 — measurements recorded.** `measure_service` (idle/steady) and the loop
  benchmark are run before and after on the same machine, and the results are
  recorded in `VALIDATION.md`. Regressions are reported with an explanation;
  they do not block completion.

## Design rules

- **Keep the coordination, drop the ownership.** `LoopPlanner` remains the single
  authority for loop timing, the media ledger, the capture lifecycle, node-pair
  reassignment and loop-coupled graph publication.
- **Guards are enforced by the runtime, not by convention.** `rt_runtime` asks
  the planner to admit:
  - every generic graph publication: `prepare_with_initial_graph` and generic
    `Commit`;
  - every generic `Install`/`Remove` of a processor.

  The default is to admit everything, so planner-free users are unaffected.
  *Revised during Stage 3:* publication through a planner's own activation
  permit is checked by that planner, which holds the permit (the runtime cannot
  call back into a planner that is mid-operation). The loop planner already
  validates its own permits. Install/Remove are admitted per node, so the loop
  planner protects every loop node slot rather than only decision consumers.
- **The loop crate never stores the `ResourceStore`.** Preparation and cleanup
  borrow it. Stopped extraction uses
  `GraphRuntime::into_planner_and_resources`.
- **`LoopClient` owns no transport.** It builds typed commands and graph
  descriptions (including the loop's decision bindings) for the application's
  control endpoint, and decodes loop results.
- **Minimise churn when decoupling processor operations.** Processors without
  operations should need no new boilerplate where the language allows it.
- **Keep generated or historical material as it is.** Mechanical renames are
  committed separately from behavioural changes. `docs/history`, `*/evidence/`
  and `.pi/` are historical and are not rewritten.
- **Callback contract unchanged.** No allocation, freeing, locking or waiting on
  render paths. Fixed capacities stay finite and checked.
- **Project rules apply.** Follow `AGENTS.md` and `.agents/`. Run commands via
  `nix develop --command …` with `RUSTFLAGS="-D warnings"`. Run `cargo fmt --all`
  before committing. Run `scripts/check_shoop_test_usage.py` when tests change.
- **Docs move with the code.** Each stage updates the guides its changes
  affect.

## Stages

Order: 0 → 1 → {2, 3} → 4 → {5, 6} → 7 → 8. Stages 2 and 3 are independent.
Stages 5 and 6 both depend on 4 but not on each other.

### Stage 0 — Baseline

- [x] Record the per-binary test counts for `rt_runtime`, the loop crate,
      `shoop_example_dsp` and `shoop_engine_service`, and for the loop host runs
      (native, Node, browser), in a "Baseline" section of this file.
- [x] Record release `measure_service` idle/steady and the loop benchmark,
      using the existing collector/script defaults.

**Verify:** the counts and commands are recorded, and every run passed.

### Stage 1 — Retire spike, rename crate and vocabulary

- [x] Delete `experiments/milestone_1_composition`. Link its last commit from
      `docs/history/README.md`.
- [x] Rename `src/rust/shoop_loop_engine` → `src/rust/shoop_loops`, along with
      every dependent manifest and import, `tracing_coverage.csv`, Miri job
      lists and CI paths. Rename the matching scripts and test hosts:
      `check_loops_boundary.py`, `run_loops_hosts.py`, `benchmark_loops.py` and
      their `scripts/tests`, plus `tests/loops_host`. Update paths and links
      in the current docs. Commit.
- [x] Rename `Role*` → `NodePair*` (types, operations, replies, module, docs).
      Commit separately.

**Verify:** workspace build, fmt check and all Stage 0 suites, with identical
counts. The boundary, tracing and test-usage scripts pass.

### Stage 2 — Independent processor operations (`rt_runtime`)

- [x] Give processor operations and results their own types, separate from
      `Planner::Operation`/`Result`. Update scheduling (`Action::Processor`),
      executions and completions.
- [x] Make sure a scheduled processor operation that hasn't been delivered yet
      can be cancelled through the generic control path. Add this if it is
      missing.
- [x] Port the existing processors, tests and examples in `rt_runtime`. Update
      the book's chapters 3, 13 and 14 and the reference pages that are
      affected.

**Verify:** `rt_runtime` tests in native debug/release and Node; Wasm build; the
runtime host runs; the runtime docs check; and the Miri jobs that cover the
executor and scheduler. There is a new test of a processor with its own
operation type under `NoPlanner` and under a planner.

### Stage 3 — Planner graph-admission hook (`rt_runtime`)

- [x] Add a planner hook, allowing everything by default. It is consulted
      before every graph publication and every install/remove of a decision
      consumer.
- [x] Add tests with a test planner covering refusal of each path, including
      that a refusal leaves the epoch, the active plan and the slots unchanged.
- [x] Update the book's chapter 15 and the execution-rules reference.

**Verify:** the same gates as in Stage 2.

### Stage 4 — `shoop_loops` as parts

- [x] Make `LoopPlanner` preparable on its own. It borrows the
      `ResourceStore` and returns the planner, `LoopNodes` (processors,
      decision bindings, prepared node-pair templates) and the prepared media
      catalogue.
- [x] Make `PlaybackNode`/`CaptureNode` public processors that application node
      enums can wrap.
- [x] Implement the Stage 3 hook with the loop's graph validation, extended to
      `Install`/`Remove` of loop nodes.
- [x] Add `LoopClient`: command builders for every current operation, graph
      compile/stage helpers that add the loop's bindings, and result decoding.
- [x] Make cleanup public, taking `&mut ResourceStore`.
- [x] Create `shoop_loops_test_support` with an assembly that matches the old
      `Engine::prepare*` variants. Point the loop tests and benchmarks at it.
- [x] Remove `Engine`, `Controller`, `Stopped` and `Node<P>`.
- [x] *Added:* port `shoop_engine_service` (own runtime and node type) and
      `tests/loops_host` (support crate) far enough that the tree builds;
      Stages 5 and 6 complete them.
- [x] Port the teaching examples (`coordinated_loop` and the docs/docs-new
      examples) to explicit assembly from the parts.
- [x] Add one A3 guard test per invariant. The generic-path guards are new
      (`tests/graph_guards.rs`, each failing with the hooks disabled); the other
      invariants keep their existing tests, mapped in `VALIDATION.md`.
- [x] Update the crate README and `docs/` guides (runtime, topology activation,
      program changes, …), and keep the docs-new draft compiling.

**Verify:** all loop tests and examples in debug/release with the Stage 0
counts plus the new tests; Wasm build and Node tests; boundary script; and
`cargo tree` showing `shoop_loops_test_support` only under dev-dependencies.

### Stage 5 — Test hosts

- [x] Port `tests/loops_host` (native, wire, two-thread, browser/AudioWorklet)
      to the support crate and `LoopClient`.

**Verify:** `run_loops_hosts.py` native and browser with `--verify-failures`,
as in CI, with the Stage 0 counts.

### Stage 6 — Demonstrator

- [x] `shoop_example_dsp`:
  - remove `Client`/`RenderControl`/the mailbox;
  - implement preset selection (and anything else the bridge offered) as
    processor operations with typed results;
  - keep meters;
  - port its tests to a minimal `rt_runtime` host;
  - update its README.
- [x] `shoop_engine_service`:
  - own `ResourceStore` and `GraphRuntime<ServiceNode, LoopPlanner>`, with
    `ServiceNode` wrapping the loop nodes and the DSP;
  - send loop and DSP requests through the single control endpoint;
  - restate credits/capacities;
  - keep the stop, disconnect, fault and cleanup semantics;
  - update its README, including the fixed-capacity table.

**Verify:** all service and DSP tests in debug/release; `one_track` and
`latency_probe` debug/release with the A6 oracles; Wasm build and Node tests;
boundary checks; release `measure_service` against the Stage 0 baseline.

### Stage 7 — Architecture, plan and validation documents

- [x] `ARCHITECTURE.md`: layer table; the "Responsibilities and timing
      authority" and "Native service composition" sections; close the open
      composability decision; record planner composition as future work.
- [x] `PLAN.md`: workstream table (replace "Composable loop boundary" with a
      link to this plan and its status); "Proposed next work" item 1; new names
      in the loop workstream and Milestone 1 sections, with their criteria
      unchanged.
- [ ] `VALIDATION.md`: refactor evidence and the A12 measurements.
- [x] Sweep the current docs, scripts and CI for old names (A11).

**Verify:** a grep for `shoop_loop_engine`, `Engine::prepare`,
`loop_engine_host` and `Role` outside the historical paths finds nothing; all
document links resolve.

### Stage 8 — End-to-end validation

- [ ] Run the CI workflow's Linux command set locally:
  - workspace build with warnings denied;
  - fmt;
  - workspace nextest;
  - `rt_runtime`/`shoop_loops`/DSP/service tests in debug and release;
  - Wasm builds and Node tests;
  - loop and runtime hosts, including browser with `--verify-failures`;
  - boundary, tracing, test-usage and docs checks;
  - the CI Miri jobs for the changed crates.
- [ ] Check every acceptance criterion against the evidence, and record the
      result and counts in `VALIDATION.md` and here.

**Verify:** all A1–A12 are satisfied, with evidence linked.

## Baseline

Source `2d13b8da`, run on 2026-10-29 in `nix develop` with `RUSTFLAGS="-D warnings"`.
All runs passed.

| Suite | Debug | Release |
| --- | --- | --- |
| `rt_runtime` | 193 tests / 36 binaries | 193 / 36 |
| `shoop_loop_engine` | 155 / 20 | 155 / 20 |
| `shoop_example_dsp` | 15 / 3 | 15 / 3 |
| `shoop_engine_service` | 20 / 6 | 20 / 6 |
| `tests/loop_engine_host` crate | 60 / 3 | — |

| Host / Wasm run | Result |
| --- | --- |
| `run_loop_engine_hosts.py --native-only --profile debug` | passed |
| `run_rt_runtime_hosts.py --native-only --profile debug` | passed |
| `run_loop_engine_hosts.py --profile debug --verify-failures` | passed (offline and live contexts, failure canaries) |
| `run_rt_runtime_hosts.py --verify-failures` | passed (failure canaries) |
| `run_wasm_tests.py --profile dev --runtime node` | `rt_runtime` 189, `shoop_loop_engine` 155, `shoop_example_dsp` 15, `shoop_engine_service` 18 |

The local environment differs from the scripts' tool pins: the Nix shell has
Node 22.23.2 (pinned 22.22.2), and Chromium/chromedriver 151 from nixpkgs (pinned
147). Local runs relax only those version checks; CI keeps its pins.

Measurements on an AMD Ryzen 9 5950X, pinned to CPU 8 (`ondemand` governor).
Service: `collect.py --cpu 8` defaults (3 runs × 2048 samples, 128 warm-up),
callback median / p99 in ns:

| Workload | Baseline |
| --- | --- |
| current/idle | 1974 / 2034 |
| current/steady | 2175 / 3527 |
| service/idle | 3697 / 3797 |
| service/steady | 3036 / 7655 |
| service/preset | 3135 / 7755 |
| service/poll | 3697 / 10921 |
| service/export | 5270 / 5751 |

Loop benchmark: `benchmark_loop_engine.py --iterations 1024 --trials 3 --cpu 8`,
median of per-trial p50 in ns: loop-idle 360, no-planner-idle 90, fx-capture 4779,
seek-replace 4428, upload-export 4940, audio-4-257 13095, nested-regular 225296,
midi-4-4 859236.

## Notes for review

Interpretations made during execution that the user should confirm. They do not
change the acceptance criteria.

- **A9 and the loop test host.** `tests/loops_host` is a separate, test-only
  workspace whose library is the fixture itself (native replay, wire and
  AudioWorklet builds). It can only use `shoop_loops_test_support` as a normal
  dependency. A9's dev-dependency check is applied to the production workspace
  crates; the fixture is treated as test code.

## Execution contract

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
