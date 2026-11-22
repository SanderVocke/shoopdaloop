# Zynq 7020 RTL area experiment plan

Execute the proposed area experiments sequentially and measure their resource,
performance, and behavioral effects. Preserve unsuccessful results as well as
improvements, then validate a combination of compatible winners. This plan covers
experimentation and a reviewed implementation; it does not promise a particular
percentage reduction or board deployment.

The starting revision is `f218087511dc28e72a436ec825445f32a722b5e9` on
`feat/backend-replacement`. Its RTL matches the archived
[connected state-enabled measurement](measurements/midi-expression-state/README.md).
At 100 MHz that build uses 41,980 LUTs, 13,205 slices, 33,441 registers,
77.5 BRAM tiles, four DSPs, and 5,491 CARRY4s; setup slack is -5.429 ns.
The registered measurement context includes 6,667 synthetic boundary registers.
PS7, board DDR arbitration, and board I/O are outside that measurement.

## Goals and immutable acceptance criteria

- Investigate every experiment family in stages 1 through 10, one candidate at a
  time. Each candidate must have a recorded outcome: retained, rejected, or
  blocked with its exact prerequisite. A blocked or invalid candidate is not a
  measured optimization; unresolved blockers remain visible at completion.
- Every result identifies its source revision and hashes, parameters, tools,
  constraints, workload, comparison control, and validation status. Preserve raw
  reports and per-quantum samples so another engineer can reproduce the decision.
- Behavior-preserving candidates retain exact samples, MIDI packets and ordering,
  logical state, ownership, errors, and fence/publication guarantees for the
  supported configurations. Cycle counts and internal physical layouts may change.
- Precision, capacity, ownership-retention, and CPU-offload variants are explicit
  experiment configurations. Specify their changed contract before implementation,
  verify against an independent reference, and quantify differences from the
  original. Do not weaken existing exact tests or silently replace default behavior.
- Report both area and execution cost. Keep nominal clock conversions separate
  from physically timing-qualified results. Never claim deadline compliance from
  a build with failing timing, or universal bounds from observed workload maxima.
- Finish with a reproducible comparison of the original, individual candidates,
  and selected combination, including rejected tradeoffs and remaining limitations.
  Required regression checks and applicable CI must pass before the implementation
  PR is ready; blocked checks must be resolved or explicitly reported as incomplete.

## Design and measurement rules

- Freeze the primary profile: 64 stereo-plus-MIDI loops, 48 kHz, Q=32, 32-frame
  chunks, 32 chunks per allocation block, current capacities, and MIDI state on.
  Primary implementation target: `xc7z020clg484-1`, Vivado 2021.1 build 3247384,
  100 MHz, 0.2 ns uncertainty, unchanged registered boundaries and flow settings.
  Treat 150 MHz and geometry/precision changes as separately labeled comparisons.
- Keep original control B0 immutable. Compare sibling alternatives with the same
  current control Cn; record deltas against both Cn and B0. After a decision,
  promote a compatible winner to Cn+1. Do not sum isolated savings. Fundamental
  alternatives in stages 7 through 9 start from the same best structural control,
  rather than inheriting unrelated architectural changes automatically.
- Preserve public paths through live measurement boundaries. Include adapters,
  replacement controllers, and configuration logic in connected measurements.
  Explain interface/context changes separately; pruning features is not an area win.
- Prefer bounded indices, synchronous memories, shared arithmetic, and explicit
  request ownership. Validate wide inputs before narrowing and retain overflow bits.
  Keep handle generations and global transport time unless a separate contract
  explicitly changes them. Preserve reset, stale-response, backpressure, and
  memory-fault behavior throughout structural experiments.
- Existing Rust models remain independent behavioral references. Teach models
  explicit semantic changes without copying the RTL's cache or allocator internals.
  Keep counters used only for profiling outside synthesized logic where possible.
- Use focused simulation and synthesis screening while iterating. Route viable
  candidates after those checks pass; a synthesis rejection records mapped usage
  and the rejection reason, with routed slices marked unavailable. A rejected
  candidate may still supply a useful area/performance tradeoff; retain its data.
