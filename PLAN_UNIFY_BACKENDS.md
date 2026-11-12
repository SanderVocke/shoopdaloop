# Unify production Rust and FPGA behavioral verification

Status: implementation in progress, starting from upstream `e5c015be`.

Upstream adjustment: `e5c015be` implements `SIMPLIFY.md`. The RTL now has one
authoritative transition bank, automatic BeginCapture/view reuse, one run operation
and sticky processing faults. Host preflight, observations, preview/retry, tokens
and external job execution were removed upstream. References to migrating those
interfaces below mean preserving only their still-applicable functional coverage;
do not recreate them. Admission refusals remain common comparisons; fatal
hardware allocation/bus failures retain hardware-specific containment checks.
The goals and acceptance criteria are unchanged.

## Goal and scope

Make the production Rust backend the behavioral oracle for the looping features
currently implemented in RTL. A common, versioned scenario/result specification
must drive the production backend and the integrated FPGA pipeline and compare
their audio and other observable results exactly. Then remove the parallel
FPGA-reference Rust implementation and tooling that no longer has a purpose.

Use one production processing implementation with compile-time numeric policies:
the normal floating-point instantiation and a Q8.24 instantiation for exact RTL
comparison. Adopt the existing FPGA arithmetic and sample-duration ramp convention.
Derive FPGA job-generation order from the production compiled graph.

The required profile is audio-only primitive looping: record, play, stop, seek,
retrigger, wrapping, absolute and source-tick scheduling, repeat synchronization,
gain/ramps, mixing, and persistent captured/playback media. Cover legal sync cycles,
request conflicts, cancellation, retained completions and supported failure paths.
Q=32 with 32-sample chunks is mandatory; additional valid quantum/chunk profiles
exercise boundary behavior and the existing small/odd capacity configurations.

Not included: MIDI in the FPGA profile, composites, FX/plugins, replacing/grab,
live topology replacement, a new ARM peripheral/wire transport, board deployment,
DDR-controller changes, or cross-quantum prefetch. Production features outside the
comparison profile must remain available in the normal floating-point backend.

## Current implementation and migration boundaries

- `rt_fpga_reference` independently implements a compiler, state processor, job
  processor, transitions, media storage and chunk access. `rt_fpga_testbench`
  exposes it through the `fpga-reference` executable and generated JSON schema.
  `scripts/fpga_testbench` and the cocotb runners depend on that executable/schema.
- `rt_runtime::AudioRenderMode::FpgaReference` is also connected to the app. It
  uses `rt_runtime/src/fpga.rs`, `executor_fpga.rs`, processor hooks and a
  reference-backed elastic audio store. It is not an independent production oracle.
- `AudioRenderMode::Current`, `GraphRuntime`, the production `shoop_loops`
  planner/resolver and their audio processors are the intended oracle. Relevant
  `f32` assumptions extend through buffers, resources, loop rendering and engine
  controls/ramps, not just `rt_runtime`'s transport or sample mixer.
- Three graph forms currently differ: production `PreparedPlan` descriptors;
  the app's FPGA-oriented `NodeProgram`, which still calls Rust processors; and
  the concrete instruction program consumed by RTL. The last two must not be
  treated as interchangeable. The new lowering must know supported node behavior.
- RTL accepts a signal program/state image, separately compiled timing topology,
  media configuration, transition commands and quantum inputs. `loop_pipeline`
  coordinates TP, SP and JP; its AXI wrappers include the real chunk cache/store
  and burst memory path. Its TP bank Commit is not signal-graph activation.
- The existing transition testbench already adapts the production resolver in
  `rt_fpga_testbench/src/transitions.rs`. Reuse that evidence and useful adapters
  without retaining reference-specific types or execution algorithms.

## Immutable acceptance criteria

1. Checked-in common scenarios drive the actual production Rust execution path
   in Q8.24 and the integrated TP -> SP -> JP -> media/chunk-memory -> AXI RTL
   path. Neither expected execution nor RTL execution is supplied by a retained
   reference backend or a newly written test-only looping implementation.
2. Every output sample and every exported media sample in the comparison profile
   matches as raw signed 32-bit Q8.24 bits, including all ramp samples and
   arithmetic saturation/truncation edge cases. There are no epsilon comparisons,
   endpoint-only checks, float round trips or automatic timing shifts.
