# Pi 4: four audio-only renderers

The ARM64 bundle contains `shoop_host`, the normal app benchmark client, runtime
libraries, timing/profiling scripts and checksums. One release binary exposes
`current`, `chunks`, `chunks-descriptors` and `compact`. **All four benchmark modes
omit internal MIDI histories, indexes, state buffers and processing**, as well as
device MIDI. No Rust installation, Git checkout, audio device or GUI is needed.

Requires 64-bit Linux ARM64 with glibc 2.36+ (Pi OS Bookworm/Debian 12+), Python 3
and `taskset`. The Cortex-A72 build disables AES/SHA extensions absent on Pi 4.
`BUILD_INFO.json` records exact source/build/library hashes. Check the extracted ZIP:

```sh
sha256sum -c SHA256SUMS
chmod +x bin/shoop_host bin/chunk_benchmark benchmark.sh profile.sh
getconf GNU_LIBC_VERSION
```

Default sizing: four prepared track slots/sixteen loop slots. Non-idle cases
create the sync track/one loop and one stereo benchmark track/eight loops.
Playback/ramps use four active audio loops, each recorded for about 0.2–0.25 seconds;
four are empty/stopped. Recording uses one recorder; record-play uses one player
and three recorders. Shared/continue uses four grabs plus the running sync clock.
Idle has only the empty sync loop. This is not a large multi-track or long-loop
working-set stress test. `--voices` changes active loops on the benchmark track;
`--tracks` changes prepared capacity without adding more benchmark tracks.

## Short tests first

Examples use host/controller CPU 0, render CPU 1 and memory worker CPU 2. Adjust
these to the Pi's allowed/isolated cores. Keep kernel, governor, cooling, IRQ and
scheduling settings identical across variants. The commands below use ordinary
scheduling. Add `--rt-priority 80` to select only the render thread as SCHED_FIFO
(root/CAP_SYS_NICE required), if that matches your setup. Record
`cat /proc/sys/kernel/sched_rt_runtime_us`: earlier Pi profiling attributed ~50 ms
stalls to the 950 ms/second RT bandwidth limit. These scripts do not change it.

1. Smoke: playback and grabs in all four modes, positive stereo readback, no drops
   or render faults. Each mode is a fresh real host with a normal socket app client.

```sh
./benchmark.sh --output results/smoke --host-cpu 0 --render-cpu 1 \
  --scenarios idle:silence play grab --warmup 0.5 --callbacks 64 --repeats 1
```

2. Main comparison: four repetitions balance all four mode-order positions. Each
   selected window is only **2.048 seconds**, with a 0.5-second warmup. This is 48
   runs and about 123 seconds of explicit warmup/audio, plus setup/readback time.
   Expect minutes, not hours; stop after this first if you only want early numbers.

```sh
./benchmark.sh --output results/timing-256 --host-cpu 0 --render-cpu 1 \
  --scenarios idle:silence play ramps --warmup 0.5 --callbacks 384 --repeats 4
```

3. Short memory-pressure comparison: continuous 64 MiB copies on CPU 2. The worker
   is stopped automatically. This adds eight playback runs.

```sh
./benchmark.sh --output results/memory-256 --host-cpu 0 --render-cpu 1 \
  --load-cpu 2 --interference memory --scenarios play \
  --warmup 0.5 --callbacks 384 --repeats 2
```

4. Optional small-buffer comparison: same 2.048-second selected duration, Q=32,
   but a 32-frame host callback. This adds 48 runs; do not combine with a 31-second
   warmup for every case unless you deliberately want a much longer experiment.

```sh
./benchmark.sh --output results/timing-32 --host-cpu 0 --render-cpu 1 \
  --buffer 32 --scenarios idle:silence play ramps \
  --warmup 0.5 --callbacks 3072 --repeats 4
```

5. Optional workflows: ordinary recording, simultaneous record/play, single and
   overlapping history grabs, continued recording and track removal/recreation.
   Keep these windows short to fit the deliberately small capture budget.

```sh
./benchmark.sh --output results/looping --host-cpu 0 --render-cpu 1 \
  --scenarios record:silence record record-play grab shared continue churn \
  --warmup 0.5 --callbacks 384 --repeats 1
```

6. Optional full-history check: one shared-grab run per mode warms the default
   30-second history. This adds about two minutes of warmup, rather than filling
   histories before every run in the main matrix.

```sh
./benchmark.sh --output results/full-history --host-cpu 0 --render-cpu 1 \
  --scenarios shared --warmup 31 --callbacks 384 --repeats 1
```

`--modes current compact` selects just those modes for a quick follow-up. Default
is all four. Never compare new audio-only timings directly with the earlier
bundle's device-only-MIDI-disabled measurements as if only compact changed.

Inspect `summary.csv`: p50/p95/p99, maximum, deadline overruns, delivery lateness
and drops. At 48 kHz budgets are 5.333 ms (256 frames) and 0.667 ms (32 frames).
Keep outliers. Ordinary scheduling, RT throttling and other load can affect elapsed
callback time; an observed maximum is not a WCET bound. Copy counters describe
logical work, not DDR traffic. Also return kernel/isolation/governor/frequency,
RT runtime settings and any other load. `machine.json` and `scheduling.json` retain
actual placement, priority, source and executable identities.

## Normal frontend use

`shoop_host` is the regular network-capable host, not a benchmark-only substitute.
Use the supplied configuration with FX/synths disabled for compact:

```sh
SHOOP_AUDIO_RENDER=compact SHOOP_AUDIO_BENCH_AUDIO_ONLY=1 \
  ./bin/shoop_host --config audio-only-host.json --export-profile profile.json
# On a compatible app frontend (protocol version 12):
shoopdaloop --ui-only --host-profile profile.json
```

For a host on another machine use its LAN IP for both endpoints, adding `--lan
--endpoint PI_IP:15400 --management-endpoint PI_IP:15401`, then connect the frontend
with `shoopdaloop --ui-only --host-address PI_IP:15401`. No audio streaming is added.
Normal looping, audio tracks, meters and waveform/readback use the usual app stack.
Compact refuses MIDI, FX/synths, composites and advanced dry/wet editing; the other
modes retain normal capabilities outside the benchmark profile. Stop the benchmark
before attaching another controlling frontend. Omit benchmark-input/output variables
for normal device input. The frontend executable is not included in this ARM64 ZIP.

## Optional profiling after timing

Use kernel-compatible `perf` and `c++filt`. Sampling is separate from timing:

```sh
./profile.sh --output results/profiles --host-cpu 0 --render-cpu 1 \
  --collector-cpu 3 --settle 0.5 --duration 5
python3 scripts/analyze_audio_chunk_profiles.py --input results/profiles \
  --output results/profile-summary --perf "$(command -v perf)"
```

Executables retain Rust symbols and unwind tables. Emulated smoke timings are
never Pi performance evidence.

## Return results

```sh
python3 scripts/collect_audio_chunk_results.py results --output pi4-fourth-results.zip
```

Upload the ZIP and system settings in a comment on
https://github.com/SanderVocke/shoopdaloop/pull/960 . It includes raw callback CSVs,
accepted setup/final state, stereo media checks, summaries and scheduling. The
exporter excludes connection profiles/credentials and raw perf stack memory.
The local four-way interpretation is in `docs/FOURTH_RESULTS.md`; architecture
and limitations are in `docs/PROTOTYPE.md`.
