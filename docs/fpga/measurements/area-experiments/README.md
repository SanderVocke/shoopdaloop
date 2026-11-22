# Zynq 7020 area experiment evidence

Execute [the experiment plan](../../area-experiments-plan.md) sequentially.
The immutable implementation control is
`f218087511dc28e72a436ec825445f32a722b5e9`; the primary profile is the
[64-loop compiled profile](../midi-expression-state/compiled-profile.json).

Current status: stages 0–2 are complete. C2 selects the serial RAM occupancy
count and directory-path cache, saving 6,491 LUTs and 1,231 routed slices against
B0 with exact retained behavior. Stage 3 is in progress: audio capacity widths
were rejected for higher slice use; bounded MIDI bookkeeping passes functional
and connected comparisons and is routing. Timing still fails at 100 MHz.
See `ledger.json` for completed checks, pending measurements, and artifact hashes.

| B0 routed control | Archived | Fresh |
| --- | ---: | ---: |
| LUTs | 41,980 | 41,980 |
| Slices | 13,205 | 13,205 |
| Registers | 33,441 | 33,441 |
| BRAM tiles | 77.5 | 77.5 |
| DSPs | 4 | 4 |
| CARRY4 | 5,491 | 5,491 |
| Setup slack ns | -5.429 | -5.429 |
| Hold slack ns | +0.008 | +0.008 |

The negative setup slack remains a timing failure. Raw reports are in `b0-100/`;
`b0-100-summary.json` also verifies the mixed workload's RTL source hashes.
The optimized simulator reproduces every original mixed per-quantum result and
cache counter, reducing measured wall time from 440.97 to 361.20 seconds.

The SP and JP/audio standalone checks pass. Unchanged B0 media lint reports
unnamed population-count generate blocks and an unused validity bitmap when
view reuse is disabled; the incremental replacement resolves these warnings.
The [workload matrix](workload-matrix.md) maps directed regression and boundary
coverage to experiment families. Raw callback controls are in `b0-matrix/`.
Descriptor-statistics controls cover repeated writes, sparse restore, reset with
stale RAM, and full occupancy at 1/7/512/513 views. B0 diagnostic reads take six
clocks. Metadata and ownership tests pass, including DRAMsim3.

Prepare a committed or explicitly hashed working-tree candidate:

```sh
python3 fpga/synthesis/midi_shared_jobs.py --revision REVISION --mhz 100 \
  --stage opt --profile docs/fpga/measurements/midi-expression-state/compiled-profile.json \
  --output EMPTY_BUILD_DIRECTORY
```

`--stage` selects `synth`, `opt`, `place`, or `route` (default). `--param NAME=INTEGER`
selects a declared top parameter and records the override. `--working-tree`
snapshots current sources, including new RTL files. Lower clock targets use the
same constraint generator. All input snapshots have SHA-256 manifests; completed
build directories are never reused for a different candidate.

Run `run_midi_container.sh` with the documented tool directory/runtime image, or
run `vivado -mode batch -source run.tcl` in the prepared directory. Preserve
optimized/routed checkpoints for controls and selected candidates by setting
`MIDI_SAVE_CHECKPOINTS=1` when using the container runner.

After a successful screen, resume its saved checkpoint into a new directory:

```sh
python3 fpga/synthesis/area_resume.py --build SCREEN_DIRECTORY \
  --output NEW_ROUTE_DIRECTORY --stage route
```

The resume path checks source and context manifests, requires the same flow,
and records the parent checkpoint hash. It retains the earlier stage reports.
Changed HDL, constraints, or implementation settings require a fresh screen.

Build the checked benchmark and run the fixed workload matrix:

```sh
cargo build --release --locked -p rt_fpga_testbench --features sim --example midi_data_quantum_64
python3 fpga/synthesis/area_benchmarks.py \
  --binary target/release/examples/midi_data_quantum_64 --output NEW_RESULT_DIRECTORY
```

Use `--case mixed-on-delay7` for the first smoke measurement. The full matrix
contains audio/MIDI/mixed workloads, tracking off/on, and DDR delays 0/7. Cold
start, 16 warmup quanta, and steady samples remain separate. `--quanta` extends
the measured steady interval. Directed ownership and musical-transition tests
remain separate from this playback fixture.

