# Render CPU sampling and timing reproduction

The Pi 4 optimizations were already present in the original measured build.
This investigation samples the unchanged release executable and repeats its native
benchmark matrix. It makes no changes to rendering, allocation or synchronization.

During four-voice playback, storage/indexing and reference evaluation together
account for roughly 46% of sampled user CPU time in the experimental variants.
Internal MIDI work accounts for 9–12% across variants despite the audio-only policy.
Named bulk-memory routines beneath planner/resolver frames account for about 3%;
that diagnostic excludes inlined copying. The timing rerun reproduces idle gains
and playback regressions, with no demonstrated worst-case improvement.

Subsequent [Pi hardware timings](PI_RESULTS.md) are now available. They support
the same broad conclusion but do not include Pi stack profiles; the function
shares below remain specific to this Xeon capture.

## Upstream verification

On 2026-11-08, the upstream `feat/backend-replacement` tip is
`888bd8a66143a6a6a675883823578a7c1d7e4bee`. Both that tip and Pi optimization
[PR #958](https://github.com/SanderVocke/shoopdaloop/pull/958), merged as
`7a77c8d5048d6ec33c305385710dd40044d039b7`, are ancestors of the measured
`df4ba416996a41cda6c65c85781334aa6cc6b55d` source and the working branch.
An explicit fetch and merge reports **Already up to date**. No rebase or new merge
commit is needed.

That Pi change removed idle control-thread staging-model copies in
`shoop_rt_engine::queue::process_queue`, replaced full-capacity native host-port
arrays with prepared-length collections, and improved snapshot pacing and DSP
telemetry. The resolver still copies its state and request-record arrays into
preview scratch in `Resolver::begin_preview`. These are different copies.
The earlier explanation should have distinguished them; the large idle control
copy was already fixed in the measured revision.

## Sampling method

- Unmodified, previously benchmarked release host and launcher; binary SHA-256
  values must match the [original manifest](results/manifest.json).
- Existing dummy host, normal app-service commands, identical three renderer modes
  and audio-only policy. No additional timers or probes inside the callback.
- Linux x86_64 / Intel Xeon Platinum 8573C, four-CPU container quota. All host/client
  threads use CPU 2; the perf collector uses CPU 0. Ordinary scheduling; no kernel,
  IRQ or isolation configuration changes.
- `perf` 6.12.115 samples only the `shoop-rt-dummy` thread with `cpu-clock:u` at
  997 Hz of user CPU time. DWARF unwinding uses the existing executable's unwind
  tables and 32 KiB stack snapshots. Collection starts eight seconds after launch
  and lasts thirty seconds. Hardware PMU events are unavailable here.
- One capture for each combination of idle/silent input, four-voice playback and
  active gain ramps with current/chunks/chunks-descriptors. Each launcher run also
  validates callback continuity, mode, render faults, capture completeness and
  stereo readback. Profiled timing values are excluded from the timing comparison.
- Category percentages use sampled event periods, and assign each sample once to
  its nearest recognized stack frame, starting at the leaf. libc bulk operations
  are charged to that caller. Standard-library/inlined work can therefore be
  assigned to its enclosing named function. The release binary has symbols and
  unwind tables but lacks full source/inlining debug information. Matching glibc
  debug symbols resolve its bulk-copy implementations.

These are rough on-CPU user-time shares, not elapsed-callback shares, total memory
traffic, cache misses, or a decomposition of deadline overruns. Kernel execution,
scheduling delays and off-CPU waiting are excluded. Sampling itself has overhead.
The longer profile runs include history growth and wrapping, whereas the ordinary
short timing cases start with only 0.5 seconds of explicit warmup. Mode percentages
have separate denominators and cannot be compared as absolute CPU-time savings.
With one capture and hundreds to roughly 1,500 samples per case, differences of
a few percentage points should not be overinterpreted.

All nine captures completed with **8,321 samples**, zero lost samples, and at most 1.8% unattributed in any capture.

Shares below are approximate percentages of sampled user CPU time; rows are exclusive.

### Idle, silent input

| Work | Current | Chunks | Descriptors |
| --- | ---: | ---: | ---: |
| Loop planning and state | 18% | 26% | 29% |
| Audio storage and indexing | 18% | 11% | 10% |
| MIDI buffers and history | 16% | 14% | 16% |
| Reference processing | 0% | 11% | 9% |
| Graph nodes, mixing and meters | 27% | 14% | 13% |
| Loop capture coordination | 2% | 3% | 3% |
| Runtime scheduling and dispatch | 11% | 10% | 9% |
| State publication | 3% | 4% | 3% |
| Host wrapper and other | 5% | 7% | 8% |
| Unattributed | 0% | 1% | 0% |

Sample counts, current / chunks / descriptors: 511 / 367 / 384.

### Four-voice playback

| Work | Current | Chunks | Descriptors |
| --- | ---: | ---: | ---: |
| Loop planning and state | 19% | 17% | 16% |
| Audio storage and indexing | 17% | 22% | 25% |
| MIDI buffers and history | 12% | 10% | 9% |
| Reference processing | 0% | 24% | 20% |
| Graph nodes, mixing and meters | 29% | 10% | 12% |
| Loop capture coordination | 1% | 1% | 2% |
| Runtime scheduling and dispatch | 10% | 6% | 6% |
| State publication | 5% | 5% | 4% |
| Host wrapper and other | 6% | 4% | 5% |
| Unattributed | 2% | 0% | 0% |

Sample counts, current / chunks / descriptors: 991 / 1122 / 1210.

### Playback with active gain ramps

| Work | Current | Chunks | Descriptors |
| --- | ---: | ---: | ---: |
| Loop planning and state | 18% | 16% | 16% |
| Audio storage and indexing | 16% | 27% | 21% |
| MIDI buffers and history | 11% | 8% | 10% |
| Reference processing | 0% | 23% | 22% |
| Graph nodes, mixing and meters | 29% | 11% | 13% |
| Loop capture coordination | 1% | 1% | 1% |
| Runtime scheduling and dispatch | 12% | 7% | 8% |
| State publication | 5% | 3% | 4% |
| Host wrapper and other | 6% | 4% | 4% |
| Unattributed | 1% | 0% | 0% |

Sample counts, current / chunks / descriptors: 1037 / 1499 / 1200.

### Named bulk-memory routines

These overlapping diagnostics are already included in the categories above. The planner column is a subset of bulk-memory samples, not an additional category.

| Scenario / mode | Bulk copy/zero | Under loop planning/resolver |
| --- | ---: | ---: |
| idle / current | 15.5% | 5.1% |
| idle / chunks | 17.2% | 5.2% |
| idle / chunks-descriptors | 15.1% | 6.2% |
| play / current | 9.1% | 3.2% |
| play / chunks | 12.1% | 2.9% |
| play / chunks-descriptors | 17.2% | 2.6% |
| ramps / current | 11.6% | 3.5% |
| ramps / chunks | 23.1% | 2.3% |
| ramps / chunks-descriptors | 11.7% | 2.1% |

## Interpretation

The reference path trades payload copying for reference evaluation, validated
storage visits, persistent-index work, ownership and graph routing. Those costs
remain even when a gain descriptor avoids a sample pass. Mixing, meters, capture,
effects adapters and output still consume samples. The resource-copy counter
omits this metadata and much graph/sample work, so its large reduction is not a
prediction of an equally large CPU-time or physical-memory-traffic reduction.

The audio-only policy removes device MIDI ports but retains empty internal MIDI
objects. `midi_writer::Core::maintain` still scans owner credits when retirement is
empty; it is a conspicuous common cost even with no MIDI events. Audio storage
and index lookup, including `AudioStore::find` on the chunk playback path, are
also visible. Both deserve targeted follow-up before attributing the callback
result primarily to the resolver's array copies. Candidate experiments are avoiding
empty MIDI owner scans and reducing repeated storage lookups/reference visits,
including meter scans. Neither change is included in this profiling task.

The bulk-memory diagnostic counts samples whose leaf is a named libc
`memcpy`/`memmove`/`memset`/`bzero` implementation. It includes non-audio copies and
omits inlined copy/zero loops and other memory accesses. It therefore neither
measures all data movement nor bounds the potential benefit of memory changes.
The planner sub-diagnostic identifies bulk-memory stacks under loop planning or
resolver frames; it does not isolate the two scratch copies from other planner
copying or clearing.

This Xeon can execute short, contiguous copies cheaply while a Pi 4 may be much
more sensitive to cache capacity and shared DDR. These captures explain plausible
CPU costs of this prototype on this machine; they do not establish the Pi result
or prove that its original spikes were caused by DDR contention.

## Timing reproduction

The full original 147-run matrix is repeated separately with sampling disabled:
81 main runs, 54 other-core interference runs, six retained-history runs and six
near-capture-quota runs. Each selects 384 consecutive callbacks; mode order rotates
between repetitions. Binary, buffer size, quantum, prepared capacities, scenario
commands, warmups and CPU assignments match the original matrix. The new helper
scripts do not participate in these timing runs.

**147 runs / 56,448 callbacks** pass the existing validation: zero recorder drops, zero render faults and no incomplete captures. No cgroup CPU-quota throttling was recorded in any matrix.

Triples are **current / chunks / descriptors**. Times below are microseconds.

| Main scenario | Original median run p50 | Repeated median run p50 | Repeated median run p99 |
| --- | ---: | ---: | ---: |
| idle (silence) | 88.5 / 70.6 / 71.9 | 74.7 / 60.4 / 61.2 | 256.0 / 268.8 / 306.6 |
| record (silence) | 163.1 / 163.9 / 178.2 | 160.6 / 153.2 / 158.5 | 541.6 / 534.4 / 521.7 |
| record (signal) | 170.5 / 188.4 / 179.1 | 177.0 / 156.8 / 162.0 | 889.0 / 522.4 / 604.8 |
| play (signal) | 178.7 / 200.7 / 198.5 | 176.6 / 189.2 / 190.5 | 774.6 / 583.7 / 579.8 |
| record-play (signal) | 200.5 / 207.8 / 218.8 | 171.8 / 189.0 / 192.7 | 550.1 / 588.7 / 523.3 |
| shared (signal) | 194.3 / 213.8 / 203.4 | 184.1 / 187.2 / 191.9 | 1731.9 / 1494.0 / 1313.8 |
| continue (signal) | 250.0 / 226.0 / 237.6 | 227.7 / 203.9 / 212.4 | 654.1 / 637.6 / 611.3 |
| churn (signal) | 175.5 / 173.4 / 174.6 | 167.3 / 162.2 / 167.8 | 545.8 / 755.7 / 1036.9 |
| ramps (signal) | 192.4 / 216.1 / 204.6 | 171.5 / 208.7 / 195.3 | 557.4 / 544.1 / 470.7 |

| Other-core load / scenario | Repeated median run p99 | Largest callback | Overruns |
| --- | ---: | ---: | ---: |
| periodic / idle | 192.3 / 160.6 / 313.2 | 981.0 / 705.9 / 6207.1 | 0 / 0 / 1 |
| periodic / play | 502.2 / 488.0 / 773.0 | 1196.2 / 1241.2 / 1959.2 | 0 / 0 / 0 |
| periodic / ramps | 495.4 / 812.5 / 637.5 | 2025.3 / 2215.8 / 7228.6 | 0 / 0 / 1 |
| memory / idle | 392.5 / 225.5 / 220.9 | 2169.3 / 3243.3 / 1076.1 | 0 / 0 / 0 |
| memory / play | 521.4 / 941.1 / 540.6 | 2637.2 / 2472.9 / 8386.4 | 0 / 0 / 1 |
| memory / ramps | 496.9 / 659.0 / 529.6 | 2096.3 / 4197.7 / 2536.6 | 0 / 0 / 0 |

| Special case | Repeated median run p50 | Repeated median run p99 | Largest callback | Overruns |
| --- | ---: | ---: | ---: | ---: |
| Full histories / eight retained voices | 195.9 / 214.1 / 226.2 | 2402.8 / 2724.3 / 2789.1 | 4530.9 / 6800.9 / 7319.8 | 0 / 1 / 3 |
| Three recorders near capture quota | 187.7 / 193.4 / 188.5 | 668.4 / 895.8 / 660.2 | 2369.5 / 2605.2 / 1392.5 | 0 / 0 / 0 |

Across all matrices, largest callbacks are **17.33 / 50.69 / 133.28 ms**, with **1 / 2 / 7 overruns**, against a 5.333 ms budget. These are observed maxima, not bounds. The original collection had maxima of 396.35 / 162.66 / 94.84 ms; their ranking is not stable between collections.

The main median pattern reproduces: idle benefits, playback regresses, and descriptors reduce the ramp cost relative to plain chunks without beating current. Recording medians improve in this collection, and tail percentiles move in both directions. The instrumented resource-copy reduction also reproduces:

- idle: 66.0 / 2.0 / 2.0 KiB/callback.
- play: 148.0 / 4.2 / 4.2 KiB/callback.
- continue: 400.0 / 4.1 / 4.1 KiB/callback.

The previous conclusion holds: the prototype has lower instrumented payload-copy work, but no demonstrated general callback-time or worst-case benefit. These on-CPU profiles do not identify the cause of the long elapsed-time outliers.

## Reproduction and evidence

Build and run timings with the commands in [the prototype guide](PROTOTYPE.md).
For sampling, use a perf build compatible with the running kernel, `c++filt` with
Rust demangling, and matching system-library debug symbols when available:

```sh
python3 scripts/profile_audio_chunks.py \
  --host target/release/shoop_host \
  --launcher target/release/examples/chunk_benchmark \
  --perf /path/to/perf --output work/chunk-profiles \
  --host-cpu 2 --collector-cpu 0
python3 scripts/analyze_audio_chunk_profiles.py \
  --input work/chunk-profiles --output work/chunk-profile-summary \
  --perf /path/to/perf
```

Optional `--perf-library-path` applies only to perf, preserving the target's
library environment. The analyzer accepts `--symfs` for a separate symbol root.
The collector uses user-only callchains and disables unused kernel/BPF metadata;
it requires permission to sample its same-user render thread. It never changes
host security policy or scheduling configuration.

- [Symbolized profiles and sampled-stack evidence](results/cpu-profiles-2026-11-08.tar.gz).
- [Repeated native timing evidence](results/native-followup-2026-11-08.tar.gz).
- [Follow-up source/binary hashes, capture counts and archive checksums](results/followup-2026-11-08.json).

The timing archive uses the original safe evidence layout. The profile archive
contains collection metadata and validated app-run evidence under `samples/`,
weighted symbolized stacks and categories under `analysis/`, and machine metadata.
Archive extraction/replay verifies the same run checks and sampled-period totals.
Python helpers pass syntax checks and were exercised by the nine real captures
and complete analysis. Rendering is unchanged, so no additional Rust build or
unit-test rerun is part of this profiling task.

Raw perf stack-memory snapshots and connection authority files stay out of the
published evidence. Symbolized stack summaries retain sample counts, weighted
periods and function names, allowing the category assignments to be audited
without publishing process memory.