3. Common observable state and outcomes match exactly at explicitly identical
   audio boundaries: modes, positions, lengths, applicable logical counters and
   revisions, scheduled-operation outcomes/frames, cancellation/supersession,
   capture publication and preserved media. Defined identity mappings may remove
   allocation-specific IDs, but cannot hide behavioral differences.
4. The supported-feature matrix is fully exercised by directed and deterministic
   randomized sessions. Unsupported features are diagnosed explicitly rather than
   silently skipped, approximated or advertised as implemented.
5. The normal float backend remains the production default. Numeric selection is
   compile-time, with no per-sample runtime dispatch or newly introduced callback
   allocation/blocking. Its ramp semantics deliberately change to the agreed
   convention. Existing production functionality and other retained render modes
   continue to pass their applicable regressions.
6. FPGA lowering preserves production node order, incoming-edge accumulation
   order and each node's arithmetic grouping. Signal programs remain persistent
   across loop-mode/parameter changes; the oracle does not supply per-quantum
   jobs, decisions or state to make RTL agree.
7. Hardware-specific protocol/cache/fault/reset coverage remains executable
   without the retired reference. Instant and delayed AXI memory produce the
   same common behavioral results; a focused live DRAMsim3 campaign also passes.
8. `rt_fpga_reference`, the old `rt_fpga_testbench`/`fpga-reference` execution
   tool, the app's `FpgaReference` mode and their execution paths/dependencies are
   removed. Necessary small numeric, schema or packing utilities are relocated;
   the duplicate backend is not preserved under another name. Obsolete fixtures,
   commands and comparisons are removed or migrated with an explicit coverage map.
9. A clean build/run reproduces the new comparisons and aligned 64-loop benchmark
   without old binaries/generated artifacts. Reports include local validation,
   float CPU performance before/after, Q8.24 CPU cost, RTL timing/resources and
   declared memory assumptions. These are not board-level deadline guarantees.

## Design rules

### Shared production execution and numeric contract

- Introduce a small compile-time numeric policy with float defaults. It covers
  samples, gains/control arithmetic, envelopes and ramps through production
  processing/storage. Timing counters and identities remain integer domain data.
  A feature may choose a default, but both numeric instantiations must be usable
  together in tests. Keep generic/static dispatch in sample loops.
- Specify signed Q8.24, 64-bit widened gain products, truncation toward zero and
  saturation to i32 after each gain/mix result. Widen ramp differences/products
  sufficiently for extreme endpoints and u32 durations; never saturate a delta
  early. A signed right shift is not a substitute for truncation toward zero.
- For duration D > 0, use raw-integer
  `gain(n) = start + trunc0((target - start) * min(n, D) / D)`.
  Sample n=0 uses start; n=D and later use target; D=0 applies target immediately.
  Interrupted ramps start from the prior ramp's value at that boundary. Preserve
  the original start/target/duration/elapsed domain across quantum boundaries.
  Float uses the same timing convention with float arithmetic. Efficient stepping
  is permitted if it reproduces the specified fixed-point sequence exactly.
- Take current FPGA arithmetic as authoritative for these numeric choices;
  validate existing RTL against the explicit contract, rather than against a
  copied implementation. Resolve any discovered conflict visibly in this plan.
- Convert external float controls/audio only at explicit boundaries. Internal
  fixed samples, stored media and test results remain integer values. Define
  storage/import/export sample encoding; do not reinterpret existing f32 media
  bytes as fixed integers or silently change public persistence/host formats.
  Common Q8.24 scenarios supply raw fixed control values through typed production
  APIs as well; do not lose their low bits through the app's float wire fields.
- Do not move the old state processor, transition engine, job executor or media
  store into a numeric-policy branch. Small independently tested arithmetic
  primitives may be extracted; they are not a second execution model.

### One behavioral contract, independent execution

- Define a versioned Rust-owned scenario/result schema, generated for Python,
  in a new production-backed testbench package (for example
  `rt_backend_testbench` plus Python helpers). Include strict validation, bounded
  dimensions, checked integer widths and explicit numeric/profile identifiers.
- Inputs describe supported nodes/connections, logical loop/timing setup,
  initial media, raw audio, timestamped controls/commands and observation cuts.
  Outputs describe logical state, semantic action identities/outcomes/frames,
  raw audio and retained/exported media. Inputs contain no expected decisions,
  hardware buffer addresses, allocator snapshots or precomputed job streams.
