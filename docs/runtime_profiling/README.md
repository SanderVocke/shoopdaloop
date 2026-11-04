# Runtime profiling measurements

These measurements describe one Linux VM. They are evidence about this
implementation, not a guarantee of device deadlines on other machines.
See the [feature guide](../runtime_profiling.md) for controls and timing semantics.

## Results

At Q=32, complete-callback medians use a 5,333.333 µs budget (256 frames at 48 kHz):

| Nodes | Baseline µs | Off µs | On µs | Off Δ budget pp | On Δ budget pp | Collector % audio budget, on | Encoded MB/s, on |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 12 | 53.656 | 53.961 | 62.609 | +0.006 | +0.168 | 0.192 | 1.03 |
| 102 | 530.999 | 528.946 | 567.508 | -0.038 | +0.685 | 0.743 | 4.45 |
| 502 | 2,978.919 | 2,893.266 | 3,008.664 | -1.606 | +0.558 | 3.261 | 17.30 |

Across steady cases, off medians range from -4.74% to +6.78% relative
to baseline, and on from +0.99% to +17.27%. VM variation makes these
observed ranges, not universal overhead estimates. Full tails and deadline misses
are retained in the [comparison table](table.md).

A follow-up pairs baseline/off at 502 nodes, Q=128: nine adjacent pairs,
alternating which binary runs first, 512 callbacks per run and one pinned virtual
CPU, with other heavy work stopped.
The median of run medians is 978.751 µs baseline versus 956.317 µs off:
**-2.29% relative DSP time, or -0.421 callback-budget percentage points**.
The negative delta illustrates timing variation; it does not establish that the
instrumented binary is faster or that off has a universal zero cost. The original
matrix is retained alongside [the paired raw data](paired-disabled.jsonl).

Ordinary draining loses no live batches across the measured workloads. Steady
queue high-water is 257 pages at Q=1, nine at Q=32, and three at Q=128.
Prepared sample-array storage is 3.30 / 7.41 / 12.17 MB for the three pool sizes.
Every steady on run transfers a latest automatic capture to the collector.
Capture receipt completion does not promise an untruncated history window.
Withholding the collector drops bounded batches once capacity fills; audio
continues and loss counters identify missing data. Large pools also fill during
the stalled Q=64 case; Q=128 fits this 128-callback stall.

Q=1 at 102 or 502 nodes already misses every deadline in the baseline.
Enabled collection uses about 29% or 112% of rendered audio time on its separate
thread, and encoded output reaches 210 or 725 MB/s. These workloads demonstrate
scaling limits, not usable real-time configurations. Near-budget large-graph
workloads can develop additional misses while recording. Use the complete DSP
bar and the tail/miss table when assessing a target graph.

The native primed clock costs a median 26.42 ns/read, with a 20 ns observed
minimum positive step. The Node proxy costs a median 54.03 ns for a direct JS
clock call, 9.48 ns for a constant Wasm call and 61.51 ns for the Wasm clock import.
Actual AudioWorklet clock timing remains unavailable on the tested Chrome build.
The final profiling window uses median 0.092 / 0.519 / 2.559 ms per CPU frame for
12 / 102 / 502 rows; P99 is 0.191 / 0.780 / 3.608 ms. GPU work is excluded.

## Method

The uninstrumented baseline is `df4683527dbde0ec7483c3e522fc3873e39a81c8`
on `feat/fixed-quantum-runtime`. Both binaries use the same release benchmark
workload in `shoop_rt_engine/examples/profiling_benchmark.rs`. The baseline copy
only removes profiling setup and message collection; graph construction,
commands, buffers and independent callback timing are identical.

Each graph contains 2, 20 or 100 stereo/MIDI strips with one loop per strip,
routes to a stereo bus and a media reader: 12, 102 or 502 executable nodes.
Setup uses a single production session replacement, then 32 warm-up callbacks.
Steady workloads cover Q=1/16/32/64/128 at 48 kHz with 256-frame callbacks.
Additional MIDI, recording/transition, graph-edit and irregular-partition
workloads cover (12 nodes, Q=1), (102, Q=32), and (502, Q=128).
Partitions repeat 1/7/127/256/33/511 frames.

For each workload, compare the baseline, profiling off, and profiling on. Steady
workloads also withhold collection until the end. Every run injects a host xrun
at its midpoint. All 288 runs (three repetitions, 128 measured callbacks each)
are shuffled with seed 4321. Builds, tests and the interactive application are
stopped during measurements.

