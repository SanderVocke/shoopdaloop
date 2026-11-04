# Plan: inline buffered processor with block timing and test MIDI

## Status and scope

Implementation and required verification are complete. All non-macOS CI passes
on implementation revision `7c214af6`, including the merge candidate with base
`820fb239`. The user waived macOS validation because hosted ARM64 runners could
not be acquired. The implementation branch integrates base `3ca192e1`; work starts from
`feat/fixed-quantum-runtime`, investigated at `3ed8ff11`. Follow
[write_plan.md](.agents/prompts/write_plan.md), the repository mandates, and the
build/test guidance when executing the plan.

Build a reusable fixed-quantum buffered processor whose first backend executes
on the render thread: stereo audio receives 2x gain, and MIDI channel messages
move to the next channel. Real batching and scheduled output make the timing
configuration observable despite immediate computation.

Included: one portable crate, prepared timing/storage, audio and MIDI processing,
the direct tick entry point and unified runtime interface, a runnable development example, and
focused engine-composition coverage. Excluded: threads, processes, IPC/networking,
plugin hosting, public asynchronous transport protocols, latency compensation,
UI/catalog/session/protocol additions, live timing changes, and performance claims.
Existing application graphs keep their current behavior; the dummy is explicitly
constructed in development/test compositions.

## Immutable goals and acceptance criteria

These criteria must not change without explicit user approval.

- **A1 — ordinary node.** The graph invokes the processor with exactly its
  prepared Q frames. The node exposes two audio inputs, two audio outputs, one
  MIDI input, and one MIDI output. Its batching and slack are private to the node.
- **A2 — timing.** Configuration consists of positive `batch_quanta = M` and
  nonnegative `slack_quanta = S`, with Q supplied by the runtime. A processing
  block contains `B = M * Q` frames. A batch whose first input tick is b is
  completed inline at tick `b + M - 1` and starts output at tick `b + M + S`.
  Consequently the node's exact stream latency is `L = (M + S) * Q` frames.
  Zero slack retains the one-tick handoff after collection; immediate completion
  never releases output early.
- **A3 — audio.** For each channel, the first L output frames after preparation
  or reset are zero, followed by `y[n + L] = 2 * x[n]`, without clipping or channel
  mixing. The backend runs once per complete B-frame batch.
- **A4 — MIDI.** Each valid MIDI channel-voice message has its zero-based channel
  c changed to `(c + 1) % 16`; all other bytes remain unchanged. System messages,
  including SysEx and realtime messages, pass through byte-for-byte. Preserve
  message count and stable order for simultaneous events. Every event at input
  stream frame t is emitted at `t + L`, with its intra-quantum position preserved.
  Apply the same mapping to note-on, note-off, and velocity-zero note-on packets.
- **A5 — bounded execution.** Preparation allocates and touches all storage.
  Rendering allocates/frees nothing, locks/waits for nothing, and performs no I/O.
  Capacities and overflow are explicit; storage never grows during rendering.
- **A6 — equivalent integration.** The direct `process_tick` entry point and
  production `Processor::process` interface use one processing core and
  agree in audio, MIDI, and latency. Supported host partitions do not change
  stream results; host buffering latency is accounted for separately.
- **A7 — lifecycle and failures.** Reset/reprepare cannot leak buffered samples
  or MIDI from an earlier stream. Invalid timing/capacity is refused before
  allocation or rendering where detectable. Runtime capacity failures are
  explicit errors through the existing terminal-fault/MIDI-recovery contract;
  MIDI is never silently truncated or selectively dropped.
- **A8 — delivery.** Focused tests, existing required regression/portability
  gates, documentation, green PR CI, and review handling are completed with
  evidence tied to the implementation revision. At the user's direction,
  contended macOS CI is waived for this delivery; unavailable macOS execution
  is not counted as a passing test.

## Design rules and code ownership

