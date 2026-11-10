# Continuing single-core optimization search

Target: Raspberry Pi 5, Cortex-A76, 64-bit Linux. Preserve the compact prototype's
existing audio-only functionality. No worker-core DSP, new features, approximate
sample arithmetic, or removed meters to manufacture performance gains.

## Method

Use short runs pinned to one CPU, including controller and host threads. Build
with one Cargo job and do not build during timing. Keep baseline and candidate
binaries separately. Rotate comparison order and retain raw callback records,
including scheduling outliers. Validate with existing sample/capture oracles.
Host results screen hypotheses; only actual Pi 5 runs can rank target performance.

## Best solutions

Measurements below are from the local x86 host. Percentages use different stated
controls and must not be added together. None establishes Pi 5 performance.

| Candidate | Evidence | Status |
| --- | --- | --- |
| Normal-host PGO + packed source + ThinLTO/CGU1 | 20 workflows pass; four rotated native comparisons: Compact p50 5–18%, Current 6–10% below thin/one; executable 11% smaller | Strongest tested native build; p99 mixed; Pi candidate independently trained and unmeasured |
| Balanced PGO + packed source + ThinLTO/CGU1 | Four rotated offline comparisons: Compact p50 8–16% and Current 9–11% below thin/one; untrained densities also improve | Offline contender; tails mixed; normal-host and Pi timing pending |
| Packed retained source + ThinLTO/CGU1 | 20 normal-host workflows pass; Compact p50 9–12% and Current 5–9% below packed/default profile | Validated build option; p99 mixed; static Pi candidate packaged |
| Packed retained source, default profile | 331 selected tests and 20 native workflows pass; Compact p50 7–14% below previous retained source, Current 0–5% better | Current source baseline; p99 mixed |
| Compact stopped-publication cache + borrowed reference visitor | Cache native medians 3–11% lower; borrowing removes a descriptor copy and improves micro timings 15–40% | Both included in packed source; cache restricted to Compact |
| Current inactive-node pruning + steady clock + constant visitor | Fresh Current callback medians 17–48% below original branch source | Included in retained source; preserves skipped-node accounting |
| Constant-gain reference visitor | Original native dense callback medians 5–17% lower with arithmetic/edge oracles passing | Included in retained source; retains sample multiply and ramp behavior |
| Packed source + ThinLTO, default codegen units | Four-backend screen: Current 4–7% below default, Compact 0–4% | Alternative build contender; larger ELF; not normal-host validated |
| Original compact branch, `69b52609` | Existing COMPACT_PERFORMANCE.md Pi 4 and FOURTH_RESULTS.md local results | Historical comparison baseline |

The packed source includes smaller validated audio descriptors and removes an
unused resolver state copy. Playback-only routing contenders, cached idle
materialization, grouped stereo reads and other rejected experiments are recorded
below, with patches and data retained; they are not part of the source baseline.

## Final status

The user ended the open optimization search after the current profiling round
and requested a PR. Experiment33 is complete and reverted: independently trained
PGO eliminated CFG-mismatch warnings but regressed every measured native median
(Current8–10%, Compact2–5%). The retained packed source and prior normal-host PGO
build remain the best verified solutions. See [the results overview](SINGLE_CORE_SEARCH.md)
for the final table, retained changes, reproducibility and validation limits.

## Environment

Session started 2026-11-09 and continues on 2026-11-10. AMD EPYC 9V74 VM, x86_64,
five visible CPUs; all experiments pinned to CPU 0. Rust 1.99.1 installed locally
under ignored work/. No native x86 SIMD flags. Native dummy-host build excludes
driver/plugin features in every compared binary; actual engine and normal socket
client remain in scope. The static Cortex-A76 bundle is available in
`work/pi5-audio-bench.tar.gz`; ARM emulation validates behavior only, never timing.

No new optimization is promoted until correctness and measurements are recorded.

## Experiment 1: constant-gain visitor

Four short rotated repetitions of the actual RenderArena methods, CPU 0,
20,000 calls per row per repetition. Median nanoseconds per call:

| Frames / gain | Operation | Baseline | Candidate | Change |
| --- | --- | ---: | ---: | ---: |
| 32 / unity | read | 126.72 | 41.38 | -67.3% |
| 32 / unity | peak | 127.01 | 39.34 | -69.0% |
| 32 / unity | accumulate | 124.57 | 42.31 | -66.0% |
| 32 / 0.7 | read | 122.79 | 41.47 | -66.2% |
| 32 / 0.7 | peak | 128.41 | 36.56 | -71.5% |
| 32 / 0.7 | accumulate | 129.63 | 38.61 | -70.2% |

At 256 frames, constant-gain reductions were 75–81%. At 32 frames ramp changes
ranged from -1.7% to +2.6%. One-frame cases were mixed (some ~13–21% regressions).
This screen uses contiguous transient references; actual persistent stereo audio
uses strided access and needs callback measurements. No Pi 5 result is implied.
Raw CSVs and immutable binaries currently live in ignored work/.

Full callback screens passed media/state/continuity checks with no dropped records
or render faults. Small session: three rotated runs; dense session: four balanced
rotations, eight stereo tracks, 32 active voices, 64 loop cells. All control and
render threads share CPU 0. Each selected window is 256 callbacks at buffer 32,
or 64 at buffer 256. Values below are medians of run percentiles, microseconds.

| Workload / buffer | Scenario | Baseline p50 / p99 | Constant p50 / p99 |
| --- | --- | ---: | ---: |
| Small / 32 | idle | 7.531 / 36.554 | 7.421 / 44.215 |
| Small / 32 | play | 16.084 / 68.541 | 14.982 / 105.966 |
| Small / 32 | record | 15.332 / 50.354 | 15.153 / 61.170 |
| Small / 32 | ramps | 17.416 / 36.604 | 17.842 / 76.993 |
| Small / 256 | idle | 33.895 / 203.900 | 32.067 / 159.013 |
| Small / 256 | play | 104.274 / 299.651 | 84.689 / 352.589 |
| Small / 256 | record | 81.615 / 196.990 | 86.727 / 334.282 |
| Small / 256 | ramps | 115.084 / 251.400 | 101.960 / 196.780 |
| Dense / 32 | idle | 38.306 / 139.425 | 34.095 / 138.729 |
| Dense / 32 | play | 74.362 / 251.250 | 64.242 / 293.923 |
| Dense / 32 | record | 66.683 / 206.559 | 63.303 / 279.151 |
| Dense / 32 | grab | 77.034 / 263.754 | 64.299 / 228.827 |

Correctness: existing reference-arena test and both engine audio-variant oracles
passed, including controls, partitions, wraps, grabs and continued capture.
Decision: retain constant gain as a throughput contender, not a universal winner.
Its local p99 and recording regressions remain unresolved. Baseline remains a
comparison incumbent. Raw callback/micro CSVs, binary hashes and per-run summaries
are preserved under results/single-core-2026-11-09/.

## Deterministic offline callback comparison

To distinguish render CPU work from normal host/controller scheduling, added an
OfflineHost benchmark with the same audio-only engine and deterministic input.
Eight stereo strips, eight loop cells per strip, four active voices per strip.
Setup/collection/checksum calculation are outside timing. Every measured report
must remain Running; playback must be non-silent; samples must remain finite.
All modes retain ordinary capture, history, routing and meters.

Six rotating repetitions per candidate, 512 callbacks at buffer 32 or 64 at
buffer 256; Q=32. The environment briefly reattached before the final six of
108 runs; completed CSVs were retained, only missing runs resumed. All candidate
output-power checksums matched at printed precision. Median run percentiles (us):

| Buffer | Scenario | Baseline p50 / p99 | Constant p50 / p99 | Topology cache p50 / p99 |
| --- | --- | ---: | ---: | ---: |
| 32 | idle | 30.140 / 92.506 | 25.753 / 69.958 | 30.059 / 56.889 |
| 32 | play | 61.466 / 144.743 | 48.502 / 131.313 | 62.437 / 151.007 |
| 32 | record | 50.625 / 194.882 | 46.493 / 155.629 | 51.125 / 139.175 |
| 256 | idle | 231.195 / 371.517 | 193.290 / 461.399 | 233.995 / 448.756 |
| 256 | play | 523.090 / 818.465 | 404.661 / 676.431 | 545.688 / 777.699 |
| 256 | record | 404.586 / 706.880 | 366.625 / 588.632 | 404.956 / 669.536 |

