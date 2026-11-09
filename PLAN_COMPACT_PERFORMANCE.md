# Plan: compact render-path performance follow-up

Implementation plan based on the completed compact variant at PR #960 head
`4a293b519fac35748267b05ae754a93f4c894bbe`. The measured Pi 4 baseline uses
source `cc9549786746999bfdfbf2cbee6a020a16b03ef1`; reproduce it on the current
head before attributing a change. This plan follows
[write_plan.md](.agents/prompts/write_plan.md) and leaves the completed
[compact variant plan](PLAN_FOURTH.md) and existing result archives intact.

## Goals and scope

Improve the existing `compact` audio-only render path, to a reasonable extent,
in four areas:

1. Avoid steady audio-graph work for stopped loops that cannot produce, capture
   or observe audio in the current quantum.
2. Reduce repeated routing, clearing and processor-dispatch work for compact
   graph operations that still run.
3. Reduce measured per-host-callback overhead, especially at a 32-frame callback.
4. Reduce compact recording's per-quantum resource/index work without changing
   capture ownership or publication semantics.

Use profiling and small experiments to choose the implementation inside each
area. Prefer a few local fast paths over a new graph/runtime framework. Preserve
all supported compact workflows, sample results, RT constraints and ordinary
MIDI-enabled behavior. Changes to the other three audio algorithms, public
protocols, session behavior, storage formats, broad DSP fusion, a scheduler
rewrite and a hard-real-time/WCET claim are outside scope.

The Raspberry Pi 4 PREEMPT_RT system is the sole performance target: all timing,
profiling and performance A/B evidence runs there. Build AArch64/Cortex-A72
binaries on the laptop in the existing WSL cross-build environment, then transfer
and verify their hashes on the Pi. Protect concurrent laptop use by confining every
build, test and build-script child to at most two WSL CPUs at low priority, limiting
all known build/test parallelism to two jobs, and never running CPU-intensive tool
calls concurrently. Laptop execution is for correctness checks only and cannot
satisfy a benchmark or performance acceptance item.

### Evidence motivating the work

The dense Pi 4 workload has eight stereo tracks, 64 allocated grid loops and 32
active loops. At Q=32, compact executes 216 node operations and about 894 route
forwards per callback. Idle still costs 397 us p50 against a 667 us deadline.
Playback, recording and grab cost 527, 661 and 507 us p50. Compact already uses
16 times fewer resource steps than chunks in the compared idle/record cases and
copies far less logical resource data than current, so this follow-up must look
beyond the resource representation alone.

The checked-in benchmark remains the reproducible starting point, but the dense
8x4 adaptation and raw Pi evidence are currently external artifacts. Stage 0
must make the exact workload replayable from the branch or retain a versioned
helper and hash with the measurement evidence before product optimization starts.

## Immutable acceptance criteria

These goals, scope and criteria require explicit user approval to change.

1. **Behavioral parity:** compact produces the same supported samples, capture
   cuts, loop states, transition/completion timing, meters and media readback as
   the pre-optimization compact implementation across callback partitions and
   Q=1/32 oracles. Stop/start, pending transitions, ramps, monitoring, record,
   replace, grab, continued capture, retained readers and topology changes retain
   their existing semantics.
2. **Safe dormant work removal:** a loop operation may be skipped only when its
   outputs are provably silent and it has no capture, replacement, history,
   monitoring, meter, ramp, publication or transition side effect for that
   quantum. Routine play/record/stop changes do not rebuild or allocate a graph.
   Dense idle and partially active tests demonstrate fewer executed compact
   operations than the baseline while active results remain exact.
3. **Bounded compact execution:** all new callback-reachable state is prepared and
   fixed. Rendering remains allocation/deallocation-free, nonblocking and free
   of locks, waits and I/O. Fast paths have bounded work and explicit fallback;
   capacity refusal and fault behavior remain truthful.
4. **Executor simplicity:** routing/dispatch changes stay within the existing
   compact executor and its narrow processor/planner contracts. Do not add a
   second planner, duplicate loop state, a generic optimization framework or
   mode-specific public graph vocabulary. Existing non-compact execution and
   arbitrary processors retain their behavior.