- Add `src/rust/rt_buffered` as a sibling of `rt_runtime`: `no_std` with `alloc`,
  safe Rust, and a normal dependency on `rt_runtime` only. Test support stays
  dev-only. Use a small module set such as `timing`, `processor`, and `inline`.
  Do not add buffered-processing policy to the runtime scheduler or loop domain.
- Public entry points should be small: `Timing`, validated `PreparedTiming`,
  `BufferedProcessor<InlineTestProcessor>`, preparation, latency queries, and quiescent reset.
  `InlineTestProcessor` names the combined 2x-audio/channel-rotation backend. Timing is
  immutable after preparation. Accept non-power-of-two block multiples such as
  M=3; a processing batch size is not another `Quantum`.
- Validate checked products/sums for block frames, latency, channel samples,
  event/byte capacity, frame timestamps, and allocation sizes before allocating.
  Publish supported preparation limits. Reject requests exceeding those limits
  rather than attempting huge allocations. Count M/S in quanta, not callbacks.
- Keep batching, readiness, and scheduled release in `BufferedProcessor`; the backend
  receives complete owned/preallocated block storage and never borrowed graph
  buffers. Retain a small crate-private submit/completion seam: the inline backend
  computes on submission and marks the result ready immediately. No real queue,
  thread machinery, or speculative public async API is needed in this milestone.
- Use bounded prepared slots for one collecting block and all completed blocks
  awaiting or undergoing playback. Derive and document a conservative slot bound
  from M/S, including S values larger than M and slot reuse after playback.
  Label work with a stream generation, sequence, and source-frame range. Release
  timing follows source frames, never completion order or host callback identity.
- Each tick emits its scheduled Q-frame audio/MIDI slice, collects the incoming
  Q-frame slice, submits a newly complete block, and retains its completion for
  future release. This ordering implements A2 without early output.
- MIDI uses prepared per-quantum event and byte limits. Size each batch for M
  quanta using checked arithmetic. During accumulation, convert tick offsets to
  batch offsets; during release, subtract the selected output-slice offset.
  Preserve equal-time insertion order. Channel rotation changes neither packet
  lengths nor offsets, so normal output limits can match normal input limits.
  Preflight capacity before mutating a batch or partially publishing a result.
- Quiescent reset is legal only after callbacks stop and the existing runtime
  shutdown/MIDI-recovery obligations are settled. It clears collection/playout,
  advances the stream generation, and sets an aligned origin. Require contiguous
  processor ticks; refuse a gap/replay before mutation. A routing epoch change
  alone does not reset stream history. Live deactivation/reactivation is deferred.
- Ordinary inline operation cannot miss an asynchronous deadline. Do not add
  deadline simulation or a second recovery state machine. A stream delay of L
  frames is not a guarantee of L/sample-rate seconds of worker execution time;
  host callbacks can execute several quanta back-to-back. Future async backends
  must define readiness and missed-deadline policy explicitly.

### Concrete reference case

At Q=16, M=2, S=1, B=32 and L=48. A0/A1 are the two input quanta of batch A.

| Tick | Input | Backend action | Output |
| --- | --- | --- | --- |
| 0 | A0 | Collect | Silence |
| 1 | A1 | Process A | Silence |
| 2 | B0 | Collect | Silence |
| 3 | B1 | Process B | 2x A0; channel-rotated A0 MIDI |
| 4 | C0 | Collect | 2x A1; channel-rotated A1 MIDI |
| 5 | C1 | Process C | 2x B0; channel-rotated B0 MIDI |

For example, `[0x90, 60, 100]` at input frame 15 becomes
`[0x91, 60, 100]` at node-output frame 63. With the existing Q-frame host
buffer enabled, it appears at host-output frame 79. All timestamps are relative
to the same stream origin.

## Staged implementation

Stages are sequential; each stage depends on the interfaces and checks from the
preceding stage. Keep fixes and verification within their owning milestone.

### 1. Branch, crate, and timing contract

- [x] Create `feat/external-inline-processor` from `feat/fixed-quantum-runtime`,
  retaining this plan and recording the actual base revision.