`Instant` around the complete `OfflineHost::process` call supplies independent
callback durations, including render adaptation and fault/recovery checks. The
profiler's own timestamps are never used to measure its overhead. These are
production engine callbacks through the offline adapter; audio driver/device
latency and worklet MessagePort serialization are outside these native timings.
Control requests and endpoint draining/collection are timed separately. Reports
are requested every 32 callbacks; the collector applies its normal 200 ms cache.
UI frame timings separately exercise the actual profiling window, without GPU
submission or the rest of the application.

The median is the median of each repetition's callback median. P99 and maximum
pool all measured callbacks. A miss means the complete callback exceeded its
actual `frames / sample_rate` period. Delta budget percentage points are
`100 * (median - baseline_median) / callback_period`; relative DSP slowdown is
`100 * (median / baseline_median - 1)`. Irregular-partition summaries normalize
the median delta by the median callback period; raw records retain every actual
period and deadline result. Collector CPU is divided by rendered audio duration,
not by callback processing time. Wire rate counts encoded engine record sizes,
including live and captured records, before carrier-specific framing.
Storage counts sample arrays, excluding owner/channel/header metadata.
The native render measurements precede the separate browser MessagePort credit
window; that browser-only backpressure does not run in `OfflineHost::process`.

Negative deltas and extreme tails can arise from VM scheduling, cache/code layout
and frequency variation. Off is a control-flow contract (no profiling clock reads
or recording), not a promise of identical machine code or a zero measurable delta.
Already overloaded baselines cannot establish safe target-device operation.

## Artifacts

- [Complete callback comparison](table.md): steady-workload timings and overhead.
- [All workload summaries](summary.json): includes MIDI, recording, rebuilds,
  partitions, collector CPU, queue high-water, losses, storage and captures.
- [Raw callback runs](callbacks.jsonl.gz): every callback duration and period,
  workload, repetition, node calls, transfer/collection totals and counters.
- [Native clock calls](native-clock.json): nine million primed epoch reads.
- [Wasm clock-import proxy](wasm-import-clock.json): Node `performance.now()`, a
  constant Wasm call, and Wasm-to-JavaScript clock import, nine million calls each.
  This is a bridge microbenchmark, not an actual AudioWorklet result.
- [Profiling-window CPU frames](ui-frames.jsonl): 256 frames at each node count,
  after eight warm-up frames, with production egui layout and text formatting.
- [Environment and source identities](environment.json).
- [Chromium verification](chrome-validation.json): 11 packages, 610 passing tests.
- [Node verification](node-validation.json): 34 packages, 2,211 passing tests,
  combining the complete suite with affected-package reruns after fixes.

Chrome for Testing 147.0.7727.117 has no `performance` object in the actual
AudioWorklet realm. Its elapsed-time profiling capability is unavailable, so an
honest on/off duration comparison in that realm is unavailable too. The feature
reports this limitation. The production worklet artifact contract uses an injected
monotonic clock to verify the import, timestamps, toggles, capture and stable
Wasm memory independently.

## Reproduce

Build current examples with:

```sh
cargo build --release -p shoop_rt_engine --example profiling_benchmark --example profiling_clock_benchmark
cargo build --release -p shoop_egui --example profiling_table_benchmark
```

Create a worktree at the baseline and copy the benchmark source into the same
example path there. Remove `use rt_runtime::profiling::{Clock, Collector};` and
replace everything from `type Profile = Collector;` through the line before
`fn drain` with:

```rust
type Profile = ();
fn profile_setup(_host: &mut OfflineHost, _mode: &str) -> Profile {}
fn profile_message(_profile: &mut Profile, _message: &EngineMessage) -> usize { 0 }
fn profile_result(_profile: &mut Profile) -> (u64, bool, u64, u64, u64) {
    (0, false, 0, 0, 0)
}
```

Build that example with a separate target directory, then run from the current
checkout with other heavy work stopped:

```sh
python3 scripts/benchmark_profiling.py --baseline /absolute/path/to/baseline/profiling_benchmark
target/release/examples/profiling_clock_benchmark > docs/runtime_profiling/native-clock.json
node scripts/benchmark_profiling_clock.mjs > docs/runtime_profiling/wasm-import-clock.json
target/release/examples/profiling_table_benchmark > docs/runtime_profiling/ui-frames.jsonl
```
