# Fixed-quantum redesign: implementation and measurements

The fixed core now executes a prepared graph at configurable Q=16/32/64/128
(default 32), with direct primitive-loop playback and whole-tick recording.
It is not selected by the production application. Sync/program parity and live
graph/media publication are still required before engine and application cutover.

Measured source: `4d8d608be184541c98af2f3254e1cb80fd4c6b6e`. The initial sweep measured
`2443a6aab76931a4b516514f1d4609ca8d06af8e` before the quiescent-shutdown addition. Both sweeps
are retained, including every missed deadline.

## Method

Each measurement times the entire **128-frame callback at 48 kHz**: the available
budget is **2666.7 µs**. Q=32 calls the fixed graph four times; Q=16 calls
it eight times. Host-scoped maintenance/ingress budgets are not renewed per tick.
The graph has three nodes and eight audio edges per stereo loop, immutable
1024-frame waveform media, and cheap half-gain FX. Heap guards cover rendering.
Every output sample, state position, receipt, tick count and graph visit count is
checked outside timing against an independent scalar model. Preparation, control
submission, verification, receipt draining, cleanup and file I/O are outside timing.

Five randomized-order trials each measure 8,192 callbacks after 1,024 warmups,
pinned to CPU 2 in a shared Linux VM: **40,960 callbacks per configuration** and
**983,040 per sweep**. Release builds use Rust 1.99.1 with warnings denied.
Steady playback has no transitions. Two-seeks changes one loop at offset 32 and
one at offset 96. Two-bursts changes every loop at both offsets (128 commands
for 64 loops). All positions and boundaries align to Q; the graph never splits
a tick. Audio-only playback is covered here; recording/MIDI/graph-edit/plugin
performance and real device callback scheduling are not covered.

## Comparison with the previous experiments

Every cell is **µs / % of the full callback budget**. To preserve the earlier
reporting convention, these comparison tables use the **median of trial means**.
Previous numbers come from earlier runs and different harnesses. These are
historical comparisons, not paired measurements or controlled speedup estimates.

The original experiment retained all interval planning while capping intervals:

| Loops | Original adaptive | Original cap 32 | Original cap 16 | Fixed core 32 | Fixed core 16 |
| --- | --- | --- | --- | --- | --- |
| 1 | 2.7 / 0.10% | 5.3 / 0.20% | 9.0 / 0.34% | 2.3 / 0.09% | 2.8 / 0.11% |
| 4 | 9.6 / 0.36% | 15.8 / 0.59% | 24.8 / 0.93% | 8.9 / 0.33% | 10.9 / 0.41% |
| 16 | 37.0 / 1.39% | 59.8 / 2.24% | 90.5 / 3.39% | 36.0 / 1.35% | 43.0 / 1.61% |
| 64 | 153.4 / 5.75% | 263.4 / 9.88% | 411.2 / 15.42% | 144.2 / 5.41% | 169.5 / 6.36% |

The later stripped-down prototype and the new implementation:

| Loops | Scenario | Adaptive (later run) | Prototype 32 | Prototype 16 | Fixed core 32 | Fixed core 16 |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | steady | 2.6 / 0.10% | 2.6 / 0.10% | 3.3 / 0.12% | 2.3 / 0.09% | 2.8 / 0.11% |
| 1 | two-seeks | 4.9 / 0.18% | 2.9 / 0.11% | 3.5 / 0.13% | 2.3 / 0.09% | 2.8 / 0.10% |
| 1 | two-bursts | 4.9 / 0.18% | 2.9 / 0.11% | 3.6 / 0.13% | 2.3 / 0.09% | 2.8 / 0.10% |
| 4 | steady | 8.9 / 0.34% | 10.0 / 0.37% | 12.9 / 0.48% | 8.9 / 0.33% | 10.9 / 0.41% |
| 4 | two-seeks | 13.3 / 0.50% | 10.1 / 0.38% | 13.3 / 0.50% | 8.9 / 0.33% | 10.9 / 0.41% |
| 4 | two-bursts | 15.0 / 0.56% | 11.6 / 0.43% | 14.3 / 0.54% | 9.2 / 0.34% | 11.2 / 0.42% |
| 16 | steady | 34.3 / 1.29% | 40.2 / 1.51% | 52.4 / 1.97% | 36.0 / 1.35% | 43.0 / 1.61% |
| 16 | two-seeks | 46.6 / 1.75% | 40.7 / 1.53% | 52.7 / 1.98% | 35.8 / 1.34% | 43.6 / 1.64% |
| 16 | two-bursts | 56.4 / 2.12% | 45.9 / 1.72% | 58.3 / 2.19% | 36.7 / 1.38% | 44.3 / 1.66% |
| 64 | steady | 143.7 / 5.39% | 166.8 / 6.25% | 215.2 / 8.07% | 144.2 / 5.41% | 169.5 / 6.36% |
| 64 | two-seeks | 205.6 / 7.71% | 167.0 / 6.26% | 212.3 / 7.96% | 143.7 / 5.39% | 171.0 / 6.41% |
| 64 | two-bursts | 253.3 / 9.50% | 192.4 / 7.21% | 237.6 / 8.91% | 148.3 / 5.56% | 176.6 / 6.62% |