- [x] Add the portable crate, standard workspace metadata, shared Wasm test
  registration, and validated timing/capacity preparation. Update the lockfile
  and tracing inventory as required by the new source modules.
- [x] Add a focused production/build dependency audit for the new crate, reusing
  existing audit machinery so engine/host/application or OS dependencies cannot
  enter it. Keep dev-only engine composition dependencies outside this boundary.
- [x] Verify timing with one table-driven group: Q=1/16/32/64/128; representative
  M=1/2/3 and S=0/1/>M cases; exact B/L; zero M and overflow/limit refusals before
  allocation. Check the crate and its Wasm target. Commit the milestone.

### 2. Inline audio/MIDI backend

- [x] Implement `InlineTestProcessor` over complete prepared blocks: stereo 2x gain and
  next-channel MIDI rotation. System packets and event timing remain unchanged.
- [x] Verify two focused groups: distinct signed stereo samples proving 2x gain
  without clipping/crosstalk; MIDI packet tables covering channel wrap, note pairs,
  velocity-zero note-on, CC/program change, realtime, and variable-length SysEx.
  Check unchanged payload lengths, offsets, counts, and equal-time order. Commit.

### 3. Batched node, scheduled release, and lifecycle

- [x] Implement collection, immediate completion, bounded retained results, exact
  Q-frame release, latency queries, generation/sequence tracking, and quiescent
  reset. Validate tick continuity and runtime quantum/port shape before mutation.
- [x] Add four focused groups: sample mapping/startup and slot reuse over several
  blocks; MIDI offset mapping at Q-1/Q/B-1/B plus simultaneous events; exact-fit
  event/byte limits and one-over-limit refusal without silent loss; reset and
  discontinuity refusal proving no stale audio/MIDI after a new origin.
- [x] Include S>M and non-power-of-two M in the stream tests. Check completions
  are retained until due and that backend invocation count follows batches, not
  graph ticks. Commit after these checks pass.

### 4. Unified runtime integration and development composition

- [x] Implement the unified production `Processor::process` entry point around
  the same core as direct `process_tick` calls. Reject invalid quantum/shape;
  do not retain retired runtime APIs or introduce another algorithm.
- [x] Check direct ticks and unified runtime integration against independent
  sample/event references. Run direct whole-Q callbacks and buffered
  host partitions of 1, 7, Q, and a larger multi-Q callback, plus zero-length
  callbacks. Assert exact timestamps with node latency L and host latency H
  separately; flush enough zero input to observe the final delayed output.
- [x] Add one engine-composition test in `shoop_rt_engine` using a dev-only
  `rt_buffered` dependency and a test node enum combining existing prepared
  production strip DSP with BufferedProcessor in a crate-private unit fixture. Route audio and MIDI through the processor and a
  downstream consumer. No application catalog, protocol, or production graph
  changes are required to select the dummy.
- [x] Add a small runnable `rt_buffered` example with Q/M/S settings and checked
  audio/MIDI output, defaulting to the reference case above. Document preparation,
  latency, capacities, reset, and the inline/worker timing distinction. Commit.

### 5. Real-time and error-path verification

- [x] Add one heap-guard group using the existing tracking allocator: startup,
  block completion, steady playback, maximum admitted MIDI, and slot reuse all
  render without allocation or deallocation. Allocate fixtures outside guards.
- [x] Add one fault-path integration group showing deliberate capacity exhaustion
  follows the existing runtime's silent terminal audio fault and MIDI recovery
  behavior, including an already emitted rotated-channel note. Verify recovery
  packets rather than adding node-specific panic/drop behavior.
- [x] Run focused crate/engine tests, boundary audits, test-attribute checks,
  formatting, tracing inventory, and a warning-denying build. Resolve failures
  and commit. Do not expand into asynchronous fault-injection or timing benchmarks.

### 6. Final end-to-end validation and delivery

