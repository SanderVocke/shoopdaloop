# Native chunk prototype

See [the measured native results](RESULTS.md), [CPU sampling follow-up](PROFILING.md)
and [raw-data manifest](results/manifest.json).

`SHOOP_AUDIO_RENDER=current|chunks|chunks-descriptors` selects a renderer at
native host preparation. Experimental modes also require
`SHOOP_AUDIO_BENCH_AUDIO_ONLY=1`; use the same audio-only profile for comparisons.
Unset selection preserves the normal renderer. There is no runtime mode switch.

```sh
cargo build --release -p shoop_host --features testing --bin shoop_host --example chunk_benchmark
SHOOP_AUDIO_RENDER=chunks SHOOP_AUDIO_BENCH_AUDIO_ONLY=1 \
  target/release/shoop_host --config host.json --export-profile profile.json
# Connect the normal native app, locally or remotely, using this host's profile.
target/release/shoopdaloop --host-profile profile.json --ui-only
```

Use the existing host configuration and dummy/JACK/CPAL settings. MIDI is excluded
from experimental acceptance; native preparation clears device MIDI ports.
Empty internal MIDI compatibility objects and their existing reservations remain.
Effects and advanced editing use existing materialized adapters; they are not the
benchmark target. Driver changes preserve the startup environment. An active
callback recorder rejects driver changes at preflight; restart that host instead.

## Storage and materialization

One persistent pool replaces its reserved byte payload with a typed `f32` slab.
Each physical chunk holds exactly one quantum of interleaved channel-group audio.
The existing binary persistent index provides snapshot/edit ownership. History
addresses wrap logically within this common pool; separate per-source history
arenas and index specializations have not been added.

The pool slab lives until engine teardown. Logical chunks and index nodes recycle
without render-time heap operations. Generic resource-page usage excludes the
byte pages detached during preparation; do not interpret those page counters as
physical chunk availability. Callback CSV contains separate live payload/index
counts. The prepared metadata reservation is significant and is reported rather
than hidden in the payload-byte comparison. Q=1 correctness uses small histories;
30-second, 48 kHz Q=1 pools can exceed the metadata admission limit and are refused
at preparation. There is no renderer fallback for that capacity refusal.

History publication and loop capture materialize at their existing observation
points. Direct aligned publication writes one new quantum. Loop capture can share
the latest matching immutable payload using a bounded bitwise comparison; it does
not rewrite older shared chunks. Partial writes use quantum-sized staging.
Grabbing retains existing data/index roots. Continued capture produces new chunks;
retained grabs never trigger a sweep that copies old payloads.

Ordinary recording reserves private chunks before processing. Capture fills those
chunks, then the existing grouped commit publishes all channels together. The
final playback export is reserved before capture, so stopping a recording needs
no payload conversion or copy. Cancelled intervals leave the old take unchanged.
Capture selects the history-enabled pool for its channel group; the existing
smaller wet-edit/import pool is excluded. Experimental ordinary recording requires
input histories to be prepared (the native default).

Connections carry at most two spans per channel over one graph quantum (including
chunk/loop-boundary splits). Unsupported windows and smoothing use bounded
materialized adapters reading the same typed storage. A prepared arena holds
transient samples and resets after host-output recovery. Transient references
carry a render epoch. Published persistent playback spans acquire bounded quantum
pins so capture, deletion or edits later in the graph cannot invalidate readers.
Graph edges borrow the resulting references without additional ownership work.

Input histories and capture are deliberate materialization boundaries. Faders,
strip output and buses bypass unity work in `chunks`; ordinary gain processing
keeps its established sample arithmetic. In `chunks-descriptors`, eligible gain
stages immediately multiply first/last-sample endpoint pairs. Mixers evaluate
contributions into a fresh unity-envelope transient chunk. Captures and effects
consume evaluated samples. Meters scan the evaluated signal at their original tap;
those scans remain real sample work.

Descriptor products are approximated by a straight line between their composed
endpoints in the original quantum domain. First/last gains match the ideal stack;
interior gains can differ. Slope evaluation happens only while consuming samples.
Neither span fragmentation nor host callback partitioning changes this domain.

## Reproducing measurements

