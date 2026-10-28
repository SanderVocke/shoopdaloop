# Carla performance investigation

These headless experiments distinguish release-build recipes, hosting interface,
worker scheduling, and historical engine behavior. They require Linux, GCC/G++,
Rust, and Carla 2.5.10 source. They do not measure hardware xruns.

## Prepare

A single preparation command automates the steps below (network access and the
listed build tools are prerequisites):

```sh
python3 experiments/carla/bootstrap.py --work "$CARLA_WORK"
```

This environment additionally uses `--apt-config /workspace/.setup/apt.conf`.
`--reuse-runtimes` reuses previously verified builds; `--current-target PATH`
selects an existing Cargo cache. The default keeps build outputs in the scratch
area. Historical adapter patches are included and applied only to isolated
checkouts. Build/download failures stop preparation and retain logs.

Use an isolated checkout at baseline `580f6191` and source the environment's Rust
activation script. Choose an absolute scratch directory, referred to below as
`$CARLA_WORK`. No benchmark changes the normal application's processing path.

1. Download `https://github.com/falkTX/Carla/archive/refs/tags/v2.5.10.tar.gz`.
   Its SHA256 must be
   `ae2835b12081f7271a6b0b25d34b87d36b022c40370028ca4a10f90fcedfa661`.
2. Extract identical source trees as `Carla-reference` and `Carla-packaged` under
   `$CARLA_WORK`. Provide liblo development files and PyQt5 5.15.11's `pyuic5` and
   `pyrcc5` in PATH. PyQt tools enable registration of Carla Rack in its LV2
   bundle even for headless use. When extracting Debian packages locally, set
   `PKG_CONFIG_PATH`, `PKG_CONFIG_SYSROOT_DIR`, and `LD_LIBRARY_PATH` accordingly.
   Historical Rust LV2 hosting also needs lilv/serd/sord/sratom/zix development
   files. The recorded run uses Debian trixie packages; consult the manifest.
3. Build the reference with `make -j2 plugin` and packaged recipe with
   `make -j2 plugin frontend`. Build with the same compiler and dependency
   settings; start clean after changing detected features. Frontend freezing is
   irrelevant to headless DSP and is excluded. These reproduce build targets,
   not every optional dependency of the Ubuntu packaging runner.
4. Build the Rust adapter:

   ```sh
   cargo build --locked --release -p shoop_engine --features carla --example carla_regression_benchmark
   cp target/release/examples/carla_regression_benchmark "$CARLA_WORK/"
   python3 experiments/carla/run.py prepare --work "$CARLA_WORK"
   ```

   The prepare command builds the fixture, timing shim and native/LV2 harness,
   writes state files, and records artifact checksums and environment metadata.

## Run

Stop compilation before timing. Use a fresh output directory for a new experiment;
the runner resumes already completed case/repetition pairs. Smoke runs are separate
from evidence runs:

```sh
python3 experiments/carla/run.py run --work "$CARLA_WORK" --output "$CARLA_WORK/smoke" --suite all --repeats 1 --blocks 10 --paced-blocks 10
python3 experiments/carla/run.py run --work "$CARLA_WORK" --output "$CARLA_WORK/results" --suite all
python3 experiments/carla/run.py run --work "$CARLA_WORK" --output "$CARLA_WORK/controls" --suite controls
python3 experiments/carla/run.py run --work "$CARLA_WORK" --output "$CARLA_WORK/history" --suite history
python3 experiments/carla/verify.py "$CARLA_WORK"
python3 experiments/carla/analyze.py --work "$CARLA_WORK" --output "$CARLA_WORK/output"
python3 experiments/carla/run.py summarize --work "$CARLA_WORK" --output "$CARLA_WORK/results"
```

Defaults are ten alternating repetitions, 200 warm-up blocks, 2,000 measured
continuous blocks and 200 measured paced blocks per repetition, at 48 kHz and
1024/256/128 frames. Paced runs are scheduling diagnostics, with fewer blocks
than the primary continuous comparison; their p99 estimates are less stable.
Pacing uses absolute deadlines and resets after falling more than a period behind.
Each run starts a fresh process. Do not combine failed smoke runs with evidence.