- [x] Run the example and engine graph scenario end-to-end: distinguishable stereo
  audio plus timestamped MIDI note pairs, channel wrap, equal-time events and
  SysEx; verify exact output against the independent reference, including startup,
  slack, delayed tail, reset, and a fresh instance. Record the tested revision.
- [x] Run the complete native regression gate once the focused checks pass; run
  portable BufferedProcessor tests in Node and Chromium and the existing application/
  worklet build gates. Keep environment failures or unavailable facilities visible;
  do not count skips as passes or weaken existing assertions.
- [x] Update this plan, crate documentation and relevant architecture/validation
  notes with results and limitations. Commit each final meaningful fix.
- [x] Push the implementation branch and open a PR targeting
  `feat/fixed-quantum-runtime`, describing the final scope and exact latency/MIDI
  behavior.
- [x] Ensure required non-macOS CI is green on the final head. The user explicitly
  waived macOS validation after repeated hosted ARM64 runner acquisition failures.
- [x] Check for automated review. Repeatedly assess and address incoming feedback,
  rerun affected checks, and continue until the reviewer approves. If no automated
  reviewer responds, record that fact; do not claim approval. Leave the PR ready
  for user review; merging is outside this plan.

## Verification commands and test budget

Aim for approximately eleven focused test groups above, parameterized with shared
fixtures. Use a small representative timing matrix instead of a full Cartesian
product or duplicated tests for every wrapper. The reference computes samples and
absolute event timestamps independently of the adapter's ring/slot algorithm.
Use `#[shoop_wasm_test_support::shoop_test]` and the existing heap guards.

Run commands in the development environment selected by the build guidance:

```sh
cargo test --locked -p rt_buffered
cargo test --locked -p shoop_rt_engine --lib buffered_tests
cargo run --locked -p rt_buffered --example inline_gain
python3 scripts/run_wasm_tests.py --runtime node --package rt_buffered
python3 scripts/run_wasm_tests.py --runtime chrome --package rt_buffered
python3 scripts/check_rt_runtime_boundary.py
python3 scripts/check_rt_engine_boundary.py
python3 scripts/check_shoop_test_usage.py
python3 scripts/check_tracing_coverage.py --require-closed
cargo fmt --all -- --check
RUSTFLAGS="-D warnings" cargo build --workspace
SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend,shoop_backend/native-fx --profile ci
```

Also run the new BufferedProcessor dependency audit and the application/worklet Wasm build
and artifact checks through the existing CI entry points. Native full-suite
execution requires cargo-nextest 0.9.116 and the documented host facilities.
During iteration, use affected tests; repeat broader gates only for changed code,
failures, or unresolved concerns. Writing this plan does not require running Rust
builds or implementation tests.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
- Do not reformat code or apply unnecessary whitespace changes.
- Use comments sparingly; in most cases the code should explain itself.
- Comments must not refer to other files, filenames, or specific callsites; describe
  usage patterns generically to avoid synchronization issues.
- Keep comments concise; do not describe the previous code or the instructions
  that led to the implementation.

## Progress and evidence

- Delivery-run infrastructure fix: the Node runner for documentation closeout
  `d3012734` exhausted its disk while running the full Wasm suite. GitHub's failure
  annotation reports `System.IO.IOException: No space left on device` in the
  runner's own diagnostic log, rather than a test assertion failure. Extend the
  existing unused Android/.NET SDK cleanup to both web profiles before cache
  restoration. Test commands, assertions, capacities and deadlines are unchanged.
  Final delivery results remain linked from the PR checks page.