## Whole-callback tails and misses

The following uses **pooled arithmetic means** and pooled nearest-rank percentiles
from the final sweep. Percentages still use the complete 2,666.7 µs callback
budget. Maxima include preemption. No sample is removed or winsorized.

| Loops | Q | Scenario | Mean µs / % | p99.9 µs / % | Maximum µs / % | Misses / 40,960 |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | 32 | steady | 2.3 / 0.09% | 24.0 / 0.90% | 1268.8 / 47.58% | 0 |
| 1 | 32 | two-seeks | 2.3 / 0.09% | 27.3 / 1.03% | 74.7 / 2.80% | 0 |
| 1 | 32 | two-bursts | 2.3 / 0.09% | 25.6 / 0.96% | 215.8 / 8.09% | 0 |
| 1 | 16 | steady | 2.9 / 0.11% | 21.9 / 0.82% | 58.1 / 2.18% | 0 |
| 1 | 16 | two-seeks | 2.8 / 0.10% | 20.7 / 0.78% | 220.9 / 8.28% | 0 |
| 1 | 16 | two-bursts | 2.8 / 0.10% | 21.4 / 0.80% | 72.2 / 2.71% | 0 |
| 4 | 32 | steady | 8.9 / 0.33% | 32.2 / 1.21% | 219.7 / 8.24% | 0 |
| 4 | 32 | two-seeks | 8.9 / 0.33% | 38.8 / 1.45% | 178.7 / 6.70% | 0 |
| 4 | 32 | two-bursts | 9.2 / 0.34% | 44.6 / 1.67% | 299.2 / 11.22% | 0 |
| 4 | 16 | steady | 11.0 / 0.41% | 48.2 / 1.81% | 209.5 / 7.86% | 0 |
| 4 | 16 | two-seeks | 11.1 / 0.42% | 47.4 / 1.78% | 2079.3 / 77.97% | 0 |
| 4 | 16 | two-bursts | 11.1 / 0.42% | 57.4 / 2.15% | 2370.4 / 88.89% | 0 |
| 16 | 32 | steady | 36.1 / 1.35% | 121.8 / 4.57% | 1671.9 / 62.70% | 0 |
| 16 | 32 | two-seeks | 35.8 / 1.34% | 96.2 / 3.61% | 2001.9 / 75.07% | 0 |
| 16 | 32 | two-bursts | 36.9 / 1.38% | 96.9 / 3.63% | 933.5 / 35.01% | 0 |
| 16 | 16 | steady | 43.3 / 1.62% | 107.9 / 4.04% | 1510.3 / 56.64% | 0 |
| 16 | 16 | two-seeks | 43.6 / 1.63% | 130.5 / 4.90% | 1729.3 / 64.85% | 0 |
| 16 | 16 | two-bursts | 44.3 / 1.66% | 135.5 / 5.08% | 1985.0 / 74.44% | 0 |
| 64 | 32 | steady | 143.8 / 5.39% | 355.2 / 13.32% | 2189.3 / 82.10% | 0 |
| 64 | 32 | two-seeks | 143.7 / 5.39% | 339.1 / 12.72% | 2188.5 / 82.07% | 0 |
| 64 | 32 | two-bursts | 148.5 / 5.57% | 342.1 / 12.83% | 3206.4 / 120.24% | 2 |
| 64 | 16 | steady | 170.8 / 6.40% | 356.2 / 13.36% | 1847.5 / 69.28% | 0 |
| 64 | 16 | two-seeks | 171.5 / 6.43% | 363.2 / 13.62% | 2387.7 / 89.54% | 0 |
| 64 | 16 | two-bursts | 176.6 / 6.62% | 360.8 / 13.53% | 4060.3 / 152.26% | 1 |

