# Production numeric policies

`GraphRuntime<P, L, Q, S>`, its processor/planner contexts, buffers and resource
store use `S: Sample = f32`. `LoopPlanner<S>` and the actual playback/capture
nodes use the same policy. `Q824` stores raw signed i32 values and supplies the
specified saturating arithmetic. There is one runtime and loop implementation;
the default application remains float. Application-only FX and plugin processors
remain float; the hardware comparison profile does not instantiate them.

Audio byte APIs encode little-endian samples of the store's policy. Float stores
retain IEEE-754 encoding; fixed stores retain raw Q8.24 bits, including low bits
that f32 cannot represent. Device/persistence adapters must explicitly convert
float data. Typed loop/processor controls preserve fixed values; the existing
float parameter action and window-control metadata remain conversion boundaries.

Production gain controls retain start, target, duration and elapsed frames.
Sample zero uses start, offset duration uses target, and shorter ramps hold their
target for the rest of a block. Gain, scale and balance coefficients are evaluated
per sample in their declared order. Restart smoothing follows the same time
convention. Reference render modes materialize changing controls when their
endpoint descriptor cannot express the curve. Their comparison now checks every
sample, including the middle of each quantum.

The production capture/playback test runs both policies through the real graph,
planner, capture groups and media codecs, checks raw fixed readback, and checks a
justified float error bound. Runtime routing tests use saturating operands to
verify canonical compiled connection order. Shared arithmetic tests cover extreme
and interrupted ramps; production control tests check partition invariance and
float agreement. Full-session error statistics and drift checks remain a required
part of the common runner, not a replacement for exact Q8.24/RTL comparisons.

`production-numeric.json` records paired release measurements against the saved
baseline: Q=32, 128-frame callbacks, four stereo voices, three alternating runs
per phase on one shared-host CPU. The ratios of median callback times are 1.012
playback, 0.985 record/play, 0.938 recording and 1.010 clear. All measured callbacks
have zero allocations, reallocations and deallocations. These measurements show
no material default-float regression; they do not establish a hardware deadline.

Final measurements after retirement are in `cpu-final.json`. In three alternating
paired runs the default-float median ratios are 1.003 playback, 0.955 record/play,
0.941 recording and 1.008 clear. No callback allocated, reallocated or freed.
The separate 64-loop production-runtime workload measures 56.63 us/quantum for
float and 89.17 us for Q8.24 (median of run medians, Q=32), a fixed/float ratio
of 1.575. It uses real production looping and shared DSP, not the app's float-only
FX/node glue, and excludes setup/observation/JSON. Host contention raises absolute
times relative to earlier runs, so the paired ratios are the appropriate float
regression comparison. These are CPU timings, separate from the skipped RTL
64-loop repetition.

Build/run the replacement benchmarks with:

```sh
cargo build --release -p rt_backend_testbench --example cpu -p shoop_rt_engine --example cpu_benchmark
python scripts/benchmark_production_cpu.py --baseline work/unify-baseline/float-benchmark --cpu 0
```

Omit `--baseline` on a clean checkout; the numeric benchmark does not need the
saved binary. `cpu_benchmark` remains a useful standalone app CPU/heap workload.
