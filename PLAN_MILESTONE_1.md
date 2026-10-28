# Milestone 1: native engine composition and control slice

## Status, purpose and execution contract

**In progress: Stage 0 baseline verified; Stage 1 composition work pending.** Written on 2026-10-27 from
[METAPLAN_SHOOP_INTEGRATION.md](METAPLAN_SHOOP_INTEGRATION.md). This milestone
combines the initial parity/architecture exercise with the native local-composition
slice. Browser/IPC deployments of the resulting service belong to the following
milestone, not this one.

The long-term objective is ShoopDaLoop feature parity with no noticeable change to
intended user behaviour, while enabling capabilities such as separate UI/engine
processes. This milestone proves an extensible local control boundary, not full
application integration or feature parity.

The executor implementing [PLAN_LOOP_LAYER.md](PLAN_LOOP_LAYER.md) is still
running. It is expected to make almost no further loop-layer changes, especially
API changes, but that expectation is **not a freeze or permission to edit its
work**. Apply the concurrency and pause rules below throughout this plan.

Execution contract:

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
- Follow repository instructions, including build environment, test attributes,
  formatting, warning-denied builds and pre-commit/pre-push gates.

## Progress

- Worktree: `/tmp/shoopdaloop-milestone-1`, branch
  `feat/milestone-1-engine-service`; the shared checkout remains untouched.
- Pinned initial loop/runtime revision: `d038c208f46c98ac8d7561ddcf09ff7c8e7a9d94`.
  Final completed-upstream agreement/integration is still pending.
- Baseline: 341 native tests passed with warnings denied, and both dependency
  boundary checks passed for their four configured targets. See
  [integration notes](docs/integration/MILESTONE_1.md) for commands and limitations.
- No Rust implementation or protected-source change has been made. The confirmed
  typed-control gap requires the Stage 1 public-extension feasibility spike.

## Goals and scope

Build a native headless service containing one logical audio/MIDI loop and an
independently supplied DSP processor in one runtime signal graph. Its caller can
prepare/import initial content, record, immediately play, seek/select retained
content, issue a meaningful typed DSP operation, query state/meters, export a live
recording prefix, perform a supported route change, stop and reclaim ownership.

Include a bounded parity inventory and a fixed-latency dry/wet capture experiment.
The latter establishes concrete compatibility findings; it does not promise full
latency compensation or prerecord implementation.

Included technical work:

- A small independently useful composition/control service and separate DSP crate.
- Native typed client/render endpoints, explicit admission and terminal outcomes.
- Prepared graph/DSP configuration activation using supported public mechanisms.
- Shared atomic meter publication, coherent loop queries and public export/read APIs.
- Deterministic executable scenarios, guarded tests and measured performance.
- Native/Wasm library compatibility without a new browser communication protocol.

Excluded: production UI/backend/device switching, complete session semantics,
plugin hosting, persistence/undo/scripting implementation, general dynamic module
loading, a full wire protocol, IPC/network execution, a new AudioWorklet application,
shared-memory acceleration and full legacy loop-mode/latency compatibility. Keep
working production backends unchanged. Startup imports are required; arbitrary
live uploads/adoption are not an additional deliverable.

## Immutable acceptance criteria

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

## Concurrent executor: ownership and mandatory pause rules

1. **Use an isolated worktree and a dedicated branch**, based on an agreed known
   revision. Do not switch/reset/stash the other executor's working tree. Arrange
   access to the relevant design documents without sweeping unrelated uncommitted
   files into this branch. Record the dependency revision and baseline commands.
2. Treat `src/rust/shoop_loop_engine/**`, `tests/loop_engine_host/**`, the loop-host
   runner, `PLAN_LOOP_LAYER.md`, and its implementation/evidence documents as owned
   by the loop executor. Do not edit, reformat, regenerate, weaken tests in, or
   commit those paths as part of this milestone. This includes private code and
   incidental formatter output, not just public API changes.
3. Treat `src/rust/rt_runtime/**`, its fixtures and shared test/build/CI machinery
   as coordination-sensitive. Prefer new additive modules. Before changing shared
   foundations, obtain explicit scope agreement and check for concurrent edits.
   A generic runtime extension must not break the protected loop API or depend on
   Shoop/domain types. Do not slip one in as a convenient implementation detail.