5. **Callback and diagnostics integrity:** optimize only callback costs confirmed
   by phase/function evidence. Audio-only paths may omit genuinely absent MIDI or
   host work, but normal MIDI-enabled hosts remain unchanged. Callback timing,
   drops, faults and work counters remain available and correctly defined; any
   measurement overhead is quantified rather than hidden by disabling evidence.
6. **Recording ownership:** recording optimization preserves atomic capture-group
   admission, immutable retained media, cancellation rollback, exact common
   captured prefixes, continued recording and bounded reclamation. It does not
   replace the fixed-depth compact tables or introduce a second media format.
7. **Measured improvement:** performance acceptance uses only matched release
   runs on the Pi 4. In the dense 8x4 Q=32 workload, median run p50 improves by at
   least 10% in each of idle, playback, record and grab, and aggregate active-
   scenario deadline overruns improve by at least 20% versus a reproduced current-
   head baseline. The Q=256 dense matrix has no greater than 5% p50 or p99
   regression in any scenario. All runs retain zero dropped records/render faults
   and pass state/media checks. Baseline and final measurements use four matched
   repetitions with short equal-duration windows; report raw distributions and
   do not present the result as WCET.
8. **Regression and delivery:** applicable native, lower-crate, Wasm compile and
   real-host tests pass on the final PR head. Required CI is green, available
   automated review is addressed through approval, documentation/counters match
   the implementation, and the PR is left unmerged with a clean worktree.

## Design rules and constraints

- Build all benchmark/profile binaries on the laptop with the pinned Rust toolchain
  and AArch64/Cortex-A72 flags. Run local builds and tests through one documented
  wrapper or command using `taskset` for no more than two WSL CPUs, `nice` for low
  priority, `CARGO_BUILD_JOBS=2`, at most two nextest/test workers, and equivalent
  two-job limits for CMake/Ninja/Make/Rayon. Do not launch CPU-intensive tool calls
  concurrently. Record source and binary hashes and verify the transferred
  executable on the Pi before every evidence run.
- Run every benchmark and profiler on the Pi. Do not use laptop, WSL, emulation or
  cross-build execution timings as performance evidence. Preserve the established
  CPU 0 controller / CPU 1 `SCHED_FIFO/80` render placement, PREEMPT_RT checks,
  temporary RT-bandwidth setting and restoration, governor, thermal/throttle and
  process-cleanup evidence.
- Keep iterative benchmarks deliberately short and compact-only: normally one or
  two repetitions and only the affected scenarios, with 64–128 selected Q=32
  callbacks or an equal audio duration at Q=256. Use these runs to reject ideas,
  not to claim tail performance. Reserve four matched repetitions and 256 Q=32
  callbacks per run (32 at Q=256, the same 0.171 s of audio) for the reproduced
  baseline and final acceptance matrices.
- Profile before selecting a mechanism. Use a new, explicit profiling experiment
  with a settle time accepted by its tooling; do not reinterpret the earlier
  contradictory 0.5-second/5-second instructions. Keep profiling runs separate
  from timing evidence.
- Treat current counters as clues, not hardware measurements. Capture render-TID
  cycles, instructions, branch/cache misses and function/phase samples where the
  Pi PMU supports them. Use Perfetto or narrow temporary phase counters when a
  call graph cannot distinguish planner, graph, resource and host-cycle costs.
- Prefer dynamic no-op classification or a prepared sparse execution decision
  over rebuilding graph topology for ordinary mode changes. The planner remains
  timing authority; a skip decision cannot maintain a competing loop simulation.
- Preserve the compact reference-forwarding model. Specialize only measured,
  common built-in operations; arbitrary processor fallback stays available.
  Avoid broad node fusion unless a smaller routing/clearing/dispatch change is
  measured and found insufficient.
- Keep work counters comparable. Add executed/skipped-operation or phase counters
  only when needed to prove a hypothesis, and document their units. Diagnostic
  collection must not become an unmeasured optimization variable.
- Change one major cost center at a time and rerun the short focused Pi benchmark
  between stages. Retain unsuccessful experiments in notes/results, not as dead
  production abstractions.