`SHOOP_FPGA_SOURCE_ROOT=/absolute/snapshot/root` selects an immutable source
tree for simulation through the same selector used by the RTL library. Include
the required FPGA test adapters in that tree. Generated simulation files go in
its `work/` directory, so use a separate simulation snapshot rather than the
manifested physical-build `source/` directory. The default remains the repository
tree. This permits a B0 harness control alongside the current candidate without
changing either run's RTL inputs.

`--parameters FILE.json` supplies explicit benchmark parameters, with string
values such as `{"TABLE_LINES":"64"}`. Physical geometry changes also require a
matching driver/reference fixture. Each output records source hashes, model
parameters, tool/revision identity, and per-quantum traffic/cycles. The matrix
manifest also hashes the frozen executable and preserves failed-run logs.

Collect area and callback evidence:

```sh
python3 fpga/synthesis/area_results.py --build BUILD_DIRECTORY --label LABEL \
  --benchmark CALLBACK_JSON --output NEW_SUMMARY_JSON
```

Add `--control CONTROL_SUMMARY_JSON` for resource deltas. The collector verifies
snapshot manifests and simulator source hashes, lists simulation/implementation
parameter differences, and rejects comparisons at different clocks/parts.
Missing routed slices remain null. Setup/hold success is separate from complete
DRC, constraint coverage, and board qualification. Original per-quantum samples
and raw reports remain the source of truth for the compact summary.

Retain a completed build as a compact source/report archive and summary:

```sh
python3 fpga/synthesis/area_archive.py --build BUILD_DIRECTORY --label LABEL \
  --benchmark CALLBACK_JSON --control CONTROL_SUMMARY_JSON \
  --output-directory docs/fpga/measurements/area-experiments
```

Each archive has a member-by-member SHA-256 manifest. Checkpoints stay in the
local build directory with their sizes and hashes recorded separately; they are
not uploaded with the source/report archive. The archive round-trip was checked
against all four B0 implementation stages.

Runner checks:

```sh
PYTHONPATH=fpga/synthesis python3 -m unittest fpga/synthesis/test_area_experiments.py
```

## Stage 1 measurements

| Routed 100 MHz result | B0 | Incremental count | Serial RAM count |
| --- | ---: | ---: | ---: |
| LUTs | 41,980 | 39,940 | 39,399 |
| Slices | 13,205 | 13,016 | 12,616 |
| Registers | 33,441 | 33,456 | 32,987 |
| CARRY4 | 5,491 | 3,965 | 3,958 |
| Control sets | 2,215 | 1,672 | 1,494 |
| Setup slack ns | -5.429 | -3.995 | -4.157 |
| Hold slack ns | +0.008 | +0.005 | +0.005 |
| Diagnostic read, clocks | 6 | 6 | 519 |
| Reset admission, clocks | 2 | 2 | 513 |

BRAM remains 77.5 tiles and DSP use remains four. All 33 mixed/on/delay7
measurement rows, including cycles and traffic, are identical: cold start
110,356 clocks, steady median/max 54,146/54,753. Diagnostic and reset measurements use 512 views with reuse enabled.
Reset timing includes one asserted reset clock and the readiness wait; diagnostic
timing covers the driver call including admission and response cleanup. The
serial count scans all configured entries, including sparse raw restores beyond
the issued-view high-water mark. Reuse-disabled counts keep their issued-view
semantics and six-clock reads. These cycle counts do not qualify 100 MHz operation because setup timing
fails. There are zero routing or DRC errors, 18 DRC warnings, and zero
unconstrained internal endpoints in the registered OOC context. The warnings
cover DSP pipelining and the OOC boundary/absent PS7. Board DDR and I/O remain
outside this context.

The original combinatorial population-count path disappears. Metadata node
arithmetic is now the worst routed path (18 CARRY4 plus two LUT3 levels).
The serial count saves 2,581 LUTs, 589 slices, and 454 registers versus B0; it
uses 400 fewer slices than the incremental candidate. Accept its longer diagnostic
and initialization latency because callback measurements are unchanged. The
serial candidate is selected as C1: its repeat reproduces every reported stage
resource, timing, and control-set metric exactly.
Deferred cleanup, fault, reset, and preemption regressions pass, including
DRAMsim3. The optimized and
routed raw archives preserve separate report stages and source/checkpoint hashes.

## Stage 2 measurements

