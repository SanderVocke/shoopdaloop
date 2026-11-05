# Profiling graph validation

PR #939 targets `feat/fixed-quantum-runtime` after basic profiling merged in #938.
The final release comparison uses `a192b0ce77325bc8bb55b7b749e8d68b324f0a43`
as its basic-profiling baseline. Both revisions include the render optimizations
from #944. This isolates graph profiling from unrelated runtime changes.
The implementation checklist and CI/review state are in `PLAN_PROFILING_GRAPH.md`.

## Correctness and integration

- The complete native CI-profile suite passes 2,782 tests, with seven skips. Warning-denying workspace builds, formatting, test policy, tracing inventory, feature parity and dependency-boundary checks pass; the Python suite passes 134 checks.
- Ten affected packages pass Node and Chrome Wasm checks, including collector, app/service/client/protocol, graph UI, backend, host worklet, engine protocol, MessagePort and application tests. Backend/worklet/application checks are repeated after reconciling the new target's render optimizations.
- Packaged socket and shared-memory canaries verify recording/recovery, names, runtime toggles, nonzero complete callback wrapper statistics and graceful shutdown through the actual application protocol.
- The production Chrome workflow passes recording, audio/MIDI playback, details/waveforms, save/load, routing/bus lifecycle and profiling restart, with zero xruns. The release large-media workflow also passes, as configured in CI. Chrome's missing AudioWorklet elapsed clock remains explicitly unavailable. An injected-clock artifact contract separately verifies node timestamps, complete topology, callback budgets/execution counts, bounded xrun capture and toggles.
- Native visual inspection and prescribed-time fixtures verify named nodes, independent color scales, hover/pinning, zoom/fit, group collapse, callback entry/exit dotted context links, and actual callback/tick timelines. Screenshots in `images/profiling_graph*.png` are synthetic examples, not production incidents.

Independent elapsed-time fixtures include a 20 µs source, 80 µs unrelated work
and a 10 µs dependent: the dependent becomes ready after 110 µs. Anti-correlated
spikes check cumulative worst/stddev without summing node statistics. Callback
fixtures verify prefix/between/suffix intervals across multiple graph executions
and split engine parts. Unknown or lost graph boundaries never become wrapper
costs. Other tests cover atomic metadata, reused owner generations, clockless
epoch changes, original deadlines, paging, loss and stale replies after selection,
generation, attachment or connection changes. Identical serialized topologies
preserve layout and pins.

An additional local debug-build large-media attempt timed out at LargeLoaded
after 120 seconds. CI runs this case only in release mode. The basic-profiling
debug reproduction was interrupted by the environment restart, so it establishes
no attribution. The release result above is the acceptance check; no timeout was
changed.

## Release measurements

These are offline synthetic measurements on a shared Linux VM, not device
acceptance or a hard real-time guarantee. No competing builds or browser tests
run during the final measurement sequence. Each child is pinned to one available
CPU. The same release harness runs two alternating baseline/current pairs, 256
callbacks per run: 110 cases and 440 runs across two/16 strips, Q=1/16/32/64/128,
off/on/stalled collection, and steady/MIDI/recording/edit/partitioned workloads.

`profiling_graph_benchmark.json` contains per-case tails, maxima, deadline misses,
publication bytes, collection cost and prepared sample storage, plus UI/capture
samples. Compressed raw callback arrays are in
`profiling_graph_measurements.jsonl.gz`; the JSON records its hash and binary hashes.

| Mode | Cases | Median of case median changes | Baseline/current deadline misses | Median control/collection cost per callback, baseline/current |
| --- | ---: | ---: | ---: | ---: |
| Off | 50 | -3.45% | 15 / 9 | 2.109 / 2.039 µs |
| On | 50 | -2.84% | 124 / 160 | 15.855 / 31.491 µs |
| Stalled | 10 | -1.91% | 2 / 0 | 9.003 / 15.985 µs |

Q=1 heavy workloads can exceed their budgets already on the baseline. Counts are
reported without treating an offline overload as a newly introduced host xrun.
On-mode aggregation and metadata publication do additional work outside rendering.
Prepared sample storage is reported separately from topology/index/UI allocations;
raw capture is capped at 16 MiB, the index at 8,192 entries and detail pages at 256
nodes. The saturated-consumer and no-heap checks retain fixed bounds.

| Processor nodes | Initial frame / layout, ms | Steady median, ms | Steady maximum, ms |
| ---: | ---: | ---: | ---: |
| 12 | 6.712 | 0.399 | 0.699 |
| 102 | 6.790 | 1.377 | 1.847 |
| 502 | 11.124 | 4.537 | 5.597 |
| 4,094 | 1088.185 | 90.115 | 127.696 |

The 4,094-node stress case has 16,293 signal links plus callback context links.
It approaches the visualization caps and is not a 60 fps case. Small and moderate
graphs reuse cached layout; overview labels are suppressed at distant zoom while
colors and inspection remain available. Layout and inspection never run on the
audio render path.

The capture query fixture retains 8,192 trace summaries and
14,221,312 bytes of raw packets. A 128-node detail query
has median 134.132 µs, p99 298.178 µs,
and maximum 1086.354 µs. It returns a bounded page rather than
copying the raw history into snapshots.

Reproduce individual workloads with:

```sh
cargo run --release -p shoop_rt_engine --example profiling_benchmark -- 16 32 256 on partitions
cargo run --release -p shoop_egui --example profiling_graph_benchmark
cargo run --release -p shoop_egui --example profiling_graph_benchmark -- --dense
cargo run --release -p rt_profiling --example capture_benchmark
```

Results are measurements, not performance assertions. Physical audio/MIDI devices
were unavailable locally; CI acceptance is recorded separately in the plan.

Suspect/noisy configurations were repeated with ten alternating pairs. The JSON
retains each pair and its median change; the plan records interpretation and any
remaining limitation rather than discarding outliers.


The four largest fresh suspects have median paired changes of −1.83%, +5.54%,
−0.96% and −0.05% after ten pairs each. The initial +30.75% off-mode partitioned
spike does not reproduce. The +5.54% paired result spans −21.49% to +15.25%; its
separate run medians are 11.883/11.818 µs. These noisy ratios do not prove a
systematic regression or a guaranteed improvement. Initial on-mode deadline
misses of 124/160 and the increased collection cost remain visible above.
