# Pi 4 targeted scheduler, DWARF and PMU profile report

## Scope and method

Profiles were collected separately from the unprofiled timing comparisons. The existing placement was retained: controller/host on CPU 0, only `shoop-rt-dummy` on CPU 1, memory interference on CPU 2, and the render thread at `SCHED_FIFO/80`. The machine exposes only CPUs 0-2 to this session.

The supplied ARM64 binaries were profiled using ARM64 `perf 6.12.115`. Only the render TID was sampled for CPU profiles, with timestamped user-only DWARF call stacks. Raw `perf.data`, scheduler traces, PMU CSVs, callback CSVs, exact commands, scheduling metadata and machine settings are included in the archive. Every launcher run passed callback continuity/readback validation with zero dropped records and no render faults.

Kernel RT settings before collection:

- `kernel.sched_rt_runtime_us = 950000`
- `kernel.sched_rt_period_us = 1000000`
- cgroup v2 `cpu.max = max 100000`; no cgroup RT runtime/period controls were present in the session or its ancestors
- performance governor, 1.5 GHz; no isolated CPUs; non-RT `PREEMPT` kernel
- temperature after collection: 41.3 C; firmware throttle status `0x0`

## Cause of the 32-frame, memory-pressure stalls

`perf sched record -a -C 1` confirms that the approximately 50 ms stalls are **runnable-but-descheduled periods caused by RT bandwidth throttling**.

| Variant | Delays over 40 ms in 10 s | Median large scheduling delay | Maximum scheduling delay |
|---|---:|---:|---:|
| current | 10 | 47.924 ms | 51.927 ms |
| chunks | 9 | 48.017 ms | 51.994 ms |
| chunks-descriptors | 10 | 49.491 ms | 53.519 ms |

The trace repeatedly shows `shoop-rt-dummy` runnable (`R`) with 48-53 ms scheduler delay. These gaps recur around the one-second RT bandwidth period and match the unreserved 50 ms in the global 950/1000 ms RT budget. The render thread approaches continuous CPU use at a 32-frame buffer under memory pressure, exhausts the RT runtime allowance, and is held off CPU until replenishment. The stalls are therefore not 50 ms executions inside one renderer function.

This also means that the previous 32-frame memory-pressure overrun ranking should not be interpreted as an engine-level worst-case ranking: all variants cross the kernel's RT-throttling threshold.

## PMU results

Counters are from separate, unsampled runs and are normalized by the nominal 48 kHz frames covered by each 15-second collection. They count only the render TID. Profiled runs are diagnostic and are not timing comparisons.

| Case | Variant | cycles/frame | instructions/frame | IPC | L1D refills/frame | L2D refills/frame |
|---|---|---:|---:|---:|---:|---:|
| playback | current | 5,353 | 5,853 | 1.09 | 136.1 | 16.9 |
| playback | chunks | 5,646 | 5,711 | 1.01 | 139.0 | 16.5 |
| playback | descriptors | 5,695 | 5,726 | 1.01 | 141.5 | 16.3 |
| playback + memory pressure | current | 28,721 | 5,958 | 0.21 | 141.6 | 146.7 |
| playback + memory pressure | chunks | 27,497 | 5,806 | 0.21 | 141.2 | 137.4 |
| playback + memory pressure | descriptors | 26,722 | 5,782 | 0.22 | 140.8 | 131.2 |
| shared playback | current | 7,258 | 8,872 | 1.22 | 141.0 | 18.6 |
| shared playback | chunks | 7,160 | 8,557 | 1.20 | 137.0 | 16.3 |
| shared playback | descriptors | 7,072 | 8,536 | 1.21 | 140.0 | 15.1 |

Memory pressure leaves instruction count almost unchanged, but increases cycles per frame by roughly 4.7-5.4x, reduces IPC from about 1.0 to 0.21, and raises L2 refill counts roughly eightfold. This is direct evidence of cache/DDR stall sensitivity rather than substantially more renderer instructions. The descriptor path has the lowest measured L2-refill and cycle counts under this pressure, though kernel RT throttling still makes all variants fail at 32 frames.

Ordinary playback confirms the earlier timing result: `current` needs about 5-6% fewer cycles per frame than the experimental paths. The experimental variants execute slightly fewer instructions, but their reference/storage work has lower IPC and costs more cycles.

## Ordinary playback: dominant on-CPU work

Approximate exclusive shares from timestamped DWARF samples:

- **current:** graph nodes/mixing/meters 26.8%; loop planning 22.0%; storage/indexing 15.3%; MIDI/history 12.8%; runtime dispatch 11.4%.
  - Frequent leaves include `LoopPlanner::settle_boundary`, `midi_writer::Core::maintain`, `StripInput::render`, `Resolver::settle_preview`, and `Buffers::clear`.
- **chunks:** loop planning 23.4%; storage/indexing 17.3%; reference processing 16.3%; graph work 15.5%; runtime dispatch 10.7%.
  - Added hot work includes `GraphRuntime::run_tick`, `RenderArena::read_into`, `AudioStore::find`, and reference routing.
- **chunks-descriptors:** loop planning 22.1%; storage/indexing 18.3%; reference processing 17.5%; graph work 15.8%; runtime dispatch 9.7%.
  - Descriptor playback still performs `RenderArena::read_into`, reference routing and repeated storage lookup; descriptors do not remove the common mixing, meters, MIDI maintenance or planner work.

This explains why the large reduction in instrumented payload copying does not become a playback-time win: the chunk variants replace it with validated storage visits, persistent-index work and reference evaluation.

## Memory-pressure profiles

The dominant named leaves under pressure remain the same renderer operations, but each memory-sensitive operation consumes many more cycles:

- **current:** `Buffers::clear`, planner boundary settlement, MIDI maintenance, strip rendering and resolver settlement.
- **chunks:** `GraphRuntime::run_tick`, planner boundary settlement, `AudioStore::find/set`, MIDI maintenance and strip rendering.
- **descriptors:** `GraphRuntime::run_tick`, planner/resolver settlement, `AudioStore::find`, MIDI maintenance and strip rendering.

The PMU result is more decisive than percentage shifts in sampled stacks: the extra cost is primarily stalled cycles and L2 refills, not an increase in retired instructions.

## Shared playback and loop-boundary spikes

The long shared-playback captures show one repeatable slow callback per loop cycle:

| Variant | Loop length | Callbacks over 5 ms | At one fixed frame residue | Median boundary callback | Largest recurring boundary callback |
|---|---:|---:|---:|---:|---:|
| current | 5,376 frames | 310 | 309 | 6.203 ms | 11.567 ms |
| chunks | 5,632 frames | 296 | 295 | 6.319 ms | 11.636 ms |
| descriptors | 5,376 frames | 311 | 310 | 6.329 ms | 11.702 ms |

Thus the spikes are phase-locked boundary work, not random scheduler outliers. Shared playback raises instructions per frame by about 46-52% over ordinary playback without a corresponding rise in L2 refills, indicating additional boundary bookkeeping rather than a cache-refill storm.

The shared profiles are dominated by:

- MIDI buffers/history: 26.0% current, 25.9% chunks, 29.0% descriptors
- loop planning/state: 20.0%, 17.9%, 15.6%
- storage/indexing: 14.2%, 15.0%, 13.2%
- reference processing: 0%, 11.9%, 12.9%

The most conspicuous boundary-specific leaves are indexed MIDI-store `Reader` operations, `midi_writer::Core::maintain`, `LoopPlanner::settle_boundary`, resolver transition/settlement, `AudioStore::find`, and—only in experimental modes—`RenderArena::read_into` and reference routing. Empty internal MIDI maintenance is therefore a major common boundary cost even in this audio-only test. The experimental paths add reference/storage visits but do not materially change the size of the recurring boundary spike.

## Conclusions

1. The ~50 ms 32-frame stalls are kernel RT-runtime throttling after the FIFO render thread consumes the 950 ms/second allowance. They are not single long callback functions.
2. Continuous memory copies produce a genuine shared-cache/DDR bottleneck: instructions remain stable while IPC collapses and L2 refills increase about eightfold.
3. `current` remains the cheapest ordinary-playback implementation in cycles per frame.
4. Descriptors reduce the experimental path's memory-pressure cycles/refills relative to plain chunks, but cannot avoid RT throttling at 32 frames.
5. The repeatable shared-playback spike is loop-phase boundary bookkeeping, dominated by common MIDI/history and planner work; chunk modes additionally pay reference and storage-lookup costs.
6. The highest-value follow-ups are to remove empty MIDI owner/history scans from audio-only boundaries, reduce repeated `AudioStore::find`/reference visits, and retest 32-frame memory pressure with RT throttling disabled or a deliberately lower sustained load. Any such retest should be treated as a new timing experiment, not mixed with these profiles.