4. If compilation, tests or measurements fail because the loop layer or its
   fixtures are in progress, **stop dependent work and notify the user/owning
   executor**. Record the command, source revision and failure. Do not repair the
   loop implementation, copy its internals, change its API, weaken expectations,
   disable the failing gate or declare a pass using a different revision.
5. When attribution is unclear, reproduce only on an unchanged agreed baseline or
   isolated worktree. If ambiguity remains, pause; do not guess and modify shared
   code. A failure caused solely by this milestone's new code should be fixed here.
6. If public APIs change upstream or local typed composition cannot be implemented
   without a protected change, pause for coordination and explicit approval of any
   prerequisite. An approved prerequisite is owned separately; resume only against
   its agreed revision. Do not quietly reduce M1-A3 to scalar parameter control.
7. Before final delivery, wait for the relevant loop-layer work to finish, refresh
   onto its agreed completed revision and rerun affected gates. While waiting,
   independent documentation/new-crate work may continue only when it does not
   rely on a failing or unstable dependency. Never combine half-applied snapshots.

## Design rules and investigated constraints

- Keep one signal graph and render owner. No second loop clock, alternate media
  store, nested engine or copy of the resolver/capture facade. Required musical
  execution must not be limited by communication quotas or internal callback cuts.
- Resolve composition before freezing a client API. Currently `GraphRuntime<P,L>`
  has one planner; `Processor<L>::operation` uses `L::Operation`/`L::Result`.
  The loop facade fixes `L = LoopPlanner`, holds its generic controller privately,
  and its node wrapper delegates parameters/process but not arbitrary external
  typed operations. A superficial enum wrapper alone does not solve this.
- Inspect supported extension seams first. A component-owned prepared control
  bridge can be considered only if its frame ordering, bounded callback admission,
  terminal ownership, cancellation/recovery and interaction with loop intervals
  are explicit. Do not build a competing generic scheduler or use shared mutable
  peer access to bypass the runtime. If no compliant external path exists, use the
  pause/prerequisite gate rather than forcing a workaround.
- DSP processing may be generic over a planner or use an integration adapter so
  the independent crate does not import `LoopPlanner`. Construction and disposal
  remain off-render; prepared state identities are generation checked.
- The application-facing service may depend on the runtime, loop engine and DSP
  component. Current-engine comparison code stays dev-only/in a benchmark host.
  Prefer safe Rust and `#![forbid(unsafe_code)]` in new production crates.
- Normal commands, reliable outcomes, recovery, telemetry and bulk reads have
  separate documented capacities/ownership. Drain owning returns and terminal
  results independently. A stalled UI must not require another normal credit to
  stop/cancel admitted work or prevent autonomous wraps.
- State queries use settled loop cuts. Meter mirrors are separate telemetry, not
  coherent cross-field transactions by virtue of using atomics. Use target-supported
  atomics and explicit publication ordering; UI read frequency must not change DSP.
- Live exports use public leases/cursors and bounded copies. Handle optional
  refusal without stopping capture; no raw store sharing, fake zero-copy claim or
  releasing an export before its pending reads finish.
- Scheduling uses authoritative engine frames. Do not split one host callback into
  extra `Engine::process` calls merely to apply DSP commands and thereby renew
  callback-wide runtime allowances. Same-frame ordering must be specified/tested.
- The preset fixture can be a deterministic stereo mixer/filter with MIDI fan-in
  and prepared presets. It stands in for plugin control, not a real plugin host.
- Update ordinary docs/examples with implementation; do not introduce a separate
  documentation audit or change existing governing loop semantics/acceptance.

## Proposed code and document locations

Confirm working names and ownership in Stage 1; these are new components, not
existing APIs:

- `src/rust/shoop_example_dsp/`: independent prepared DSP and component-native types.
- `src/rust/shoop_engine_service/`: native control/render composition, tests, and
  `examples/one_track.rs`; keep host-specific code out of portable library parts.