- Use the production graph compiler and expose its ordered plan plus supported
  node descriptions to a control-side RTL lowering. Preserve left-to-right mix
  grouping and gain stages; do not reassociate, fuse gains, or quantize at different
  points. Explicitly lower node internals as well as routing. Reuse the production
  timing compiler for the separate TP topology/order export where possible;
  never impose audio order on transition dependencies.
- The Rust driver executes production processors/planner/resources. The RTL
  driver compiles/loads configuration and supplies commands/audio, then observes
  RTL results. Hardware media preflight may use its own preview and preparation
  interfaces, but cannot consult Rust expected results. Keep allocation/preparation
  distinct from musical commit/publication.
- Define observations before/after the same logical quantum, event ordering,
  admission versus terminal completion, and known host-buffer latency explicitly.
  Compare by semantic operation labels, not incidental request-slot numbers.
  Do not infer alignment by correlating outputs or drop startup/tail samples.
- Match failures only where capacities/conditions have a common behavioral
  contract. Keep hardware-only capacity/configuration errors and bus faults in
  explicit hardware checks; do not invent equivalent Rust failures or suppress
  later observable effects. No requirement equates physical allocation layouts.
- Preserve stable node-to-state bindings across control changes. Initial-state
  loading must not reset an existing ramp or overwrite live media/loop state on
  each quantum. Live graph replacement remains outside this milestone.
- Diagnostics may include FPGA instructions, jobs, cycles and allocation state,
  but these are not required equal to Rust internals. Keep failures replayable
  with source scenario, seed, profile, compiled artifacts and first differing
  sample/event/boundary.

## Staged implementation

Stages are ordered; pass each dependent gate before retiring its old coverage.

### 1. Freeze the profile, coverage map and baselines

- [x] Start from the latest upstream feature state and preserve concurrent work.
  Record revision/tool versions. Reuse the current branch/PR if appropriate;
  otherwise create a dedicated branch from that feature base and open a PR.
- [x] Inventory old crate/mode dependencies, every cocotb runner/schema/fixture,
  app selector, benchmark and shared helper. Record each item's migration,
  retention or deletion destination and its replacement coverage.
- [x] Specify the common input/output contract and supported-feature matrix,
  including exact observation frames, ramp interruption rules, numeric encoding,
  capacity differences and explicit unsupported features.
- [x] Capture float release CPU/heap baselines and current RTL correctness/timing
  for a small record/play scenario and the TP-inclusive 8x8, Q=32 workload.
  Verify existing float tests and targeted RTL tests are green; record inherited
  failures separately. Baselines are evidence, not outputs to preserve when ramp
  semantics intentionally change.

### 2. Establish the shared numeric and ramp primitives

- [x] Add the compile-time numeric policy and minimal reusable arithmetic module,
  extracting useful fixed primitives without importing reference execution code.
  Implement float/fixed operations and the agreed duration-based ramp contract.
- [x] Add independent directed arithmetic vectors and sequence tests: signed
  extremes, negative fractional products, saturation, zero/one duration,
  nondivisible slopes in both directions, interrupted/completed ramps, short ramps
  inside a quantum, and cross-quantum continuation. Check optimized stepping
  against the wide-integer specification, including extreme u32 counters.
- [x] Verify those vectors against existing gain/mix RTL, retaining streaming
  throughput, reset and in-flight fault checks. Document any RTL correction.
  Gate: all sample positions match exactly, not just ramp endpoints.

### 3. Parameterize the real production audio path

- [ ] Thread the policy through `rt_runtime` buffers/host-buffer abstraction,
  routing/mixing, production resource audio storage/codecs, `Processor` contexts,
  `shoop_loops` capture/playback, and the engine's supported control/gain/ramp DSP.
  Keep float defaults and unsupported fixed processors explicit. Do not implement
  numeric substitution by selecting `FpgaReference` internally.
- [ ] Replace the production quantum-stretched/increment-first ramp convention
  with the agreed convention in both instantiations. Update precise affected
  expectations and documentation without weakening unrelated production tests.
  Audit gain composition, balances and intermediate rounding throughout the
  supported path; eliminate hidden float calculations in the fixed instantiation.
