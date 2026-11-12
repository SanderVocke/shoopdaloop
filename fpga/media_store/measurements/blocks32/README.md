# Thirty-two-chunk block measurements

Reference-counted allocation blocks replace individual chunk ownership. Sample cache lines and transfers remain 32 frames. The implementation is on `proto/fpga-block-allocation`; the preceding metadata-cache implementation remains separately published in PR #971 at `676093aa`.

## Same audio workload, different allocation

All runs assume 100 MHz fabric, Q=32 at 48 kHz (666.67 µs budget), AXI64, 128 sample read buffers, four write buffers, 128 table lines, four reference lines and two stack lines. The secondary mixed runs use 32 reference lines. The DDR3-1066 x16 DRAMsim3 profile/library/drain patch are unchanged, with hashes recorded in every timing report. These are behavioral estimates; no clock/timing-closure guarantee is made.

Each blocked case is checked against the original Rust chunk-allocation case for identical per-quantum audio, jobs, readiness, node/loop state, statuses and transitions. Physical media IDs/capacities change. The measured runs then compare RTL with block-aware Rust at every quantum and compare complete fenced media at the end. Separate native replays compare full intermediate media and independently counted block references after every quantum.

| Fixture | Graph / duration | Previous physical chunks | Blocked physical chunks | Reserved sample arena before → after |
| --- | --- | ---: | ---: | --- |
| Original | Eight tracks × eight mono playbacks, gains/mixes/outputs; 4 quanta, 16 chunks per loop | 1,024 | 2,048 | 128 → 256 KiB |
| Streaming | Same graph; 24 quanta, 64 chunks per loop, seeks at quanta 8/16 | 4,096 | 4,096 | 512 → 512 KiB |
| Mixed | 32 playing / 32 recording, shared ranges, playback then simultaneous retake; 24 quanta | 2,048 | 2,304 | 256 → 288 KiB |

A block is 1,024 frames / 21.33 ms / 4 KiB mono. The short original loops each reserve one half-full block. Mixed retains eight old blocks through ranges in addition to 64 active views; its sample high-water mark can reach all 72 blocks. The 32 initially free chunk IDs pack into one free block. Long recordings waste less than 4 KiB per channel at the tail; partial range boundaries can retain extra chunks in their boundary blocks.

The streaming case now uses only two table entries per view, so its metadata fits the configured cache after warmup. It remains the same audio/seek benchmark, rather than being a metadata-capacity stress case in block mode. The standalone maximum-capacity tests exercise more than sixteen entries and table-page boundaries.

## Timing comparison

| Workload / quantum | Chunk ownership, refs=4 | Block ownership, refs=4 |
| --- | ---: | ---: |
| original / 0 | 166.53 µs | 172.67 µs |
| original / 3 | 131.74 µs | 131.76 µs |
| streaming / 0 | 166.80 µs | 172.82 µs |
| streaming / 8 | 165.99 µs | 147.23 µs |
| streaming / 16 | 165.68 µs | 147.33 µs |
| streaming / 23 | 132.58 µs | 131.30 µs |
| mixed / 0 | 410.09 µs | 262.31 µs |
| mixed / 1 | 182.45 µs | 182.54 µs |
| mixed / 12 | 143.64 µs | 143.65 µs |
| mixed / 13 | 738.05 µs | 234.33 µs |
| mixed / 23 | 232.06 µs | 182.51 µs |

The simultaneous retake drops from 738.05 to 234.33 µs (68.3% lower). It is 1.28× the final ordinary-recording quantum. The old takes contain twelve chunks each, so release work is reduced from 384 chunk entries to 32 block entries (12× for this fixture). Longer takes approach a 32× reduction in entry count; release remains proportional to the number of blocks and this test is not a bound for thirty-second takes.

| Retake detail | Chunks / refs=4 | Chunks / refs=32 | Blocks / refs=4 | Blocks / refs=32 |
| --- | ---: | ---: | ---: | ---: |
| Whole quantum (µs) | 738.05 | 430.68 | 234.33 | 234.33 |
| BeginCapture jobs (µs) | 517.16 | 209.79 | 26.24 | 26.24 |
| Reference reads | 384 | 384 | 32 | 32 |
| Reference writes | 416 | 416 | 64 | 64 |
| Stack pushes | 384 | 384 | 32 | 32 |
| Reference fills / writebacks | 384 / 384 | 0 / 0 | 0 / 0 | 0 / 0 |
| All metadata DDR bytes | 51840 | 2688 | 0 | 0 |

Fewer ownership entries remove the per-take walk over every physical chunk. Block IDs also concentrate count updates into a small working set, avoiding the repeated count-line eviction measured in chunk mode. Whole-block retirement scans the bounded sample-cache tags and drains matching transfers in one command. Sample payloads are not erased as part of release or whole-block allocation.

Cold original playback is 6.14 us slower despite reducing metadata fills from 223
to 128. Foreground metadata stalls rise from 2,698 to 3,312 clocks, accounting
for that increase; less of the serialized lookup wait is hidden. Physical DDR
data-bus utilization is only 4.45% in that quantum. Warm playback is essentially
unchanged. The block change therefore improves ownership work without claiming
that fewer bytes always shorten the critical path. No additional prefetch
scheduling optimization is included.

## Completion timestamps

