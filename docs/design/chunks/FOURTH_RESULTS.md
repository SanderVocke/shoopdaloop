# Fourth-variant local results

Status: balanced final timing and all host workflow scenarios are complete.
Default-history, memory-pressure and final native/frontend checks are complete.
CI/review publication is in progress.
These are short exploratory local runs, not Pi measurements or WCET bounds.

## Completed balanced comparison

The completed implementation is compiled from Rust source `cb98366a`; measurement
checkout `a0802122` adds only plan, evidence and replay tooling. All four modes use
one release binary, true audio-only policy and the same normal socket app client.
Prepared capacity is four tracks/sixteen loops. Actual non-idle sessions have
one sync track/loop plus one stereo benchmark track/eight created loops. Playback
and ramps use four playing audio loops (roughly 0.2–0.25 seconds each); four remain
empty/stopped. Record-play uses one player/three recorders, recording uses one,
and shared/continue uses four grabs plus the running sync clock. Idle contains
only the empty sync track/loop. These do not establish large-session or long-loop
playback performance. `--tracks` raises capacity; `--voices` raises active loops
on the single benchmark track.

There are four repetitions per mode/scenario, rotating through every order
position. Each run selects 2.048 seconds of audio: 384 callbacks at buffer 256 or
3,072 at buffer 32. Other settings match the baseline below, including Q=32 and
0.5-second warmup. These short runs do not fill the default 30-second history.

| Buffer | Scenario | Current p50 | Chunks p50 | Descriptors p50 | Compact p50 | Compact/current |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| 256 | idle | 68.81 | 54.24 | 55.74 | 35.58 | 0.52 |
| 256 | play | 148.36 | 179.68 | 168.16 | 111.86 | 0.75 |
| 256 | ramps | 163.13 | 190.69 | 188.05 | 127.76 | 0.78 |
| 32 | idle | 10.50 | 9.30 | 8.84 | 6.94 | 0.66 |
| 32 | play | 19.81 | 23.20 | 24.90 | 17.22 | 0.87 |
| 32 | ramps | 23.14 | 31.09 | 26.81 | 18.34 | 0.79 |

Times are microseconds, medians of the four run p50 values. Compact/descriptors
ratios are 0.64/0.67/0.68 at buffer 256 and 0.79/0.69/0.68 at buffer 32
(idle/play/ramps). Compact is the first chunk prototype here to consistently beat
current's typical callback cost in these scenarios. MIDI-only gains are separate:
the before/after section below measures that common change before compact.

At buffer 256, compact and descriptors both report about 4.3 KB of resource copies
per playback callback, but compact's median is 33% lower. Graph sample reads fall
from 11,776 to 7,424 per playback callback. This supports reducing the combined
planning, routing, ownership and redundant sample-materialization work; copy bytes
alone did not predict CPU cost. This experiment does not isolate each change's
individual contribution. Counts describe logical work, not measured DDR traffic.

For the worst-case objective the evidence is mixed. Playback p99 improves from
733 to 344 us at buffer 256 but worsens from 103 to 129 us at buffer 32. Compact's
small-buffer playback has an 8.2 ms wall-time outlier. There are 3 overruns among
18,432 selected callbacks at buffer 256 and 109 among 147,456 at buffer 32, with
zero drops/faults. Retain all these outliers; ordinary container scheduling and
quota interference prevent attributing their entire duration to renderer work.
Neither this table nor the percentile improvements establish a WCET bound.
General settlement still runs at related synchronization wraps and pending
transitions. Input/meter/smoothing observation paths also retain real sample work.
The prototype reduces steady overhead; it does not eliminate all boundary costs.

Playback RSS is approximately 254 MiB current, 423 MiB chunks/descriptors and
371 MiB compact. The flat index reduces live table counts in these playback runs
to 55, versus about 23,500 binary nodes in the original chunk variants, but its
prepared metadata still reserves about 138 MiB. Working metadata and reserved
metadata are different measures; compact has not solved the startup-memory cost.