- Preserve crate boundaries, `no_std + alloc` support and existing unsafe-code
  policy. Do not reformat unrelated code or mix this work with unfinished items
  in `PLAN.md`.

## Likely change points

| Area | Responsibility in this plan |
| --- | --- |
| `rt_runtime/src/executor_compact.rs` | Prepared compact operations, skip/run decision, route/clear/dispatch loop and operation counters. |
| `rt_runtime/src/{executor,processor,audio_refs}.rs` | Narrow activity/fast-path contracts and callback phase accounting if evidence requires them. |
| `shoop_loops/src/runtime/{processor,planner}.rs` | Authoritative playback/capture activity and recording fast paths without duplicated state. |
| `rt_runtime/src/resource/{group,shared_audio}.rs` | Measured compact capture/index update and publication costs. |
| `shoop_rt_engine/src/node.rs` | Built-in compact node classification and reference publication behavior. |
| `shoop_rt_host_native/src/{cycle,benchmark}.rs` | Measured outer-callback/audio-only and benchmark-recording overhead only. |
| Existing runtime/loop/engine tests and `shoop_host/examples/chunk_benchmark.rs` | Exact behavior, RT guards, replayable dense scaling and matched performance evidence. |

Placement may change when profiling provides a better owner, but changes should
remain on the shortest sound path and respect the rules above.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

Stages are dependency-ordered. Keep intermediate commits building. Before Rust
commits run `cargo fmt --all` and a warning-denying build; when Rust tests change,
run `python3 scripts/check_shoop_test_usage.py`. All local builds and tests use the
two-CPU, low-priority limit above, including final release binaries and nextest.
Use focused tests while iterating, the complete applicable gates before behavior-
changing pushes, and CI
for the cross-platform matrix. Do not merge autonomously.

## Stage 0 — Branch, reproduce the baseline and locate costs

Depends on: none.

- [x] Create `perf/compact-render` from current `proto/audio-compact` head. Preserve
  unrelated work, record the exact base SHA/toolchain, and commit this plan.
- [x] Make the dense 8x4 workload replayable without embedding product behavior in
  the benchmark: eight actual tracks, eight allocated cells per track, 0/32 active
  loops by scenario, 64 MiB loop arena, Q=32 and buffer 32/256. Retain helper source,
  binary hashes, machine/scheduling data and raw callbacks outside ordinary source
  history where appropriate.
- [x] Add and verify one laptop build command/wrapper that pins the build and every
  child to at most two WSL CPUs at low priority, caps Cargo/CMake/Ninja/Make/Rayon
  and local test runners at two workers, and cross-builds the warning-denying
  release host/launcher for
  AArch64 Cortex-A72. Never benchmark that build locally. Transfer only checksummed
  artifacts/source metadata to the Pi and verify architecture/hash before launch.
- [x] Reproduce current-head Q=32 and Q=256 compact results on the Pi 4 with four
  matched repetitions, 256 and 32 selected callbacks per run respectively, and the
  established PREEMPT_RT CPU/FIFO setup. Verify topology, states, media, callback
  continuity, drops, faults, temperature and throttling.
- [x] Run separate short compact idle/play/record profiles on the Pi after at least
  a supported 5-second settle: capture 3–5 seconds of render-TID `perf stat`, call
  stacks with frame pointers where reliable, and available tracing phases. The standalone host exposes no direct Perfetto
  capture control; record that limitation and use narrow temporary phase counters
  only if PMU/call-stack evidence is insufficient. Measure profiler and callback-
  recorder overhead independently.
- [x] Record a ranked cost model covering planner/tick work, compact graph routing,
  processor/reference calls, resource recording and native callback setup. Choose
  the smallest candidate for each later stage; document unsupported PMU events or
  ambiguous samples instead of guessing.
- [x] Verification: focused baseline tests and both dense matrices pass unchanged;
  the profile is repeatable enough to justify Stage 1–4 choices. Update later
  implementation checklists if evidence changes placement or ordering, without
  changing goals or acceptance criteria.

## Stage 1 — Skip semantically dormant loop work

Depends on: Stage 0.

