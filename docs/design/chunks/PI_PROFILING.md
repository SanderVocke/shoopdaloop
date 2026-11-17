# Pi 4 targeted profiling: scheduler pauses and remaining CPU costs

The follow-up confirms the suspected cause of the approximately 50 ms callbacks:
**the render thread exhausts the kernel's RT runtime allowance and is held off CPU
until replenishment**. It also independently demonstrates severe cache/memory
stall sensitivity. These are distinct effects. The original callback timer
includes both CPU work and time spent descheduled.

The chunk design still has no demonstrated general callback-time or worst-case
win. However, the PMU captures give it a more encouraging result under memory
pressure than a reading of the earlier deadline counts alone would suggest.
They also qualify the hypothesis that its larger RSS necessarily causes more
cache refills. Common MIDI/state work is a substantial remaining target.

## Evidence and verification

Source: [executor's profiling upload](https://github.com/SanderVocke/shoopdaloop/pull/959#issuecomment-6727413066),
[original ZIP](https://github.com/SanderVocke/shoopdaloop/releases/download/pr-959-pi4-results-20261108/pi4-targeted-profiles.zip),
and the preserved [operator report](results/pi4-profiles-2026-11-08/operator-report.md).
The 11,749,484-byte ZIP has SHA-256
`fce289841ba0a6d71f2bfe59afd657a6fb9bff461c713613ed97b984b1336e67`.

We verified all **345 member hashes**, replayed the existing app/callback checks
for **21 runs / 208,695 selected callbacks**, and reproduced the PMU counts,
scheduler delays and sampled-stack categories. All checks pass: no recorder
drops, render faults, failed readbacks or lost CPU samples. The nine user CPU
profiles contain **13,844 samples**. All nine raw CPU profiles identify the same
host ELF build ID as the delivered ARM64 executable:
`f3492f4e3fe6dc649f0de33e1338f384ee88847d`.
The build manifest is byte-identical to the delivered bundle, with binary source
`d40bfdaa` and bundle helpers `5c5009cc`; upstream Pi optimizations are included.

Placement remains controller/host CPU 0, render CPU 1, interference CPU 2,
render `SCHED_FIFO/80`. This is the same ordinary PREEMPT kernel, without isolated
CPUs, rather than the original user's PREEMPT_RT setup. Profiles are separate
diagnostic captures, not replacements for the [unprofiled timing matrix](PI_RESULTS.md).
Targeted launchers specify 0.5-second warmup, versus 31 seconds in that matrix;
the collection delay does not establish an equivalent full-history condition.
The Pi's wall clock reports July 2023; period comparisons here use monotonic
perf timestamps, not that calendar clock.

## RT throttling explains the long off-CPU gaps

Captured global limits are `sched_rt_runtime_us=950000` and
`sched_rt_period_us=1000000`. No cgroup RT runtime controls are present in the
recorded ancestry. In the loaded 32-frame scheduler captures, `sched_switch`
shows the render thread switched out while still runnable (`prev_state=R`),
with idle or ordinary work running in its place. It resumes approximately
48–53 ms later. Successive resume timestamps are one second apart within tens
of microseconds, matching RT runtime replenishment.

| Mode | Runnable gaps over 40 ms | Median gap, ms | Largest gap, ms |
| --- | ---: | ---: | ---: |
| current | 10 | 47.924 | 51.928 |
| chunks | 9 | 48.017 | 51.995 |
| chunks-descriptors | 10 | 49.492 | 53.520 |

These **29 gaps are scheduling delays**, not 50 ms executions of a renderer
function. The supplied timehist summary's intervals between large events use a
different timestamp; our one-second check uses raw switch-in timestamps.

The earlier ~53 ms maxima and loaded 32-frame overrun rankings describe service
behavior under this runtime cap, not intrinsic renderer WCET. They remain valid
observations of that configuration. Removing the cap would not by itself explain
away the failures: the earlier ordinary loaded medians already exceeded the
666.7 us budget, and most overruns were not the rare long gaps. A new timing
comparison needs an explicit RT budget and adequate sustained CPU headroom.

## PMU evidence: real memory sensitivity, a modest experimental benefit

Each counter capture attaches to the render TID for a requested 15 seconds.
All four events report 100% running, without counter multiplexing. Values per
frame below divide by **nominal** `15 * 48000` frames; the precise processed-frame
count at PMU start/end was not recorded. IPC and refills per instruction do not
depend on this denominator. The event commands do not specify a user-only
privilege filter; the separate sampled-stack captures do.

| Case | Mode | Cycles / nominal frame | Instructions / nominal frame | IPC | L2D refills / nominal frame |
| --- | --- | ---: | ---: | ---: | ---: |
| play, quiet | current | 5,353 | 5,853 | 1.09 | 16.9 |
| play, quiet | chunks | 5,646 | 5,711 | 1.01 | 16.5 |
| play, quiet | descriptors | 5,695 | 5,726 | 1.01 | 16.3 |
| play, memory load | current | 28,721 | 5,958 | 0.21 | 146.7 |
| play, memory load | chunks | 27,497 | 5,806 | 0.21 | 137.4 |
| play, memory load | descriptors | 26,722 | 5,782 | 0.22 | 131.2 |
| shared, quiet | current | 7,258 | 8,872 | 1.22 | 18.6 |
| shared, quiet | chunks | 7,160 | 8,557 | 1.20 | 16.3 |
| shared, quiet | descriptors | 7,072 | 8,536 | 1.21 | 15.1 |

Memory load raises cycles by **4.7–5.4 times** while instructions change by only
about **1–2%**. L2D refills rise **8.1–8.7 times**, with L1D refills changing much
less. IPC falls from about 1 to 0.21. This supports shared-cache/memory stall
sensitivity independently of the scheduler explanation. It does not measure DDR
bandwidth directly or separate cache eviction from DRAM queueing/latency.

Under memory load, chunks have **4.3% fewer cycles and 6.4% fewer L2D refills**
than current; descriptors have **7.0% fewer cycles and 10.6% fewer refills**.
In quiet playback the experimental modes instead use **5.5–6.4% more cycles**,
despite slightly fewer retired instructions. The tradeoff is consistent with
copy avoidance helping under contention while reference evaluation has costs.
These aggregate counters do not isolate the instruction/dependency cost of
individual functions.

Treat the small cross-mode percentages as directional: there is one PMU capture
per mode/case, a different warmup protocol, no exact PMU-window frame counts,
and no repeated randomized PMU comparison or measurement of interference-worker
throughput. They do not reverse the earlier repeated timing/tail results.

The earlier **420 versus 590 MiB RSS** difference remains a capacity cost, and
additional reference/index operations are visible in stacks. But a claim that
the experimental path causes more L2 refills is **not supported by these data**:
refills are lower in all three profiled cases. RSS alone does not identify the
active cache working set or explain the quiet-playback regression.

## Common boundary work and MIDI state are concrete targets

Approximate exclusive shares of sampled user CPU time:

| Work / case | Current | Chunks | Descriptors |
| --- | ---: | ---: | ---: |
| planning / ordinary play | 22.0% | 23.4% | 22.1% |
| storage/indexing / ordinary play | 15.3% | 17.3% | 18.3% |
| reference processing / ordinary play | 0.0% | 16.3% | 17.5% |
| graph/mixing/meters / ordinary play | 26.8% | 15.5% | 15.8% |
| MIDI/history / shared play | 26.0% | 25.9% | 29.0% |
| planning / shared play | 20.0% | 17.9% | 15.6% |

Even with device MIDI disabled, shared playback spends **26–29%** in MIDI/history
work. Prominent leaves include indexed MIDI-store `Reader::cuts`,
`Timeline::last`, and `midi_writer::Core::maintain`, alongside planner/resolver
settlement. Stacks show the reader calls beneath MIDI state reconstruction.
Source inspection confirms that reconstruction queries state across all sixteen
MIDI channels. An empty-MIDI state/history fast path is therefore a concrete
candidate to test in all three modes.

Experimental paths also spend time in `AudioStore::find`,
`RenderArena::read_into`, reference routing and persistent indexing. Payload
sharing does not remove these visits, planning, metering or internal MIDI work.
These findings support simplifying repeated storage/reference work, but do not
prove which individual mechanism causes the quiet-playback regression.

Shared playback retires roughly **49–52% more instructions per nominal frame**
than ordinary playback, without a comparable L2 refill increase. Together with
the loop-phase-locked callbacks, this points toward common boundary bookkeeping.
The profiles aggregate the whole capture: they are not separately gated to the
slow callbacks and cannot prove that MIDI alone accounts for the boundary spike.

The provided report includes warmup/verification callbacks in its shared-case
peak table. Filtering by each run's actual selected frame window gives:

| Mode | Selected callbacks >5 ms | Median recurring peak, ms | Selected maximum, ms |
| --- | ---: | ---: | ---: |
| current | 303 | 6.203 | 6.482 |
| chunks | 290 | 6.321 | 6.707 |
| descriptors | 304 | 6.329 | 6.791 |

Every selected peak occurs at one fixed frame residue for its run. The reported
11.6–11.7 ms recurring maxima, and the approximately 13 ms whole-file maxima,
occur **outside the selected windows**. They should not be presented as new
steady-state regressions. These are also profiled runs; the earlier unprofiled
shared peaks remain the appropriate timing comparison.

Categories assign a sample to its nearest recognized named frame. Only
0.12–2.81% is wholly unattributed, but **5.9–13.9% has an unknown leaf**, often
within libc, and is charged to a named ancestor. Consequently, the small share
of explicitly named bulk-copy routines is not an upper bound on copying costs.
Percentages have separate per-capture denominators and are rough on-CPU shares,
not elapsed callback shares or absolute time saved.

## Replaying and auditing

The [manifest](results/pi4-profiles-2026-11-08/manifest.json) records the source
archive, original member hashes, checked ELF identity and derived-artifact hashes.
The checked-in [inspection archive](results/pi4-profiles-2026-11-08/evidence.tar.xz)
preserves selected input files byte for byte, including callback CSVs, settings,
commands, PMU CSVs, textual traces and weighted symbolic stacks. Raw perf files
with sampled stack memory and host/launcher logs remain in the original linked
ZIP, rather than being duplicated into git. Here `profile.json` contains
collection commands/metadata, not an application connection profile.

Download and extract the original ZIP into an ignored working directory to run
the complete member-hash and analysis replay:

```sh
python3 scripts/analyze_pi_targeted_profiles.py \
  --input work/pi4-targeted-original \
  --output work/pi4-targeted-replayed
```

The script validates all supplied hashes, checks the delivered build manifest,
replays existing app/callback validation without rewriting input, recomputes
counter normalization and exclusive stack categories, and reconstructs runnable
pauses from raw scheduler text. Derived [PMU CSV](results/pi4-profiles-2026-11-08/pmu.csv),
[scheduler pauses](results/pi4-profiles-2026-11-08/scheduler.json),
[profile summaries](results/pi4-profiles-2026-11-08/profiles.json),
[boundary checks](results/pi4-profiles-2026-11-08/boundaries.json) and
[validation](results/pi4-profiles-2026-11-08/validation.json) are directly readable.
This import changes no rendering implementation or machine settings.