Constant gain is the current local render-throughput leader (8–23% lower p50).
Target ranking and a live p99 improvement are still unproven. Topology caching
has no consistent p50 gain and is reverted; its differential clock oracle and
both audio-variant oracles passed. Retain topology-cache-rejected.patch for the
record, rather than accumulating an unhelpful optimization.

Replay example (same command for each saved source variant):

```sh
cargo build --locked --release -j 1 -p shoop_rt_engine --features testing --example audio_render_benchmark
taskset -c 0 target/release/examples/audio_render_benchmark compact play 32 8 512
```

## Experiment 3: overwrite-specific host input import

Starting from constant gain, factor arena reservation from zero initialization.
Keep public allocation zero-initialized; only a private copy_input method reserves
and immediately overwrites its complete range before exposing a sequence. No
uninitialized memory or unsafe code. Only compact host-input routing uses it.
Logical work counters omit the removed zero writes. Correctness/timing pending.

Overwrite-specific import passed the reference-arena and engine oracles, and the
Cortex-A76 warning-denying runtime-library cross-build passed. The constant-gain
peak loop contains NEON vector multiply and integer maximum instructions; its
assembly is retained as cortex-a76-peak.s. This is compilation evidence, not Pi
benchmark data.

Six rotated offline comparisons against constant gain gave changes from about
-1.2% to +1.0% in median callback cost, without a consistent win. Reverted the
extra arena API/factoring and retained overwrite-rejected.patch and per-run data.
The third experiment therefore does not enter the best-solutions list.

## Experiment 4: prepared input initialization

At graph compilation, mark each input port's first incoming route and record only
unconnected input ports for explicit clearing. At execution, overwrite connected
ports on their first route, reset their mix flag there, and clear only unconnected
ports. Later fan-in contributions retain the existing mixing algorithm. Operation
outputs are still cleared before activity checks, including skipped nodes.

Prepared scratch/capacity is bounded by max_ports and prefaulted; no render-time
allocation is added. The tradeoff is larger prepared route/operation metadata.
Correctness and timing pending; source currently includes constant gain plus this
candidate. Topology-cache and overwrite-import experiments are reverted.

Prepared input initialization passed 13 tests across audio_only, audio_variants,
structure and workflow, including slot reuse/churn, routing and silent-loop skips.
The Cortex-A76 runtime-library cross-build also passed with warnings denied.
Six rotating comparisons showed small, inconsistent p50 changes: roughly +1.3%
idle/record at buffer 32, -0.7% playback; at buffer 256, -2.5% playback but +1.4%
idle and +1.6% recording. Reverted; patch and full results retained.

## Experiment 5: avoid redundant peak RMW

Retain finite/positive validation. Load the held peak with Relaxed ordering, and
perform the existing Relaxed fetch_max only if the new positive-float bits are
higher. A skipped update linearizes at the load: a concurrent take after that
load already observes an equal or greater held peak. Updates that proceed still
use fetch_max, preserving a concurrent larger peak. No ordinary store replaces
the atomic maximum. Test monitoring semantics and inspect Cortex-A76 codegen,
where LSE atomics can make the cost tradeoff differ from x86.

Meter publication passed 25 tests and Cortex-A76 warning-denying codegen (which
uses LSE ldumax). Its apparent ~4% small-buffer playback gain shrank below 1% in
12 further rotating repetitions; p99 worsened. Reverted and preserved both sets
of results plus meter-rejected.patch.

## Experiment 6: factor constant gain out of peak reduction

A finite constant gain permits reducing absolute sample values before multiplying
once; nonfinite unscaled samples fall back to original evaluation. New explicit
zero/negative gain, infinity and NaN regression cases passed, as did engine sample
and 14 monitoring oracles. The test-attribute policy check passed.

Callback p50 improved only 0.3–1.7% in six rotations. Eight paired micro-screen
repetitions showed Q=32 peak scans actually ~3% slower, and one-frame scans
~12–16% slower. Reverted the algebraic rewrite; retained the new exceptional-value
regression test to protect the incumbent constant-gain visitor. Raw micro rows,
callback results and peak-algebra-rejected.patch are preserved.

## Experiment 7: materialized renderer with inactive-node pruning

Switching backend approaches: try the compact activity contract in the ordinary
materialized evaluator for MIDI-free, explicitly opted-in processor types. Clear
outputs before querying activity; propagate only proven silence from skipped
nodes to downstream activity queries. Active/host sources remain unknown. A
prefaulted per-node flag array records silence without sample scanning. Preserve
normal routing/mixing, storage and loop settlement. Counters report actual skipped
operations. This is an experimental source variant, not a new startup mode.

Compare original current, pruned current and incumbent compact in the same
OfflineHost session shapes. Correctness/timing pending. Working source currently
contains the constant-gain incumbent, its extra semantic test, and this prototype.

Materialized pruning passed 18 distinct engine tests. The one initial structure
failure was the explicit diagnostic count (five executed instead of eight);
updated that test to require five executed/three skipped for both current and
compact. Its rerun and workflow test passed, as did the test-attribute check.

Six balanced rotating offline repetitions, median run percentiles in us:

| Buffer | Scenario | Original current p50 / p99 | Pruned current p50 / p99 | Compact + constant p50 / p99 |
| --- | --- | ---: | ---: | ---: |
| 32 | idle | 56.213 / 133.527 | 43.299 / 100.153 | 25.658 / 66.032 |
| 32 | play | 66.538 / 176.425 | 58.036 / 129.421 | 48.251 / 83.603 |
| 32 | record | 73.223 / 156.496 | 66.974 / 119.150 | 46.248 / 101.665 |
| 256 | idle | 455.681 / 691.543 | 336.235 / 608.015 | 195.183 / 440.915 |
| 256 | play | 525.784 / 758.512 | 466.096 / 708.674 | 400.460 / 706.821 |
| 256 | record | 590.084 / 911.797 | 533.912 / 847.452 | 369.253 / 568.948 |

All output-power checksums agreed within 1e-6 relative tolerance. Pruned current
is the best materialized contender, while compact remains the throughput leader.
Current's ordinary storage/materialization remains in use.

Normal socket app workflows: all ten scenarios passed for both pruned current and
compact, including silence, record/play, sharing, continued recording, grab,
churn and ramps. Cortex-A76 runtime-library warning-denying build passed.

Four balanced live dense buffer-32 repetitions (us):

| Scenario | Original p50 / p99 | Pruned p50 / p99 |
| --- | ---: | ---: |
| idle | 82.108 / 203.605 | 68.418 / 309.660 |
| play | 96.384 / 259.642 | 84.502 / 197.892 |
| record | 102.816 / 298.965 | 103.099 / 609.813 |
| grab | 108.775 / 288.675 | 103.240 / 440.799 |

All runs passed capture/readback/continuity checks with zero dropped records and
render faults. Live recording p50 did not improve and most p99s worsened; do not
substitute the offline CPU table for live tail behavior. Evidence is retained in
materialized-live-results.json and materialized-callbacks.tar.gz.

## Experiment 8: alias unity fader outputs in materialized execution

Starting from pruned current, test a Processor passthrough result for equal audio
input/output arity. Eligible unity/no-processor faders retain peak publication and
clock advancement but let outgoing routes read the already prepared input slab.
The input slab stays owned by the same slot until all downstream consumers finish.
No pointers, unsafe code or render-time allocations. Clear the forwarding flag
with every buffer clear, preserving fallback and callback-partition behavior.
Inactive-node output clearing is retained. Correctness/timing pending.

Alias faders passed 27 engine tests and the test-attribute check. Six balanced
rotations showed small buffer-32 gains but buffer-256 playback/record regressions
of about 1–2%. Reverted the new trait API, route flag and fader handler; retained
materialized-alias-rejected.patch and alias-results.json.

## Experiment 9: compact clock shortcut with current storage

Enable LoopPlanner's existing compact clock/view eligibility machinery for
MIDI-free Current as well as Compact. Keep Current's control policy, ordinary
storage and materialized evaluator. Existing guards retain full settlement for
pending transitions, sync-wrap boundaries, dry/wet edits and ineligible views.
No new media representation or processing API.

Passed 27 engine tests and 98 selected loop unit/integration tests covering audio,
runtime, shared audio, dry/wet, topology replacement and borrowed views.
Six balanced rotating comparisons, median run percentiles in us:

| Buffer | Scenario | Pruned current p50 / p99 | + steady clock p50 / p99 | Compact + constant p50 / p99 |
| --- | --- | ---: | ---: | ---: |
| 32 | idle | 43.124 / 114.599 | 30.505 / 70.239 | 25.943 / 76.493 |
| 32 | play | 58.651 / 125.135 | 44.681 / 111.900 | 48.076 / 119.666 |
| 32 | record | 66.949 / 152.645 | 60.784 / 138.484 | 46.634 / 115.676 |
| 256 | idle | 336.220 / 592.132 | 234.135 / 471.980 | 194.842 / 411.186 |
| 256 | play | 459.933 / 757.115 | 370.485 / 687.498 | 403.745 / 580.390 |
| 256 | record | 545.343 / 784.531 | 487.869 / 694.813 | 366.410 / 532.138 |

All output-power checksums agreed within 1e-6 relative tolerance. The new Current
variant leads local playback p50, while Compact still leads idle and recording.
Its 256-frame playback p99 does not beat Compact's. Native workflow/timing
validation is next. Only actual Pi 5 measurements can rank target hardware.

A separate process-RSS screen (three repetitions, offline playback, same capacities)
gave median maximum RSS of 141.54 MiB original Current, 141.48 MiB pruned Current,
and 185.93 MiB Compact. This describes the fixed one-second-history offline setup,
not a general application memory forecast. Raw process rusage is preserved.

Steady Current native validation passed all ten workflow scenarios (both current
and compact) and a Cortex-A76 warning-denying loop-library cross-build.
Four rotating dense Current comparisons at buffer 32 (us):

| Scenario | Pruned p50 / p99 | + steady clock p50 / p99 |
| --- | ---: | ---: |
| idle | 67.067 / 170.737 | 44.085 / 141.288 |
| play | 85.954 / 210.700 | 60.757 / 148.444 |
| record | 100.297 / 343.536 | 76.850 / 171.233 |
| grab | 103.948 / 325.595 | 74.254 / 204.811 |

A separate direct, same-binary, four-rotation native leader comparison gave:

| Scenario | Steady Current p50 / p99 | Pure Compact + constant p50 / p99 |
| --- | ---: | ---: |
| idle | 42.165 / 127.598 | 33.775 / 212.450 |
| play | 62.582 / 181.588 | 59.054 / 191.843 |
| record | 78.831 / 228.762 | 59.711 / 147.282 |
| grab | 74.627 / 227.285 | 61.648 / 188.253 |

Compact wins native playback p50 by about 6%, reversing the offline ranking;
Current's smaller memory and offline throughput remain useful tradeoffs. Keep
both, and do not report a universal winner. Every native run passed state/media
checks with zero dropped records and render faults. Raw evidence retained.

## Experiment 10: flat storage with fully materialized execution

An isolated source probe retained Compact storage, admission and clocks while
forcing generic materialized execution. Basic playback/recording state and
checksum checks passed. Idle p50 improved about 8–10%, but active playback lost
to Current and recording lost to reference Compact. The probe deliberately
changes copy work; it is not claimed to meet Compact's reference-copy bound.
Reverted its constructor override; preserve typed-materialized-probe.patch and
results. It motivated selective dispatch rather than a new public render mode.

## Experiment 11: cached idle materialization in Compact

Planner hint defaults false. For LoopPlanner, reuse steady-view eligibility and
cache whether all settled loop modes are stopped. Only cached-view idle ticks
choose ordinary sample execution; active playback/capture remains reference
execution. Existing generic processor activity checks prune inactive nodes.
Both paths use the same prepared buffers/storage and have no callback allocations.

Initial row-scanning hint passed 27 engine tests and improved idle p50 ~7–9%.
Cached hint avoids the extra row scan; its six-rotation comparison against pure
Compact gave idle p50 23.435 vs 25.572 us at buffer 32 and 176.650 vs 194.732 us
at buffer 256. Active medians are close and tails are mixed. Retain results for
both hint variants. The benchmark counts arena materializations: zero on idle
hybrid windows, nonzero during reference playback. These counters exclude fixed
sample-buffer work; benchmark metadata now explicitly states that scope.

Keep prepared program compilation synchronized on graph epochs even while idle,
so the optimization does not defer compile work to the first active callback.
After that adjustment, all 97 runtime unit, 27 selected engine and 98 selected loop
tests passed; test-attribute policy passed. Native/ARM validation is pending.

## Full-history benchmark validity

The offline benchmark now optionally prefills one second of input history before
starting capture, then retains the original short capture/warmup/measurement.
It validates every active loop's final mode, nonzero length and incomplete-capture
flag after a telemetry flush. The original short recording configuration passed
this stricter validation. An initial attempt to keep all 32 recorders active
through a one-second warmup exhausted Compact's configured capture reservation;
that failed run is excluded. Prefilling before capture passes for both leaders
without increasing any pool capacity.

With history prefilled, six-rotation hybrid idle p50 remained ~8–9% lower than pure
Compact. Active playback medians were ~1–2% higher; recording essentially equal.
Current's isolated playback lead narrowed to ~2% at buffer 256. Detailed rows are
in fullhistory-results.json. No Pi 5 measurements have been obtained.

Hybrid native validation passed both modes' ten workflows and a Cortex-A76
warning-denying build. However, the four-rotation dense native AB was weaker than
the offline screen (us):

| Scenario | Pure Compact p50 / p99 | Cached hybrid p50 / p99 |
| --- | ---: | ---: |
| idle | 32.986 / 118.815 | 32.806 / 107.279 |
| play | 60.682 / 265.175 | 60.266 / 171.097 |
| record | 59.703 / 163.997 | 63.163 / 245.792 |
| grab | 62.063 / 216.815 | 63.997 / 212.743 |

Idle arena materializations were 13/tick pure and zero/tick hybrid in every run,
so the alternate path really executed. Idle p50 barely improved, recording p50
regressed ~6%, and grab ~3%. Reverted the hint API, cached field, dispatch and
program-sync helper from the default source. Keep cached-idle-hybrid-rejected.patch
as an offline idle contender and retain all native evidence. Do not promote it
as the general Compact winner.

## Experiment 12: process after an already-completed activity query

Pruned materialized execution already calls EngineNode::compact_active, which
applies configuration. EngineNode::process then checked configuration again.
Add a default process_after_activity hook that normally delegates to process;
EngineNode reuses its processing body without the second configuration read.
Only callers that have just obtained an active result use it. Ordinary/MIDI and
legacy reference callers retain normal process behavior; Compact fallback sample
processing can also reuse the completed activity query. No new state or buffers.

The 27 selected engine tests passed. Build a fresh control with the identical
benchmark harness, then compare both Current and Compact before retaining the API.
Fresh, source-verified follow-up rejected this candidate; see the audit below.


## Build-freshness audit and renewed controls

Restoring source with metadata-preserving copy2 left old modification times and
allowed Cargo to reuse a binary built from different source. One preliminary
process-after-activity comparison had identical binary hashes and is excluded.
Its apparent ~3% p50 difference is useful only as a null variability check.
Earlier incremental ablations should not be treated as source-exact controls.

Rebuilt original, retained bundle, and after-activity variants from explicit full
source snapshots. Every restore writes fresh files and touches dependent crate
roots. Record source hashes before/after compilation, all crate Rust hashes,
benchmark hash, Cargo.lock, command/toolchain and distinct binary hashes. Fresh
six-rotation results reconfirm the retained bundle (median p50 microseconds):

| Mode / buffer | Scenario | Original | Retained |
| --- | --- | ---: | ---: |
| Current / 32 | idle | 56.669 | 30.210 |
| Current / 32 | play | 65.772 | 45.697 |
| Current / 32 | record | 72.592 | 60.364 |
| Compact / 32 | idle | 30.019 | 25.893 |
| Compact / 32 | play | 61.696 | 49.909 |
| Compact / 32 | record | 50.384 | 46.699 |
| Current / 256 | idle | 458.886 | 238.001 |
| Current / 256 | play | 528.995 | 363.821 |
| Current / 256 | record | 583.134 | 487.288 |
| Compact / 256 | idle | 228.942 | 191.041 |
| Compact / 256 | play | 529.325 | 398.568 |
| Compact / 256 | record | 408.877 | 369.855 |

The benchmark validates finite samples, nonzero playback, active loop modes,
lengths and complete captures. Checksums match across compared implementations.
These are AMD VM screens, not Pi 5 measurements or worst-case timing bounds.
Raw renewed controls and build manifests are archived as verified-*.

The after-activity hook's initial fresh result suggested small gains, but a
16-rotation follow-up shrank Current playback benefit to 0.6% at buffer 32 and
2.2% at 256, with worse p99 in some cases. Compact idle was essentially flat.
Reverted the extra Processor API and archived after-activity-rejected.patch.