```sh
python3 scripts/benchmark_audio_chunks.py \
  --host target/release/shoop_host \
  --launcher target/release/examples/chunk_benchmark \
  --output work/chunk-bench --callbacks 384 --repeats 3 \
  --scenarios idle:silence record:silence record play record-play shared continue churn ramps
# Explicitly select two allowed cores, retaining your own RT/IRQ/isolation setup.
python3 scripts/benchmark_audio_chunks.py \
  --host target/release/shoop_host \
  --launcher target/release/examples/chunk_benchmark \
  --output work/chunk-interference --scenarios idle:silence play ramps \
  --host-cpu 2 --load-cpu 3 --interference periodic memory
# Fill the default 30-second histories before measuring retained shared grabs.
python3 scripts/benchmark_audio_chunks.py \
  --host target/release/shoop_host \
  --launcher target/release/examples/chunk_benchmark \
  --output work/chunk-steady --scenarios shared --warmup 31 --repeats 2 \
  --loops 9 --voices 8 --host-cpu 2 --load-cpu 3
# Approach the ordinary capture byte quota with three simultaneous recorders.
python3 scripts/benchmark_audio_chunks.py \
  --host target/release/shoop_host \
  --launcher target/release/examples/chunk_benchmark \
  --output work/chunk-quota --scenarios record-play --warmup 7 --repeats 2 \
  --host-cpu 2 --load-cpu 3
```

The launcher starts a real `shoop_host`, attaches the ordinary app client, creates
tracks/loops and sends normal commands. It requires accepted operations and
nonzero media readback for signal scenarios. The shared/continue scenarios retain
history in multiple loops; ramps sends controls throughout measurement; churn
removes/recreates whole tracks and records them again. `record:silence` uses the
same setup with zero input. Setup and final snapshots are saved for inspection. The existing app service
refuses bulk media reads while a take is recording; recording scenarios finish their
takes after the timing window and then verify its audio. This does not stop capture
during measurement.

All modes use one binary, configuration, MIDI policy and recorder. Each run has
explicit warmup and exactly the selected number of consecutive measured callbacks;
CSV validation rejects faults, missing/duplicate callbacks, collection drops and
incomplete output. Runs rotate mode order. Reports retain slow callbacks, p50/95/99,
maximum, deadline overruns and separate callback-delivery lateness. Other-core load
is optional and explicit. `--host-cpu` pins all host/client threads, not exclusively
the render thread; apply finer affinity and RT policy externally when required.

The CSV callback recorder is wired to the existing dummy callback boundary.
JACK/CPAL retain their native callback integrations; profile those drivers with
their existing tools rather than expecting this dummy-specific CSV recorder.

Software counters are not DDR traffic measurements. Resource copy bytes include
required resource reads/writes. Index allocation/reclamation is counted separately.
Graph reads/writes/materializations count the reference evaluator and bridges, not
every processor's private arithmetic; zeros for `current` mean uninstrumented.
The prepared memory header excludes allocator overhead and common engine metadata.
It also excludes metadata for the existing private wet-edit pool. Linux run JSON
records the host's virtual/resident memory and resident high-water mark separately.
Use PMU/cache/DDR counters separately when hardware permissions allow them.

For the precompiled ARM64 bundle and run commands, see [the Pi benchmark guide](PI_BENCHMARKS.md).

On Pi 4, build the same checkout with the same release features and run these
commands with your existing PREEMPT_RT and core-isolation configuration. Save the
kernel/configuration, affinities, CPU frequency/governor, IRQ placement, build SHA,
binary hashes, all raw CSV/JSON and script command with the result. Compare observed
maxima and overrun counts as well as percentiles. An observed maximum is not a WCET
proof. Pi target measurements require Pi hardware; native results cannot establish
the Pi's DDR sensitivity or deadline margin.

## Correctness gates

The parameterized engine workflow/structure/content tests exercise all modes.
`audio_variants` compares complete ordinary-chunk output and descriptor endpoints
against the existing renderer at Q=1/32 and callback partitions 1/13/128, and checks
exact captured/readback media across continued history wrap. New runtime tests
exercise split-span composition/mixing, stale transient references, arena refusal,
overlapping grabs, owner pressure, continued recording and an in-flight reader
surviving resource/snapshot deletion under heap guards.

The full native suite remains the repository gate. Native device smoke evidence,
empirical results and any unavailable external checks are recorded alongside the
final measurement report.
