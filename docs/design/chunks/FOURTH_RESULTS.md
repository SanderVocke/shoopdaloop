# Fourth-variant local results

Status: true audio-only baselines are complete; compact implementation is pending.
These are short exploratory local runs, not Pi measurements or WCET bounds.

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

## Evidence and verification

The [manifest](results/fourth-local-2026-11-09/manifest.json) records archive and
member hashes, build/source identity, replay validation and reservation headers.
Archives preserve the safe callback/config/state/media evidence, excluding
connection profiles and authority files:

- [Before MIDI removal](results/fourth-local-2026-11-09/before-midi.zip).
- [True audio-only baseline](results/fourth-local-2026-11-09/audio-only-three.zip).
- [Audio-only workflow smoke](results/fourth-local-2026-11-09/audio-only-smoke.zip).

Replay each extracted run using `scripts/benchmark_audio_chunks.py`'s
`summarize(directory, write=False)`. Original rows outside each selected frame
window remain archived. The existing engine suite passes, including ordinary
MIDI regressions, and sample/capture/control/partition comparisons across the
audio-only modes. The public-protocol regression verifies MIDI refusal, audio
capture and retained audio grabs without MIDI history/index reservation. Release
host/launcher builds deny warnings; formatting and test-attribute checks pass.