- [x] Define a planner-owned, quantum-scoped activity result for compact playback,
  capture and any associated fader/observation work. Include pending transitions,
  ramps, monitoring, history/capture, replacement, meters and publication needs.
  Research whether the safest implementation is a processor no-op result, a
  prepared operation predicate or a small combination; do not recompile topology
  on routine mode changes.
- [x] Teach compact execution to bypass only proven dormant built-in loop work and
  leave its already-reset wires silent. Preserve arbitrary processor fallback and
  execute any operation whose side effects are uncertain.
- [x] Expose comparable executed/skipped-operation evidence and keep node/route
  counters truthful. Avoid scanning all loop state a second time merely to skip it.
- [x] Verification: exact sample/state tests cover empty stopped loops, stopped
  loops with retained media, stop/start in one quantum, delayed/synced transitions,
  smoothing, mute/monitor/meter behavior, record/replace/grab and removal/recreation.
  Heap/destructor guards pass. A short compact-only Pi idle/active A/B shows the
  intended operation reduction and no timing regression before committing the
  stage.

## Stage 2 — Simplify the remaining compact operation loop

Depends on: Stage 1.

- [x] Profile the reduced graph again. Remove only demonstrated repeated work in
  input/output clearing, route setup, slot lookup or processor/reference dispatch.
  Candidate techniques include prepared typed built-in operations, narrower clear
  ranges or generation-based reuse; select the least invasive option supported by
  evidence.
- [x] Keep compilation on graph/topology activation, preserve generation checks and
  use the generic processor path for uncommon or unsupported cases. Do not fuse
  observation points or alter gain/ramp products to save work.
- [x] Keep transient arena ownership, silent defaults and output publication
  explicit so stale references cannot survive a skipped clear or changed graph.
- [x] Verification: compact graph structure/order tests, arbitrary processor tests,
  graph activation/replacement, host input/output routing, ramps and partition
  invariance pass. Compare phase counters and a short Q=32 dense idle/play Pi run
  against Stage 1 before retaining the change.

## Stage 3 — Remove measured outer-callback overhead

Depends on: Stage 0; normally follows Stage 2 so graph cost is no longer masking it.

- [x] Split the measured Q=32 callback cost into native driver/cycle setup, runtime
  begin/end work, scheduler/planner work, diagnostics and benchmark publication.
  Confirm which portion exists in production with benchmarking disabled.
- [x] Implement at most the few material fast paths supported by that evidence—for
  example omitting empty MIDI bridge work in the prepared audio-only profile,
  reusing prepared callback metadata, or reducing redundant diagnostics snapshots.
  Preserve normal MIDI mode, fault silencing, recovery, phase publication and one
  truthful benchmark record per callback.
- [x] If OS/IRQ wakeup cost or irreducible timing instrumentation dominates, retain
  no speculative code change; document the bound and move optimization effort to
  the measured engine phases.
- [x] Verification: native dummy plus available JACK/CPAL callback tests cover
  audio-only and MIDI-enabled configurations, faults and shutdown. A/B recorder
  off/on measurements remain explainable, and Q=32 gains persist when timing
  evidence is enabled.

## Stage 4 — Reduce compact recording index/update work

Depends on: Stages 1–2 and the Stage 0 recording profile.

- [x] Attribute recording's extra resource steps to capture-group bookkeeping,
  shared-page writes, flat root/leaf updates, publication or reclamation. Inspect
  costs across 1, 8 and 32 simultaneous recordings to distinguish fixed and
  per-writer work.
- [x] Apply one bounded optimization to the dominant routine. Prefer reuse of
  exclusive prepared roots/leaves, batched group bookkeeping or deferred/coalesced
  metadata publication within existing timing semantics. Do not redesign compact
  storage or make callback work depend on recording length.
- [x] Preserve admission credits and rollback for partial group failure; retained
  views and exports cannot observe mutable or prematurely published media.
- [x] Verification: recording sample/cut oracles, simultaneous members, continued
  recording, replacement, cancellation, exhaustion, history wrap, retained readers
  and bounded reclamation pass under heap guards. Resource-step evidence and dense
  record timing improve without shifting work into unbounded later callbacks.