| Blocked workload / quantum | TP Settle done | SP done | JP done | Finish / total | TP / SP / JP busy |
| --- | ---: | ---: | ---: | ---: | --- |
| original / 0 | 23.76 | 35.29 | 171.96 | 172.67 | 24.45 / 11.52 / 136.65 |
| original / 3 | 8.04 | 19.57 | 131.05 | 131.76 | 8.73 / 11.52 / 111.46 |
| streaming / 8 | 23.76 | 35.29 | 146.52 | 147.23 | 24.45 / 11.52 / 111.21 |
| streaming / 23 | 7.83 | 19.36 | 130.59 | 131.30 | 8.52 / 11.52 / 111.21 |
| mixed / 0 | 23.76 | 39.17 | 261.60 | 262.31 | 24.45 / 15.40 / 222.41 |
| mixed / 1 | 7.76 | 22.85 | 181.83 | 182.54 | 8.45 / 15.08 / 158.96 |
| mixed / 13 | 15.76 | 31.17 | 233.62 | 234.33 | 16.45 / 15.40 / 202.43 |
| mixed / 23 | 7.76 | 22.85 | 181.80 | 182.51 | 8.45 / 15.08 / 158.93 |

Times are microseconds relative to run acceptance; busy columns are durations. Traffic/stall counters stop at done, and overlapping memory work is not added to processor totals. Physical sample placement and metadata-region addresses change with block allocation, so DDR row/bank behavior can also change. Full per-quantum raw clocks, DDR utilization, client bursts/bytes/wait, useful metadata bytes, cache hits/fills/writebacks and foreground lookup stalls remain in the JSON reports.

| Blocked workload | Min / max quantum | Worst quantum budget use | Final fence | Host simulation seconds |
| --- | --- | ---: | ---: | ---: |
| original | 131.60 / 172.67 µs | 25.9% | 1.60 µs | 73.05 |
| streaming | 131.30 / 172.82 µs | 25.9% | 1.60 µs | 215.56 |
| mixed | 143.65 / 262.31 µs | 39.3% | 28.12 µs | 276.16 |
| mixed, refs=32 | 143.59 / 262.28 µs | 39.3% | 28.40 µs | 285.45 |

Setup, one initial metadata invalidation, final snapshots and host readback are outside the active interval. No intermediate snapshot/fence warms measured caches. Initialization, cold preparation, final fence and snapshot costs are recorded separately. Host seconds exclude compilation and oracle generation, include setup/readback, and were collected with concurrent runs on a five-core host; they are not a simulator speed benchmark. Whole-run DDR statistics include physical final drain.

The eight bounded post-run Reclaim commands release one block each. In this fixture each retained range fits one block and therefore becomes empty after one command; the previous one-chunk cleanup left one of its two chunks. This follows the new allocation-unit contract and does not change the measured audio sequence. Cleanup command costs are separate JSON fields.

## Resources

Yosys 0.52 generic/ECP5 checks and Verilator 5.032 both-width lint pass. The default integrated synthesis profile has eight loops, 16 requests, 32 views, program/state/job depths 64/32/64, 16 scratch buffers, two input/output ports, four sample read/write buffers, metadata caches 8/4/2 lines and AXI32. Chunk mode has 32 sample slots; block mode has 1,024 physical slots / 32 allocation blocks. These support 32 independent nonempty mono views at their respective ownership granularities. External sample capacity grows; cache/descriptor BRAM does not. The profile is smaller than the 64-loop performance profile.

| Cell | Metadata baseline | Block allocation | Delta |
| --- | ---: | ---: | ---: |
| LUT4 | 29265 | 29433 | +168 |
| TRELLIS_FF | 16142 | 16141 | -1 |
| DP16KD | 94 | 94 | +0 |
| MULT18X18D | 4 | 4 | +0 |

Counts exclude simulation memory, a physical DDR controller and place/route. Default RTL block size is 32; Rust and simulation fixtures retain selectable one-chunk compatibility.

## Reproduce

```sh
python fpga/axi_memory/build_dramsim3.py
for workload in original streaming mixed; do
  python fpga/loop_pipeline/simulate.py --media --config 8x8 --timing \
    --timing-workload "$workload" --block-chunks 32 \
    --read-buffers 128 --write-buffers 4 --axi-data-width 64 \
    --metadata-lines 128 --reference-lines 4 --stack-lines 2 \
    --memory-model dramsim3 --fabric-hz 100000000 --output "work/blocks-$workload"
done
```

Use `--reference-lines 32` for the secondary mixed measurement. Add `--timing-verify --memory-model instant` in a separate directory for full intermediate-snapshot verification; exclude its timings from performance comparisons. Reports retain case, seed, revision, clean-tree flag and DDR provenance. `*-equivalence.json` records the original chunk-mode audio/jobs/state check.

Rust targeted tests, 19 wire-contract tests, formatting and the warning-denying workspace build pass. The full workspace nextest run had 3,264 passes and one LAN-profile startup timeout; that test passes in isolation. Blocked standalone/audio/connected comparisons cover both AXI widths and all normal/odd/small profiles across native and realistic backends, partial blocks, nested ranges, restore, capacity refusal, cross-block recording, retake/reuse, faults and reset. Focused block retirement tests hold responses, pause dirty writeback, drain in-flight prefetch, verify cache invalidation and inject writeback errors. Scalar AXI, unaligned physical-base, tiny-cache and legacy diagnostics pass.