All ten host workflow scenarios pass in each mode, including signal grabbing,
shared retention, continued capture, record/play and track churn. Compact has
sample/capture-cut oracles against current through repeated wraps at Q=1/32 and
callback partitions 1/13/128/256. Final validation status is recorded separately from these measurement results.
All four native frontends now pass LAN-addressed TCP recording/playback with
meters and waveform readback; [UI evidence](results/fourth-ui/README.md) includes
compact grabbing and track/loop churn.

Full-history shared playback (31-second warmup, one run per mode) gives compact
p50/p99 of **140.59/564.06 us**, versus current **170.86/825.50 us**. Continuous
memory-pressure playback (two runs per mode) gives compact **124.25/384.90 us**,
versus current **157.69/548.71 us**. Both follow-ups have zero overruns/drops/faults.
They support a typical-cost improvement when history is full and under this load;
the small-buffer playback tail remains mixed. Final matrices contain 138 runs /
182,016 selected callbacks, plus the ten compact workflow runs (3,840 callbacks).

## Removing internal MIDI from the three existing modes

Fresh before/after runs use the existing dummy host, normal socket app client,
stereo 48 kHz, Q=32, 256-frame callbacks, four voices, four track slots, sixteen
loop slots and default 30-second histories. Controller/host CPU 0, rendering CPU 1,
ordinary scheduling; allowed CPUs 0–4 with a four-CPU cgroup quota. No load worker,
concurrent build, RT-priority request or kernel-setting change during timing.

Each mode/scenario has two runs, a 0.5-second explicit warmup and 384 selected
callbacks (2.048 seconds of audio per run). Mode order rotates. This does not fill
the histories and is not a fully balanced or statistically strong comparison.
Rows report the median of the two run p50 values, in microseconds.

| Scenario | Mode | Device-only MIDI exclusion | True audio-only | Change |
| --- | --- | ---: | ---: | ---: |
| idle/silence | current | 83.41 | 70.57 | -15.4% |
| idle/silence | chunks | 67.95 | 59.08 | -13.1% |
| idle/silence | descriptors | 65.54 | 55.56 | -15.2% |
| play | current | 175.15 | 162.16 | -7.4% |
| play | chunks | 198.72 | 179.80 | -9.5% |
| play | descriptors | 203.60 | 171.66 | -15.7% |
| ramps | current | 192.27 | 168.11 | -12.6% |
| ramps | chunks | 233.17 | 201.87 | -13.4% |
| ramps | descriptors | 215.76 | 199.72 | -7.4% |

The following columns are median run p99 / largest selected callback / overrun
count; times are microseconds and the budget is 5,333.3 us. Maxima include every
selected outlier. The container uses ordinary scheduling, so they include
scheduling delays as well as render work.

| Scenario | Mode | Before | Audio-only |
| --- | --- | ---: | ---: |
| idle/silence | current | 325.9 / 3672.0 / 0 | 286.9 / 3124.6 / 0 |
| idle/silence | chunks | 328.7 / 8526.8 / 1 | 268.4 / 1187.1 / 0 |
| idle/silence | descriptors | 231.8 / 1364.8 / 0 | 325.0 / 1188.6 / 0 |
| play | current | 553.9 / 6788.7 / 1 | 441.4 / 1894.0 / 0 |
| play | chunks | 581.9 / 4631.6 / 0 | 564.9 / 1364.0 / 0 |
| play | descriptors | 736.3 / 4247.9 / 0 | 643.1 / 1102.5 / 0 |
| ramps | current | 709.6 / 5962.2 / 1 | 660.9 / 2231.8 / 0 |
| ramps | chunks | 663.9 / 1765.3 / 0 | 645.5 / 1901.3 / 0 |
| ramps | descriptors | 737.9 / 5886.8 / 1 | 678.4 / 1924.0 / 0 |

