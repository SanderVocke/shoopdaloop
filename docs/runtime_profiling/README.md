# Runtime profiling measurements

These measurements describe one Linux VM. They are evidence about this
implementation, not a guarantee of device deadlines on other machines.
See the [feature guide](../runtime_profiling.md) for controls and timing semantics.

## Results

At Q=32, these representative complete-callback medians used a 5,333.333 µs
budget (256 frames at 48 kHz):

| Nodes | Baseline µs | Off µs | On µs | Off Δ budget pp | On Δ budget pp | Collector % audio budget, on | Encoded MB/s, on |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 12 | 53.630 | 54.622 | 64.192 | +0.019 | +0.198 | 0.205 | 1.03 |
| 102 | 563.773 | 583.158 | 655.126 | +0.363 | +1.713 | 1.002 | 4.45 |
| 502 | 3,203.523 | 3,239.616 | 3,201.319 | +0.677 | −0.041 | 3.450 | 17.79 |

The 502-node numbers have substantial run-to-run variation; the negative on
delta is not evidence that recording accelerates audio. Across steady cases,
off medians range from −7.05% to +18.14% relative to baseline, while on ranges
from −0.07% to +30.71%. Consult the raw repetitions and tails, rather than treating
those endpoints as general overhead estimates. The 12-node runs are more stable:
off adds 0.28–3.53 µs at Q=16–128, and on adds 2.95–18.31 µs.

A follow-up checks the largest apparent off regression (502 nodes, Q=128) with
nine adjacent baseline/off pairs, alternating which binary runs first, 512
callbacks per run and one pinned virtual CPU. All other heavy work is stopped.
The median of run medians is 1,137.220 µs baseline versus 1,140.174 µs off:
**+0.26% relative DSP time, or +0.055 callback-budget percentage points**. This
supports run-to-run VM variation as the explanation for the unpaired +18.14%
outlier. It does not establish a universal off-cost bound. The original matrix
is retained unchanged, alongside [the paired raw data](paired-disabled.jsonl).

With ordinary draining, all measured workloads have zero dropped live batches.
Steady queue high-water is 257 pages at Q=1, nine at Q=32, and three at Q=128.
Prepared sample-array storage is 3.30 / 7.41 / 12.17 MB for the three pool sizes.
Every steady on run retains a completed automatic capture. Withholding the
collector drops bounded batches once capacity fills; audio continues and the
loss counters identify the missing data. Large pools also fill during the
stalled Q=64 case because their prepared stream has fewer pages; Q=128 fits this
128-callback stall.

Q=1 at 102 or 502 nodes already misses every deadline in the baseline. Enabled
collection would require about 32% or 113% of rendered audio time on its separate
thread, and encoded output reaches 210 or 725 MB/s. These workloads demonstrate
scaling limits, not usable real-time configurations. Near-budget Q=16/502-node
workloads also develop additional misses while recording. Use the complete DSP
bar and the [tail/miss table](table.md) when assessing a target graph.

The native primed clock costs a median 28.65 ns/read, with a 20 ns observed
minimum positive step. The Node proxy costs a median 62.53 ns for a direct JS
clock call, 8.61 ns for a constant Wasm call and 72.29 ns for the Wasm clock import.
Actual AudioWorklet clock timing remains unavailable on the tested Chrome build.
The final profiling window uses median 0.093 / 0.573 / 2.924 ms per CPU frame for
12 / 102 / 502 rows; P99 is 0.342 / 0.905 / 4.034 ms. GPU work is excluded.

## Method

The uninstrumented baseline is `3ca192e10552afe9ca9dfe6f4482c8b5081602bc`
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
- [Node verification](node-validation.json): 34 packages, 2,207 passing tests,
  combining the complete suite with the application/UI rerun after fixes.

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
