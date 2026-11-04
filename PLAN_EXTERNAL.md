# Plan: inline External processor with block timing and test MIDI

## Status and scope

Implementation authorized by the user; execute the stages below. Base the work
on `feat/fixed-quantum-runtime`, investigated at `3ed8ff11`. Follow
[write_plan.md](.agents/prompts/write_plan.md), the repository mandates, and the
build/test guidance when executing the plan.

Build a reusable fixed-quantum External processor whose first backend executes
on the render thread: stereo audio receives 2x gain, and MIDI channel messages
move to the next channel. Real batching and scheduled output make the timing
configuration observable despite immediate computation.

Included: one portable crate, prepared timing/storage, audio and MIDI processing,
both current fixed-processing interfaces, a runnable development example, and
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
- **A2 — timing.** Configuration consists of positive `block_quanta = M` and
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
- **A6 — equivalent integration.** The direct `fixed::Processor` interface and
  production `Processor::process_fixed` interface use one processing core and
  agree in audio, MIDI, and latency. Supported host partitions do not change
  stream results; host buffering latency is accounted for separately.
- **A7 — lifecycle and failures.** Reset/reprepare cannot leak buffered samples
  or MIDI from an earlier stream. Invalid timing/capacity is refused before
  allocation or rendering where detectable. Runtime capacity failures are
  explicit errors through the existing terminal-fault/MIDI-recovery contract;
  MIDI is never silently truncated or selectively dropped.
- **A8 — delivery.** Focused tests, existing required regression/portability
  gates, documentation, green PR CI, and review handling are completed with
  evidence tied to the implementation revision.

## Design rules and code ownership

- Add `src/rust/rt_external` as a sibling of `rt_runtime`: `no_std` with `alloc`,
  safe Rust, and a normal dependency on `rt_runtime` only. Test support stays
  dev-only. Use a small module set such as `timing`, `processor`, and `inline`.
  Do not add external-processing policy to the runtime scheduler or loop domain.
- Public entry points should be small: `Timing`, validated `PreparedTiming`,
  `External<InlineTest>`, preparation, latency queries, and quiescent reset.
  `InlineTest` names the combined 2x-audio/channel-rotation backend. Timing is
  immutable after preparation. Accept non-power-of-two block multiples such as
  M=3; an external block size is not another `fixed::Quantum`.
- Validate checked products/sums for block frames, latency, channel samples,
  event/byte capacity, frame timestamps, and allocation sizes before allocating.
  Publish supported preparation limits. Reject requests exceeding those limits
  rather than attempting huge allocations. Count M/S in quanta, not callbacks.
- Keep batching, readiness, and scheduled release in `External`; the backend
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

- [x] Implement `InlineTest` over complete prepared blocks: stereo 2x gain and
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

### 4. Both runtime interfaces and development composition

- [x] Implement thin direct `fixed::Processor` and production `process_fixed`
  wrappers around the same core. Keep the required older trait entry point a
  whole-tick adapter; reject invalid shape and do not introduce another algorithm.
- [x] Add one shared integration group comparing both interfaces against an
  independent sample/event reference. Run direct whole-Q callbacks and buffered
  host partitions of 1, 7, Q, and a larger multi-Q callback, plus zero-length
  callbacks. Assert exact timestamps with node latency L and host latency H
  separately; flush enough zero input to observe the final delayed output.
- [x] Add one engine-composition test in `shoop_rt_engine` using a dev-only
  `rt_external` dependency and a test node enum combining existing prepared
  engine nodes with External. Route audio and MIDI through the processor and a
  downstream consumer. No application catalog, protocol, or production graph
  changes are required to select the dummy.
- [x] Add a small runnable `rt_external` example with Q/M/S settings and checked
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
- [ ] Run focused crate/engine tests, boundary audits, test-attribute checks,
  formatting, tracing inventory, and a warning-denying build. Resolve failures
  and commit. Do not expand into asynchronous fault-injection or timing benchmarks.

### 6. Final end-to-end validation and delivery

- [ ] Run the example and engine graph scenario end-to-end: distinguishable stereo
  audio plus timestamped MIDI note pairs, channel wrap, equal-time events and
  SysEx; verify exact output against the independent reference, including startup,
  slack, delayed tail, reset, and a fresh instance. Record the tested revision.
- [ ] Run the complete native regression gate once the focused checks pass; run
  portable External tests in Node and Chromium and the existing application/
  worklet build gates. Keep environment failures or unavailable facilities visible;
  do not count skips as passes or weaken existing assertions.
- [ ] Update this plan, crate documentation and relevant architecture/validation
  notes with results and limitations. Commit each final meaningful fix.
- [ ] Push the implementation branch and open a PR targeting
  `feat/fixed-quantum-runtime`, describing the final scope and exact latency/MIDI
  behavior. Ensure required CI is green on the final head.
- [ ] Check for automated review. Repeatedly assess and address incoming feedback,
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
cargo test --locked -p rt_external
cargo test --locked -p shoop_rt_engine --test external_processor
cargo run --locked -p rt_external --example inline_gain
python3 scripts/run_wasm_tests.py --runtime node --package rt_external
python3 scripts/run_wasm_tests.py --runtime chrome --package rt_external
python3 scripts/check_rt_runtime_boundary.py
python3 scripts/check_rt_engine_boundary.py
python3 scripts/check_shoop_test_usage.py
python3 scripts/check_tracing_coverage.py --require-closed
cargo fmt --all -- --check
RUSTFLAGS="-D warnings" cargo build --workspace
SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend,shoop_backend/native-fx --profile ci
```

Also run the new External dependency audit and the application/worklet Wasm build
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

- Planning: inspected the fixed runtime interfaces, production fixed hook, MIDI
  validation/storage/recovery, engine composition, portable test runner, and
  dependency audits on `3ed8ff11`. No implementation or implementation checks
  have been performed. All implementation stages remain unchecked.

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
  External returns `Capacity` on exhaustion, since the integrated runtime treats
  `Full` as recoverable MIDI overflow. No runtime scheduling policy was changed.

- Stage 4: all eleven focused native tests and the crate documentation example
  pass. Direct/integrated runtimes agree across direct and buffered callback
  partitions. The engine fixture combines External with prepared strip DSP and
  typed half-gain control under a heap guard. The executable passes Q/M/S cases
  16/2/1, 32/3/7, and 1/1/0, including zero-input tail flushing.
- The warning-denying full workspace build passes. Full native regression,
  Node/Chromium tests, and dependency/format gates are now running. Validation
  disables debug information to bound build storage; this is not a benchmark.