Idle RSS falls from approximately 419 to 252 MiB for current and 588 to 421 MiB
for the chunk modes. Removing MIDI makes a meaningful difference, but these
ordinary playback medians still favor current. Compact's additional architectural
effect will be compared against the new audio-only baseline.

All **36 timed runs / 13,824 selected callbacks** pass callback continuity,
mode/state and required signal-media readback checks, with no drops or render
faults. The new callback headers identify MIDI disabled, and reserved MIDI
history payload/index metadata is zero. The same release binary serves every
mode within each matrix; machine metadata preserves source and executable hashes.
Before source: `3f2e1973` (production Rust unchanged from `61ae8539`); audio-only
source: `40a02636`.

Nine additional 64-callback actual-host runs pass shared grabbing,
grab-and-continue and track churn in all three modes. Their timings are smoke
evidence only. The first attempted grab smoke exposed an app capability predicate
that depended on MIDI history being present; it was corrected to recognize audio
history independently and covered by a public-protocol regression test. This
capability fix changes no steady audio processing; its source and binary hashes
are recorded separately from the timed matrix.

## First compact comparison

One common release build at `9ff329a4` runs all four modes with MIDI disabled.
Configuration and selected duration match the baseline above. These two repetitions
were exploratory; continued-capture/churn integration was incomplete at that point.
Compact uses direct prepared wire operations, lean loop advancement and fixed-depth
flat audio tables. It preserves current sample arithmetic rather than applying the
third mode's approximate ramp composition.

Times below are microseconds: median run percentiles, largest selected callback,
and total overruns. All 24 runs / 9,216 callbacks pass state/media/continuity checks,
with zero dropped rows and render faults.

| Scenario | Mode | p50 | p95 | p99 | Max | Overruns |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| idle | current | 62.63 | 128.62 | 378.78 | 1363.06 | 0 |
| idle | chunks | 59.05 | 151.48 | 459.44 | 3126.27 | 0 |
| idle | descriptors | 57.62 | 102.54 | 347.54 | 964.54 | 0 |
| idle | compact | 35.23 | 66.93 | 203.16 | 1028.54 | 0 |
| play | current | 143.12 | 257.00 | 824.66 | 12806.65 | 1 |
| play | chunks | 177.15 | 304.94 | 593.84 | 1274.94 | 0 |
| play | descriptors | 166.60 | 269.82 | 565.21 | 2242.43 | 0 |
| play | compact | 125.59 | 201.15 | 359.98 | 6619.15 | 1 |
| ramps | current | 179.84 | 494.87 | 2126.55 | 9247.75 | 1 |
| ramps | chunks | 186.09 | 324.56 | 598.19 | 2148.22 | 0 |
| ramps | descriptors | 203.98 | 375.06 | 571.92 | 3239.08 | 0 |
| ramps | compact | 164.17 | 293.47 | 550.68 | 1269.29 | 0 |

Compact/current p50 ratios are 0.56 idle, 0.88 play and 0.91 ramps;
compact/descriptors ratios are 0.61, 0.75 and 0.80. This suggests the additional
planner/router/storage work matters, beyond MIDI exclusion. It does not isolate
which change helped or establish a worst-case improvement. Ordinary-scheduler
outliers occur in both current and compact. Largest delivery lateness is 67.8 ms
(current ramps) and 24.0 ms (compact play), separate from callback execution time.
All raw lateness and unselected verification rows remain in the archives.

A separate play-only memory run uses a continuous 64 MiB memcpy worker on CPU 2,
with the same four-CPU quota and controller/render placement. All eight runs /
3,072 callbacks pass validation, with no drops/faults. Descriptors has one overrun.

| Mode | p50 | p95 | p99 | Max | Overruns |
| --- | ---: | ---: | ---: | ---: | ---: |
| current | 157.10 | 242.05 | 425.04 | 819.94 | 0 |
| chunks | 177.45 | 369.68 | 695.16 | 2685.60 | 0 |
| descriptors | 168.97 | 317.37 | 773.90 | 6031.73 | 1 |
| compact | 140.16 | 311.17 | 1061.71 | 2783.82 | 0 |

