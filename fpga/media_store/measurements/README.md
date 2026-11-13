# DDR metadata measurements

This is the one-chunk ownership baseline. The follow-up [32-chunk block comparison](blocks32/README.md) preserves these measurements and reports the new allocation policy.

All measured quanta compare jobs, loop/node state, scratch/output samples and statuses with Rust; final fenced media and independently derived reference counts also match. Separate native replays compare complete media after every quantum. Rust storage/arithmetic semantics are unchanged.

The primary experiment assumes 100 MHz fabric, Q=32 at 48 kHz (666.67 µs budget), 32-sample chunks, AXI64, 128 sample read buffers, four write buffers, 128 table lines, four reference lines and two stack lines. It uses the pinned 512 MiB DDR3-1066 x16 DRAMsim3 profile. These are behavioral clock estimates, with no placement/routing guarantee.

## Workloads and observation

| Workload | Loops / views / slots | Tables | Quanta |
| --- | --- | --- | ---: |
| Original | 64 / 64 / 1,024 | 16 IDs per view, 597-page pool | 4 |
| Streaming | 64 / 64 / 4,096 | 64 IDs per view, 802-page pool | 24 |
| Mixed | 64 / 72 / 2,048 | recording, sharing, reuse; 729-page pool | 24 |

The original graph has eight tracks of eight mono loops, 64 gains, 56 mixes and eight outputs: 192 jobs per quantum. Its saved fixture is unchanged. Streaming retains that graph, starts at staggered positions, seeks all loops at quanta 8/16, crosses page/line boundaries and wraps; the total metadata working set exceeds cache capacity.

Mixed adds one input and 64 logical captures. Loops 0–31 play and 32–63 record, initially with both existing and empty recordings. Eight retained ranges share old chunks. A 32-ID free stack is consumed before fresh allocation. Quantum 12 switches the recording half to playback; quantum 13 records again, automatically clearing/reusing the views. Eight bounded host Reclaim commands on retained ranges run after the measured sequence and are reported separately. Upstream automatic BeginCapture puts retake cleanup inside job execution; it is not a hidden host preparation phase.

Setup fences existing writes and invalidates metadata once; sample caches start cold. Measured runs perform no intermediate media snapshots, table walks, fences or invalidations. Logical samples/state/jobs remain checked every quantum. Full-snapshot replays use the same cases separately and are excluded from performance tables. The original verification report retains an instrumentation-only dirty flag; no Rust/RTL edits occurred during these verification replays.

Counters stop at the RTL done pulse. Client burst counts/bytes count scheduler-issued commands (a final command may still be in flight); axi_reads/writes count slave-accepted requests. Cache operations/useful bytes, directory reads, fills/writebacks, arbitration wait, foreground metadata stalls, service busy time and physical DDR activity are recorded separately. Logical byte counts cover requested table/count/stack words; directory words and internal page allocation/zeroing are additional. Overlapping memory costs must not be added to processor totals.

## Timing

| Original fixture | Before DDR metadata | DDR metadata |
| --- | ---: | ---: |
| Startup | 139.55 µs | 166.53 µs |
| Warm final quantum | 123.83 µs | 131.74 µs |

Warm playback is about 6.4% slower, even with zero metadata DDR reads: coherent variable-latency lookup adds foreground control work. Cold startup fetches table/directory lines and exposes more lookup latency. Physical DDR bandwidth is not saturated. The bounded caches trade a modest playback cost for capacity-independent on-chip storage.

| Workload / quantum | TP Settle done | SP done | JP done | TP Finish / total | TP / SP / JP busy |
| --- | ---: | ---: | ---: | ---: | --- |
| original / 0 | 23.76 | 35.29 | 165.82 | 166.53 | 24.45 / 11.52 / 130.51 |
| original / 3 | 8.04 | 19.57 | 131.03 | 131.74 | 8.73 / 11.52 / 111.44 |
| streaming / 0 | 23.76 | 35.29 | 166.09 | 166.80 | 24.45 / 11.52 / 130.78 |
| streaming / 8 | 23.76 | 35.29 | 165.28 | 165.99 | 24.45 / 11.52 / 129.97 |
| streaming / 16 | 23.76 | 35.29 | 164.97 | 165.68 | 24.45 / 11.52 / 129.66 |
| streaming / 23 | 7.83 | 19.36 | 131.87 | 132.58 | 8.52 / 11.52 / 112.49 |
| mixed / 0 | 23.76 | 39.17 | 409.38 | 410.09 | 24.45 / 15.40 / 370.19 |
| mixed / 1 | 7.76 | 22.85 | 181.74 | 182.45 | 8.45 / 15.08 / 158.87 |
| mixed / 12 | 15.76 | 30.53 | 142.93 | 143.64 | 16.45 / 14.76 / 112.38 |
| mixed / 13 | 15.76 | 31.17 | 737.34 | 738.05 | 16.45 / 15.40 / 706.15 |
| mixed / 23 | 7.76 | 22.85 | 231.35 | 232.06 | 8.45 / 15.08 / 208.48 |

