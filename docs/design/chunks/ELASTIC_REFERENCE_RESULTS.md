# Elastic reference media integration

The app-selected `fpga-reference` renderer now stores audio in the Rust reference
`MediaStore`. Recording, playback, bulk readback and PCM import/export use its
contiguous fixed-point slot arena and per-channel chunk tables. Native resource
versions and snapshot leases retain ownership; there is no native PCM mirror.

Payload storage grows beyond the configured native page pool. Engine metadata,
queues, resource/version counts and other prepared limits remain finite. The
native page pool is still prepared for other facilities. Dynamic arena and table
growth may reallocate during capture. Playback after warmup performs no heap
operations. Clear retires ownership, then the existing reclamation budget releases
one chunk per step; pinned snapshots delay reclamation until their final release.
Arena capacity remains available for subsequent recordings.

## Measurement setup

Measured on 2026-11-10 using the integration working tree based on `6ca2cd74`,
Linux x86-64, AMD EPYC 9V45, Rust release builds. Both experiments use stereo,
48 kHz, quantum 32 and callback 128 (2,666.7 microseconds available per callback).
These are shared cloud CPU measurements with ordinary scheduling, not worst-case
execution bounds or FPGA timing estimates. No optimization was added to sample
arithmetic for these results.

The offline runner calls the real engine endpoint synchronously. It times only
`host.process`, including a thread-local allocator counter. Input filling,
control polling, result collection and CSV writing are outside that interval.
Both modes disable history and smoothing. It measures 1,024 callbacks per case,
three repeats, at 1/4/8 voices, pinned to CPU 2. Renderer order alternates.
`record-play` means one recording voice and the remaining voices playing; at one
voice it is simply recording. `record` records all voices; `clear` includes the
clear requests and subsequent incremental reclamation.

The app-connected runner uses the dummy native host and normal app commands,
with controller/host threads on CPU 1 and rendering on CPU 2. Playback and
record/play cases have 384 measured callbacks, two repeats, at 1/4/8 voices
(record/play starts at four). Gain-ramp and transition-churn cases have one
eight-voice repeat. The native host's existing setup disables history/smoothing
for the reference mode; the current mode retains its normal settings. Thus the
offline experiment provides the closer comparison of the supported subset.
App-connected `record-play` uses one playing voice and the rest recording.

All app-connected runs completed with zero render faults, no dropped callback
records, complete captures, and finite stereo bulk readback.

## Results

Times below are microseconds. Each p50/p99 value is the median of the per-repeat
statistics; max is the largest observed callback across repeats.

Offline playback:

| Voices | Current p50 | Reference p50 | Current p99 | Reference p99 | Current max | Reference max |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 11.31 | 47.40 | 24.05 | 81.55 | 2,108.06 | 2,086.95 |
| 4 | 18.68 | 66.69 | 70.66 | 182.51 | 4,064.74 | 2,111.36 |
| 8 | 28.46 | 88.35 | 92.30 | 198.09 | 657.33 | 2,171.18 |

Offline eight-voice capture/reclamation:

| Case | Current p50 | Reference p50 | Current p99 | Reference p99 | Current max | Reference max |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| One recording, seven playing | 28.71 | 87.32 | 55.58 | 170.76 | 241.97 | 333.41 |
| Eight recording | 37.17 | 84.43 | 67.27 | 127.26 | 220.50 | 324.66 |
| Clear/reclaim | 21.19 | 73.57 | 36.24 | 145.40 | 184.37 | 691.88 |

For eight voices, every reference playback callback had zero allocations,
reallocations and deallocations. One-recording runs performed 28 reallocations
over 14 callbacks, including three arena growth events. Eight-recording runs
performed 180 reallocations over 25 callbacks, including six arena growth events;
the slowest arena-growth callback among those repeats took 132.26 microseconds.
Clear reclaimed all 1,488 original slots, with 16 table deallocations across eight
callbacks. The logical arena retained its slots and capacity for reuse. Current
mode had zero callback heap operations in every offline case.

App-connected eight-voice results:

| Case | Current p50 | Reference p50 | Current p99 | Reference p99 | Current max | Reference max |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Playback | 67.48 | 135.80 | 307.63 | 559.35 | 2,202.52 | 4,031.51 |
| One playing, seven recording | 68.77 | 130.91 | 250.78 | 878.04 | 1,479.32 | 1,643.82 |
| Gain ramps | 66.36 | 117.12 | 138.62 | 342.34 | 173.08 | 415.41 |
| Transition churn | 62.23 | 114.04 | 279.81 | 266.64 | 864.04 | 465.55 |

The reference's typical app callback cost was roughly twice the current mode's.
All measured p99 values stayed within the callback budget, but one app-connected
reference playback callback exceeded it. Offline runs also observed callbacks
over budget in both modes (maximum 4,073.89 microseconds). Short recordings and
observed growth events do not bound the cost of reallocating a large arena in a
long session. These results support continuing the CPU feasibility study, not
claiming hard real-time behavior for elastic capture.

## Reproduce

Run Cargo in the repository's configured build environment:

```sh
cargo build --release --locked -p shoop_rt_engine --example elastic_cpu_benchmark
python3 scripts/benchmark_elastic_cpu.py --output work/app-elastic-cpu \
  --voices 1 4 8 --callbacks 1024 --repeats 3 --cpu 2
cargo build --release --locked -p shoop_host --no-default-features \
  --features testing --bin shoop_host --example chunk_benchmark
python3 scripts/benchmark_audio_chunks.py \
  --host target/release/shoop_host --launcher target/release/examples/chunk_benchmark \
  --output work/app-elastic-host-v8 --modes current fpga-reference \
  --scenarios play record-play --callbacks 384 --repeats 2 --warmup .25 \
  --tracks 2 --loops 16 --voices 8 --buffer 128 --quantum 32 \
  --host-cpu 1 --render-cpu 2
```

Choose CPUs from the machine's allowed affinity. For the other app runs, use
voices 1 with `--scenarios play`, voices 4 with `play record-play`, or voices 8
with `--scenarios ramps churn --repeats 1`. Each output directory contains raw
callback CSVs, summaries and run metadata. The host CSV also reports elastic
slot usage, capacity, growth, samples read/captured and chunks reclaimed.

Regression coverage checks split-byte PCM imports, raw fixed-point capture bits,
pinned snapshot prefixes through growth/removal, capture cancellation,
incremental reclamation, slot/capacity reuse, recordings larger than the native
page pool, bulk readback/WAV round trips, allocation-free warmed playback, and
existing renderer comparisons across callback partitions.
