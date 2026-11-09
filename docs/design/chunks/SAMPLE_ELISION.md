# Compact sample-elision experiment

`benchmark-no-sample-processing` is a default-off Cargo feature of `shoop_host`
(and forwarded to `shoop_rt_engine`/`rt_runtime`). It is a diagnostic build of
compact, with deliberately meaningless audio. Preparation refuses other renderers
and MIDI-enabled configurations. Normal builds retain sample processing.

The feature skips sample arithmetic, mixing, PCM copies/clearing, gain-vector
interpolation, peak scans, capture interleaving and history payload writes in the
basic compact path. The dummy adapter's synthetic input generation is also
skipped, since it is inside its elapsed callback measurement. Prepared buffers
are still initialized and touched before rendering.

Graph walks, reference shapes, validity checks, chunk lookup, allocation, retention,
flat-table updates, timing/ramp advancement, capture/publication and retirement
continue. Disabled kernels consume opaque argument objects, preserving argument
setup instead of replacing whole processors with an immediate return. Synthetic
peak/last-sample results are zero. The signal-independent basic looping graph
continues to use the same references and resource lifecycle. The feature is not an
output-correctness mode or a completed two-thread implementation.

The CSV header and run/summary JSON explicitly report `sample_processing=0/false`.
The launcher checks the host's identity against its own build and retains state,
media shape and completion validation; its positive-signal assertion is disabled
only in the diagnostic build. Logical sample/copy counters still describe requested
work. They are not actual memory traffic when kernels are elided.

## Build and run

Build both variants from the same source, toolchain and flags. The existing dummy
host and normal socket app client are used. These commands build dummy-only hosts:

```sh
RUSTFLAGS='-D warnings' CARGO_TARGET_DIR=target/samples-on \
  cargo build --locked --release -p shoop_host --no-default-features \
  --features testing --bin shoop_host --example chunk_benchmark
RUSTFLAGS='-D warnings' CARGO_TARGET_DIR=target/samples-off \
  cargo build --locked --release -p shoop_host --no-default-features \
  --features testing,benchmark-no-sample-processing \
  --bin shoop_host --example chunk_benchmark

python3 scripts/benchmark_sample_elision.py \
  --normal-host target/samples-on/release/shoop_host \
  --normal-launcher target/samples-on/release/examples/chunk_benchmark \
  --elided-host target/samples-off/release/shoop_host \
  --elided-launcher target/samples-off/release/examples/chunk_benchmark \
  --output work/sample-elision --host-cpu 0 --render-cpu 1
```

Choose CPUs from the machine's allowed set. Defaults select buffers 256 and 32,
Q=32, four voices, 0.5-second warmup, 1.024 seconds of selected audio and four
repetitions. Order alternates on/off and off/on. The default matrix is 64 runs,
including silence playback to contextualize synthetic-input cost. It took about
five minutes locally, excluding builds; this is not a Pi duration estimate.
`--buffers 256 --repeats 2` gives a smaller exploratory matrix. RT priority is an
explicit optional argument. Existing scheduling/RT-runtime limits are recorded
per run; keep them consistent across the comparison.

For source-only feature checks, run the `rt_runtime` test filtered to
`audio_samples::tests` and the engine's `sample_elision` integration test with the
feature enabled. Ordinary sample-output suites belong to the default build;
numerical audio parity is intentionally absent in the feature build.

## Local results, 2026-11-09

Binary production source is `3bebc9d2`; subsequent Rust changes only improve the
feature test. Both release builds deny warnings and use the same toolchain and
non-default host feature set. ARM/Pi measurements have not been performed.

The placement is controller CPU 0/render CPU 1, ordinary scheduling, allowed CPUs
0–4 and a four-CPU cgroup quota. There was no concurrent build/test or load worker
during timing. Cgroup throttling counters did not increase. Session size is the
same small workload as the earlier comparison: Sync plus one stereo benchmark
track, eight created benchmark loops, four playing voices for playback/ramps.
Prepared capacity is four tracks/sixteen loops. Histories are not filled by the
short warmup.

The matrix contains **64 runs / 55,296 selected callbacks**. Entries are medians
of four run medians, in microseconds:

| Buffer | Case | Samples on | Samples off | Residual |
| --- | --- | ---: | ---: | ---: |
| 256 | Idle/silence | 34.64 | 27.87 | 80.4% |
| 256 | Playback/signal | 117.75 | 86.44 | 73.4% |
| 256 | Playback/silence | 116.45 | 79.93 | 68.6% |
| 256 | Ramps/signal | 125.22 | 106.25 | 84.8% |
| 32 | Idle/silence | 5.97 | 5.18 | 86.7% |
| 32 | Playback/signal | 14.91 | 12.36 | 82.9% |
| 32 | Playback/silence | 15.36 | 11.44 | 74.5% |
| 32 | Ramps/signal | 17.27 | 13.81 | 79.9% |