- Record LUT logic/RAM, FFs, routed slices, BRAM36 equivalents, DSPs, CARRY4,
  MUXF7/F8, control sets, setup/hold slack, DRC, route status, and critical paths.
  Hierarchical slice counts overlap and must not be added. Repeat small or
  surprising physical wins with the same flow before selecting them.
- For each candidate record cycles for admission through completion, cold start,
  warm steady operation, boundary crossings, transitions, and maintenance, plus
  cache hits/misses/writebacks, AXI traffic, and any backlog or capacity refusals.
  Compare samples matched by logical frame/event. Use a workload-matched Pareto
  comparison of slices/LUTs, timing, latency, memory, and behavior to select winners;
  a higher cycle count is an allowed measured tradeoff. Show deadline slack against
  the 666.667 microsecond Q=32 budget at 48 kHz, including cold starts separately.

## Stage 0 Establish the control and experiment runner

Depends on the starting revision; finish before changing implementation RTL.

- [x] Create `feat/backend-replacement-area-experiments` from the verified base,
  preserving this plan and existing edits. Keep disposable candidate branches or
  worktrees for alternatives and a separate branch for the selected combination.
- [x] Reuse or provision the documented pinned Vivado toolchain and Verilator
  5.032 environment using [build guidance](../../.agents/info/build.md). Reproduce
  B0 at 100 MHz and its functional/cycle controls; investigate differences from
  archived results before comparing optimizations.
- [x] Extend existing preparation/benchmark scripts only as needed to select
  candidates and record exact parameters, source/context hashes, tool versions,
  seeds, and common JSON/CSV results. Add synthesis/optimization stop controls for
  screening and verify harness-only changes preserve B0.
  Add small fixture-based checks for report parsing and configuration propagation.
- [x] Establish the workload matrix: audio/midi/mixed, state on/off, fixed DDR
  added delays 0/7, cold/warm caches, capture/playback/retake, seek/wrap/stop,
  musical restoration, allocator pressure, and maintenance. Add directed cache,
  block, and tree-boundary positions; retain original short fixtures for comparison.
- [x] Create `docs/fpga/measurements/area-experiments/` for manifests, a result
  ledger, summaries, and selected raw reports. Keep bulky build artifacts in
  separate empty build directories with durable artifact references and hashes.
  Retain optimized/routed checkpoints for B0 and selected candidates for auditing.
  Verify one clean rerun reproduces a result and one intentional mismatch is caught.

The ledger records candidate/control revisions, changed contract, validation,
usage and cycle deltas, timing status, and the retain/reject reason. Commit this
measurement foundation before proceeding.

## Stage 1 Replace the valid view population count

Depends on stage 0. Target `media_store.sv`'s diagnostic adder tree and bitmap.

- [x] Compare an incremental occupancy count with an on-demand serial count using
  RAM-backed validity. Measure each separately, including any new bookkeeping.
- [x] Verify create/slice/clear/remove, repeated descriptor writes, configuration
  restore, full pools, reuse on/off, and reset with stale descriptor RAM. Check
  diagnostic values exactly against the logical model and record response/reset
  latency with its measurement scope. The logical model does not define RTL clocks.
- [x] Run focused ownership/media tests and connected cycle/area measurements.
  Confirm the former `valid_views` to `cfg_globals_data` critical path disappears,
  identify the next limiting path, and record the selected implementation.

## Stage 2 Replace metadata cache lookup

Depends on stage 1's recorded decision; all cache alternatives share one control.

Execution status: four-way, banked and full-leaf experiments are complete. The
current-block and generated-hint policy siblings remain. See the [ledger](measurements/area-experiments/ledger.json) for retained
source, unit, connected and physical results.

- [ ] Implement and measure a 128-line, four-way cache with RAM-backed tags, then
  a banked full-associativity search, then a leaf cache indexed by view. For the
  last variant retain a small directory cache and include coherence logic costs.
  Preserve the original as a control; record actual extra hit and miss cycles.
  Compare a full leaf with a cached current block translation per view: B0's
  mixed steady quantum performs 384 table-cache hits and zero misses while each
  allocation block spans 32 quanta. Test invalidation on table writes and retirement.
