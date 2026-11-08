# Pi 4 chunk benchmarks

This bundle contains ARM64 release builds of `shoop_host` and `chunk_benchmark`,
with all three renderers in the same binary: `current`, `chunks`, and
`chunks-descriptors`. It uses the existing dummy driver and normal app commands;
no audio device, GUI, Git checkout, or Rust installation is needed. Device MIDI
is disabled for every variant, but empty internal MIDI maintenance remains.

Requirements: **64-bit Linux, glibc 2.36 or newer** (Pi OS Bookworm/Debian 12 or
newer), Python 3, and `taskset` from util-linux. Runtime audio/C++ libraries are
included. The Cortex-A72 build disables AES/SHA extensions absent on Pi 4.
`BUILD_INFO.json` records source, toolchain, features and binary hashes.

```sh
sha256sum -c SHA256SUMS
chmod +x bin/shoop_host bin/chunk_benchmark benchmark.sh profile.sh
getconf GNU_LIBC_VERSION
```

Use your existing PREEMPT_RT, core-isolation and IRQ setup. Examples put control,
client and writer threads on CPU 0, only rendering on CPU 3, and interference on
CPU 2. Change those CPU numbers to match your setup. `--rt-priority 80` explicitly
sets **only rendering** to SCHED_FIFO and requires root or CAP_SYS_NICE. Omit it
for ordinary scheduling. The scripts do not configure the kernel, IRQs or governor.
Each run saves the observed render affinity, policy and priority in `scheduling.json`.

## What to test

1. **Smoke check:** all variants must complete playback and grabbing, with valid
   stereo readback and no dropped records or render faults.

```sh
./benchmark.sh --output results/smoke --host-cpu 0 --render-cpu 3 \
  --scenarios idle:silence play grab --callbacks 64 --repeats 1
```

2. **Steady callback timing:** compare an empty session, four playing voices,
   and active gain ramps. Histories warm for 31 seconds; each measurement is ten
   seconds. This takes about twenty minutes. Run as root for the FIFO option.

```sh
./benchmark.sh --output results/timing-256 --host-cpu 0 --render-cpu 3 \
  --rt-priority 80 --scenarios idle:silence play ramps \
  --warmup 31 --callbacks 1875 --repeats 3
```

3. **Shared-memory pressure:** repeat with periodic activity and continuous
   64 MiB memory copies on another core. Each worker is stopped automatically.

```sh
./benchmark.sh --output results/interference-256 --host-cpu 0 --render-cpu 3 \
  --rt-priority 80 --load-cpu 2 --interference periodic memory \
  --scenarios idle:silence play ramps --warmup 31 --callbacks 1875 --repeats 3
```

The defaults are 48 kHz, stereo, quantum 32 and buffer 256. Also repeat steps 2–3
at your original buffer size. For **32-frame buffers**, add `--buffer 32` and use
`--callbacks 15000` for the same ten-second window. Use a fresh output directory.
For recording/continued grabs/churn, use the ordinary short window to stay within
the intentionally small capture budget:

```sh
./benchmark.sh --output results/looping --host-cpu 0 --render-cpu 3 \
  --rt-priority 80 --scenarios record:silence record record-play shared continue churn \
  --warmup 0.5 --callbacks 384 --repeats 3
```

Inspect each matrix's `summary.csv`: p50/p95/p99, maximum, deadline overruns,
callback-delivery lateness and drops. The 256-frame deadline is 5.333 ms; at 32
frames it is 0.667 ms. Focus on maxima/overruns as well as medians. Software-copy
counters are not total DDR traffic, and an observed maximum is not a WCET bound.
Record cooling, governor, frequency, kernel/isolation settings, and any other load
alongside the results; compare every variant under the same conditions.

## Optional CPU profile, after timing runs

Install a `perf` compatible with your kernel and GNU `c++filt` (binutils).
Sampling runs separately from the unprofiled timings. This profiles all nine
idle/play/ramps combinations, with the collector on CPU 1:

```sh
./profile.sh --output results/profiles --host-cpu 0 --render-cpu 3 \
  --rt-priority 80 --collector-cpu 1 --settle 35 --duration 30
python3 scripts/analyze_audio_chunk_profiles.py --input results/profiles \
  --output results/profile-summary --perf "$(command -v perf)"
```

Add `--buffer 32` to both timing and profiling commands when testing that buffer
size. Matching libc debug symbols improve copy-function names; the supplied
executables retain Rust symbols and unwind tables.

## Send results back

```sh
python3 scripts/collect_audio_chunk_results.py results --output pi4-results.zip
```

Upload that ZIP and your system/RT configuration to PR #959. It contains raw
callback CSVs, summaries, setup/readback evidence, scheduling and symbolized
profiles. Connection credentials and raw perf stack-memory dumps are excluded.
The ARM64 builds are smoke-tested under emulation; no emulated timing is evidence
of Pi performance. Detailed design/native comparisons are in `docs/`.
