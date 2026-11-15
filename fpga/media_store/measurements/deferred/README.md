# Deferred retake/clear evidence

These measurements cover the retired-root FIFO, incremental block reference
release and background sample retirement. The tested RTL sources are identified
by hashes in the long-campaign reports. The original implementation is based on
`bc833fc310817648c9e9c1d4c7ae3d6d21ac24c6`. The full timing benchmark uses
milestone `c2f63163`; final focused/long gates additionally cover the offline
drain expiration fix. No routed FPGA timing or physical DDR deadline is
established here. The sections after the current-policy evidence describe the
historical 20-block/10,000-elapsed-cycle policy and retain its original reports.

## Current 50-microsecond policy and iteration handoff

`cycle-budget/` records the requested change to 5,000 active cleanup cycles at
100 MHz, a 1,024-cycle guard, and no block-count ceiling. Accepted DDR waits count
against the allowance; waiting for foreground resource arbitration does not.
An accepted indivisible transaction can finish after the allowance closes.
This limits admitted cleanup work under the stated service assumptions, rather
than establishing a hard deadline against arbitrarily stalled DDR.

A pending iteration suppresses new collector admission before its run
handshake. The allowance therefore imposes no minimum processing duration.
Explicit host settlement and its subsequent audio run share one grant, and the
next iteration replaces unused allowance instead of accumulating it. In the
fixed-seven-cycle AXI64 regression, empty processing completes in 33--74 cycles;
tested run/settlement admissions take 1--52 cycles with cleanup pending and
allowance still available. These are scheduling probes, not a complete audio
workload benchmark.

The focused gates pass 11 native and 11 single-line-cache DRAMsim3 ownership
tests, three capacity/fault tests, two reused-view tests, two long ownership
campaigns, two full-pipeline tests (including the early-run handoff), and one
isolated timing test. The additional final-source gates pass all 21 production
sessions and fatal recovery under stalled AXI64, plus the 64-loop DDR benchmark
and its early-iteration probe: 37 FPGA test cases in total. All 16 Marlin runtime
tests also pass after integrating
target `821ba859`, including its opt-in invalid-view-ID reuse. The selected
destination's page capacity is checked before slice reference publication.

The final-source mixed 64-mono-loop retake peak is 26,523 cycles (265.23 us at
100 MHz), releasing all 32 short old takes during that quantum with 3,328
charged cleanup cycles and zero records pending at audio completion. Its
recording lengths and 96-block arena match the historical mixed fixture below.
In the same DDR configuration, empty processing takes 852--858 cycles and the
tested run/settlement admissions take 1--16 cycles, with cleanup pending and
allowance remaining. These modeled observations do not bound physical DDR.

The new full native workspace run, with two test workers, reports 3,200 passes,
22 failures and 11 skips. Twenty-one failures concern host startup/recovery;
one legacy native-FX test fails with a pending loop creation and also fails an
isolated retry. These gates remain failed in `cycle-budget/validation.json`.
No base-branch rerun establishes baseline status. Host/legacy-backend source is
unchanged relative to the integrated target; no test deadline was extended.

The new 30-second mono/stereo campaigns still retire 45,024 references without
copying their ID lists. Forty grants alongside continued recording free 355
blocks; 315 are free while replacement takes remain live. Final drain recovers
all 45,048 issued IDs exactly once (eight capacity blocks were never issued)
and all metadata pages. Each replacement channel reads back its 1,280 frames
exactly. Charged maintenance through old-recording drain is 5,112,309 cycles.

The refreshed default-top resource comparison against integrated target
`821ba859`, using the same Yosys 0.52 / Verilator 5.032 tools, adds 629 LUT4 and
882 flip-flops for the media store, or 1,893 LUT4 and 882 flip-flops for the
media/audio top. Block RAM counts remain 15 and 18 respectively. These are
unrouted ECP5 mappings, not Zynq timing or fit results; see
`cycle-budget/resources.json`. The historical resource report below compares
the earlier implementation against its earlier base.

All 16 remote CI checks pass on implementation commit `a14f99fb`, including both
memory models, reclamation, the separated audio/demo inference and mapping
stages, hosted DSP variants and Miri. No review feedback was submitted. The final
publication update changes documentation/evidence only, so GitHub may repeat
the checks automatically. Exact CI links are recorded in `validation.json`.

An isolated profiling override of 20 releases and 100,000 cycles measures the
trade-off independently of the automatic budget:

| Retired tables / reference owners | Metadata cache | Release completion | Including explicit fence |
| --- | --- | ---: | ---: |
| One 20-entry table / one owner | Warm | 20.94 us | 26.55 us |
| One 20-entry table / one owner | Cold | 24.25 us | 29.86 us |
| Twenty one-entry tables / one owner | Warm | 29.51 us | 72.72 us |
| Twenty one-entry tables / one owner | Cold | 65.71 us | 108.92 us |
| One 20-entry table / two owners | Cold | 20.27 us | 25.08 us |

The last row frees no blocks because another owner remains. These DRAMsim3
AXI64 measurements include queue/page reclamation, use 128 table-cache lines,
and have no competing audio or dirty sample payloads. Explicit fencing writes
back cached metadata and is measured separately; normal iterations do not
force this fence. Under the default policy a costly batch spans grants instead
of forcing all 20 releases into one iteration. `reclaim-timing.json` identifies
the profiling override, memory profile, geometry and sources.

Reproduce the new admission probe with
`python fpga/verification/simulate.py --block-chunks 32 --axi-data-width 64 --memory-model fixed --memory-cycles 7`.
Reproduce isolated timing with
`python fpga/media_store/simulate.py --config retirement --block-chunks 32 --metadata-lines 128 --axi-data-width 64 --memory-model dramsim3 --reclaim-timing`.

## Foreground detachment

In one fixed small configuration, clearing one or 17 blocks takes 62 cycles
on native AXI32 and 57 cycles on DRAMsim3 AXI64 with single-line caches.
Both perform zero old-table lookups. These are different hardware/memory
configurations, so their cycle counts are not a memory-model speed comparison.

The long campaigns use a larger four-level table geometry. Each of their
32 clears takes 95 cycles, including the last clear with 31 tables pending.
All 32 clears together take 3,104 driver cycles (31.04 microseconds at an
assumed 100 MHz). Clearing transfers only the root/count; it does not sweep
the old entries. Foreground admission can still wait for an accepted atomic
metadata operation and its external-memory tail.

## Thirty-second ownership campaigns

| Configuration | Mono | Stereo |
| --- | ---: | ---: |
| Loops | 32 | 16 |
| Channels per loop | 1 | 2 |
| Frames per channel at 48 kHz | 1,440,000 | 1,440,000 |
| Recording duration | 30 seconds | 30 seconds |
| Frames per chunk / chunks per block | 32 / 32 | 32 / 32 |
| Blocks per channel take | 1,407 | 1,407 |
| Total references retired | 45,024 | 45,024 |
| Rounded allocated sample bytes | 184,418,304 | 184,418,304 |
| Existing block-ID bytes | 180,096 | 180,096 |
| Additional queue-record bytes | 2,048 | 2,048 |

The arena has 64 live view slots, 45,056 allocation blocks (1,441,792 physical
chunks), 3,532 metadata pages and 256 retired records. The tables initially use
3,072 pages, exceeding the 128-line table cache and crossing leaf/directory
boundaries. The two tests exercise the same 32 channel stores under mono/stereo
groupings. This is a media-store ownership test; it does not instantiate 16
complete stereo processing graphs or observe old sample payloads.

After mass clear, 40 grants of at most 20 releases and a 10,000-cycle window
run alongside 32 channel captures of 32 frames each per iteration. Cleanup
returns 351 blocks during those iterations. The final free stack contains 315
blocks while the new takes remain live. Each new channel's 1,280 frames reads
back exactly. An explicit offline drain completes all 45,024 old releases;
128 pages remain for the replacement recordings. Removing those recordings
and draining recovers all 45,052 allocated block IDs exactly once and all
metadata pages (four arena blocks were never issued).

Total charged cleanup time for the old recordings is 5,172,606 cycles in this
functional native-memory model. This includes the explicit final drain and is
not a foreground latency or a physical DDR throughput guarantee. FIFO ordering
and shared references can delay when a particular recording returns storage.

The reports retain capacity, the actual build revision/dirty flag, generated-top
and model hashes, source hashes and iteration samples. Final recovery also
checks that the reusable-ID stack contains every allocated block ID exactly
once, rather than relying only on its count.

## Full 64-loop retake timing

The DRAMsim3 production comparison passes all 24 quanta of the mixed 64-mono-loop
fixture. Thirty-two loops play and thirty-two capture. At frame 416, the capture
loops simultaneously retake 384 frames each (8 milliseconds), one allocation
block per old take. The table cache has 128 lines; reference/stack caches have
four/two lines; AXI is 64 bits and the fabric is assumed to run at 100 MHz.

The peak is the retake quantum: 27,453 cycles, or 274.53 microseconds. During
that quantum 11 references complete release and 21 retired records remain at
audio completion. Cleanup charges 4,707 cycles; the transport transfers 12,288
sample bytes and 4,160 metadata bytes. The following quantum releases another
16 entries. Queue high-water is 21, and end-of-quantum page use peaks at 170.
There is no intermediate offline cleanup drain in this workload.