- [ ] Separately trial the current-block cache with a cached level-one directory
  parent per view. The basic current-block candidate takes 74,575 clocks at its
  first leaf crossing, versus 84,284 for C1 and the nominal 66,666.7-clock budget
  at 100 MHz. Include the extra RAM/register/mux cost and the same invalidation
  lifetime. Verify minimum tree levels and parent-index crossings at ordinals
  255/256; two leaf spans alone do not cover every higher-level boundary.
- [ ] Verify read/write/zero, dirty eviction, fence/invalidate, held responses,
  reset, and fill/writeback errors. Exercise competing views, nested slices,
  append, trim, clear, reclaim, and stale speculative hints through the real store.
- [ ] Measure sequential, strided, adversarial set conflicts, all 128 audio
  channels, and cold/retake traffic. Extend steady runs beyond two 16,384-frame
  leaf spans, or supply equivalent directed boundary fixtures. Compare cache
  capacity and traffic as well as comparator count and synthesized RAM inference.
- [ ] Route viable alternatives, select by connected area and latency, and record
  any capacity/hash tuning as separate runs. Do not treat a fully serial scan's
  reduced logic as a win without accounting for its complete quantum cost.
- [ ] Separately trial late-only generated payload hints, then no generated
  payload hints, from the same C1 control. The measured control issues 256 payload
  burst reads for 128 channel chunks and emits both early SP and late JP hints.
  Preserve the original default policy, all read/media operations, and public
  hint handling. Measure FIFO/resolver cost, traffic, stalls, transitions,
  fault/progress behavior, and any FIFO-capacity tuning as separate runs.

## Stage 3 Narrow internal metadata and address types

Depends on stage 2's decision. Keep external precision and capacities unchanged.

- [ ] Derive ID, count, page, offset, and intermediate widths from capacities;
  replace saved traversal addresses with page/index context where useful. Split
  changes into audio metadata and MIDI storage candidates for attribution.
- [ ] Separately trial bounded local frame positions and arena-relative addresses;
  label any new duration/address restriction as a changed-contract variant.
  Check commands and DDR metadata before truncation, including carry/borrow cases.
- [ ] Verify minimum and non-power-of-two capacities, highest IDs, full counts,
  invalid high input bits, overflow, malformed metadata, and generation handling.
  Run metadata/media/MIDI tests and measure connected FF/LUT/carry and cycle deltas.

## Stage 4 Share address and bookkeeping arithmetic

Depends on stage 3's decision; use explicit registered operand selection.

- [ ] Replace repeated MIDI store address expressions with a shared region/index/
  field request datapath. Measure before separately sharing the remaining MIDI
  scratch address paths and audio metadata address calculations.
- [ ] Trial iterative metadata node/capacity calculations using the same bounded
  arithmetic unit. Keep per-sample throughput changes separate from setup costs.
- [ ] Verify every region boundary, 32/64-bit AXI mode, backpressure, faults, and
  latched command operands. Run affected MIDI/metadata/AXI tests; measure request
  latency, connected callback cycles, carry count, mux cost, area, and critical paths.

## Stage 5 Compact working state and reset controls

Depends on stage 4's decision. Start with the MIDI store descriptor staging bank.

- [ ] Keep hot descriptor fields in registers and move colder working fields to
  synchronous RAM. Separately test reuse of registers with disjoint lifetimes;
  account for extra reads/writes and verify actual RAM inference.
- [ ] Separately remove unnecessary resets from datapath storage behind explicit
  validity, and fold large validity vectors into memories where appropriate.
  Measure initialization latency and include required validity/epoch machinery.
- [ ] Verify overlapping operations, abort/reset, stale RAM, configuration, and
  accepted-output progress with MIDI/expression/loading tests. Record FF, LUT,
  BRAM, control-set, cycle, and routed-slice changes. Treat synthesis control-set
  settings as separate optional trials; earlier reset-replication suppression
  alone did not materially rescue packing.

## Stage 6 Use DSP arithmetic while preserving exact numerics