All siblings compare with C1, the selected serial-count implementation. The
four-way, banked and full-leaf measurements are complete. The current-block
alternative and generated-hint policies remain before selection.

| Routed 100 MHz result | C1 | Four-way | Banked | Full leaf | Current block |
| --- | ---: | ---: | ---: | ---: | ---: |
| LUTs | 39,399 | 36,656 | 36,470 | 36,168 | 35,152 |
| Slices | 12,616 | 12,336 | 12,263 | 12,119 | 11,953 |
| Registers | 32,987 | 28,737 | 28,801 | 28,975 | 28,511 |
| CARRY4 | 3,958 | 3,352 | 3,369 | 3,382 | 3,341 |
| Control sets | 1,494 | 1,406 | 1,434 | 1,444 | 1,319 |
| BRAM tiles | 77.5 | 77.5 | 77.5 | 83 | 77 |
| Setup slack ns | -4.157 | -4.744 | -3.950 | -3.799 | -3.665 |
| Hold slack ns | +0.005 | +0.015 | +0.041 | +0.046 | +0.006 |
| Playback-start mixed/on/delay7 clocks | 110,356 | 110,572 | 116,057 | 110,168 | 97,951 |
| 1,024-quantum steady median clocks | 54,146 | 54,146 | 56,066 | 52,994 | 52,994 |
| 1,024-quantum steady maximum clocks | 84,284 | 83,891 | 98,244 | 86,955 | 74,575 |

The four-way cache reduces the main tag lookup from 128 comparisons to four,
using synchronous tag RAM. Read hits still take six clocks in the standalone
fixture. Five competing lines in one set now cause evictions, independently
checked against an array oracle; final fenced memory remains exact. Metadata,
ownership, deferred cleanup and media regressions pass, including DRAMsim3 and
legacy scalar memory.

The long connected fixture reaches frames 16,384 and 32,768. Those leaf crossings
cost 84,284/83,711 clocks with C1 and 83,891/83,257 with four-way tags. Both exceed
the nominal 66,666.7-clock callback budget at 100 MHz, and both fail physical
setup timing. The short fixture alone hides these boundary costs. Raw long-run
samples are retained in `c1-extended-100-raw.tar.gz` and
`s2-fourway-extended-100-raw.tar.gz`; summaries and retention manifests identify
their sources and hashes.

The banked alternative retains all 128 fully associative entries while comparing
eight tags at a time. Independent sequential, strided and dirty-conflict traces
on AXI32/64 match C1's 261 misses exactly. Read hits range from six to 36 clocks
(median 20 in this unit workload); cold fills add 30 clocks. All metadata tests
pass. The routed banked result uses 36,470 LUTs, 28,801 registers and 12,263
slices, with 3,369 CARRY4s and 1,434 control sets. BRAM and DSP use stay at
77.5 and four. Setup/hold slack is -3.950/+0.041 ns: timing still fails.
This is 2,929 fewer LUTs and 353 fewer slices than C1, and 73 fewer slices than
the four-way route. Compare that small slice difference with the complete
connected latency and boundary costs before selecting or repeating a winner.
The complete long run passes all 1,041 rows: steady median/max is
56,066/98,244 clocks, versus C1's 54,146/84,284. The frame-32,768 crossing costs
97,699 clocks versus 83,711. Connected steady table misses are 1,724 versus
1,698; traffic differs on five rows as lookup timing changes interleaving.
Cold start costs 116,057 clocks (+5,701), despite 97 fewer cold audio reads.
Reject this banked variant for the default: its 73-slice advantage over four-way
comes with greater ordinary and boundary latency. Retain it as a measured
full-associativity tradeoff. `s2-banked-extended-100-*` retains physical reports
and every long-run sample. The full-leaf and current-block alternatives follow.


The full-leaf variant extends each private roots-RAM entry from 128 to 640 bits
and caches sixteen table words per view. Its effective directory cache is eight
lines, explicitly capped even with the top-level TABLE_LINES budget of 128.
Existing leaf-valid lifetimes guard reads; writes update the cache, and
trim/detach/reset/global invalidation disable it. Hot whole-controller reads take
six clocks versus 15 with C1. Strided AXI64 unit reads that change leaf take 247
clocks versus 30 median with C1; the independent map verifies data and lifetime
semantics across those paths.