## Experiment 13: unchanged stopped-loop publication

Loop boundary publication writes many SeqCst atomics each quantum, including
stopped primitive loops whose public telemetry does not change. Cache the last
stopped primitive state together with topology epoch and pending transition.
Suppress a publication only when all are equal. Preserve all atomic orderings,
force publication for a pulse at the current frame, invalidate the cache while
active, and always publish composites because their public frame must advance.
The fixed-size cache is allocated/touched before render. No meter or monitoring
work is removed. Existing 59 engine library tests (one ignored) and selected
integration tests passed. Short source-verified single-core comparison pending.


Six-rotation offline screen: unchanged stopped publication reduced Current idle
p50 by 5.1% at buffer 32 and 4.4% at 256; Compact idle by 7.0% and 8.2%.
Active changes are smaller: Current play 1.3–2.7%, record ~1.8%; Compact play
3.4% at 32 and flat at 256, record 1.6–3.0%. Tails remain mixed. All sample/state
checks pass. Retain as a candidate pending normal-host and Cortex-A76 validation.
Try reducing the cache key to fields actually published before native comparison;
the full State cache carries private resolver fields irrelevant to telemetry.


The smaller publication key removes private resolver fields and keeps epoch,
pending transition, generation, content length/revision, timing revision,
position and capture-incomplete. Stopped mode and absent composite are guards;
pulse-now always publishes. A second six-rotation three-way screen retained the
idle benefit: small cache vs baseline Current 6.5% / 6.5% at buffers 32 / 256,
Compact 10.5% / 7.0%. Active p50 gains are smaller and variable (Current playback
1–2%, recording 0–4%; Compact playback 2–4%, recording 3–6%). Tails mixed.
Use the smaller key for the normal-host validation. Raw stopped-small-* contains
both cache forms and the verified retained control. The new integration test
checks that stopped composite telemetry's frame advances every callback; passed.


Smaller-key validation passed 104 selected engine library/integration tests
(one existing ignored), the test-attribute policy and a warning-denying engine
library compile for AArch64 with root target-cpu=cortex-a76. This checks code
generation, not execution on a Pi. Two fresh normal-host builds record full
source hashes and distinct binaries; native workflows/AB now in progress.


All 20 normal-host workflows passed for Current and Compact. The A76 observer
assembly confirms a 64-byte cache slot and retains STLR/LDADDAL publication on
changed states. The unchanged comparison itself has several loads/branches;
its net ARM timing remains unknown. The local x86 result must not be used to
claim a Pi speedup. Paired native timings still pending.


Four-rotation native screen (median p50 / p99 microseconds):

| Mode / scenario | Retained control | Stopped cache |
| --- | ---: | ---: |
| Compact idle | 35.187 / 155.779 | 31.406 / 65.541 |
| Compact play | 59.845 / 207.841 | 57.422 / 170.396 |
| Compact record | 60.139 / 178.238 | 57.252 / 176.540 |
| Compact grab | 62.367 / 182.383 | 60.246 / 197.466 |
| Current idle | 41.198 / 105.675 | 39.191 / 84.835 |
| Current play | 59.696 / 147.291 | 60.216 / 202.228 |
| Current record | 77.111 / 181.882 | 77.276 / 213.925 |
| Current grab | 71.363 / 181.112 | 74.442 / 195.738 |

No general winner: retain the full-key patch/results as evidence, but restrict the
cache experiment to Compact before promotion. Current active medians were flat
or worse and tails worse. Compact medians improved 3–11%; its grab tail worsened.
Both source-exact native build manifests and raw callback CSVs are archived.
Next independent direction: fixed-stride persistent visitor loops and borrowing
the wide sequence descriptor inside the private visitor; inspect A76 codegen.


## Experiment 14: visitor descriptor borrowing and fixed source strides

The A76 peak prologue copies the 144-byte AudioSequence onto a 224-byte stack
frame. The private visitor unnecessarily takes/mutably iterates a value copy.
Borrow it immutably without changing public APIs. A separate variant specializes
constant-gain persistent stride 1/2 using contiguous/fixed-size iterators; arbitrary
strides and ramps retain the original path. Preserve every sample multiply.

Three fresh micro builds each compile the same transient and new persistent
benchmark examples, plus a warnings-denying A76 runtime library. The borrowed
variant removes the descriptor copy and reduces the peak stack frame to 96 bytes.
The stride variant adds NEON arithmetic but increases code size/stack; performance
still requires measurement. No Pi timing claim follows from static codegen.

The new persistent benchmark checks bitwise samples and integer-bit peak against
an independent scalar oracle for all channels before timing. An initial attempt
included unsupported Quantum(256), which failed with InvalidConfig and is
excluded. Correct supported quantum cases are 1,32,128; the transient screen can
still use 256 samples because its configured runtime quantum remains32. Rebuild
all micro controls with the identical corrected harness before timing.


Five rotated micro repetitions passed every sample/peak oracle with corrected
matching harnesses. At32 frames the borrowed visitor reduced transient unity
read43.08->33.84ns, peak37.06->25.82ns, accumulation39.06->31.77ns. Persistent
stereo unity read52.99->39.17ns, peak63.14->39.61ns, accumulation52.02->37.66ns.
Stereo ramp read128.02->116.34ns, peak130.83->112.51ns, accumulation127.65->112.45ns.
Results include channels1/2/3 and all supported benchmark lengths. This remains a
micro screen; callback confirmation pending.

The fixed-stride-only visitor is not retained: stereo unity peak63.14->92.22ns
on this VM, mono63.55->83.06ns, most other operations flat. Its A76 vector code
uses lane gathers and grows the peak function/stack. Archive patch and assembly
as a target-specific contender only; do not infer ARM speed from vector op count.
Working visitor now borrows the descriptor; cache is restricted to Compact.


Compact-only gating allocates no publication cache for other backends. The
Current composite-frame regression remains in its supported configuration;
an attempted Compact composite case was refused Unsupported by the prototype's
explicit feature gate and was removed from the test, with no functionality
change. The corrected selected engine run passed104 tests, plus97 runtime tests.
Fresh three-way callback controls now build the retained source, Compact cache,
and Compact cache+borrowed visitor under identical harness/flags. No timing
runs overlap compilation. Tests/source stay unchanged during each build.


Six source-verified callback rotations confirmed descriptor borrowing. Compact
cache-only -> cache+borrow p50 microseconds: idle32 23.635->22.654, play32
48.096->44.235, record32 45.578->44.500; idle256 176.951->171.743, play256
397.742->355.298, record256 356.866->346.731. Playback improves8–11% beyond the
cache, recording2–3%, idle3–4%. Combined versus retained Compact: idle11–12%,
play9%, record4–7%. Some p99s improve and others worsen; no WCET claim.

Current mostly bypasses the reference visitor and is a useful control: callback
p50 shifts are around1–2%, including a slight idle/record cost after cache gating.
The retained control rebuilt to EXACT previous SHA3d14dc0..., validating fresh
source restoration and reproducible flags. All checksum/capture assertions pass.
Compact cache+borrow is the newest offline contender, pending native validation.

## Experiment 15: eliminate unused compact resolver scratch copy

compact_tick writes each whole State to scratch.states and also emits a Decision.
Its special commit path reads Decisions, and normal settlement/topology replacement
first copies committed states into scratch. Other scratch-state readers belong
to Boundary settlement after that initialization. Remove the unused per-node copy
without changing committed state, decisions, events or eligibility. Existing
compact-clock oracle covers256ticks, synchronized wraps, recording, transitions,
fallback and full state/event/receipt equality. Test and measure before retaining.


The unused scratch-copy removal passed130 selected loop library/integration tests.
Build only that change on top of Compact cache+borrow, verify identical benchmark
hash/flags and changed binary hash, then compare both Current/Compact at32/256.
A further staged hypothesis: the visitor still computes ramp-step FP division
before testing finite nonzero constant endpoints. A76 assembly confirms FDIV on
the common path; consider skipping that calculation only under the already
established constant guard, preserving zero/nonfinite/ramp behavior.


Six-rotation scratch-copy screen mostly shifts p50 by0–2.5%, tails mixed.
Current idle32 30.781->30.319us, Compact idle32 22.849->22.273us; Compact play32
44.621->43.794us, play256 367.326->361.853us. Some cases flat or slightly worse.
Retain as a one-line dead-write elimination with130 tests passed; do not rank it
as a confirmed target speed win. Raw no-scratch-* and source patch archived.