Depends on stage 5's decision. Precision stays unchanged in this stage.

- [ ] Compare a selected DSP add/subtract datapath with its fabric implementation,
  then separately investigate JP partial-product assembly using DSP cascades and
  internal registers. Remove preservation attributes only where the candidate needs it.
- [ ] Check signed extremes, saturation, truncation toward zero, interrupted ramps,
  smoothing endpoints, and reset/drain with exact independent widened arithmetic.
  Run audio/smoothing/state tests and measure initiation interval and pipeline drain.
- [ ] Record complete mux/register costs, DSP use, LUT/carry/slice changes, timing,
  and mixed-workload cycles. Retain only evidence-supported mappings; converting
  arithmetic to DSPs does not by itself establish a connected area saving.

## Stage 7 Trial fixed tables for bounded recordings

Depends on stages 1 through 6. Freeze their selected structural control for
the three architectural experiments that follow.

- [ ] Specify a 30-second per-view experiment with 2,048-entry tables and a table
  pool covering live plus detached views. Record actual DDR cost, limits, and
  layout/version changes; retain aggregate payload capacity and the common fixture.
- [ ] Replace radix traversal and page allocation with direct table indexing and
  bounded table-object allocation. Preserve deferred cleanup and per-block ownership.
- [ ] Verify offset slices, maximum length, maximum-plus-one refusal, retakes with
  retained exports, detached-table exhaustion/reuse, reset, and reclamation backlog.
  Compare common behavior exactly; test the new refusal contract independently.
- [ ] Measure create/slice/append/read/clear/reclaim latency, cold/steady callbacks,
  DDR reservation/traffic, and connected area/timing against the structural control.

## Stage 8 Trial coarser allocation and recording ownership

Depends on stage 7's outcome record, but starts from the same structural control.

- [ ] First measure 64/128-chunk allocation blocks individually, keeping total
  sample capacity constant. Record internal fragmentation, metadata/cache sizing,
  refusal points, and cleanup cost; do not assume fewer blocks automatically save LUTs.
- [ ] Separately implement immutable recording objects with slices holding an
  object/start/length reference. Published prefixes must survive ongoing append,
  clear/remove, and replacement. Specify whole-recording retention explicitly.
- [ ] Verify nested/partial/empty slices, retained exports, capture replacement,
  final-reference release, and pools exhausted by tiny retained ranges. Compare
  logical samples exactly while measuring any changed allocation/refusal behavior.
- [ ] Record retained bytes, peak memory, reclamation latency, traffic, cycle cost,
  and area. Compare combining this ownership scheme with fixed tables only as a
  separately identified follow-up candidate.

## Stage 9 Trial CPU managed allocation and retirement

Depends on stage 8's outcome record; starts from the structural control.

- [ ] Move allocation/reference/retirement decisions to a bounded CPU service;
  hardware consumes prepared spans. Start with audio storage and measure MIDI
  management offload separately if the audio experiment supports proceeding.
- [ ] Implement publication/acknowledgement and double buffering, preallocation
  while running, and delayed reuse until all hardware ownership is released.
  Specify preparation lookahead, queue limits, underrun policy, and CPU responsibilities.
- [ ] Exercise late preparation, stale descriptors, queue exhaustion, CPU stalls,
  reset and memory errors through real RTL plus the CPU-side service. Verify
  samples, timing/order of accepted commands, and ownership against the reference.
- [ ] Measure hardware resources including queues/adapters, CPU preparation time
  and memory, lookahead, total command-to-output latency, and missed deadlines.
  Host simulation establishes service cost, not ARM timing. Use ARM/board evidence
  where available; otherwise label that qualification outstanding explicitly.

## Stage 10 Trial reduced precision and simpler ramps

Depends on stage 9's outcome record. Use the best compatible control and record
its relationship to both the structural control and B0.

- [ ] Define independent candidate numeric policies before RTL changes: 24-bit
  samples, 18-bit gains, 16-bit recording storage with wider mixing, approximate
  ramp reciprocals/restricted durations, and quantum-rate gain updates. Trial each
  change separately before any combination, including explicit rounding/clipping rules.