Workloads are an empty rack, Carla's open-source stereo gain at 0.5, and the same
LADSPA binary containing a stable 192-step recurrence followed by gain 0.5. The
latter is single-threaded and stateless across blocks. Output is verified against
the same recurrence with a floating-point tolerance. A mono LADSPA plugin is forced
stereo by Carla Rack, so both channels must pass validation.

The timing shim intercepts the actual native ABI process callback, calls the
unchanged real Carla library, and writes up to 16,384 samples to a preallocated
shared file. No logging or file I/O occurs inside process. Four clock reads per
call have measurable overhead; compare shim/no-shim runs, especially empty racks.
Its mapping is per process and the harness instantiates one chain per process.
The LV2 hook is available for historical adapters. Capacity exhaustion is flagged
in summaries. Inner samples are matched to caller intervals; late/stale work is
not silently assigned to another block. Caller CPU time is not worker CPU time.

Per-case JSON records include command, exit status, elapsed time and cgroup
throttling counters. CSV holds per-block observations, and binary captures hold
inner timings/frame counts. Summary files preserve failures, invalid output,
misses and incomplete matching. Bridge request-to-inner-start timing includes
copies and dispatch as well as wake-up; it is an upper bound on wake-up delay,
not an isolated scheduler measurement. An inner `process` sample also includes
Carla bookkeeping around its narrower DSP-meter scope.

## Historical versions and interpretation

The pre/post native-switch revisions are adjacent: `9b169b5` and `0cec63c`.
`v0.2.0` resolves to `d08aafcb049f6c7a848c97a18b5c3a1456046696`.
Use isolated checkouts and minimal benchmark adapters; preserve the real processing
implementations. Record adapter patches and build errors. The 0.2.0 Linux release
contains `libshoopdaloop_backend.so` but no Carla runtime. Its user's Carla version
cannot be inferred from that release. A packaged old-engine measurement must be
identified as an as-shipped engine measurement, not a locally rebuilt binary.

Compare independent run summaries, not individual blocks as independent trials.
Require reproducibility beyond baseline-versus-baseline variation before declaring
a regression. Report effect sizes and paired confidence intervals. Separate
continuous throughput, paced completion latency, inner CPU cost, and inner wall
time. VM descheduling and throttling can affect wall time and tail latency.
A missing historical/runtime cell is inconclusive, not evidence of no regression.

## Contention and historical controls

Use `--suite contention --pin-cpu CPU` in one output directory, then the same
command with `--load` in another. Choose a CPU in the allowed affinity mask. The
load is a bounded-lifetime child sharing the selected CPU and is terminated in a
`finally` block. `--pin-cpu` affects the runner and its children, not other tasks.
Use `pin-control` and `contention` directory names for automatic analysis.

The controls suite compares native calls with/without timing interposition and
an independently repeated identical configuration. The history suite compares
both Rust revisions and the as-shipped 0.2.0 backend. For the old backend, 128-frame
runs use 2,000 blocks per repetition; 1024-frame runs are paced diagnostics using
200 blocks per repetition. Its driver attachment is asynchronous: the adapter
waits before creating FX and independently verifies the nominal period captured
by the LV2 shim. The old dummy driver always paces internally, so no old whole
callback latency is compared against new continuous throughput.

For a supplemental old no-FX routing/profiling check, run `legacy020.py` with
`none` instead of a state path and the same environment used by a history case.
The output CSV intentionally has no per-block timings; the `.profile.json` file
contains aggregate backend profiling. Do not treat that as a primary paired CPU
comparison. Legacy backend profiling units must be checked against its source.

The analyzer uses geometric means of paired per-run ratios and a deterministic
bootstrap 95% interval, requiring at least eight valid independent pairs. The CPU
screening threshold is the larger of 5% and three times the median absolute A/A
ratio deviation; at least 80% of pairs must agree in direction. Latency screening
requires a 20% effect. A flag is a reason to investigate, not proof of causality.
The analyzer preserves runs with invalid output and misses in exported records;
CPU comparisons exclude those runs rather than interpreting fallback as faster DSP.