## Stage 5 — End-to-end validation, evidence and delivery

Depends on: Stages 1–4.

- [ ] Run formatting, warning-denying workspace build, test-attribute check,
  affected runtime/loop/engine/native suites and the complete native nextest gate
  from `.agents/info/test.md`. Compile affected lower crates for their supported
  `no_std`/Wasm targets and run applicable browser/worklet smokes through CI.
- [x] Exercise the normal app/service/RT/native-host path for compact record,
  playback, grab, continued capture, ramps, routing, track churn and media readback.
  Smoke ordinary MIDI-enabled startup and the other three render variants for
  regressions; keep performance comparisons compact-focused.
- [x] Produce the final release binaries on the laptop under the two-CPU,
  low-priority build limit, transfer them to the Pi and verify architecture and
  hashes. Repeat the short Stage 0 profile and four-repetition Pi 4 compact-only
  dense Q=32/Q=256 matrices using 256/32 selected callbacks per run. Compare raw
  distributions, phase/work counters, overruns, memory reservations, temperatures
  and scheduler evidence. Check every immutable performance threshold and
  investigate regressions rather than removing outliers.
- [x] Update architecture/performance documentation and the visual/raw evidence
  report with baseline/final commands, hashes, successful and rejected approaches,
  counter definitions and limitations. Make no Pi 5 or WCET claim from Pi 4 data.
- [ ] Push milestone commits and open a PR against `proto/audio-compact`. Monitor
  required CI to a final green head, investigate failures, and repeatedly check
  and address available automated review feedback until approval. Report unavailable
  review honestly, map evidence to criteria 1–8, leave the PR unmerged and finish
  with a clean worktree.

## Testing cost and confidence

Stages 1–4 use focused unit/oracle tests and very short compact-only Pi A/B runs
after each change; this limits development interruption while catching semantic
mistakes at the owner being changed. Laptop correctness builds and tests are capped
at two low-priority WSL CPUs and never overlap. Exact sample/capture and heap-guard tests
provide confidence that fast paths do not change music or RT behavior. Four short
matched repetitions on the Pi, the broad native nextest gate, real-host workflows
and cross-platform CI are reserved for Stage 5, avoiding repeated matrices before
the design stabilizes. Performance thresholds are checked only with matched Pi
release evidence, never with laptop timings or flaky unit-test timing assertions.

## Execution notes

- Stage 0 baseline: 32 validated compact Pi runs, four repetitions at Q=32/Q=256,
  zero drops/faults. Render-thread 99 Hz call stacks had no lost samples; PMU IPC
  was 0.55�0.64. The standalone host has no application Perfetto capture control.
- Stages 1�2: compact skips inactive capture, settled stopped/no-source playback,
  and settled silent faders after clearing outputs. Dense executed operations fell
  from 216 to 21 idle, 85 playback/record and 87 grab. An initial stale-output
  experiment failed exact samples and was rejected before the output-clear fix.
- Stage 3: audio-only preparation skips the empty per-node MIDI callback sweep.
  Native dummy/direct tests pass. CPAL/JACK contract tests pass in the full native
  gate; the separate real-server-only JACK test is unavailable without `jackd`.
- Stage 4: compact append removes one redundant release lookup, retaining a debug
  invariant and passing all 96 runtime tests plus capture/cancellation/wrap tests.
- Final Pi evidence: Q=32 p50 reductions are 41.9% idle, 22.8% playback, 20.2%
  record and 21.1% grab. Active overruns fell 545 to 112. Every Q=256 percentile
  improved. All 32 final runs passed with zero drops/faults. Evidence and hashes
  are recorded in `docs/design/chunks/COMPACT_PERFORMANCE.md`.
- Local full nextest ran 3,192 tests under the two-CPU cap: 3,191 passed and one
  unrelated LAN-profile startup test timed out reproducibly after 15 seconds in
  the constrained environment. CPAL, JACK, dummy, app workflow and changed-crate
  tests passed. Wasm builds and dependency boundaries pass; the local Nix shell
  lacks the thumb no-std target, which remains a CI gate.