All times above are microseconds from run acceptance; busy times are durations. Accepted-run-to-done and active intervals are identical here. Full per-quantum clock counts/timestamps are in the timing JSON files.

| Workload | Min / max quantum | Worst budget use | Final fence | Host simulation |
| --- | --- | ---: | ---: | ---: |
| original | 131.74 / 166.53 µs | 25.0% | 1.60 µs | 103.00 s |
| streaming | 132.21 / 166.80 µs | 25.0% | 1.60 µs | 309.24 s |
| mixed | 143.64 / 738.05 µs | 110.7% | 40.60 µs | 439.35 s |
| mixed, reference=32 | 143.57 / 430.68 µs | 64.6% | 49.07 µs | 347.01 s |

Host seconds include setup/readback/oracle work, exclude compilation and were collected with concurrent simulations on a five-core host. They are provenance, not a backend speed comparison. Initial setup, cold-cache preparation and final snapshot cycles are separate JSON fields. Overall service statistics include setup, snapshots, cleanup and the final native DDR drain; per-quantum statistics use only the execution window.

### Retake and reference-cache sizing

The simultaneous retake remains synchronous: it releases old chunk ownership, updates counts/free IDs, retires last-reference sample transfers and appends new data. Tail trimming now exploits dense-prefix vectors instead of scanning every parent entry. That reduced the four-reference-line peak from 81,281 to 73,805 clocks. A long cleanup still has no hard real-time bound; bounded Reclaim is available for incremental preparation.

| Mixed quantum 13 | Four reference lines | 32 reference lines |
| --- | ---: | ---: |
| Total | 738.05 µs | 430.68 µs |
| Reference fills | 384 | 0 |
| Reference writebacks | 384 | 0 |
| Metadata transferred bytes | 51840 | 2688 |
| Metadata arbitration wait clocks | 2914 | 78 |

The primary four-line profile exceeds the quantum budget at the simultaneous retake (738.05 µs). With 32 reference lines that retake takes 430.68 µs; the worst measured quantum uses 64.6% of the budget. This is measured headroom for this sequence, not a bound for arbitrarily long takes.

This secondary run changes only reference cache capacity (256 bytes to 2 KiB of data). It preserves exact Rust results and the primary four-line measurements. It tests the observed count-line eviction pressure rather than assuming more DDR bandwidth will solve control/lookup latency.

## Traffic and stalls

| Workload / quantum | Sample read/write bursts | Metadata read/write bursts | Metadata bytes | Logical read/write bytes | Table hit/miss | Ref fill/writeback | Stack fill/writeback | Metadata foreground stall clocks | Sample fill wait clocks |
| --- | --- | --- | ---: | --- | --- | --- | --- | ---: | ---: |
| original / 0 | 64/0 | 223/0 | 14272 | 768/0 | 97/223 | 0/0 | 0/0 | 2698 | 0 |
| original / 3 | 64/0 | 0/0 | 0 | 768/0 | 192/0 | 0/0 | 0/0 | 791 | 0 |
| streaming / 8 | 64/0 | 136/0 | 8704 | 768/0 | 152/136 | 0/0 | 0/0 | 2584 | 60 |
| streaming / 23 | 64/0 | 12/0 | 768 | 768/0 | 188/12 | 0/0 | 0/0 | 896 | 0 |
| mixed / 0 | 32/32 | 175/25 | 12800 | 2688/2240 | 672/192 | 16/12 | 15/13 | 20840 | 0 |
| mixed / 13 | 32/32 | 405/405 | 51840 | 3712/3328 | 1024/0 | 384/384 | 21/21 | 53220 | 0 |
| mixed / 23 | 32/32 | 34/32 | 4224 | 640/256 | 160/0 | 32/32 | 2/0 | 7197 | 0 |

