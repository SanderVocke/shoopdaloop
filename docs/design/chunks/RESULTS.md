# Native audio chunk prototype: measured results

All three variants run through the normal app and native host stack. Instrumented
resource-copy traffic falls by **33x at idle**, about **35x during playback**, and
about **97x during grab-and-continue**. This prototype does **not** demonstrate a
general callback-time or worst-case improvement: playback median time regresses,
metadata/RSS grow substantially, and all variants have deadline overruns.

Across the complete matrix, the largest observed callback was **396.35 ms current,
162.66 ms chunks, and 94.84 ms chunks-descriptors**, against a **5.333 ms** budget.
These are retained observations, not WCET bounds or evidence of a guaranteed ranking.
The cause of individual outliers is unresolved by the available counters. No cgroup
CPU-quota throttling was recorded during any matrix.

A follow-up [CPU sampling and timing reproduction](PROFILING.md) checks the upstream
Pi optimizations and investigates where the callback spends its time.

The subsequent [Pi 4 hardware results](PI_RESULTS.md) reproduce the broad median
ranking and expose additional memory-pressure and loop-boundary deadline failures.

## Source, build and conditions

- Source: `df4ba416`, based on `feat/backend-replacement` at `888bd8a6`.
- Rust: `rustc 1.99.1 (58cf8f9c3 2026-10-13)`; release profile; `RUSTFLAGS=-D warnings`.
- Build: `cargo build --release -p shoop_host --features testing --bin shoop_host --example chunk_benchmark`.
- One host and launcher binary for every mode. Full SHA-256 hashes and exact commands
  are in the manifest and each matrix's `machine.json`.
- Linux x86_64 container, Intel Xeon Platinum 8573C; four-CPU cgroup quota. All host
  and client threads share CPU 2; interference uses CPU 3. This is not render-only
  isolation. Ordinary scheduler; no PREEMPT_RT or IRQ/isolation changes were made.
- 48 kHz, stereo, Q=32, 256-frame callbacks; four prepared track slots and sixteen
  loop slots except the explicit eight-voice retention case. Native dummy driver,
  common audio-only policy, thirty-second histories, nominal 16 MiB loop budget.

Setup uses the normal socket app service and commands. Signal input varies with
frame and channel; idle and silent recording use zero input. Real app timing means
recorded lengths/control arrival can differ slightly between runs; setup/final
state and media readbacks are archived. Exact timing/sample equivalence is tested
separately by deterministic engine fixtures.

Each run selects exactly 384 consecutive callbacks after warmup. Main/interference
cases repeat three times with rotating mode order; retention/quota cases repeat
twice. There are **147 runs / 56,448 measured callbacks**. Every mode has 49 runs,
18,816 callbacks, zero recorder drops, zero render faults and no incomplete
captures. Overruns total **4 current / 17 chunks / 10 descriptors**. Callback-delivery
lateness is reported separately in raw CSV and summaries, alongside p50/95/99.

## Main matrix

These runs use 0.5 seconds of explicit warmup. Playback uses four voices. Capture
and readback finish through normal commands; bulk verification happens after the
timing window. All slow callbacks remain in the statistics.

Triples below are **current / chunks / chunks-descriptors**. Times are microseconds.

| Scenario | Median run p50 | Median run p99 | Largest observed callback | Total overruns |
| --- | ---: | ---: | ---: | ---: |
| idle (silence) | 88.5 / 70.6 / 71.9 | 388.9 / 284.3 / 239.5 | 1717.2 / 7233.5 / 7839.9 | 0 / 1 / 1 |
| record (silence) | 163.1 / 163.9 / 178.2 | 729.1 / 534.0 / 726.2 | 2092.6 / 7766.6 / 4716.2 | 0 / 1 / 0 |
| record (signal) | 170.5 / 188.4 / 179.1 | 735.3 / 631.5 / 803.7 | 9999.1 / 12672.0 / 2795.0 | 1 / 1 / 0 |
| play (signal) | 178.7 / 200.7 / 198.5 | 590.3 / 562.5 / 556.5 | 3093.2 / 1078.4 / 727.2 | 0 / 0 / 0 |
| record-play (signal) | 200.5 / 207.8 / 218.8 | 997.2 / 525.4 / 548.1 | 6447.3 / 4270.5 / 2781.3 | 1 / 0 / 0 |
| shared (signal) | 194.3 / 213.8 / 203.4 | 1395.1 / 1421.7 / 1386.1 | 2106.0 / 2186.1 / 20830.7 | 0 / 0 / 1 |
| continue (signal) | 250.0 / 226.0 / 237.6 | 798.1 / 741.8 / 830.4 | 2344.2 / 4751.3 / 7302.0 | 0 / 0 / 1 |
| churn (signal) | 175.5 / 173.4 / 174.6 | 633.1 / 621.6 / 709.8 | 4564.4 / 1710.0 / 8261.4 | 0 / 0 / 1 |
| ramps (signal) | 192.4 / 216.1 / 204.6 | 605.5 / 786.2 / 645.2 | 3352.8 / 53774.5 / 3453.5 | 0 / 5 / 0 |