The complete record of deadline misses across **both** new-core sweeps:

| Sweep | Raw file | Callback | Elapsed µs / % |
| --- | --- | --- | --- |
| Initial | trial-2-q32-n64-steady.csv | 4845 | 3174.4 / 119.04% |
| Initial | trial-3-q16-n1-two-bursts.csv | 2820 | 3015.9 / 113.10% |
| Final | trial-0-q32-n64-two-bursts.csv | 4246 | 3206.4 / 120.24% |
| Final | trial-0-q32-n64-two-bursts.csv | 4257 | 3192.9 / 119.73% |
| Final | trial-2-q16-n64-two-bursts.csv | 4434 | 4060.3 / 152.26% |

Across both sweeps: **5 misses in
1,966,080 measured callbacks**. A miss is
retained even for a small workload. This run does not establish a worst-case
execution bound or xrun-free operation. The wall-clock measurements alone cannot
attribute individual misses to scheduling versus execution; no outlier is
subtracted from the deadline assessment.

The earlier prototype sweep also observed misses: its adaptive 64-loop two-burst
case had two; several capped and simple modes had misses as recorded in
`evidence/aggregate.json`. A zero count in another finite VM run is not a safety
guarantee. The useful architectural observation is that transition-heavy mean
cost stays close to steady cost in the direct fixed core. Device-level tails,
recording/MIDI workloads, plugin block adaptation and real-time scheduling remain
acceptance work.

## Validation and remaining migration

- Warning-denying workspace build and the full native suite passed: 2,578 tests,
  four host-facility skips. Native loopback permission is required by IPC tests.
- Nine fixed-core and seven fixed-loop tests pass natively, in Node 24.19.0,
  and in headless Chromium 154.0.8037.57. Browser checks exercise portable library
  and host-adapter behavior; they do not select this core in AudioWorklet.
- The application and worklet compile separately for Wasm; the client-codec and
  AudioWorklet artifact contract passes. Runtime and engine dependency checks
  pass for Linux/Windows/macOS/Wasm. These retain the existing production path.
- Heap guards cover graph execution, command handling and capture publication.
  Shutdown tests cover unread/saturated results, admission-budget deferral and
  a fault after staging a partial capture tick, with all resource pages reclaimed.
- Formatting, test-attribute, portable-runtime boundary, tracing-inventory and
  documentation checks pass. Earlier failed attempts and the eventual corrected
  passes are retained in local `work/` logs; existing tests were not weakened.

The next semantic milestone is sync/program compilation and request-family/domain
precedence at tick boundaries. After that, implement atomic live graph/media
publication and export/projection transport, then integrate protocol, engine,
host settings, persistence/conversion and recovery. Production cutover and removal
of the interval executor remain gated on parity and end-to-end validation.

Plugin block size remains separate from Q. A future adapter must define buffering
latency, readiness/deadline policy and parameter/event offset mapping. A Q-frame
stream delay alone does not guarantee Q/sample-rate seconds of worker time when
the host invokes multiple ticks back-to-back.

## Evidence and reproduction

`evidence/initial-sweep.json` and `evidence/final-sweep.json` preserve source and
binary hashes, trial records, raw file hashes and limitations. `aggregate.json`
contains the pooled results, exact missed-callback identities and historical
comparison aggregates. Raw callbacks and all validation logs remain in the local
`work/fixed-core-benchmark*` and `work/fixed-quantum-review` directories. Raw files
were hash-checked while generating this report; no artifacts are uploaded here.

```sh
cargo build --locked --release -p shoop_loops --example fixed_quantum_benchmark
python3 scripts/benchmark_fixed_core.py --output work/new-sweep --cpu 2
```

Choose an allowed CPU for the execution environment. Construction of Q=64/128 is
covered by correctness tests; transition performance cases here intentionally use
Q=16/32 so that both changes fit at the same requested boundaries.
