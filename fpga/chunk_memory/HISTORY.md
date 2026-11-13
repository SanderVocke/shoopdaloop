# Historical verification and measurements

These notes describe the retired FPGA Rust reference and earlier RTL revisions.
Their commands, case formats and tool names are historical and are not current
reproduction instructions. Use [the current README](README.md) and
[production verification](../verification/README.md). Numerical measurements retain
their original workload and revision assumptions.

## Validation and reproduction

`chunk_case` / `chunk_trace` are additive format-version-2 payloads. Requests contain
an ID and tagged operation; results contain ID, status, read samples and logical
arena contents. Rust supplies expected values. Python harness drives independently stalled
memory and sample streams, compares reads/statuses and checks complete backing
contents after fences. It also checks whole-chunk grouping on the memory port.

```sh
python fpga/chunk_memory/simulate.py
python fpga/chunk_memory/simulate.py --config small \
  --case fpga/chunk_memory/examples/ordering.json --waves
bash fpga/chunk_memory/check.sh
python fpga/media_store/simulate.py --pipeline --memory-model legacy --response-cycles 7
python fpga/loop_pipeline/simulate.py --media --memory-model legacy --response-cycles 7
python fpga/loop_pipeline/simulate.py --media --config small \
  --read-buffers 1 --write-buffers 1 --memory-model legacy --response-cycles 7
```

Independent profiles cover four-word, 32-word, odd-sized and one-word pools. Protocol
cases exercise posted completion with DDR paused, forwarding, partial writes, dirty
pool admission pressure, retirement during writeback, pinned reads, prefetch, held
completions, late faults and reset during a transfer. Full-loop tests compare
transition decisions, jobs, state, samples and media with Rust, including preflight,
seeks, shared ranges and recovery. A separate test completes a recording quantum
before releasing a failed writeback response, checking fault reporting without
clock rollback.

The 8x8 timing workload now stores 512 samples per loop (128 KiB total), exceeding
128 read buffers (16 KiB). It retains 64 gains, 56 mixes and eight outputs at Q=32:

```sh
python fpga/media_store/simulate.py --pipeline --config 8x8 --timing \
  --read-buffers 128 --memory-model legacy --response-cycles 7 --output work/chunk-8x8
```

Timing excludes setup, readback and snapshot fences. Backing transaction counts
include those verification drains. Cold and successive quanta compare independently
with Rust. Explicit position writes emulate external loop-clock advances in this
isolated benchmark; full-loop tests exercise the actual transition processor.
These are estimates at an assumed clock, without a measured board
frequency or worst-case DDR service bound.

## Verification checkpoint

Verilator 5.032, historical Python harness 1.9.2 and Yosys 0.52; seeds 20261110/20261111:

| Comparison | Cases | Requests or quanta |
| --- | ---: | ---: |
| Independent chunk service, four configurations | 20 | 1,708 serialized requests |
| Standalone media store, three configurations | 147 | 4,257 commands |
| State/job/media pipeline, three configurations | 147 | 546 quanta |
| Full loop/sample pipeline, three configurations | 118 | 1,682 quanta |
| Full loop/sample pipeline with one read and one write buffer | 19 | 196 quanta |
| External-job loop regression, three configurations | 98 | 1,239 quanta |
| BRAM job regression, two configurations | 172 | 507 quanta |

All compared results matched Rust. Separate protocol tests and waveform replay
passed. After correcting initial free-buffer placement to follow the replacement
ring, independent chunk tests were repeated in all four configurations; full-loop
sample comparisons were repeated for 58 cases/629 quanta and the single-buffer
profile. The 64-loop moving-position workload also matched Rust in all four quanta.

At 100 MHz with seven-cycle backing responses and 128 read buffers, that workload
takes 48,567 cycles (485.67 µs) cold and 42,301 cycles (423.01 µs) thereafter. It
issues 4,032 physical reads initially and 2,048 per subsequent quantum, so the
steady result includes new backing traffic. Each quantum has 1,408 state cycles
and 192 jobs. Setup, position updates, sample readback and fence cleanup are
outside these execution counts.

| ECP5 cell | Chunk test profile: 16 slots, CF32, R4/W4 | Full-loop default profile |
| --- | ---: | ---: |
| LUT4 | 4,623 | 21,755 |
| TRELLIS_FF | 1,087 | 12,485 |
| DP16KD | 3 | 86 |
| MULT18X18D | 0 | 4 |

The full-loop profile has eight loops, 16 request slots, 32 event slots and the
default bounded media capacities. These are synthesis estimates without a DDR
controller, board mapping or placement/routing. Generic checks exclude latches
and combinational division/modulo; ordinary, odd and minimum capacities passed.
The final media regression also rejects changed arena bases before allocating
slots, then verifies the original arena remains usable and unchanged.

Rust formatting, warning-denying workspace build and test-attribute checks passed,
as did 18 Python contract tests. The workspace run completed 3,267 tests: 3,266
passed, one application smoke test timed out waiting for MIDI detail, and 11 were
skipped. That smoke test passed in isolation on retry; no MIDI behavior was changed.