Every run passes identity, frame continuity, expected modes, media-shape and
completion checks, with zero dropped records/render faults. All runs report the
same prepared memory layout. Playback at buffer 256 has the same median logical
work in both builds: 272 node executions, 7,424 sample reads and about 4.3 KB of
requested resource copies per callback. At buffer 32 the corresponding work is
also essentially identical, with small variation at loop boundaries.

Ten additional sample-free workflow runs / 640 callbacks cover idle, recording
with silence/signal, playback, record-play, grab, shared retention, continued
capture, churn and ramps. Capture/grab/control progression passes; their audio is
intentionally synthetic zero data.

There are **31 elapsed-budget overruns**, retained in the evidence. One normal
silence-playback run includes a 121 ms outlier. Some sample-free p99 values are
worse despite lower medians. Elapsed measurements include scheduling delays;
these short ordinary-scheduler container runs establish no WCET improvement.
[Full percentile/max/overrun tables](results/sample-elision-local-2026-11-09/summary.json).

### Interpretation

Most elapsed cost remains after removing sample work. With today's non-sample
path unchanged, the residual suggests optimistic playback limits of about
1.36x speedup at buffer 256 and 1.21x at buffer 32, before queueing, synchronization
and pipeline startup/completion. This is an exploratory model, not a hard bound
on an implementation that also reorganizes metadata work. A quantum-batched
pipeline has eight quanta per 256-frame callback, but only one at buffer 32.

The subtraction includes changed cache pressure and compiler/code-layout effects;
it is not a directly measured independent executor stage. In particular, the
sample-free residual already benefits from removing same-core sample traffic.
Do not add a second cache-isolation gain to this estimate. A real second core
would also introduce queue/coherence traffic and continued shared-cache/DDR use.
The residual includes kernel argument/validation overhead deliberately retained
by the experiment; a command representation could reorganize that work.

The consistent conclusion is that basic sample offload alone offers limited
room. Improving repeated reference validation, routing, resource management and
state publication remains relevant to a larger architectural gain.

### Non-invasive CPU samples

Separate 15-second playback captures use user `cpu-clock` samples at 997 Hz on
the unchanged binaries, collector CPU 2. There are 360 normal / 300 sample-free
samples, zero lost. Profiled callbacks are excluded from the timing matrix.
Approximate CPU shares by the closest recognized symbol owner are:

| Symbol-owner group | Samples on | Samples off |
| --- | ---: | ---: |
| Runtime/executor dispatch, including inlined graph work | 26.1% | 34.3% |
| Audio storage/indexing | 18.1% | 17.0% |
| Reference processing | 15.8% | 11.0% |
| Graph nodes | 15.3% | 13.0% |
| State publication | 11.4% | 9.3% |
| Loop planning/state | 4.7% | 8.0% |
| Capture coordination | 0.8% | 1.7% |
| Host wrapper/other | 7.8% | 5.7% |

These are coarse ownership categories, not clean phases. Sample-free graph-node
and reference functions perform metadata/argument work; their names do not imply
sample processing remains enabled. About 27–28% of leaf symbols are unresolved
and assigned using recognized caller frames. Inlining and the small sample count
limit fine attribution. The samples support a distributed executor/resource/state
cost, rather than one dominant loop-settlement function.

## Evidence

[Manifest and member hashes](results/sample-elision-local-2026-11-09/manifest.json),
[machine/build identity](results/sample-elision-local-2026-11-09/machine.json),
[processed profile details](results/sample-elision-local-2026-11-09/profiles.json).
Archives preserve safe raw callbacks, states, media checks and profile commands:

- [Paired timing](results/sample-elision-local-2026-11-09/paired.zip).
- [Sample-free workflows](results/sample-elision-local-2026-11-09/workflows-off.zip).
- [Normal profile](results/sample-elision-local-2026-11-09/profile-on.zip).
- [Sample-free profile](results/sample-elision-local-2026-11-09/profile-off.zip).
- [Symbolized stacks and reports](results/sample-elision-local-2026-11-09/profile-analysis.zip).

Connection credentials and raw perf stack memory are excluded. Replay extracted
callback runs with `benchmark_audio_chunks.summarize(directory, write=False)`;
verify archive/member hashes against the manifest first.

## Validation

The warning-denying workspace build, formatting and test-attribute policy pass.
The full default native gate passes **3,194 tests**, with 11 skipped, in 434.5 s
using the documented missing-hardware allowance. Feature-enabled heap-guarded
checks verify that sample storage stays untouched while reference validation and
logical work remain, and that capture/control/retained-grab workflows progress.
`rt_runtime` and `shoop_loops` build for `thumbv7em-none-eabihf` with the feature.
All archived callback runs and member hashes replay successfully.
[Validation record](results/sample-elision-local-2026-11-09/validation.json).