Full metadata, ownership, deferred-cleanup and media regressions pass, including
DRAMsim3 and scalar legacy paths. All 1,041 connected rows pass. Ordinary steady
cost falls 1,152 clocks, but the two leaf crossings cost 86,955/86,382 versus
C1's 84,284/83,711. Steady table misses fall from 1,698 to 1,280. Keep this candidate
on the shortlist pending the current-block sibling. It has the best measured
area so far, with 5.5 extra BRAM tiles and worse strided/boundary latency. Setup
still fails; repeat before selecting a small physical winner.

Playback-start metrics exclude preparation. The full-leaf start row has five
metadata writebacks versus 128 with C1; its smaller directory cache evicts dirty
lines during preparation. The 188-clock start improvement does not measure
complete preparation or boot time. `s2-leaf-extended-100-*` retains all physical
reports and long-run samples; unit archives retain the controller traces.

The read-only checkpoint taxonomy in `carry-taxonomy-c1-leaf-summary.json`
attributes 640 CARRY4s to C1's main metadata cache and 40 to the full-leaf design's
eight-line directory cache. Other C1 direct owners include MIDI content 628,
MIDI orchestration 452, media_store 496 and media_metadata 282. These are disjoint
primitive-owner counts, unlike overlapping hierarchical slice counts. They
identify targets for the following width and arithmetic-sharing experiments.


The current-block cache extends each roots-RAM entry to 192 bits, caching one
block value and ordinal per view. It keeps the same validity lifetime and an
eight-line shared directory cache. All metadata, ownership, deferred-cleanup and
media tests pass, including DRAMsim3 and scalar legacy modes. All 1,041 connected
rows agree with the original logical oracles. The two leaf crossings cost
74,575/74,002 clocks, versus C1's 84,284/83,711. BRAM drops by half a tile.

The smaller directory cache increases steady table misses to 5,120 from 1,698;
traffic differs on 35 rows. Most block-crossing work overlaps JP execution, so
the observed median improves despite that extra traffic. Standalone strided
AXI64 reads take 135 clocks versus C1's 30; hot repeated-block reads take six
versus 15. Playback-start still excludes preparation and begins with five dirty
writebacks versus C1's 128. Do not treat its measured start reduction as a total
preparation or boot saving.

Keep current-block on the shortlist pending the directory-path and generated-
hint siblings. It improves the measured area, BRAM, ordinary and leaf-boundary
costs relative to C1; setup timing and nominal leaf deadlines still fail.
The next directory-path trial reuses the existing traversal register bank and
retains middle-directory pages per view. Deeper parent crossings need directed
checks: the long connected interval alone establishes no universal bound.


The directory-path sibling retains three middle-page pointers per view in a
288-bit roots row, reusing traversal state and the original invalidation lifetime.
All five metadata tests and ownership/deferred/media regressions pass, including
DRAMsim3. The 1,041 connected rows preserve every logical oracle. Its 35,489 LUTs,
11,974 slices, 28,681 FFs, 78 BRAMs, four DSPs and 3,343 CARRY4s save 3,910 LUTs
and 642 slices versus C1. It costs 337 LUTs, 21 slices and one BRAM versus basic
current-block. Setup slack -3.881 ns remains a failure; hold slack is +0.043 ns.

Ordinary median is 52,994 clocks; the maximum is 59,446, versus 74,575 for basic
current-block and 84,284 for C1. Nominal 100 MHz arithmetic places these observed
steady samples within the quantum budget, but neither physical timing nor every
higher-level boundary is qualified. Playback-start is 88,446 clocks and still
excludes preparation; cold traffic is 694 reads/eight writes versus C1's
1,248/128. Preparation and deferred dirty work must not be called a boot saving.

Independent four-view fixtures cross ordinals 255/256, 4095/4096 and 65535/65536,
trim/regrow deep branches, detach/reuse IDs and reclaim stale trees. Both AXI
widths agree with the logical map, including opaque high-bit values. These
fixtures expose costs: AXI64 alternating reads have median179 clocks versus C1's
46; selected deep reclamation reads take371/449 versus59. They establish behavior
and controller latency, not full128-channel high-boundary deadline guarantees.
See `s2-path-extended-100-*`, `s2-path-translation-unit-*` and
`s2-path-directory-unit-*`; hints and a repeated winning physical run remain.