- Final implementation checkpoint: revision `7c214af6` passes
  [CI run 41891267327](https://github.com/SanderVocke/shoopdaloop/actions/runs/41891267327),
  covering Linux and Windows debug/release, both web profiles, full Node tests,
  browser/session/AudioWorklet checks, independent hosts, coverage, JACK and
  bare-metal builds. Documentation, all CodeQL languages and both Codecov checks
  also pass. The tested merge candidate `cea197b2` includes base `820fb239`;
  its full native and coverage runs each pass 2677 tests with six expected skips.
  The branch-local native run passes 2676 tests against integrated base `3ca192e1`.
  macOS is explicitly waived, with no macOS pass claimed. No review submissions
  or inline threads were posted; automated approval is not claimed. The final
  closeout updates only delivery documents; its checks are linked from
  [PR #937](https://github.com/SanderVocke/shoopdaloop/pull/937).

- Latest base 3ca192e1 includes wide host-I/O preservation, structural-tick
  ordering, final MIDI-reset confirmation and buffered-MIDI recovery fixes.
  These resolve the lifecycle diagnostics noted below. After integration, the
  warning-denying workspace build, focused native tests and API doctest pass;
  the complete native suite passes all 2676 tests with six intentional default
  skips (four failure canaries and two manual device-acceptance checks). The
  base's recovery fixtures and assertions remain unchanged. Renewed portability
  and non-macOS CI checks pass as recorded above; macOS remains waived by the user.

- Updated-base local gates: all eleven focused processor/production-DSP cases
  plus the API doctest pass natively and in Node/Chromium. Warning-denying
  workspace build, all three Q/M/S examples, four-target dependency boundaries,
  bare-metal library build, Wasm dependency isolation, 395-module tracing inventory,
  shared test attributes, 28 Python policy tests, and runtime/loop-host native
  debug/release replays pass. The unified API port preserves quantum mismatch,
  shape, timing, MIDI, typed-operation and heap-guard assertions.
- Broader local diagnostics: the first merged workspace run executed 2672 tests
  (2669 passed, three host/LAN lifecycle failures, six skips). Rechecking the
  unchanged upstream recovery fixtures in a fresh container passed those three
  cases, with 19/20 selected tests passing and a different existing shutdown
  confirmation failing. These results are retained without changing lifecycle
  assertions or host production code. The earlier Q=32 fixture edits are removed
  after integration; the updated base's Q=1 fixtures are preserved. The six
  default skips are four intentional failure canaries and two manual isolated
  device-acceptance tests. The final native and CI passes above supersede these
  earlier diagnostics.

- Base synchronization after closeout: refreshed PR state revealed base 435eb2a2
  (fixed-quantum API cleanup and host-manager integration). Its consolidation
  removes the former parallel fixed runtime/processor hooks. API references in
  A6 and stage 4 are updated mechanically to direct `process_tick` plus the
  unified `Processor::process`; timing, audio/MIDI, heap and host-partition
  equivalence requirements are unchanged. The production-DSP composition fixture
  is now a private engine unit test, preserving its assertions without restoring
  a retired public test facade. All eleven focused tests and the API doctest pass.
  The new base's canonical browser journal has the same uncompressed digest as
  our actual browser recording, so its journal, sidecar and runner are retained.
  Earlier CI checkpoints below predate this integration; renewed non-macOS
  verification passes as recorded above. macOS stays waived by explicit user instruction.

- Delivery checkpoint: [PR #937](https://github.com/SanderVocke/shoopdaloop/pull/937)
  targets `feat/fixed-quantum-runtime`. At `e51fe082`,
  [CI run 41868336475](https://github.com/SanderVocke/shoopdaloop/actions/runs/41868336475)
  passes Linux debug/release, Windows debug/release, both web profiles, coverage,
  JACK acceptance and bare-metal gates; documentation and CodeQL also pass.
  The Linux workspace run confirms 2641 passed and four intentional canary skips,
  including every renamed processor and engine-composition test.
- macOS validation is waived by explicit user instruction after repeated jobs
  could not acquire hosted ARM64 runners. The pending targeted retry was
  cancelled; no macOS pass is claimed. The PR has the `ci:skip-macos` label,
  which excludes macOS from both the process-inspection preflight and main build
  matrix. This prevents unavailable macOS runners from blocking Linux, Windows
  and web through the preflight dependency. Runs without the label retain all
  eight build entries and three preflight platforms. Actionlint and a comparison
  of both matrix variants pass; all non-macOS entries are unchanged. Final
  non-macOS checks are available through the PR checks page.
- Final review check found no submitted reviews or inline threads. No automated
  code reviewer responded, and approval is not claimed. The implementation is
  ready for user review; merging remains outside this plan.

- Planning: inspected the fixed runtime interfaces, production fixed hook, MIDI
  validation/storage/recovery, engine composition, portable test runner, and
  dependency audits on `3ed8ff11`. This was the planning snapshot; execution results follow below.

- Implementation started on `feat/external-inline-processor` from `3ed8ff11`.
  The managed Debian workspace has no Rust toolchain; validation uses a Docker
  Rust toolchain with the session proxy and read-only trusted CA bundle.

- Stage 1: timing test passes natively; warning-denying native build and Wasm
  library check pass. The extended portable dependency audit passes on all four
  targets; existing audit unit tests, test-attribute policy and tracing inventory
  pass. Rust 1.99.1 is used in the build container.

- Stages 2/3: backend and adapter are one committed milestone because the
  submission/completion seam is intentionally crate-private. Eight timing/backend/
  node tests and two runtime integration tests pass natively with warnings denied.
  The runtime exposes generic remaining MIDI capacities for atomic preflight;
  BufferedProcessor returns `Capacity` on exhaustion, since the integrated runtime treats
  `Full` as recoverable MIDI overflow. No runtime scheduling policy was changed.

- Stage 4: all eleven focused native tests and the crate documentation example
  pass. Direct/integrated runtimes agree across direct and buffered callback
  partitions. The engine fixture combines BufferedProcessor with prepared strip DSP and
  typed half-gain control under a heap guard. The executable passes Q/M/S cases
  16/2/1, 32/3/7, and 1/1/0, including zero-input tail flushing.
- The warning-denying full workspace build passes. Full native regression,
  Node/Chromium tests, and dependency/format gates are now running. Validation
  disables debug information to bound build storage; this is not a benchmark.

- Stage 5: all focused native checks pass, including maximum-MIDI heap guards,
  independent direct/integrated callback references, atomic event/byte refusal,
  quiescent reset of pending and partially collected blocks, and terminal MIDI
  recovery without resuming ordinary output. Runtime/BufferedProcessor and engine boundary
  audits pass on all four targets; Wasm test dependency isolation, formatting,
  test attributes, tracing inventory, and the 14 existing boundary-policy tests
  pass. Existing CI filters omitted the target branch, so build/docs/CodeQL
  workflows now include `feat/fixed-quantum-runtime`.

- Regression investigation: the initial complete native run passed 2605 tests
  and failed four existing live session recovery tests (four ignored failure canaries).
  All four failures repeated in isolation at Q=1 with the small recording pools.
  Changing only live recovery fixture quanta to production Q=32 makes all four
  pass with their recovery/media/shutdown assertions unchanged. Runtime/reference
  Q=1 tests and the startup-only fixture remain. This is a verification-fixture
  adjustment; no production recovery or recording behavior was modified. Initial
  and isolated logs are retained under work/. Full regression will be repeated.

- Wasm investigation: all eleven processor/engine cases and the API doctest
  passed in Node, but report validation rejected the native Rustdoc summary
  (`0 measured`) mixed into wasm-pack output. The report parser now accounts for
  the optional measured field and native `FAILED` spelling; a regression test
  preserves successful, failed and ignored doctest accounting. All 14 report
  policy tests pass, and the retained first-run logs reparse without errors.

- Base synchronization: while validation ran, the target advanced from 3ed8ff11
  to cb79706e (explicit media conversion and atomic family imports). Merged those
  changes, preserving both tracing inventories; the combined warning-denying
  workspace build and all source/attribute inventory checks pass. The older
  snapshot repeat was interrupted after additional existing legacy-driver/LAN
  fixture failures; its partial results are retained and are not a passing gate.
  Final regression uses the merged source and a fresh isolated build container.

- Final local validation at 1dcf26f9 (merged base cb79706e): warning-denying
  workspace build, all 2641 native tests (four intentionally ignored failure
  canaries), eleven focused Wasm cases plus the API doctest in both Node and
  Chromium, Wasm application check, paired codec/worklet artifact contracts,
  bare-metal BufferedProcessor library build, four-target dependency audits, Wasm test
  dependency isolation, formatting, tracing inventory, test attributes, 28 Python
  report/boundary tests, and both executable timing examples pass. Hardware
  availability was handled with the documented missing-backend setting; physical
  device, remote DSP and performance acceptance are not claimed.

- Delivery: [PR #937](https://github.com/SanderVocke/shoopdaloop/pull/937) is open
  against feat/fixed-quantum-runtime. The initial CI at 31ddc295 identified
  the historical host-journal mismatch described below; final validation runs
  against the renamed implementation and refreshed recording at 9cee6c15.
  Remote build/test run 41860172019 is in progress.
  No automated review or inline review threads have been posted so far.

- User-approved naming revision: the reusable wrapper is `BufferedProcessor`,
  the crate is `rt_buffered`, and timing uses `batch_quanta`/`slack_quanta`. The
  inline gain/MIDI backend is `InlineTestProcessor`. Renamed the plan to
  PLAN_BUFFERED.md; timing behavior and acceptance criteria are unchanged.
  Warning-denying workspace build, all focused native tests and the API doctest,
  both timing examples, bare-metal build, dependency audits, formatting, tracing
  inventory and shared test attributes pass after the rename. Historical branch
  and commit identifiers retain their actual names.

- CI investigation at 31ddc295: all executed native matrix jobs reached the
  same independent-host replay failure. The historical browser journal contained
  four obsolete capture revision values after fixed-quantum migration. A fresh
  non-isolated Chrome worklet run passes offline/live checks, exact offline and
  two-thread native replay, and both intentional failure canaries. All ingress
  packets and every callback audio hash match the old recording; only the four
  capture revision fields differ. Refreshed the actual recorded artifact, pinned
  digest and current provenance report; retained historical evidence and every
  exact replay assertion. Both original web jobs, coverage, CodeQL and JACK
  acceptance passed. macOS debug never acquired a runner and will be retried
  on the final head if necessary.
  The refreshed recording passes native-only debug and release replay; the
  separate loop host also passes debug and release. All 44 selected host-runner,
  loop-host, dependency-boundary and Wasm-report policy tests pass.

- Renamed-package portability: eleven focused Wasm cases plus the API doctest
  pass in both Node and Chromium at 41f0dc56 (the recording-only follow-up
  9cee6c15 does not change these packages). The final-head CI matrix is running;
  bare-metal and JACK acceptance are already green.

- Post-rename local regression repeat: 2640 passed and one existing legacy
  Built-in FX track-creation test failed with a pending-loop observation. The
  unchanged legacy test failed 8/10 isolated repetitions; an experimental
  acknowledgment wait also exposed driver-progress timeouts and was removed.
  No legacy implementation or assertion changes are included. CI on the same
  pushed 9cee6c15 revision passes the complete Linux workspace suite and both
  Linux independent-host steps. Raw local diagnostics remain in work/.

- External CI delay: GitHub reports an active Actions/Pages incident
  (https://stspg.io/pjgptg3cfc6n). Several jobs have all steps completed
  successfully but remain in progress, and macOS runners remain queued. Windows
  debug hit an existing persistence-probe companion-exit timeout; GitHub rejects
  its job retry while the run is active. The CI gate remains open until recovery
  and successful retries; step success is not substituted for final job status.

- Review checkpoint: repeatedly checked PR reviews, inline threads and issue
  comments through the pushed 9cee6c15 implementation. No automated code
  reviewer has responded and no review threads exist; approval is not claimed.
  The only comment is Codecov coverage reporting (91.40271% patch coverage;
  project/patch checks pass, with an informational missing-base report). The
  focused boundary/lifecycle/audio/MIDI tests are retained without coverage-only
  assertion duplication. Recheck for new review feedback before final delivery.