## Experiment 16: skip ramp-step calculation under constant guard

Compute the existing finite/nonzero/equal-endpoint guard before step. Setstep0
under that guard; preserve original formula for every other gain. This avoids
unnecessary floating subtract/conversion/division in the common constant path.
Zero, nonfinite endpoints and ramps keep their exact previous handling. Build
identical borrowed-visitor control/candidate micro harnesses and inspect A76
control flow before whole-callback comparison.


Five micro rotations passed all sample/peak oracles for the no-step variant.
A76 assembly skips FDIV on constant gain. Local Q32 effects are mostly flat or
mixed: transient unity read33.90->35.31ns, peak26.57->26.36ns, accumulation
31.46->34.61ns; persistent stereo unity peak39.57->37.72ns. Archive as an ARM
contender pending real Pi measurement rather than retain by default. Restore
the borrowed visitor before the next independent metadata experiment.

## Experiment 17: clear only monitoring scratch that was used

Demand::update clears its direct pending-transition table every quantum, even
when already empty, and clears all future-program slots when none were queued.
Track whether direct scratch was populated; clear it on the following update
only when dirty. Clear future slots through the previous bounded queue before
resetting it. Keep recording reset and all pending/composite semantics. Move the
program lookup after the live composite cursor check. One bool, no extra arrays,
allocation or atomic-ordering change. Run the existing nested-monitoring oracles.


The monitoring scratch experiment passed104 tests but six rotations showed
mostly0–1% effects, mixed tails and Current record256~2% worse. Compact play256
was2.5% better; other active cases did not consistently benefit. Reverted the
extra dirty flag, sparse clear and lookup reorder. Preserve sparse-monitoring-*
raw data and rejected patch. Move to descriptor layout rather than chase noise.

## Experiment 18: narrow internal audio descriptor indexes

Opaque AudioStoreId's slot is bounded by the validated resource-slot capacity;
AudioChunkRef's first/stride are page-local sample indexes. Prepared storage
has an i32 ceiling, and actual chunk page sizes are bounded by the validated
max_frames*channels*4 limit; global loop positions remainu64. Store these three
internal fields asu32 and widen at indexing boundaries. No serialization, PCM,
gain, ownership/generation or public handle-construction change.

Old64-bit sizes measured: AudioStoreId16, AudioChunkRef48, ChunkRef56,
ChunkSpan64, AudioSequence144 bytes. Narrowed implementation passed all97 runtime
tests, including lease lifetime/COW/capture/refusal oracles. Engine tests, actual
new sizes and throughput screens pending. Current source has narrow descriptors
on top of the compact-only publication cache, borrowed visitor and dead scratch
write removal. The no-step and monitoring variants are both reverted.


Measured64-bit packed sizes: AudioStoreId8, AudioChunkRef28, ChunkRef32,
ChunkSpan40, AudioSequence96 bytes (33% less per sequence). All97 runtime and104
selected engine tests passed. Root Cortex-A76 runtime compile passed-Dwarnings;
its assembly now loads sequence length atoffset80 and gains88, consistent with
the smaller64-bit representation. Fresh callback candidate binary/source hashes
recorded as packed-descriptors. The four-backend timing screen uses the unchanged
callback harness, identical flags and no overlapping compilation.


Four rotated callback repetitions across ALL FOUR backends passed matching
sample/capture assertions. Narrowing adds Compact p50 improvements7.1%/8.5% idle,
10.6%/11.0% playback and5.4%/7.5% recording at32/256 buffers. At32: Compact
22.108->20.541us idle,44.626->39.899us play,43.875->41.512us record. At256:
167.427->153.266us,360.632->320.938us,346.576->320.657us. Tails remain mixed.
Older Chunks/ChunksDescriptors improve roughly4–9%; Current mostly flat (one
play256 case3% better). This tests the unchanged whole callback, not just a
struct-size claim. Native bundle build now pending, compared with the immutable
source-verified retained host under identical feature flags.


Packed native bundle built with the same command/compiler/flags as immutable
retained control. Full source-before/after hashes match; binarySHA606c8eea...,
control000d572d.... Root engine Cortex-A76 compile passed-Dwarnings. All130
selected loop tests and test-attribute policy passed after packing. Native
workflow/AB starts only after all compilation and target installation finish.
Rust ARM64-musl target is installed for a later runnable target bundle; no Pi
execution or deployment has occurred.


Native validation completed across midnight2026-11-09/10UTC. All20 workflows pass.
Four-rotation dense native p50/p99 microseconds, retained -> packed bundle:

| Mode / scenario | Retained | Packed bundle |
| --- | ---: | ---: |
| Compact idle | 33.367 / 105.300 | 28.569 / 103.177 |
| Compact play | 58.832 / 223.815 | 50.316 / 207.315 |
| Compact record | 59.831 / 130.087 | 55.714 / 219.228 |
| Compact grab | 62.455 / 176.895 | 55.747 / 203.510 |
| Current idle | 43.229 / 131.138 | 41.912 / 140.358 |
| Current play | 60.472 / 176.255 | 59.468 / 165.043 |
| Current record | 81.703 / 267.084 | 77.284 / 241.626 |
| Current grab | 73.771 / 197.411 | 73.691 / 271.856 |

Retain as throughput leader: Compact p50 improves7–14%, Current0–5%. Some tails
worsen substantially, so this is not a stronger realtime bound. All normal-host
state/media/capture oracles pass; source/binary/launcher hashes recorded. Next
check native playback at1and8 active voices/track, then build a runnable static
Pi5 comparison bundle with original/retained/packed under the same A76 flags.


Native density check, three rotated playback comparisons per case, Q32/buffer32:

| Mode / active voices per track | Retained p50/p99us | Packed p50/p99us |
| --- | ---: | ---: |
| Current /1 | 48.701 /189.999 | 48.917 /141.609 |
| Compact /1 | 42.502 /140.787 | 37.511 /100.759 |
| Current /8 | 82.662 /204.021 | 77.689 /315.185 |
| Compact /8 | 80.569 /210.079 | 69.477 /239.313 |

Compact throughput gain survives light/dense playback (~12%/~14%). Current is
flat at1voice, ~6% better at8; tails mixed. Every active voice on all8tracks is
Playing, nonempty and capture-complete. The dense launcher exposed --voices but
had an exact-four guard; replace it with the actual eight-cell bound. Its loops
and validations were already parameterized. Initial rejected1voice invocation
never timed audio and is excluded. Both hosts use the same newly hashed launcher.

Now build original/retained/packed static ARM64-musl offline executables with
identical Cortex-A76 flags and matching harness. Validate ELF architecture and no
interpreter; include source hashes/patches, runner and raw-result instructions.
This prepares a real-Pi comparison; it does not supply hardware measurements.


## Pi5 runnable comparison bundle

work/pi5-audio-bench.tar.gz contains static AArch64-musl original/retained/packed
executables (Cortex-A76), source patches/hashes, build/lock hashes and a single-core
runner. ELF machine183/class64 verified, no interpreter. OriginalSHA0c91c3ff...,
retainedc806f1f3..., packed52fcf3dd.... No ARM execution/Pi timing has occurred.

The normal example pulls an unrelated native tracing test dependency requiring
C++ cross tooling. A standalone23-package harness reuses the exact benchmark,
support and heap instrumentation with a small root alias; all registry versions
match the project lock. Against the original example on this host, all48cases
(4modes*3scenarios*2buffers*2history conditions) match sample checksums and pass
capture assertions; timings from this equivalence check are discarded. Included
runner records affinity, CPU/model/governor/frequency/temperature, preserves raw
CSV, rotates variants and verifies hashes/checksums. Source restored to packed.

## Experiment19: ThinLTO without source changes

Fresh default-release and ThinLTO builds use the same packed source, standalone
harness, lock and generic host CPU flags. Only release LTO differs. Default ELF
3,154,864bytes, ThinLTO3,567,000bytes (+13%). Source-before/after hashes match;
whole-callback four-backend screen pending. No profile setting is changed in the
repository. This explores build-level gains after metadata/layout work.


Four rotated ThinLTO comparisons passed every sample/capture assertion. Current
p50 improves4–7% over default release across idle/play/record32/256. Compact
improves0–4%; older chunked modes roughly1–3%. Current play32 44.416->41.932us,
record32 60.349->56.328us; Compact play32 40.049->38.842us. Tails mixed. Retain
as a build contender, not a repository-default change or Pi measurement. Next
compare codegen_units1 with LTOoff and ThinLTO using unchanged source/harness.


## Experiment20: codegen-unit partitioning