- [ ] Exercise the same production entry points with float and Q8.24 for raw
  input/output, capture, media readback, constant/ramped gains and ordered mixes.
  Use heap guards and adversarial nonassociative arithmetic cases. Verify default
  float package/host builds, existing render-mode regressions and paired release
  CPU measurements; investigate reproducible regressions before proceeding.

### 4. Build the common runner and production-plan RTL lowering

- [ ] Implement the versioned schema, checked Rust/Python serialization and a
  batch Rust CLI that runs production Q8.24 scenarios through `GraphRuntime` and
  the production loop planner/processors. Reuse or factor production DSP; do not
  write toy equivalents in the runner. Keep the package headless and independent
  of device/UI startup. Preserve native no_std/float API compatibility.
- [ ] Implement export/lowering from the production ordered plan and explicit
  supported-node definitions to RTL program/state/buffer bindings and TP setup.
  Add only necessary read-only compiler exports; do not copy a second resolver
  or graph-ordering algorithm. Check bounded operands and hardware capacities.
- [ ] Adapt the existing cocotb drivers to load that image and run the common
  scenario through `loop_pipeline_axi`, including real media preflight, cache
  and AXI. Implement semantic result collection and raw fixed media readback.
- [ ] Gate with schema roundtrip/rejection tests and one complete mono Q=32
  record -> stop -> play -> wrap session. Both executions must independently
  produce identical audio, captured content, states and action outcomes. Inject
  a deliberate output mismatch to verify useful failure/replay diagnostics.

### 5. Establish full supported-loop differential coverage

- [ ] Extend directed common scenarios to all matrix rows: seek/retrigger;
  absolute/tick-delayed transitions; repeat waits; sync chains, self-sync and
  legal cycles; simultaneous/conflicting commands; supersession/cancellation;
  unread completions; retakes and retained prior content; aligned multi-chunk
  playback/wrap and read/write/read dependencies. Include gain/ramp/mix edge
  cases with every output sample checked, and unrelated playback preservation.
- [ ] Exercise supported preparation refusal/capture-unavailable behavior with
  explicit comparable limits and outcomes. Test rejected/stale identities and
  matching logical revision behavior without equating allocator IDs. Unsupported
  features must fail profile validation visibly.
- [ ] Add deterministic randomized sessions within the supported contract,
  several graph connection orders/fan-ins and valid quantum/chunk profiles.
  Include interrupted ramps and callback partition tests where the same command
  admission timeline can be maintained. Keep audio and transition orders distinct.
- [ ] Run fast instant AXI comparisons first, then selected fixed/random-stall
  cases and a focused live DRAMsim3 campaign. Replay the TP-inclusive 64-loop
  benchmark against production Q8.24 outputs. Gate: no unexplained difference,
  tolerated sample, silently dropped case or dependency on the old execution oracle.

### 6. Migrate subsystem verification without duplicating a backend

- [ ] Move transition comparisons directly to production `shoop_loops` APIs,
  adapting the existing native-resolver oracle and replacing old reference types.
  Retain request lifecycle, preview/retry and canonical event-order coverage.
- [ ] Move SP/job/resolver/media tests to production-derived expectations where
  the behavior is shared. Use small arithmetic specifications, logical media
  assertions and local protocol invariants for hardware-only details. Handwritten
  instruction vectors may test opcodes; no full substitute Rust SP/JP/TP/store
  may remain as the golden execution model.
- [ ] Preserve cache forwarding, queued/completed/in-flight prefetch invalidation,
  retirement/reuse, pinned reads, dirty writeback/fences, AXI 32/64-bit bursts,
  backpressure, reset and fault containment checks. Keep DRAMsim3/ctypes, memory
  services, useful codecs, timing instrumentation and synthesis entry points.
- [ ] Gate each old test's removal on its mapped replacement. Update all simulator
  entry points to the new runner/helpers. Run the full subsystem matrix once;
  stop depending on the old CLI before deleting it.

### 7. Retire the reference implementation and obsolete tooling

- [ ] Remove `rt_fpga_reference` and old `rt_fpga_testbench` crates, old executable
  and workspace/lockfile dependencies. Remove the app's `FpgaReference` enum/parser
  mode, `rt_runtime/src/fpga.rs`/`executor_fpga.rs` execution machinery, obsolete
  processor/planner hooks, reference-backed elastic-store branches and engine
  special cases. Keep only separately justified production/shared primitives.