- `docs/integration/PARITY.md`: concise capability/owner/reference map.
- `docs/integration/MILESTONE_1.md`: scenario contract, composition decision,
  fixed-delay findings, run commands and scoped measurement results.
- `ARCHITECTURE.md`: extend only with the new responsibility boundaries/findings.

The root workspace includes `src/rust/*`; add only required dependencies/lockfile
changes. Use existing test macros, heap instrumentation and runners rather than
importing private tests or creating a new test framework. Package names below are
working names; update commands if Stage 1 chooses different names.

## Staged implementation

### Stage 0 — Isolate work and establish the dependency baseline

- [x] Confirm the loop executor's protected scope, pin the known initial dependency
      commit and inspect current public APIs. Record the typed-control gap and
      keep final completed-upstream agreement pending in Stage 7.
- [x] Create `feat/milestone-1-engine-service` in an isolated worktree; inventory
      existing changes and preserve unrelated user/agent work.
- [x] Read repository/build/test guidance and current public runtime/loop examples.
      Inspect the current `Backend` API, relevant old-engine tests and reusable
      latency/session modules; do not treat historical proposal names as APIs.
- [x] Run targeted baseline runtime/loop tests and boundary checks in the agreed
      environment. If a dependency fails, invoke the pause rules before coding.
- [ ] Push the isolated branch and open a draft PR when there is a meaningful
      owned commit; state that loop integration is dependency-gated.

**Verification:** baseline commands identify their source revision; no protected
files are changed. Branch/PR base and permitted shared-file edits are explicit.

### Stage 1 — Parity contracts and a local-composition feasibility gate

- [ ] Build `docs/integration/PARITY.md` from actual app operations and tests:
      record/play/seek/sync/composites, replacing/overdub, dry/wet, prerecord/preplay,
      latency, routing/processors/devices, state/waveforms, media replacement,
      persistence/undo and scripting. Mark unsupported/future behaviour honestly.
- [ ] Specify this slice's inputs, outputs, finite capacities, state/export cuts,
      same-frame command ordering, timing, recovery and teardown. Specify the
      native public caller API without prematurely introducing byte serialization.
- [ ] Prove the custom-operation path in a minimal external compile/test spike.
      Trace how `SelectPreparedPreset` reaches the DSP at frame F and how an actual
      terminal result returns while retaining bounded owners through stalls.
- [ ] Compare only viable public extension paths. Record the chosen composition
      and why it preserves the existing loop planner and callback allowances.
      If a protected change is required, stop and request a separately owned
      prerequisite; do not proceed with an imitation or unapproved modification.
- [ ] Define the fixed-delay probe's D-frame delay, physical/logical intervals,
      retained margins/tail needs and old-engine reference expectations. Full
      compensation is not assumed; define how a missing mechanism is demonstrated.
- [ ] Update `ARCHITECTURE.md` with the proposed service and remaining gaps; settle
      new module names and the allowed change set.

**Verification:** an external spike demonstrates a real typed operation and
receipt, or the stage is explicitly blocked awaiting approval. Review the parity
contracts before substantive implementation; no acceptance criteria are weakened.

### Stage 2 — Independent prepared DSP and its tests

- [ ] Add the independent DSP crate, component-defined preset IDs/operations/results,
      fixed pre-touched scratch/preset banks and off-render lifecycle.
- [ ] Implement deterministic stereo processing and bounded stable MIDI passthrough/
      fan-in. Include a fixed-delay mode suitable for the later probe.
- [ ] Implement the Stage 1 control path with typed preset activation/revision,
      malformed/stale request refusal and exact frame semantics. No magic float IDs.
- [ ] Add independent scalar/packet references, equal-time ordering, zero/edge frame,
      preset-transition, capacity, destructor and heap-guard tests. Add portable
      test registration and Wasm configuration using existing repository conventions.

**Verification:** native debug/release tests and Wasm build pass. Dependency
inspection proves no loop/old-engine/backend/UI dependency. Processing and normal
refusal paths perform no heap allocation/freeing; invalid requests change no state.

### Stage 3 — Engine service, native endpoints and telemetry

- [ ] Add preparation and separate typed control/render endpoints around the actual
      loop facade and Stage 2 DSP, with one graph/render owner and checked IDs.