Compact's p50 remains lower, but its p99 is worse here. This small sample provides
**no consistent tail-latency win under pressure**. It is not a Pi or RT scheduling
experiment. The memory worker and cgroup can affect delivery as well as processing.

Playback RSS is about 254 MiB current, 423 MiB chunks/descriptors, and 371 MiB
compact. Compact reserves 144,737,664 bytes of audio metadata, 134,250,496 shared
audio bytes and 16,777,216 ordinary payload bytes. MIDI payload/index bytes are
zero. This first flat-table layout has substantial metadata headroom; sharing
avoids PCM duplication but does not make metadata free. Callback counters preserve
live payload/table counts and retirement/copy work. They describe logical work,
not measured DDR traffic. The final report will use the completed build.

## Evidence and verification

The [complete group tables](results/fourth-local-2026-11-09/groups.csv) include
p50/p95/p99, maxima, overruns, lateness, RSS and logical work for every dataset.
[Replay totals](results/fourth-local-2026-11-09/validation.json) are machine-readable.
The [manifest](results/fourth-local-2026-11-09/manifest.json) records archive and
member hashes, build/source identity, replay validation and reservation headers.
Archives preserve the safe callback/config/state/media evidence, excluding
connection profiles and authority files:

- [Before MIDI removal](results/fourth-local-2026-11-09/before-midi.zip).
- [True audio-only baseline](results/fourth-local-2026-11-09/audio-only-three.zip).
- [Audio-only workflow smoke](results/fourth-local-2026-11-09/audio-only-smoke.zip).
- [First four-way comparison](results/fourth-local-2026-11-09/quick-four.zip).
- [First memory-pressure comparison](results/fourth-local-2026-11-09/quick-memory.zip).
- [Completed compact workflows](results/fourth-local-2026-11-09/compact-workflows.zip).
- [Balanced buffer 256](results/fourth-local-2026-11-09/final-256.zip).
- [Balanced buffer 32](results/fourth-local-2026-11-09/final-32.zip).
- [Other three modes, complete workflows](results/fourth-local-2026-11-09/final-workflows.zip).
- [Full-history follow-up](results/fourth-local-2026-11-09/final-wrap.zip).
- [Completed memory-pressure follow-up](results/fourth-local-2026-11-09/final-memory.zip).

Replay each extracted run using `scripts/benchmark_audio_chunks.py`'s
`summarize(directory, write=False)`. Original rows outside each selected frame
window remain archived. The existing engine suite passes, including ordinary
MIDI regressions, and sample/capture/control/partition comparisons across the
audio-only modes. The public-protocol regression verifies MIDI refusal, audio
capture and retained audio grabs without MIDI history/index reservation. Release
host/launcher builds deny warnings; formatting and test-attribute checks pass.

## Final local validation

The warning-denying workspace build, formatting and Rust test-attribute policy
pass. The complete native gate passes **3,192 tests**, with 11 skipped and no
failures, in 517.8 seconds. `rt_runtime`, `rt_buffered`, `shoop_loops` and
`shoop_port_graph` also build for `thumbv7em-none-eabihf` with warnings denied.
All four modes start through a real local JACK service backed by dummy hardware
and a normal frontend can attach/create audio tracks. Native JACK acceptance
passes. Physical CPAL hardware is unavailable; the existing real-driver mock
covers all four modes, buffer timing, callback heap guards and MIDI exclusion.
[Validation identity and log hashes](results/fourth-local-2026-11-09/final-validation.json).

The requested [Pi 4 bundle](https://github.com/SanderVocke/shoopdaloop/pull/960#issuecomment-6734979574)
is delivered with all four modes and short test instructions. Its ARM64 build,
12 emulated actual-app workflows and public download checksum pass. No Pi hardware
timing was performed here.