Four identical-source release settings, four-backend4-rotation screen, all checks
pass. CGU1 alone cuts ELF to2,755,728bytes and Compact p50 roughly10–14% below
default; ThinLTO+CGU1 is2,893,208bytes and Compact11–16% below default. Compact
play32 default39.824us, thin39.447, one34.600, thin-one33.800; record32 41.481,
40.791,36.969,36.809. Current prefers different settings: thin16 often best for
idle/play, thin1 forrecord; CGU1 alone is flat or worse. Keep distinct contenders,
no global Cargo profile change. Tail timings remain mixed.

Pi bundle now adds packed-thin-one (same packed source, release thin/CGU1), ELF
SHA97b18898..., AArch64/static, no hardware timing. Other three retain original
identical default release settings. Bundle manifests make the profile difference
explicit so source and compiler effects can be compared separately.

## Experiment21: borrow contiguous unmixed graph inputs

Compact routing normally clears each active node's input descriptor/mixed arrays
and copies forwarded wires into them. Compile a direct-input range only when every
input port has exactly one contiguous graph-wire route, with no host import or
input/output overlap. Borrow the immutable input wire slice plus disjoint mutable
output slice. Mixed/host routes retain old routing; sample-processing fallback and
publish hooks receive the same inputs. Logical forwarding and node/edge counters
are unchanged. This removes temporary descriptor traffic without changing PCM,
meters, gain arithmetic, graph ownership or public Processor APIs.

97 runtime tests pass; engine suite still compiling. Measure before retaining.
A targeted graph case should cover reversed slot ordering and materialized fallback
if the callback screen justifies the extra implementation.


The first direct-input version passed97runtime+104engine tests. Six rotations
show Compactplay32 39.969->37.265us (-6.8%), play256 335.079->306.551us (-8.5%),
but idle/record medians~1–2% worse. Current control shifts~1%. The optional input
base enlarged every operation. Refine it to a bool, recover the base from the
first existing route, and narrow cached operation indexes/counts tou32 under
prepared graph bounds. No extra input-base word is needed.

Narrow version passed97runtime tests and a new exact-sample/no-heap regression
covering both slot-ID orders, mixed input, reference forwarding, sample fallback
and publish_references input/output consistency. All test attributes comply.
Fresh three-way callback comparison pending: retained packed, initial direct,
compact-record direct. Source/harness hashes and flags verified. No promotion yet.


Three-way six-rotation repeat keeps a playback-only benefit: compact-record
direct vs packed play32 39.784->37.416us (~6%), play256 318.579->303.377us (~5%).
Idle is ~0.5–1% worse and record ~0.5–1.3% worse. Larger optional-base form is
slightly faster on playback but worse idle/record. Final targeted policy excludes
outputless capture nodes from direct borrowing, preserving their original route
setup. If this remains mixed, archive it rather than promote the extra path.


Final output-node-only direct policy still trades workloads: Compactplay32
39.483->37.105us (~6%), play256 328.840->298.520us (~9%), but idle~0.7% and
record~0.6–2% worse; record256 p99 especially noisy/worse. Archive all three forms
as playback contenders and restore the simpler packed default. The routing
regression is archived with them. No extra routing path/operation flag/index
narrowing is retained in source. Move to validating build-level gains in the
normal host instead of further tuning this mixed-return path.


Raw measurement storage was consolidated:2,430 original CSV.gz files are inside
results/single-core-2026-11-09/raw-measurements.tar.gz. Every member was verified
byte-for-byte with SHA256 before loose files were removed. The accompanying
index preserves original names, hashes and sizes; JSON summaries/build manifests
remain directly readable. No measured data was discarded.


## Normal-host validation of ThinLTO/CGU1

Fullhost built source-exact with profileoverrides only, SHA839c7791...,12,706,368
bytes. All20 current/compact workflows pass. Both hosts use the SAME default-built
launcher to hold client code fixed, allprocessesCPU0. Four rotated native runs:

| Mode /scenario | Packed default p50/p99us | Packed thin/one p50/p99us |
| --- | ---: | ---: |
| Compact idle |28.527 /100.053|25.510 /88.555|
| Compact play |49.578 /118.650|45.166 /132.044|
| Compact record |54.963 /186.185|50.049 /121.834|
| Compact grab |52.948 /224.866|46.498 /145.288|
| Current idle |42.167 /146.300|38.953 /126.071|
| Current play |59.528 /256.425|56.168 /159.390|
| Current record |76.776 /224.240|69.766 /206.719|
| Current grab |74.793 /251.465|70.732 /213.524|

This validates the profile in the normalapp: Compact medians9–12% lower and
Current5–9% lower. Compactplay tail slightly worse; no worst-case timing claim.
The standalone mode-specific ranking does not fully predict live scheduling.
Keep profile as an explicit supported-workload build choice; repositoryCargo
profiles remain unchanged. No realPi measurements have been obtained.

## Experiment22: optimization level2 with ThinLTO/CGU1

Keep packed source and the strongerprofile, compare opt-level2 againstdefault3.
This tests whether less unrolling/inlining helps the smallquantum instruction
working set. Sameharness/lock/CPU flags; build pending before shortcallback screen.


Four rotated opt2 comparisons pass sample/capture assertions but Current p50
regresses3–6% in everycase. Compact playback improves~1.5%, idle regresses~1%,
record flat to~1%better. Keep opt3; opt2 does not justify the Current penalty.
Profile changes remain environment-only; archive smaller opt2 ELF/metadata.

## Experiment23: fuse unity fader copy with integer-bit peak

Current's sample LoopFader copies unity-presence audio, then scans that output
again for its peak. For nondelayed metered unity ports, copy each sample and fold
its absolute bit-pattern maximum in one pass. Preserve raw sample bits, original
integer peak semantics and meter publication. Delays, non-unity/ramped presence,
unmetered ports and Compact reference processing keep their existing paths.
No buffer, API, allocation or arithmetic approximation is introduced. Run node
and audio-variant oracles, then compare wholecallbacks before retaining.


Fused copy/peak passes selected oracles but six rotations are not consistent:
Currentplay32 44.701->44.105us (~1%better), play256355.148->374.291us (~5%worse),
othercasesmostlyflat. Compact control varies~1–2%. Revert extra branch; preserve
fused-fader-copy-rejected.patch and rawdata. Compiler memcpy/vectorization is
already competitive; no retained kernel change from this experiment.


## ARM execution checks and stereo-load direction

Downloaded DebianTrixie qemu-user10.0.13 to ignoredwork only, verified packageSHA256
against officialPackages.xz. No system install. Allfour staticPi bundle variants
pass32 ARM sample/capture cases under cortex-a76 emulation; checksums match across
variants. Emulator timings are discarded and never ranked asPi performance.

An explicit NEON load attempt did not compile because rt_runtime forbids unsafe
code. Restore it immediately; do not weaken that invariant. Try a safe eight-sample
chunk formulation selecting four stereo samples, with scalar tail bounded by the
actual source length. This may expose structured loads to LLVM without intrinsics.
Other strides/gains and ramps retain their paths. ARM compile/codegen pending.


## Experiment 24: safe grouped stereo reference reads (rejected)

The eight-sample chunk formulation passes 3,186 edge cases on native x86 and
Cortex-A76 QEMU, including both channels, short tails, nonfinite values and gains.
QEMU is used only for correctness. The ARM build passes warnings-denying compile;
no unsafe code or invariant change is retained. The first edge oracle incorrectly
ignored NaNs in the integer peak; corrected it to match the existing bit maximum.

Six rotated, CPU0-only native screens use identical harness/compiler/flags and
fresh source-hashed binaries. At Q32, stereo read improves 3–5%, but peak regresses
58% (unity 38.93->61.74ns; 0.7 gain 40.00->63.23ns). Q128 stereo peak is also
14–27% worse. Reject and restore the packed baseline. Preserve patch, source hashes,
ARM assembly, raw measurements and the standalone correctness harness. These are
not Pi performance measurements. Next direction: profile-guided optimization of
the existing ThinLTO/CGU1 contender with workload-balanced training.


## Experiment 25: balanced profile-guided optimization

Build the exact retained packed source with ThinLTO/CGU1 and compiler profile
instrumentation. Train equally across Current, Chunks, ChunksDescriptors and
Compact, each idle/play/record at host buffers32/256, eight tracks/four active
voices, 48k frames of history before capture. Instrumented timings are discarded.
The profile optimizes the same compiler/source/harness, then an uninstrumented
binary is compared against the immutable ThinLTO/CGU1 control with short rotated
runs. Training includes setup and telemetry; this limitation must be checked
against held-out density before promoting a profile. No source/profile defaults
or Pi ranking changes based on this experiment alone.


