# Standalone SP: compact loop execution

Measured on 2026-11-16 against the integrated SP at commit
`beda9cdd2e07ea96ac8f284a1b4d739387b544fb`. That implementation was committed and
its sources, netlist, and measurements preserved before optimization. This is a
comparison of two SP implementations with the same capabilities and capacities.
The [original integration report](sp-sync-resource-comparison.md) separately
compares the initial implementation with the SP before it supported loop execution.

Both runs use pinned Yosys 0.52+45 (`ea2c1e7ac`, OSS CAD Suite 2025-04-15),
`synth_xilinx -family xc7 -flatten -noiopad -noclkbuf`, standalone
`state_processor`, program/state/job capacities 256/256/256 and `LOOP_DEPTH=256`.
All interfaces remain live. These are Xilinx 7-series mapped synthesis cells;
they are not placed/routed utilization, slice counts, or a timing-closure result.

| Resource | Initial integrated SP | Optimized SP | Reduction |
| --- | ---: | ---: | ---: |
| LUT1–LUT6 cells | 7,120 | 4,196 | 2,924 (41.1%) |
| Flip-flops | 2,955 | 1,616 | 1,339 (45.3%) |
| RAMB36E1 | 21 | 14 | 7 (33.3%) |
| RAMB18E1 | 3 | 3 | 0 |
| BRAM, 36-Kib equivalents | 22.5 | 15.5 | 7 (31.1%) |
| DSP48E1 | 0 | 0 | 0 |
| MUXF7 | 884 | 436 | 448 (50.7%) |
| MUXF8 | 267 | 69 | 198 (74.2%) |
| CARRY4 | 123 | 105 | 18 (14.6%) |

The SP before loop execution used 1,871 LUTs, 1,198 flip-flops and 7.5 BRAM
equivalents. The remaining cost of loop execution is therefore 2,325 LUTs,
418 flip-flops and eight BRAM equivalents, at the same original capacities.

## Changes

1. **Word-level execution instead of full-record transformation.** The 1,536-bit
   dedicated working register and whole-record operation/result multiplexers are
   removed. The existing SP FSM reads and writes 64-bit words, caches flags,
   position, content length and capture count, and accesses cycle/revision/plan
   words only as needed. Each operation explicitly determines whether it changed
   logical state, eliminating the wide before/after comparison. RAM read stages,
   operand and audio registers, event/job descriptor and 65-bit adder are shared.
   There is still one execution FSM and one combined sync/audio program.
2. **Denser RAM.** A loop slot now occupies sixteen 64-bit words, 128 bytes rather
   than 256. There are 995 defined bits and 29 reserved bits per slot. The old
   physical slot had three unused 128-bit words in addition to internal padding.
   For 256 slots, mapped loop RAM falls from 15 to eight RAMB36 blocks. Program,
   node-state and job FIFO memory counts are unchanged. Content length also serves
   as render length; revision watermark also serves as capture revision. Those
   pairs remain equal under the supported operations, and comparisons check the
   complete logical projections.
3. **Control metadata outside the execution image.** The control side retains
   request IDs, full sequence numbers and last-pulse history. Each admitted plan
   carries its operation/timing data; one per-loop bit identifies which of the two
   slots has the higher sequence. That rank preserves comparisons within and
   between the absolute and pulse passes. Resolution descriptors carry the plan
   index, which control maps back to the admitted request/sequence. Pulse events
   update control-side last-pulse history. A reusable testbench image codec and
   observer verify this against the independent functional model.

All musical cursors, timing/countdown counters, cycle counts and revisions remain
64-bit. Loop generations remain 32-bit; the source slot is 16-bit, sufficient for
all 4,096 supported slots. The two-plan limit is unchanged. The Rust functional
model retains its full logical state and semantics. No rollback, retry, separate
sync controller, or post-JP operation is introduced.