All five cache siblings and C1 now pass the same 1,282-row public storage
operation fixture at primary capacity. It measures 128 captures, offset exports,
clear/retake, retained readback, and two bounded reclamation drains.
[Operation costs](cache-store-operations.md) retain slower results as well as
improvements, alongside every command, sample result and AXI counter. These
ideal-memory controller clocks complement connected playback and the delayed/
DRAMsim3 regressions; they are not full callback or board timing qualifications.

The hint policy's canonical default reproduces all 33 original callback rows
and passes all eight protocol cases. Every optimized resource count matches C1
exactly; pre-route setup slack differs by 0.001 ns. Late-only hints preserve
normal data but fail a first-buffer publication assertion after an injected
second-view fault in three AXI memory modes. They also add 4,634 clocks to the
start and 5,170 to the first warm quantum; the remaining short rows are unchanged.
The original policy stays default, and failed variants remain experiment results.
See [policy contract and status](hint-policy.md); no-hint and area screens remain.


![Cache footprint and observed boundary latency](cache-tradeoffs.png)

[Exportable PDF](cache-tradeoffs.pdf). The plot uses the same 1,024 steady
quanta for every point; B0 has no matching long run and is excluded. All six
physical builds fail setup timing. One-block and directory-path remain the
area/latency alternatives; BRAM and adversarial/maintenance costs remain in the
tables and raw reports.


Before width changes, `24d767e7` adds minimum/odd/primary-count metadata input
controls. C1 and path return identical results on all 187 requests per AXI
width, including malformed raw DDR retirement counts. The expanded six-test
metadata suite passes. `86af343f` adds standalone MIDI full-pool/retirement
controls at 1/3/5 views and 1/3-event capacities, checking 564 transactions over
three generation/queue wraps against the independent ContentStore. High frame
bits, poisoned accepted command inputs, held outputs, refusals, generation
overflow, faults and reset are included. These are baselines, not area wins.

Compact per-opcode/error statistics are regenerated from the retained raw
samples by `compact_unit_summaries.py`. Every original request and result stays
in the verified archives; compacting a summary does not discard raw evidence.


Stage 2 is complete. The full-flow path repeat reproduces the original
checkpoint-resumed run exactly at all four stages: resources, timing and control
sets. The expanded six-test path metadata suite passes in 217.75 seconds.
C2 selects this cache and keeps original generated hints. Against B0, the
combined implementation saves 6,491 LUTs, 1,231 slices, 4,760 FFs and 2,148 CARRY4s;
it adds half a BRAM tile. Routed setup slack remains -3.881 ns.

The one-block alternative saves another 21 slices, 337 LUTs and one BRAM, but
its observed maximum is 15,129 clocks higher. C2 favors the primary sequential
playback workload while retaining all strided, deep retirement and operation
regressions/costs in the comparison. This selection establishes the control for
Stage 3; it does not complete the remaining architectural or precision trials.


The first Stage 3 audio bookkeeping trial is complete and rejected for the
default. Every one of 33 callback rows, 1,282 public storage rows and 187 boundary
rows per AXI width matches C2 exactly, including cycles and traffic. Full
metadata, ownership, media and deferred regressions pass. It saves 115 LUTs/28
FFs/22 carries at optimization, but after routing saves only 97 LUTs/four FFs
and uses **100 more slices** and 25 more physical control sets. Setup slack
improves from -3.881 to -3.610 ns and still fails.

This small type change illustrates the packing constraint: logical control sets
fall by one while physical control groups increase. C2 remains the default;
the MIDI width sibling starts from C2, without inheriting this rejected trial.
See `s3-audio-100-*` for full reports and matching operation/callback samples.

The MIDI width sibling `f28096cb` passes all 10,311 standalone boundary requests
with identical responses, clocks and memory traffic, including minimum/odd pools
and primary 256-view/128-retirement indices. Full 16-test MIDI regression passes
in 527.04 seconds. Its 33 mixed/state-on/delay7 callback rows exactly match the
same prefix of C2's extended run. Optimization saves 363 LUTs, 146 FFs and 59
CARRY4s, with unchanged memory/DSP/control-set counts. Routed slices and the
selection decision remain pending. The critical path still runs through the
audio metadata node-count arithmetic, unchanged by this MIDI candidate.