Balanced native PGO lowers all24 measured workload medians: Compact8–16%,
Current9–11%, older chunk modes7–12%, relative to retained ThinLTO/CGU1. Four-rotation
held-out1/4-track screens also improve; at buffer32 Compact6–16%, Current5–10%.
All sample/capture assertions match. Tails remain mixed, including some recording
regressions; do not claim better worst-case behavior. Retain as an offline build
contender, with normal-host validation and actualPi timing still outstanding.

The profile merge explicitly excludes six default profiles produced by instrumented
build tools; only the24 named training invocations are merged. The initial count
assertion caught these extra files before any profile-use build. ARM PGO will use
separately generated target execution counts under QEMU, never x86 profile reuse
or emulator timing. Preserve target flags, profile hashes and source identity.


ARM PGO training completed all24 cases under Cortex-A76 QEMU. The separately
trained ARM profile produces static ELF16736f1c...; 24 additional functional
comparisons across all4 modes,3 scenarios,both buffers,one untrained track match
the previous ARM ThinLTO/CGU1 binary exactly in printed checksum and materialization
counts. All emulator timing fields are discarded. The Pi bundle now includes a
fifth candidate, packed-pgo, with target profile, hashes and training/check evidence.
No target performance result or default change is inferred. Normal-host PGO is now
being trained separately through its actual executable/client workflows.


## Normal-host PGO validation

All20 workflows pass. Four rotated dense comparisons use the SAME default-built
launchers and CPU0 for both hosts; 256-callback windows at host buffer32/Q32.

| Mode / scenario | ThinLTO/CGU1 p50 / p99 us | PGO p50 / p99 us | p50 change |
| --- | ---: | ---: | ---: |
| compact idle | 26.169 / 92.617 | 21.357 / 75.947 | -18.4% |
| compact play | 43.229 / 108.595 | 38.249 / 118.084 | -11.5% |
| compact record | 47.923 / 118.019 | 45.517 / 156.941 | -5.0% |
| compact grab | 45.317 / 180.626 | 41.083 / 128.168 | -9.3% |
| current idle | 39.218 / 129.962 | 35.417 / 123.157 | -9.7% |
| current play | 53.799 / 139.060 | 49.741 / 114.469 | -7.5% |
| current record | 67.499 / 201.486 | 63.751 / 231.376 | -5.6% |
| current grab | 68.734 / 174.782 | 62.615 / 156.881 | -8.9% |

Normal-host PGO confirms a throughput gain: Compact5–18% and Current6–10% beyond
ThinLTO/CGU1. Compactplay/record and Currentrecord p99 regress in this short screen;
no worst-case claim. ELF11,322,264bytes vs12,706,368 (~11% smaller). Retain this as
the strongest tested native build choice; repository compiler defaults remain
unchanged. Source is exactly the packed baseline. The36 application training
profiles cover20 workflows plus16 dense cases across all4 modes, excluding build
tool profiles. The environment reconnect interrupted the first profile-use build;
logs were preserved and the hash-checked saved profile resumed successfully.
Actual Pi performance remains unmeasured; its packaged PGO profile is independently
trained on ARM execution counts and is not the native host profile.


## Experiment 26: reuse steady eligibility for prerouting queries

prerouting_transition currently scans request slots outside compact_preview even
when compact_ready already proves there are no live requests. All request creation
and transactional preview paths invalidate that existing cache. Reuse its proof
between previews, keeping topology/ID validation first and the full scan whenever
eligibility is invalidated. No new field, allocation or preparation cost.

The expanded transactional-vs-compact oracle first passes on the old code, then
the candidate: compare prerouting queries before preview and after finish across
1,024 ticks, immediate/delayed requests, wrap, seek, recording and sync relations.
All224 loop tests and152 engine tests pass; test attribute policy and Cortex-A76
warnings-denying compile pass. Fresh source-hashed binary c46eec20... now enters
the callback screen against the retained packed/default control. ARM codegen is
recorded; no target timing is inferred from assembly.


Six rotated screens reject the cache-query change: Q32 Currentidle/play/record
p50 changes+0.6/-0.3/+0.3%, Compact-0.8/+1.0/+1.4%. Q256 is similarly mixed,
mostly within1–2%. No repeatable steady-callback benefit. Revert production change
and experiment-only oracle additions; preserve both patches/assembly/376passing
test evidence/rawdata. Aggregate execution counts include setup and must not be
mistaken for callback cost. Move to arithmetic specialization guaranteed by
Quantum's existing private, nonzero-power-of-two constructor invariant.


## Experiment 27: exact power-of-two quantum alignment masks

Quantum's private u16 can only be constructed as1/16/32/64/128 (default32). Replace
aligned/ceil remainders with low-bit masks, retaining checked addition and overflow
errors. In compact_tick combine content-length, position and captured-frame
alignment into one OR/mask predicate instead of three remainders. No API, storage,
allocation, quantum domain or floating-point arithmetic changes.

All291 runtime,224 loop and152 engine tests pass (667 total), including alignment
and clock-overflow oracles. The immutable retained ARM packed/default binary has
three UDIV instructions in compact_tick. Record fresh source-hashed native/ARM
candidates and target codegen before short rotated callback comparisons. Target
instruction counts and emulator checks do not establish Pi performance.


Cortex-A76 warnings-denying builds pass and all24 ARM functional comparisons
match. compact_tick UDIV instructions drop3->0 in immutable static ARM binaries.
Six native rotations nevertheless show no steady callback gain: Current medians
mostly within1%, Compactplay/idle flat and recordQ32~1.9%worse; older chunks record
Q256~2.7%worse. Restore the packed source. Preserve the mathematically exact patch,
ARM ELF56a8234a...,source hashes,667-test evidence,24 ARM checks and codegen as an
unranked Pi contender. Do not add it to the measured native best list or claim a
Pi speedup from fewer instructions. Proceed to checked dense topology lookup.


## Experiment 28: checked dense-slot topology lookup

PreparedTopology::node uses binary search. First check the common dense layout
at nodes[id.slot()] and return only when its entire ID matches. Preserve the
binary-search fallback for sparse slots, generation ordering and invalid IDs.
No index map, allocation or storage change. All224 loop and152 engine tests pass.
Fresh default-profile callback comparison follows Cortex-A76 compile and an
independent diagnostic counter-subtraction pass on the old instrumented binary.

For the diagnostic, keep setup/configuration fixed and compare4vs512 callbacks;
subtract per-function counter arrays to cancel shared initialization. Instrumented
timings are discarded. Differences also include per-callback controller work and
state-dependent post-run validation, so they identify repeated processing rather
than directly measuring function time. Do not use the subtracted counters as PGO.


Dense lookup repeats an offline recording gain1–3%, but normal-host ThinLTO/CGU1
is mixed. All20 workflows pass. Currentplay54.227->58.085us (~7%worse), Currentidle
38.204->38.965us, record66.784->66.806us (flat); Compactplay43.630->43.006us,
record48.504->48.491us (flat), grab47.490->44.298us (~7%better). Revert globally;
preserve as a Compactgrab contender with376-test evidence/native rawdata.

Counter subtraction succeeds across12 short instrumented invocations; shared
ResourceStore initialization counters cancel exactly. Repeated processing still
includes reference reads, allocation/zeroing, peak and accumulation. Counters also
show compact clock loops executing; absence of an x86 mask gain does not prove
that loop is cold. These are block-count deltas, not function timings. Preserve
all raw profiles and do not promote them into a synthesized PGO profile.


## Experiment 29: complete-span read overwrite

read_into(false) clears output and then adds each source sample. A complete
nonsilent single span writes every output element. Give that case a visitor
consumer assigning0.0+sample, preserving signed-zero/nonfinite arithmetic while
avoiding the separate clear and old-output load. Source validation occurs before
any single-span write; on error clear output before returning the original error.
Keep the original consumer for split, silent, short/mismatched and additive reads.
Keep logical work-counter behavior unchanged; claim speed only from timing.

An independent no-heap oracle first passes on the old implementation. It covers
nonfinite and signed-zero gains, ramps, additive negative zero, silence, short and
split spans, out-of-range sources and stale epochs, including output on errors.
The candidate passes the full runtime suite; loop/engine suites and ARM checks
follow before short micro/callback comparisons. No unsafe code, allocation or API
change. This is an unpromoted source experiment.