| Scenario | Resource-copy KiB/callback | Index nodes allocated/callback | Index nodes reclaimed/callback |
| --- | ---: | ---: | ---: |
| idle (silence) | 66.0 / 2.0 / 2.0 | 136.0 / 168.0 / 168.0 | 135.0 / 152.0 / 152.0 |
| record (silence) | 138.0 / 4.0 / 4.0 | 272.0 / 504.0 / 504.0 | 270.0 / 456.0 / 456.0 |
| record (signal) | 138.0 / 4.0 / 4.0 | 272.0 / 504.0 / 504.0 | 270.0 / 456.0 / 456.0 |
| play (signal) | 148.0 / 4.2 / 4.2 | 272.0 / 336.0 / 336.0 | 270.0 / 304.0 / 304.0 |
| record-play (signal) | 154.0 / 4.1 / 4.1 | 272.0 / 840.0 / 840.0 | 270.0 / 760.0 / 760.0 |
| shared (signal) | 152.0 / 4.6 / 4.6 | 272.0 / 336.0 / 336.0 | 270.0 / 304.0 / 304.0 |
| continue (signal) | 400.0 / 4.1 / 4.1 | 816.0 / 1008.0 / 1008.0 | 810.0 / 912.0 / 912.0 |
| churn (signal) | 125.7 / 5.7 / 5.7 | 251.2 / 360.2 / 358.9 | 250.6 / 352.4 / 351.2 |
| ramps (signal) | 148.0 / 4.2 / 4.2 | 272.0 / 336.0 / 336.0 | 270.0 / 304.0 / 304.0 |

## Other-core activity

The periodic worker allocates/scans/sorts 30,000 integers once per second. The
memory worker continuously copies a 64 MiB source into a separate 64 MiB target.
Both run on CPU 3; the host remains on CPU 2.

| Other-core load / scenario | Median run p99 | Largest observed callback | Total overruns |
| --- | ---: | ---: | ---: |
| periodic / idle | 295.5 / 735.9 / 225.2 | 396345.6 / 162662.0 / 387.9 | 1 / 4 / 0 |
| periodic / play | 622.4 / 725.1 / 569.9 | 2588.6 / 12903.6 / 8351.9 | 0 / 1 / 1 |
| periodic / ramps | 769.6 / 634.2 / 520.5 | 2491.0 / 1036.3 / 5642.6 | 0 / 0 / 1 |
| memory / idle | 437.2 / 282.1 / 555.6 | 3872.5 / 8027.7 / 3877.7 | 0 / 2 / 0 |
| memory / play | 900.9 / 657.6 / 850.9 | 4127.3 / 5477.8 / 3436.9 | 0 / 1 / 0 |
| memory / ramps | 762.2 / 729.7 / 821.0 | 4736.3 / 9259.5 / 4222.7 | 0 / 1 / 0 |


## Retention and byte-quota cases

| Case | Median run p50, us | Median run p99, us | Largest observed callback, us | Total overruns |
| --- | ---: | ---: | ---: | ---: |
| Full histories, all user loop slots occupied | 230.8 / 237.5 / 238.2 | 2470.3 / 2924.8 / 5033.4 | 53100.0 / 4474.4 / 94840.0 | 1 / 0 / 4 |
| Three recorders near ordinary capture quota | 200.8 / 218.2 / 215.8 | 588.3 / 761.6 / 734.9 | 1918.4 / 1753.6 / 1306.6 | 0 / 0 / 0 |

Retention uses nine loop slots: one sync loop and eight retained playing voices.
The histories are warmed for 31 seconds after grabbing and have wrapped before
timing. Grabs cover the same shorter sync cycle. This fills configured loop slots,
not the physical payload slab; it must not be read as a near-full-payload result.
Storage tests separately exercise owner/debt and payload admission refusal.

The byte-quota case warms for seven seconds with one playing loop and three
recorders. The existing private wet-edit pool reserves 4 MiB out of the nominal
16 MiB ordinary budget, leaving approximately 12 MiB for ordinary capture. Recorded
bytes at the window boundary are about 86% of that available quota. All recorders
stop after the timing window, before bulk readback, so verification cannot consume
remaining capture headroom.

## Sample work and footprint

The descriptor option reduces reference-evaluator materialization under active
controls; it does not eliminate mixing, meter scans, smoothing or capture.