This changes the owned-loop RAM layout and resolution-event payload. The exact
[layout and control protocol](../../fpga/state_processor/README.md#integrated-loop-instructions)
are documented with the RTL. Control must retain each admitted slot binding until
its resolution has been consumed and update ranks while idle. The media/AXI
wrappers still use explicit-record mode. Control bookkeeping runs in the testbench
scaffolding; this does not add an ARM transport or application integration.

## Execution cost

The same 20 seeded sessions, 48 quanta each, were measured with Verilator 5.032.
Each program has three sync instructions and ten audio instructions; quanta are
1/16/32/64/128. Stimulus and commands are unchanged. Event readiness is held high
for both runs. Counts span run assertion through done, including admission, and
exclude configuration, readback and later JP execution.

| Cycles per quantum | Initial integrated SP | Optimized SP | Change |
| --- | ---: | ---: | ---: |
| Average over 960 quanta | 351.24 | 375.04 | +6.8% |
| Minimum observed | 336 | 341 | +1.5% |
| Maximum observed | 379 | 445 | +17.4% |
| Total | 337,188 | 360,035 | +6.8% |

These are workload observations, not worst-case bounds or clock frequency
measurements. More RAM accesses replace wide parallel logic, especially for
transitions and wraps. Ordinary correctness tests retain randomized event
backpressure. A separate shared-comparator experiment increased LUT count to
4,503, so the simpler 4,196-LUT implementation is retained.

## Validation

Passed locally:

- All 12 entry points in the sync/reference/state/audio/smoothing RTL targets.
  Sync tests compare 1,152 quanta with the independent Rust model, including full
  sequence values admitted out of order, jobs, render/next cursors, ordered events,
  physical plans/countdowns and reconstructed completions/last-pulse history.
- 120 combined SP/JP quanta with exact sample/buffer comparisons; generation,
  prefix order, duplicate visit, bounds and latched-fault checks.
- Directed 64-bit cursor/cycle/revision cases, overflow failures and compact
  configuration-bus read/write behavior.
- All 45 functional reference tests with all features and without default
  features, including 8,000 native-resolver trace quanta and full pipeline tests;
  all 224 native loop tests.
- SP default/small/minimum fatal lint, generic/no-latch checks and ECP5 synthesis;
  JP/audio lint and generic/ECP5 gates; AXI32/64 integration lint and execution
  generic/ECP5 synthesis.
- Warning-denying targeted Rust build, default testbench isolation check,
  formatting, test-attribute policy and historical coverage guard.

Full workspace build/nextest were attempted and remain blocked by missing host
ALSA/JACK development files. The full DRAMsim3 matrix was not run locally.

## Evidence and reproduction

[Before resources](measurements/sp-sync-optimization/before-resources.json),
[after resources](measurements/sp-sync-optimization/after-resources.json),
[before provenance](measurements/sp-sync-optimization/before-provenance.json),
[after provenance](measurements/sp-sync-optimization/after-provenance.json) and
[per-session cycle counts](measurements/sp-sync-optimization/execution-cycles.json)
include tool identity, capacities and source hashes. Full preserved source
snapshots, logs and netlists remain at `/tmp/shoop-sp-optimization/before` and
`/tmp/shoop-sp-optimization/compact-resources`.

```sh
mkdir -p /tmp/sp-initial
git archive beda9cdd fpga/state_processor/rtl | tar -x -C /tmp/sp-initial
python3 fpga/state_processor/measure_resources.py \
  --rtl /tmp/sp-initial/fpga/state_processor/rtl --loop-depth 256 \
  --yosys /path/to/pinned/yosys --output /tmp/sp-initial-resources
python3 fpga/state_processor/measure_resources.py \
  --loop-depth 256 --yosys /path/to/pinned/yosys --output /tmp/sp-compact-resources
SHOOP_SYNC_NO_STALLS=1 cargo test --locked -p rt_fpga_testbench --features sim \
  --test sync integrated_sp_sync_instructions_match_rust_jobs_cursors_events_and_planned_transitions \
  -- --nocapture
```

Output directories must be empty. For baseline cycles, check out `beda9cdd` in a
separate worktree and apply the archived
[cycle instrumentation patch](measurements/sp-sync-optimization/baseline-cycle-instrumentation.patch),
then run the same Cargo command. It adds the cycle counter and always-ready event
option without changing stimulus or expected results.