Warm original playback performs 192 coherent ID queries (early hints, fallback hints and demand) but no DDR metadata reads; cached per-view leaf translations avoid directory rereads. Cold startup has 223 line fills, including replacement rereads of directories. Streaming seeks bring nonresident leaves/directories back; ordinary sequential accesses reuse retained leaf translations. Mixed recording exercises reference and free-stack fills/writebacks. The small reference cache can repeatedly evict strided ownership lines; the sizing experiment quantifies that cost.

Sample/metadata arbitration alternates ready clients at complete burst boundaries. Wait counters measure contention separately from line-fill latency; physical data-bus utilization in each raw_ddr_bus object includes actual overlapping sample/metadata service. No starvation or hanging dependency was observed in the contention/backpressure tests.

## Resources and scaling

Yosys 0.52 ECP5 synthesis; Verilator 5.032 lint; no place/route. The integrated profile is eight loop slots, 16 requests, 32 views, 32 sample slots, Q/chunk=32, four sample read/write buffers, AXI32, and default 8/4/2 metadata lines. It is smaller than the 64-loop performance profile. Counts exclude a physical DDR controller and simulation RAM.

| Integrated cell | Before | DDR metadata | Delta |
| --- | ---: | ---: | ---: |
| LUT4 | 24691 | 29265 | +4574 |
| TRELLIS_FF | 13716 | 16142 | +2426 |
| DP16KD | 91 | 94 | +3 |
| MULT18X18D | 4 | 4 | +0 |

The standalone metadata capacity check holds 128 views and cache sizes fixed at 8/4/2 lines:

| Sample slots | Generic memory bits | LUT4 | FF | DP16KD |
| ---: | ---: | ---: | ---: | ---: |
| 4,096 | 24,736 | 5,404 | 2,676 | 7 |
| 5,760,000 | 25,024 | 5,765 | 2,676 | 7 |

Recording capacity grows external tables/counts/stack, not on-chip arrays. The generic traversal-path storage grows by 288 bits when tree depth grows from three to six; mapped metadata BRAM remains seven blocks. Roots/descriptors scale with views, and cache data scales with explicit line parameters. The 64-stereo/30-second case needs 703.125 MiB of samples plus metadata and exceeds the unchanged 512 MiB timing profile; it is an address/synthesis audit, not a full-duration behavioral run.

## Reproduction

Activate Cargo, the Python harness environment and Verilator; build the pinned timing library. From the repository root:

```sh
python fpga/axi_memory/build_dramsim3.py
for workload in original streaming mixed; do
  python fpga/loop_pipeline/simulate.py --media --config 8x8 --timing \
    --timing-workload "$workload" --read-buffers 128 --write-buffers 4 \
    --axi-data-width 64 --metadata-lines 128 --reference-lines 4 --stack-lines 2 \
    --memory-model dramsim3 --fabric-hz 100000000 --output "work/replay-$workload"
done
```

For the sizing experiment use mixed and `--reference-lines 32`. For full intermediate verification add `--timing-verify`, select `--memory-model instant`, and use a separate output directory; its timing is not comparable. All cases use seed 20261110. Reports retain source revision, dirty flag, profile/library/drain-patch hashes and DRAMsim3 revision.

For the before measurement create a worktree at `4b2fe897`, apply `baseline-observation.patch`, and share or recreate the tool environment/target directory. Run its original `--media --config 8x8 --timing --read-buffers 128 --write-buffers 4 --axi-data-width 64 --memory-model dramsim3` command, without metadata options. The stored dirty flag records only those three observation-harness file changes; RTL and Rust are the baseline. `execution-timing.json` retains the earlier full-snapshot baseline; `baseline-observation.json` uses the matched no-intermediate-snapshot policy. Both yield identical original-fixture cycles.

```sh
python fpga/media_store/simulate_metadata.py
bash fpga/media_store/check_metadata.sh
bash fpga/media_store/check.sh
bash fpga/axi_memory/check_pipeline.sh
```

Validation includes both AXI widths, native SV and DRAMsim3, standalone media/audio/full loops, odd/minimum cache capacities, word and legacy diagnostics, stereo views on one loop, range sharing, wrap/capture forwarding, page/free-ID reuse, coordinated reset, and injected fill/late-writeback faults in every metadata bank. Lint/generic synthesis reject latches and unintended division/modulo; generated wrappers are checked against their generator.
