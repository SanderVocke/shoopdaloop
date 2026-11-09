# Compact render-path performance follow-up

This follow-up implements and measures the bounded optimizations planned in
[`PLAN_COMPACT_PERFORMANCE.md`](../../../PLAN_COMPACT_PERFORMANCE.md). All timing
and profiling ran on the PREEMPT_RT Raspberry Pi 4; laptop builds were confined
to two low-priority WSL CPUs. Results are observed values, not WCET bounds.

## Workload and evidence

The dense workload has eight stereo tracks, eight allocated grid cells per track,
four active loops per track in active scenarios, and one Sync loop: 64 benchmark
loop objects and 32 active voices. Both callback sizes use engine Q=32, a 64 MiB
ordinary loop arena, CPU 0 for control, and CPU 1 at `SCHED_FIFO/80` for render.
RT bandwidth throttling is disabled during each run and restored afterwards.

Baseline source is PR #960 head `4a293b51`; final implementation source is
`8b76497d`. Baseline/final acceptance uses four compact-only repetitions and the
same 0.171 seconds of selected audio per run: 256 callbacks at buffer 32 or 32
callbacks at buffer 256.

| Artifact | SHA-256 |
| --- | --- |
| `compact-baseline-4a293b51.tar.gz` | `b2c02d51bcbc2599bfbbd5dc4cf5d043e044dd6ee6aaba754f2a01bc8474af10` |
| `compact-final-8b76497d.tar.gz` | `eb19553dc178250b89a122c02dcce4c120a37626c26fe9dca218fa6c2cafa2b5` |
| Baseline host | `06c190b5be483bcbd75e19c47b90bfac4f63188448ee258f2b75be5ad6f40626` |
| Final host | `c801cf562c5c3450d30d14c97a2ef333af889587347d9e01af51343f38e44185` |

Every acceptance run passed topology, state, stereo media, callback continuity,
CPU affinity and FIFO checks with zero dropped records and zero render faults.
Firmware throttle flags remained clear.

## Results

### Buffer 32: 666.7 us deadline

Values are medians of four run percentiles. Overruns are totals across 1,024
selected callbacks per scenario.

| Scenario | Baseline p50 / p99 us | Final p50 / p99 us | p50 reduction | Baseline → final overruns |
| --- | ---: | ---: | ---: | ---: |
| Idle | 368.53 / 582.00 | 213.97 / 381.56 | 41.9% | 0 → 0 |
| Playback | 497.80 / 707.93 | 384.50 / 617.12 | 22.8% | 52 → 6 |
| Record | 635.80 / 912.87 | 507.30 / 727.67 | 20.2% | 455 → 102 |
| Grab | 479.96 / 721.57 | 378.70 / 575.90 | 21.1% | 38 → 4 |

Active-scenario overruns fell from 545 to 112, a 79.4% reduction. Recording
remains the only scenario whose p99 is above the deadline.

### Buffer 256: 5,333.3 us deadline

| Scenario | Baseline p50 / p99 us | Final p50 / p99 us | p50 reduction | p99 reduction |
| --- | ---: | ---: | ---: | ---: |
| Idle | 2,304.13 / 2,597.94 | 1,446.99 / 1,609.62 | 37.2% | 38.0% |
| Playback | 3,344.81 / 3,952.86 | 2,721.57 / 3,134.61 | 18.6% | 20.7% |
| Record | 4,611.46 / 5,337.58 | 3,680.82 / 4,163.33 | 20.2% | 22.0% |
| Grab | 3,387.16 / 4,428.34 | 2,736.19 / 3,692.00 | 19.2% | 16.6% |

The two baseline record overruns were eliminated. No Q=256 percentile regressed.

## Implemented changes

1. Compact processors receive a bounded activity query before routing. Playback
   is skipped only for settled stopped/no-source views; capture is skipped only
   without MIDI, dry/wet, replacement or capture work. Outputs are cleared before
   the decision so skipped operations cannot expose stale references.
2. A built-in fader is skipped when all prepared upstream wires are known silent,
   no host input is involved, no hosted processor exists and its presence ramp is
   settled. Generic and uncertain processors retain the old path.
3. Audio-only preparation records that no MIDI buffers exist and omits the former
   per-callback sweep over every prepared node. MIDI-enabled preparation and
   recovery are unchanged.
4. Compact live capture no longer performs a release lookup for the next append
   page. The append-only frame/page invariant is retained as a debug assertion;
   cancellation, wrap and retained-owner tests cover rollback and reuse.

The dense executed-operation counts fell from 216 per Q=32 callback to 21 idle,
85 playback/record and 87 grab. Skipped-operation counts are exported beside the
existing benchmark counters.

An earlier playback skip experiment initially failed exact sample oracles because
it exited before clearing the operation's output wires. That version was rejected;
the output-clear invariant was fixed before reinstating the fast path.

## Profiling

Render-thread call stacks and PMU counters were captured separately from timing,
after a five-second launch settle. Initial 499 Hz DWARF recording lost samples
under NFS pressure, so conclusions use repeated 99 Hz captures with zero lost
samples. The standalone host benchmark does not expose the application Perfetto
capture control; no Perfetto file is claimed.

Three-second PMU samples changed as follows:

| Scenario | Baseline cycles / instructions / IPC | Final cycles / instructions / IPC |
| --- | ---: | ---: |
| Idle | 2.730B / 1.690B / 0.62 | 1.695B / 1.237B / 0.73 |
| Playback | 3.901B / 2.498B / 0.64 | 2.965B / 2.151B / 0.73 |
| Record | 4.288B / 2.345B / 0.55 | 3.328B / 1.949B / 0.59 |

Baseline profiles ranked `run_tick`, compact reference dispatch/routing,
`Buffers::begin_callback`, loop boundary settlement and compact page-index work.
After optimization, `run_tick` and loop settlement remain the largest broad
symbols. Final recording still spends visible time in capture reservation/commit,
but deeper cross-loop batching would add ownership complexity disproportionate
to the measured bounded lookup gain and is outside this follow-up.