| Main scenario / metric per callback | Chunks | Descriptors |
| --- | ---: | ---: |
| Playback materializations | 48.4 | 48.4 |
| Active ramps materializations | 87.9 | 48.4 |
| Active ramps sample reads | 12785.8 | 11776.0 |
| Active ramps sample writes | 33537.7 | 28983.0 |
| Active ramps mean arena peak, samples | 1031.2 | 779.0 |
| Simultaneous record/play shared payloads | 24.0 | 24.0 |
| Grab-and-continue shared payloads | 32.0 | 32.0 |

These are medians of per-run means. The largest observed arena high-water mark
across all cases was 1,952 f32 samples (7.625 KiB), in both experimental modes.
The arena is prepared for the complete configured node pool; that observation is
not its reserved capacity. Reference routing forwards roughly 1,183 edges per
callback in the playback case. `current` graph-work zeros mean uninstrumented,
not that its processors do no sample work.

| Main matrix memory | Current | Chunks | Descriptors |
| --- | ---: | ---: | ---: |
| Median host RSS, MiB | 419.5 | 588.5 | 588.4 |
| Largest host RSS / resident high-water mark, MiB | 428.1 | 597.1 | 596.8 |
| Prepared history-pool audio metadata, decimal MB | 13.46 | 194.86 | 194.86 |

RSS and Linux VM counters are sampled at the timing-window end, before final bulk
verification. Header payload budgets are held equal across modes. The memory
header excludes allocator/common engine metadata and metadata for the existing
private wet-edit pool; RSS includes those. The prototype reuses a binary index and
validated reference handles, so more metadata work remains despite lower payload
copying. Histories share one persistent pool with recorded media; separate history
arenas/index specialization were not implemented.

Resource-copy counters count instrumented resource reads/writes. They exclude the
sample comparison used to identify a matching latest payload and are not total
sample traffic. Graph counters cover reference evaluation/adapters, not all private
processor arithmetic. Index allocation/reclamation counts are separate software
work measures, not measured index-bus traffic. No CPU hardware PMU is exposed, so
there are no cache-miss or DDR-bandwidth measurements.

## Correctness and integration

Deterministic comparisons cover Q=1/32 and callback partitions 1/13/128: complete
ordinary-chunk samples, descriptor quantum endpoints, exact captures, control
ramps, mixing, monitoring, stop/start, wrap, clear/reuse, grabs and continued grabs.
Storage tests cover shared snapshots, reclamation/admission, stale transient
references and pinned readers surviving later edits/deletion, with heap guards.
The long native-pool regression prepares both the private edit and history pools,
plays one loop and captures three beyond the smaller pool's budget; retained audio
prefixes and one-history-write sharing bounds are checked.

The actual native UI was exercised for track/loop creation, recording, play/stop,
gains, monitoring, meters, waveforms, grabbing and track recreation during
implementation. [UI evidence](results/ui/README.md) is illustrative, not timing data.
The final actual-host harness repeats the public app protocol and stereo media
readback on the delivered storage path.

Descriptor products intentionally use a straight line between composed quantum
endpoint gains. Endpoints match the intended product; interior differences are
accepted approximation. This remains independently selectable. Stored capture
contains evaluated samples, not deferred gain operations.

The final native gate passes **3,188 tests**, with 11 device-dependent skips.
Formatting, warning-denying workspace build, test-attribute policy and actual
bare-metal ARM runtime/loop builds pass. Validation status and exact gate logs
are recorded in [the result manifest](results/manifest.json)
and [the implementation plan](../../../PLAN_CHUNK_PROTO.md). The guide contains
[ordinary launch and reproduction commands](PROTOTYPE.md).

## Data and limits

The [raw archive](results/native-2026-11-07.tar.gz) includes callback CSV, mode/config
identity, machine metadata, setup/final/post-window state, media sample/length
checks, per-run summaries and matrix summaries. Connection profiles and temporary
authority material are excluded. JSON/CSV summaries expose p95, delivery lateness,
per-callback software work, payload/index high-water marks and all overruns.

This is an audio-only opt-in prototype. Empty MIDI compatibility reservations remain;
MIDI, advanced editing/composites and web/embedded feature parity are not acceptance
targets. Effects/smoothing can use existing materialized adapters. Q=1 correctness
uses short histories; large Q=1/history layouts can exceed metadata admission and
are rejected at preparation. Ordinary experimental capture requires histories.

Physical CPAL/JACK devices and Pi 4 hardware were unavailable locally. Production
CPAL mock callback/prefill/sample-delay/heap-guard fixtures cover all variants; native
JACK acceptance is supplied by CI. [Operator-supplied Pi measurements](PI_RESULTS.md)
are now integrated separately: 225 validated runs on a cloud Pi with FIFO render
priority, ordinary PREEMPT and no kernel-isolated CPUs. The original PREEMPT_RT/
core-isolated setup is not covered by these archived three-variant comparisons. These native results alone establish
neither a Pi DDR benefit nor deadline safety.