- [ ] Build the stereo/MIDI live+playback -> DSP -> output+capture graph before the
      first callback, plus optional staged graph storage for the required route cut.
- [ ] Expose only the needed loop/DSP operations, state/export/read/release and
      explicit stop/disconnect/cleanup. Preserve requested/admitted/terminal phases
      and local owning returns; specify late and callback-end behaviour.
- [ ] Add peak-since-read mirrors with independent UI reads and loop-state
      publication via supported query/observer APIs. Avoid a meter request queue.
- [ ] Implement prepared credit limits, recovery and stopped reclamation for the
      new service-owned resources. Do not replace loop-owned lifecycle machinery.
- [ ] Test the public endpoints, including ordinary command failures, no-warm-up
      initial state and the selected graph route activation semantics.

**Verification:** guarded native tests prove local calls reach both real components,
configuration is coherent at the declared cuts and repeated state/meter reads do
not allocate or alter musical execution. Inspect protected-path diffs before commit.

### Stage 4 — Complete public scenario and media visibility

- [ ] Add `one_track` using only public service APIs and deterministic stereo/MIDI
      inputs, including initial MIDI state while stopped and equal-frame events.
- [ ] Import initial immutable content off-render; record a distinct revision;
      finish and play at an interior frame; wrap, seek and select retained original
      content; activate a DSP preset and supported fixed-role route change.
- [ ] Export a prefix while recording continues. Read audio/MIDI and initial-state
      metadata through owning buffers, continue appending, reread the same export
      unchanged, then release it and verify stale/double release refusal.
- [ ] Check all samples, absolute packets, state cuts, media metadata and terminal
      records against independently constructed expectations, not just hashes or
      the implementation's own journal. Exercise whole/single/irregular partitions
      and alternate valid declaration order with sufficient capacity.
- [ ] Stop, deliver required MIDI cleanup/recovery while callbacks still run, then
      extract/clean up and drive finite reclamation. Print summaries only off-render.

**Verification:** the example runs in native debug/release and exits nonzero on
injected sample/packet/outcome mismatch. Every new callback-reachable service path
is heap/destructor guarded; complete resource/owner recovery is asserted.

### Stage 5 — Fixed-latency dry/wet compatibility probe

- [ ] Execute the predeclared D-frame delay with impulse/ramp stereo inputs and
      timestamped MIDI markers. Keep raw physical capture and proposed logical
      windows separately observable; use independent expectations and partitions.
- [ ] Compare a narrowly matched current-engine scenario/reference, including
      start/finish edges and any required tail. Reuse pure latency mapping helpers
      only after verifying their contracts; do not import old session machinery.
- [ ] Demonstrate what supported public mechanisms can express. Report missing
      prehistory/postroll/playback-window mechanisms and their proposed owning
      layer, rather than shifting timestamps or trimming output to fake parity.
- [ ] Document the supported path and concrete gaps. If a required protected-layer
      change is discovered, pause and coordinate; a documented gap satisfies the
      probe criterion but not unrelated blocked composition acceptance criteria.

**Verification:** repeatable sample/marker evidence explains both implemented and
missing mappings. No claim of production compensation or full dry/wet parity is
made, and no loop implementation is changed to force the experiment to pass.

### Stage 6 — Adversarial lifecycle and representative measurements

- [ ] Saturate/stall normal results, meter readers and exports independently;
      exercise invalid generations/presets, late/cancelled commands, active capture,
      route rejection and repeated prepare/run/stop cycles with complete cleanup.
- [ ] Prove immediate stop/cancel recovery with normal credits full, continuing
      admitted wraps under stalled results, and truthful optional-export failure.
- [ ] Exercise disconnect during pending work/recording and an injected processor
      fault. Preserve actual published state and owners; do not replay a failed
      callback or drop final owners while rendering.
- [ ] Add a bounded two-thread native scenario; waits/coordination and joins stay
      outside guarded callbacks. The render path never waits for the controller.