- [ ] Remove or replace old `scripts/fpga_testbench` schema/API/tests/examples,
  fixture generators and trace variants that assume the deleted model. Retain
  useful strict JSON validation/schema generation under the new contract. No
  compatibility shim may execute the deleted model or silently accept old cases
  with changed meaning.
- [ ] Update app/host descriptions, native/worklet configuration, benchmark mode
  lists and all active test/build entry points. Delete reference-only elastic
  comparison tooling; retain general CPU/RTL performance tooling with the new
  labels. Preserve useful test scenarios in the new specification.
- [ ] Update `FPGA_ARCHITECTURE.md` and relevant runtime/FPGA READMEs with the new
  oracle, numeric contract, supported profile and commands. Label historical
  reference benchmark documents as historical. Do not delete unrelated legacy
  application/reference engines or their feature-parity tests.
- [ ] Gate with dependency/symbol searches, fresh builds and every retained runner
  from fresh output paths. No active code/import/command may require retired
  crates or stale `fpga-reference` binaries. Check the coverage map is complete.

### 8. Final end-to-end validation and delivery

- [ ] Run arithmetic/schema tests, affected production runtime/loop/engine tests,
  the complete common differential suite and preserved subsystem protocol tests.
  Run formatting, warning-denying workspace build and the repository's complete
  native Rust test gate; run `scripts/check_shoop_test_usage.py` after Rust test
  changes and dependency-boundary checks where affected. Build relevant Wasm
  production/worklet targets when the toolchain is available; document limitations.
- [ ] Run RTL lint/generic synthesis and relevant ECP5 estimates. Validate instant
  and delayed AXI behavior, then representative live DRAMsim3 sessions including
  the 64-loop Q=32 workload with actual TP/SP/JP. Report comparison counts, seeds,
  assumptions, stage timing and DDR traffic/utilization separately from host
  preflight pauses. Reuse unchanged expensive checks unless later edits invalidate
  their evidence.
- [ ] Repeat paired release CPU/heap measurements for the default float backend
  and measure production Q8.24. Compare identical workloads/settings, explain the
  deliberate ramp change, and resolve repeatable regressions. Record code/resource
  costs without claiming monomorphization proves performance or synthesis proves
  physical timing/deadline guarantees.
- [ ] Demonstrate clean-checkout setup, one-command directed/randomized comparison
  and failure replay without the old model. Publish final coverage/removal and
  performance reports with exact commands/revisions. Commit and push milestones;
  open/update the PR, check for incoming automated reviews and address applicable
  feedback iteratively until resolved/approved. Do not inspect PR CI under the
  user's standing instruction. Do not merge autonomously.

## Execution evidence

- Stage 1: upstream `e5c015be`; contract and migration ledger in
  `fpga/verification/`. Production tests: 669 passed, one pre-existing ignored
  test. Default integrated RTL directed/random/protocol gate and TP-inclusive
  64-loop instant-memory benchmark passed. Float release baseline covers four
  phases, three repeats of 1,024 callbacks each with no heap operations. Full
  baseline parameters/results are in `fpga/verification/baseline.json`.

- Stage 2: `rt_audio_math` unit/specification tests pass. The separate
  `numeric_simulate.py` gate compares 480 raw RTL output samples across default
  and small configurations without the reference execution model. Existing
  directed/stream/reset protocol comparisons remain passing in the migration
  baseline. The numeric crate builds with denied warnings and test-policy checks.

## Testing cost and confidence

Use focused arithmetic, schema and affected-package tests during early stages.
The first integrated gate is one short record/play scenario, followed by directed
edge cases and bounded seeded sessions with instant memory. Use delayed/stalled
memory to expose ordering bugs before spending time on live DRAMsim3. Preserve
hardware protocol tests throughout migration. Run the broad native, full RTL,
synthesis and representative DDR/performance gates at final integration, repeating
them only for changes that invalidate earlier results. Report actual coverage and
environment limits; successful compilation is not execution evidence.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

The user's instruction not to check PR CI overrides the plan template's default
CI-monitoring guidance. Local required checks and automated-review follow-up are
included; autonomous PR merging is not.
