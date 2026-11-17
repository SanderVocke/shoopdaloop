# Native backend performance comparison

Measured on 2026-11-17, comparing `feat/backend-replacement` at `c6bc4b3d` with
the work branch at `674ad102`. The latter commit contains the recording
optimization proposal, not an implementation of that proposal.

The native production float renderer shows small CPU cost increases in some
workloads. Large playback is about 3–4% slower, 128-frame recording about 2%
slower, and recording with input history about 4% slower. Other cases are broadly
comparable. This comparison does not isolate the responsible commits.

| Workload, all loops stereo | Callback frames | Upstream median, us | Work branch median, us | Change |
| --- | ---: | ---: | ---: | ---: |
| 4 loops playing | 128 | 40.48 | 40.26 | -0.5% |
| 64 loops playing | 32 | 147.68 | 151.71 | +2.7% |
| 64 loops playing | 128 | 390.51 | 406.92 | +4.2% |
| 32 playing, 32 recording | 128 | 435.50 | 443.23 | +1.8% |
| 64 loops recording | 32 | 168.00 | 167.96 | 0.0% |
| 64 loops recording | 128 | 471.10 | 482.62 | +2.4% |
| 64 recording, input history enabled | 128 | 564.06 | 585.88 | +3.9% |
| 64 playing, immediate commands and snapshots | 32 | 146.86 | 152.23 | +3.7% |

All rows above have seven paired repetitions. Complete results for 26
configurations are in [the summary CSV](measurements/native-backend-comparison.csv).
The paired changes and bootstrap intervals in that file are calculated from
matched run ratios; the table above uses the ratio of the two reported medians.

## Conditions

The test used `shoop_rt_engine::testing::OfflineHost`, which runs the native CPU
engine without physical audio-device scheduling. Neither FPGA reference execution
nor RTL simulation was selected. Both revisions used:

- Rust 1.100.0, ordinary Cargo release optimization and no custom Rust flags.
- One pinned CPU on an AMD EPYC 9V74 virtual machine, with sequential runs.
- 48 kHz audio, 32-frame processing quanta and 32- or 128-frame host callbacks.
- One strip for four loops, or eight strips for 64 loops, with stereo input,
  loop gain, strip gain and stereo Master output.
- The default `Current` float renderer; experimental chunk renderers disabled.
- Matching resource capacity, disabled processor plugins and no MIDI events.
- Input history disabled except for the explicitly labelled one-second cases.
- Legacy telemetry after each callback; snapshot observation additionally enabled
  for the immediate-command cases.

An identical temporary Rust fixture and support module ran on both revisions.
The fixture hash was
`beaf7caa7faffa9465a18d7c2b51d4c4b8d2526ca07dad8d318623a83308c011`.
It submitted ordinary native requests for setup and used the production
processing entry point for timed callbacks. The extended fixture added sync
relationships and up to eight immediate gain commands per callback with push
snapshots capped at 120 Hz.

Each process measured 1,024 callbacks, excluding its initial 64 from steady
statistics. Revisions alternated AB/BA order, with workload order shuffled for
each repetition. The main 22 configurations had seven pairs. Four additional
sync/immediate configurations had three pairs, with four further confirmation
pairs for the 32-frame immediate case. There were 348,160 measured callbacks.
No additional cgroup CPU throttling occurred during the main matrix.

Input generation, output checksum calculation and control polling were outside
the render timer. Control polling was measured separately and remained broadly
comparable. Every measured callback asserted zero heap allocations,
reallocations and deallocations. All paired output checksums and processing frame
sequences agreed.

VM scheduling produced variable startup, p99 and maximum timings. The CPU-cost
conclusions use repeated median callback times, not a worst-case latency claim.
These measurements do not cover application startup, graph compilation, UI work,
physical drivers or processor plugins. The small native CPU regressions remain
recorded follow-up work; this merge cleanup does not optimize the renderer.