- [ ] Measure release idle/control overhead, steady looping, preset bursts,
      state/meter polling and concurrent live exports. Preallocate timing/result
      storage; format/file I/O stays outside rendering. Record raw samples, median,
      p95/p99/max callback cost, refusals, transfer throughput and retained memory.
- [ ] Run a current-engine baseline on a declared common workload/configuration.
      Separate differing work/semantics from genuine overhead. Establish no
      superiority or "no noticeable difference" claim from noisy averages.

**Verification:** new adversarial tests pass under guards in debug/release;
portable cases run in Node. Raw release results and source/configuration details
are recorded, including any regression needing follow-up rather than hidden tuning.

### Stage 7 — Final integration, end-to-end validation and delivery

- [ ] Wait for the relevant loop executor to finish; integrate only the agreed
      completed revision in the isolated branch. Resolve this milestone's adapter
      conflicts locally, never by undoing upstream fixes or editing protected APIs.
- [ ] Rerun Stages 2–6 against the final integrated head, including the complete
      public example, live export, route/preset cuts, latency probe, stalls/faults,
      two-thread execution, measured common workload and total owner recovery.
- [ ] Run repository gates below and the loop/runtime regression/host commands
      required by the completed upstream plan. Do not claim prior upstream results
      cover changed source. Pause on dependency failures under the same rules.
- [ ] Add the new packages/examples to existing CI discovery where needed after
      agreeing shared workflow edits. Ensure final-head native and Wasm gates
      actually discover/run the new tests; do not count an empty selection as a pass.
- [ ] Update README/run instructions, `ARCHITECTURE.md`, parity/probe/results docs
      and this checklist. Audit M1-A1–A10 using named tests and observed results.
- [ ] Commit owned changes, push, mark the PR ready and monitor CI through green.
      Inspect any automated review and repeatedly address feedback as appropriate
      until approval; report reviewer absence/unavailability without inventing it.
      Changes after validation require relevant reruns. Report any external blocker.

**Verification:** final-head evidence covers all immutable criteria, protected work
is unchanged except separately approved upstream prerequisites, CI is green and
review status is explicit. No full feature-parity or remote-hosting claim is made.

## Verification commands and environment

Run commands in the environment selected by `.agents/info/build.md`; on Nix/NixOS
use `nix develop` or `nix develop --command ...`, and use the provisioned environment
inside CI. Confirm options/required features against the scripts at execution time.

Targeted payloads once the working package names are established:

```sh
cargo test --locked -p shoop_example_dsp -p shoop_engine_service
cargo test --locked --release -p shoop_example_dsp -p shoop_engine_service
cargo run --locked -p shoop_engine_service --example one_track
cargo run --locked --release -p shoop_engine_service --example one_track
RUSTFLAGS="-D warnings" cargo build --locked -p shoop_example_dsp -p shoop_engine_service --target wasm32-unknown-unknown
python3 scripts/run_wasm_tests.py --profile dev --runtime node --package shoop_example_dsp --package shoop_engine_service
cargo test --locked -p rt_runtime -p shoop_loop_engine
python3 scripts/check_rt_runtime_boundary.py
python3 scripts/check_loop_engine_boundary.py
```

Register new portable tests using `#[shoop_wasm_test_support::shoop_test]` and the
existing package metadata/browser feature conventions. Extend the existing Wasm
dependency check invocation for the new packages. Keep any native-only timing or
thread host outside the portable library and mark native-only tests explicitly.

Repository gates, including before behaviour commits/pushes as required by the
agent instructions:

```sh
cargo fmt --all
cargo fmt --all -- --check
RUSTFLAGS="-D warnings" cargo build --workspace
SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend --profile ci
python3 scripts/check_shoop_test_usage.py
python3 scripts/check_tracing_coverage.py --require-closed
```

Formatting runs only in the isolated worktree; if it changes protected sources,
pause rather than committing those changes. Omit `SHOOP_ALLOW_MISSING_BACKENDS=1`
for tests requiring real devices/plugins. Existing actual-worklet/application
smokes remain regressions, not evidence of a new browser service. No new device,
plugin installation or browser host is a prerequisite for the headless example.
Unrelated/upstream failures or unavailable facilities are reported explicitly;
required acceptance remains pending rather than silently skipped.