- [ ] Implement the matching reference policy and require exact RTL agreement with
  it. Preserve original-policy tests. Verify full-scale multi-voice mixing, quiet
  signals, cancellation, extreme coefficients, ramp interruptions, and endpoints.
- [ ] Report maximum/RMS error against original fixed and float references,
  clipping changes, repeated record/play drift, ramp timing error, and measured
  discontinuities or zipper-noise spectra where applicable. Keep exportable audio
  examples for listening comparisons; do not infer inaudibility from bit width.
- [ ] Measure complete connected LUT/slice/DSP/BRAM use, DDR bytes, and cycles.
  Keep behavior-changing variants opt-in and list their tradeoffs explicitly.

## Stage 11 Validate the selected combination and complete review

Depends on recorded outcomes for stages 0 through 10.

- [ ] Combine compatible selected changes one at a time, committing and measuring
  after each addition. Compare against B0 and the individual results; investigate
  interactions instead of adding predicted savings. Publish a Pareto comparison
  and clearly identify the default configuration and optional alternatives.
- [ ] Run the full FPGA simulation matrix, relevant Rust reference/compiler and
  production numeric checks, coverage/generated-source guards, and all four
  standalone synthesis checks. Run warning-denying builds, formatting, and the
  applicable full workspace gates from [test guidance](../../.agents/info/test.md).
- [ ] Run connected cold/steady audio/midi/mixed measurements at DDR delays 0/7,
  musical transitions and tracker scaling, boundary/conflict/ownership stress,
  and the retained DRAMsim3 audio/MIDI workloads. Preserve original fixture sizes
  and clearly label longer runs. For CPU/precision variants run their additional
  service/behavior checks and report deadlines per workload, including cold starts.
- [ ] Route the selected connected configuration at 100 MHz and then 150 MHz as
  a separate characterization. Check setup/hold, DRC, routing, and report coverage.
  If 100 MHz fails, characterize lower clocks for B0 and the selected combination
  to establish a usable comparison. Qualify execution time only at clocks with
  passing physical timing; preserve failure reports if closure remains unresolved.
- [ ] Push the experiment and selected implementation branches; create a PR into
  `feat/backend-replacement` with evidence and the final tradeoffs. A draft PR
  may be opened during stage 0 to overlap measurement-tool CI with the baseline
  matrix; it stays draft until completion gates pass. Monitor the
  applicable FPGA and workspace CI until green. Check incoming automated review,
  repeatedly address actionable feedback, rerun affected checks, and continue
  until approval when a reviewer is active. Do not merge autonomously.

Representative existing commands, to run in the documented build environment:

```sh
python3 fpga/synthesis/midi_shared_jobs.py --revision CANDIDATE_SHA --mhz 100 \
  --profile docs/fpga/measurements/midi-expression-state/compiled-profile.json \
  --output EMPTY_BUILD_DIRECTORY
bash fpga/synthesis/run_midi_container.sh EMPTY_BUILD_DIRECTORY TOOLS_DIRECTORY RUNTIME_IMAGE
cargo run --release --locked -p rt_fpga_testbench --features sim \
  --example midi_data_quantum_64 -- --scenario mixed --state on \
  --ddr-delay 7 --output RESULT_JSON
cargo run --release --locked -p rt_fpga_testbench --features sim \
  --example midi_expression_transitions -- 7 RESULT_JSON
cargo run --release --locked -p rt_fpga_testbench --features sim \
  --example midi_expression_scaling -- RESULT_JSON
cargo test --locked -p rt_fpga_testbench --features sim
python3 scripts/check_fpga_testbench_coverage.py
bash fpga/state_processor/check.sh
bash fpga/job_processor/check.sh
bash fpga/media_store/check.sh
bash fpga/axi_memory/check_pipeline.sh
```

Use distinct output paths for each run. Stage 0 must add candidate selectors to
the shared preparation path where required; the existing commands alone do not
select experimental hardware. The DRAMsim3 benchmark scripts currently convert
cycles at their own nominal clock; normalize raw cycles to the clock of each
physically qualified build rather than comparing those nominal times directly.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