All668 tests,3,186 native+3,186 ARM persistent edge cases,24 ARM callback comparisons
and A76-Dwarnings pass. The runner's first pilot hit a missing output folder;
preserve that log and rerun cleanly after fixing it. Six valid micro rotations
show Q32 constant read improvements4–14%, ramps1–3%; other operations mostly flat.
Whole callbacks are modest: Compact0.5–2.5%better, older chunks mostly0–2%better,
Current~0.7–1.2%worse. Revert the larger path and its experiment-only oracle; preserve
as a read-throughput contender with all builds/rawdata. Logical counters were
unchanged, so this is a timing result, not a manufactured counter reduction.


## Experiment 30: inline reference publication dispatch

EngineNode's publication hook handles StripInput and returns immediately for other
variants. Force inlining so generic graph callers can optimize the unused
ReferenceBlock setup in those cases. No new preparation cache, operation fields,
capability API or functional branch is added. The baseline binaries retain a
publication symbol, although symbol presence alone does not prove where hot
calls remain; benchmark the hypothesis rather than infer a timing win from it.


Publication inlining passes152 engine tests,24 ARM comparisons and A76-Dwarnings.
Six rotated callback screens are mostly within1%; playback has mixed outliers
(CurrentQ32~-2.8%, descriptorsQ256~+2.3%). No broad convincing benefit; revert the
attribute and preserve evidence. Move to actual type-layout measurements using
stable compiler LLVM debug metadata, without modifying production types or using
unstable compiler flags. Runtime IDs already use NonZeroU32 generations, so their
Option niches are already exploited; do not pursue an already-present change.


## Experiment31: separate fixed cold arrays from hot node objects

Stable A76 compiler LLVM debug metadata reports EngineNode1616bytes, LoopNode536,
StripInput1312, HostedProcessor1616, and Slot<EngineNode>2208. Large fixed arrays
in the injected-MIDI and hosted-parameter owners inflate every enum slot even
when audio-only rendering does not use those facilities. Move just those arrays
to fixed Box<[T;N]> fields allocated/initialized in existing off-render constructors;
reset fills the existing injection box in place. Keep injection capacity16 and
parameter capacity64 and all processing/order/validation semantics unchanged.
Charge the parameter box plus allocation-rounding margin to processor reservation.
No callback allocation, growing container, unsafe code or new capability.

Engine/MIDI/processor tests and measured reservation checks must pass before any
speed claim. Recompute target type sizes from stable compiler metadata and compare
whole callbacks. The goal is less hot-object stride, not a claim that total process
RAM falls by the same percentage as a type.


Boxed arrays pass152 engine+28 host-processor tests (including reservation), and
both variants pass24 ARM output comparisons. A76 slot size2208->1624bytes (-26%);
its enum tag moves0->1027. Adding repr(u8) puts the tag at0 with slot1632bytes.
Six explicit within-mode forward/reverse permutations match checksums and logical
materialization counts in all4 backends. Timing remains mixed: Currentrecord1–1.5%
better, CompactplayQ256~1.8–2.6%better, other Compact mostly flat, older chunk modes
can regress1–3.6%. Restore the baseline; preserve both as unranked Pi locality
contenders. A smaller reserved type is not itself evidence of faster callbacks.

Method refinement: the new three-way driver reverses candidate order inside each
backend, giving all six permutations. Earlier multi-backend offline drivers rotated
the entire case list, which did not balance every within-mode pair. Retained major
gains also have separately paired normal-host validation; tiny offline differences
remain tentative. Continue with narrower private operation/route indices that the
hot graph loop reads, under Config's existing i32::MAX prepared limits.


## Experiment32: compact hot graph metadata

Keep the original node objects and move to metadata read on every graph quantum:
Route wire/port fields usize->u32 and Operation first/end/output/output-count
usize->u32. Config::prepare explicitly rejects positive limits above i32::MAX,
and compile refuses wire totals above the prepared arena. Destination ports are
already u16. Narrowing is lossless for accepted plans; widen locally to usize
before all existing slice, allocation and count arithmetic. Node IDs and global
frame/epoch/counter widths remain unchanged. No new branch, allocation or API.

All291 runtime and152 engine tests pass. The expected 64-bit sizes are Route16->8
and Operation40->24; confirm through target metadata. Fresh callback/control hashes,
ARM functional checks and within-backend balanced order precede a timing decision.


Packed program indices pass291 runtime+152 engine tests,24 ARM comparisons and
A76-Dwarnings. Six within-mode-balanced rotations show Compact mostly within1%,
while older chunk cases regress~1.7–3.7%. Revert; keep patch/data for inspection.
The changed generic code can affect code generation outside the selected branch;
do not infer causality from type-size arithmetic. Next obtain actual user-space
CPU-clock samples. A disabled self-only event probe succeeds with kernel/hypervisor
excluded. Install perf into ignored work from SHA-verified official Debian packages;
no system install or performance-policy change.

## Experiment33: borrow the Compact route source

A workspace-local, SHA-verified Debian perf installation can record this process
with user-only, per-thread, non-inheriting CPU-clock events. A diagnostic-only
PGO harness uses FIFO enable/disable markers after warmup and before telemetry
flush. All processes stay on CPU0. Its minimal environment avoids collecting
inherited application secrets in user-stack records. The 72 short runs collected
3,336 samples without losses; sampled timings are discarded. Flat attribution has
incomplete stacks and 13–22% unknown symbols, so these are hypothesis screens.
See `results/single-core-2026-11-09/steady-profile-evidence.json` and its archive.

Recurring Compact addresses are in routing immediately after Program::source:
the helper returns Result<AudioSequence>, then routing copies the96-byte value
before checking silence or forwarding it. Change the private helper to accept a
mutable wire slice and separate host base, returning a borrowed descriptor. This
also permits borrowing operations/routes instead of copying them before each
call. Forwarding and accumulation still copy when their existing APIs need a
value; arithmetic, edge order, lazy host materialization and work counters remain
unchanged. The existing unit suites pass97 runtime+59engine tests, one ignored.
Fresh offline binary SHA2569fc3710ce4da29e87077bb5de229630add70c3f66ffa963a3fa18fb4955e463b.
Integration oracles and ARM checks passed before balanced callback comparisons.

Six balanced offline rotations pass with identical checksums and logical counters.
Compact p50 improves5.3–12.5% at32frames and6.9–12.2% at256frames; other backends
are broadly flat (mostly within1.5%). Tails are mixed. All24 Cortex-A76 emulated
functional cases match, and the A76 runtime build passes-Dwarnings. Native host
PGO validation is pending. The first host builder accidentally populated an empty
CARGO_ENCODED_RUSTFLAGS, overriding RUSTFLAGS; Cargo's fingerprint confirms no
PGO. That unmeasured binary/manifest is marked invalid as a PGO comparison and
preserved. Rebuilding with the encoded variable absent; use-v2 immutable binary.

The corrected old-profile host binary isf15447ee630960ce8ad3696c416a6221004943280eeee2377e684f413e36bde6,
11,278,536bytes. Cargo fingerprint verifies profile-use. It passes20workflows;
four native paired rotations show mixed Compact changes (-3%record,+4.6%play)
and Current1.3–6.3% regressions. The build explicitly warns that source and
run_ticks CFG hashes changed, discarding profile counts (up to226,510,879 in
run_ticks). Therefore do not retain based on the default-build lead: independently
retrain this candidate with the same36 native training workloads before judging
PGO performance. All training timings remain excluded. Production source remains
only this private routing experiment beyond the retained packed baseline.


Fresh candidate PGO completes36training workloads with no hash-mismatch warning.
20validation workflows pass. Four native paired rotations yield Current p50
8.0–10.2% worse, Compact1.9–5.2% worse, executable12,169,352vs11,322,264bytes.
This does not support retention even after retraining. Revert executor_compact.rs;
all crate Rust hashes match the packed-descriptors baseline. Preserve the patch
asborrowed-route-contender.patch and results/manifests/merged profile in
borrowed-route-evidence.{json,tar.gz}. No subsequent optimization was started.


Publication validation: formatting, test policy and the warning-denying workspace
build pass. The full native run was not completed. The first run passed2,223 then
stalled in a child-readiness fixture underRUST_TEST_THREADS=1; the identical test
binary passed2.78s with the repository default8. A corrected serial CPU0 run was
stopped at the user's explicit request after592passes, one SIGINT,2601unrun and
11skipped. No assertion, fixture or runtime source was weakened. Publish the
available results with this incomplete-gate limitation.