This uses a 96-block arena (3,072 physical chunks) to admit replacement takes
while old references remain owned. The former 72-block fixture failed explicitly
with MemoryFull during the retake. The saved 234.33-microsecond block-allocation
result used that smaller arena and synchronous reclamation. The new whole-quantum
measurement includes queue/collector traffic and different memory placement;
it does not claim a short-take speedup. Foreground detachment work is independent
of the old recording length; reclaiming the recording still takes total work
proportional to its entries.

`mixed64-timing.json` contains every quantum's completion, cleanup, metadata and
DDR-bus observations plus the exact compiled fixture geometry. Its workload
never invokes offline drain; the later drain-mode fix preserves its automatic
grant path and is verified by the final focused/integrated gates. Legacy metadata
operation counters cover opcodes 0..9; internal queue/trim work is represented
by cleanup counters and actual transport bytes, not all legacy logical-byte
counters. These modeled times require the stated memory profile and are not
physical-board deadline guarantees.

## Full processing and fault checks

The broader production gate compares 21 mono/stereo sessions exactly, including
retakes, retained exports, clipping/numeric cases and long sessions, with
stalled AXI64 and seven-cycle fixed response service. Retained-export cases also
run with the helper's optional cleanup drain disabled, so no offline drain is
inserted between measured audio quanta.

Focused gates cover AXI32/64, single-line metadata caches, DRAMsim3, queue wrap
and exhaustion, full sample/page pools, duplicate entries, retained partial
ranges, every ownership/checkpoint phase, stale hints, concurrent configuration,
late writeback faults and coordinated reset. The offline drain continues even
with exhausted budget counters and leaves no unlimited automatic grant behind;
its expiration regression also fails on the previous controller. A last reference waits for dirty
transfers before reuse while unrelated cached playback can proceed. An explicit
snapshot fence audits live owners, retired owners and a released active entry.

`validation.json` records individual gates and the broader native result.
The warning-denying workspace build and formatting checks pass. The complete
native suite reports 3,203 passes, 18 host startup/recovery failures and 11 skips;
a focused host rerun reports seven passes, 18 failures and two skips. Failures
include 10/15-second readiness timeouts and consequent startup/recovery errors
in unchanged host code. This native gate remains failed; no base-branch native
rerun establishes whether these failures also occur there.

`resources.json` compares the same default tops against the base with the same
Yosys 0.52 / Verilator 5.032 toolchain. The store adds 1,270 LUT4 and 881 flip-flops;
the integrated media/audio top adds 2,396 LUT4 and 881 flip-flops. Both retain the
same number of ECP5 block RAMs (15 and 18 respectively). These default-top
ECP5 mappings are resource diagnostics, not routed Zynq timing or board fit.

## Reproduction

Use the repository's simulation environment and pinned Verilator 5.032. Build
DRAMsim3 with `python fpga/axi_memory/build_dramsim3.py` before realistic runs.

```sh
python fpga/media_store/simulate_metadata.py
python fpga/media_store/simulate.py --config wide --block-chunks 32 --retired-tables 2 --deferred
python fpga/media_store/simulate.py --config wide --block-chunks 32 --retired-tables 2 --deferred --axi-data-width 64 --metadata-lines 1 --reference-lines 1 --stack-lines 1 --memory-model dramsim3
python fpga/media_store/simulate.py --config wide --block-chunks 32 --metadata-pages 3 --deferred-capacity
python fpga/media_store/simulate.py --config retirement --block-chunks 32 --metadata-lines 128 --axi-data-width 64 --deferred-stress
python fpga/chunk_memory/simulate.py --config small --retire-chunks 32 --memory-model dramsim3
python fpga/verification/simulate.py --directed --random-cases 2 --long-sessions --block-chunks 32 --axi-data-width 64 --memory-model fixed --memory-cycles 7 --stalls --faults
python fpga/verification/simulate.py --benchmark64 --scale-workload mixed --config 8x8 --block-chunks 32 --metadata-lines 128 --axi-data-width 64 --memory-model dramsim3
python fpga/audio_io/simulate_pipeline.py --p 16 --frame-cycles 1600 --memory-model dramsim3 --axi-data-width 64 --stalls
bash fpga/media_store/check.sh
```

CI includes dedicated native/DRAMsim3 reclamation gates and a shorter one-second
version of the large-arena ownership campaign. The saved 30-second campaigns
are separate local runs. A deadline claim needs an external-memory service
bound, a sufficient guard for the complete accepted transaction tail, and
measured progress under the actual competing audio workload. An arbitrarily
stalled DDR service admits no finite deadline guarantee.
