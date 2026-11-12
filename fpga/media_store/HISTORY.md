# Historical verification and measurements

These notes describe the retired FPGA Rust reference and earlier RTL revisions.
Their commands, case formats and tool names are historical and are not current
reproduction instructions. Use [the current README](README.md) and
[production verification](../verification/README.md). Numerical measurements retain
their original workload and revision assumptions.

## Comparison and reproduction

Format-version-3 `media_case`/`media_trace` payloads expose serial commands separately
from the signal program. Cases contain `name`, initial `media`, and `steps`. Results
contain `status`, tagged `value` (none, view, empty flag, samples), and complete
`media`. Failed commands are recorded and subsequent commands execute; failed audio
quanta still stop. Invalid initial snapshots reject initialization.

Rust executes actual store methods. Python transports schema-checked values and
compares exact results, without reimplementing sample arithmetic/storage. The
driver independently reads RTL reference counters and compares them with ownership
derived from returned view tables. Resume cases restore holes, shared-only ownership,
partial tails and free-stack order. Combined `audio_case` tests compare state, jobs,
remaining FIFO entries, readiness, scratch/output samples and media.

Use the repository Cargo environment, Verilator, Make and a C++ compiler:

```sh
python3 -m venv work/fpga-sim-venv
. work/fpga-sim-venv/bin/activate
python -m pip install -r fpga/media_store/requirements.txt
python fpga/media_store/simulate.py
python fpga/media_store/simulate.py --pipeline
python fpga/media_store/simulate.py --config small \
  --case fpga/media_store/examples/shared_ranges.json --waves
python fpga/media_store/simulate.py --pipeline --config small \
  --case scripts/fpga_testbench/examples/shared_ranges.json --waves
bash fpga/media_store/check.sh
```

The `all` regression configurations are default (32 views, 32 slots, chunk/quantum maximum 32), small
(16 views, 8 slots, chunk/quantum maximum 4), and odd (5 views, 5 slots,
chunk/quantum maximum 8). Program/state/FIFO sizes are 32/16/32 for default and
16/8/16 otherwise. Options include `--config`, `--seed`, `--random-cases`, `--case`,
`--waves` and `--output`. Failure artifacts include case, expected/observed results
and timing. Logs, xUnit reports, summaries and optional waveforms go under `work/`.
Synthesis checks forbid latches and combinational division/modulo.

The separate `8x8` configuration has 64 views, 1,024 slots, chunk/quantum maximum 32,
program/state/FIFO depths 256/512/256, 256 scratch buffers and eight input/output
ports. Select it explicitly; `all` retains the smaller regression configurations.
With `--pipeline --config 8x8 --timing`, the workload is 64 simultaneous mono loop
playbacks, each with a constant 0.5 gain, grouped into eight tracks of eight loops.
Seven binary mixes per track feed one mono output per track: 192 instructions/jobs
per quantum (64 reads, 64 gains, 56 mixes, eight outputs), with 320 state entries.
Each loop has 512 samples in sixteen distinct chunks. Aligned, staggered chunk
positions advance across four quanta, including loop wraparound. All media, state,
jobs, scratch samples and output samples compare exactly with Rust. This measures
playback/mixing, without recording or loop-transition processing.

```sh
python fpga/media_store/simulate.py --pipeline --config 8x8 --timing \
  --read-buffers 128 --memory-model legacy --response-cycles 7 --output work/chunk-8x8
```

The selected case is saved beside `8x8/memory-timing.json` for replay. Initial
program/state/media setup and per-quantum result readback are outside the measured
audio execution interval. This configuration enlarges RAM capacities, retaining
one state processor, one job processor and one outstanding sample-memory request.
The 128 KiB sample fixture exceeds the selected 16 KiB read pool; cold fills and
steady access are both measured. Older timing tables below used the smaller
96-sample loops and the direct-memory implementation.

### Within-quantum prefetch opportunity

Add `--prefetch-analysis` to record the aligned playback state
job emission, job execution, chunk-fill waits and physical transfers:

```sh
python fpga/media_store/simulate.py --pipeline --config 8x8 --timing \
  --prefetch-analysis --read-buffers 128 --memory-model legacy --response-cycles 7 \
  --output work/prefetch-opportunity-c7
```

`prefetch-analysis.json` contains the measured timeline and an optimistic replay.
The replay serializes chunk transfers at their measured service rate, releases
each request only after its state-processor emission and eight cycles of serial
metadata resolution, and preserves execution time outside chunk-fill waits.
Every replay starts with no resident chunks. It assumes enough hint buffering
to retain all 64 intents and enough read buffers to avoid eviction. The diagnostic
observes the current RTL and also produces a replay estimate.

Before implementing early hints, four successive quanta at each service rate
matched Rust. These baseline measurements and replay estimates use 100 MHz:

| Word response delay | Shallow-prefetch baseline | Waiting for fills | Early-prefetch replay | Remaining replay wait |
| --- | ---: | ---: | ---: | ---: |
| 7 cycles | 355.47 µs | 65.51 µs | 289.96 µs | 0 µs |
| 13 cycles | 478.35 µs | 188.39 µs | 292.78 µs | 2.82 µs |

Each quantum fetches 64 distinct chunks, or 2,048 words, with zero writes and no
reuse from earlier quanta. At seven cycles, a complete transfer costs 257 cycles
including dispatch: 164.48 µs for all chunks, against 289.96 µs of measured work
outside fill waits. The replay finishes all transfers by 164.66 µs and meets every
read deadline. The potential reduction from current aligned RTL is 18.4%; at
13 cycles it is 38.8%. With 25-cycle responses, the replay takes 538.54 µs and
retains 248.58 µs of exposed wait: bandwidth eventually limits the overlap.

These estimates support intra-iteration hiding for this playback/gain/mix graph.
They do not establish the same overlap for dependent capture/read chains, dropped
hints, DDR-backed metadata or a future faster arithmetic engine. The original
timing fixture uses unaligned positions, so compare these numbers within this
aligned experiment. The word-delay model remains a rough service-rate estimate.

### Implemented early prefetch

The implemented state-enqueue FIFO and separate resolver achieve the following
actual RTL results at the same assumed 100 MHz, with 128 read buffers:

| Response delay | Total | Exposed fill wait | Reduction from aligned baseline |
| --- | ---: | ---: | ---: |
| 7 cycles | 289.96 µs | 0 µs | 18.4% |
| 13 cycles | 292.93 µs | 2.97 µs | 38.8% |

All four moving quanta at each setting matched Rust and fetched exactly 2,048
new words, with no writes or reuse from earlier iterations. These results include
1,408 state cycles and 192 jobs per quantum. The 13-cycle result is 0.15 µs above
the optimistic replay estimate. No whole-iteration prefetch barrier is used.

Separate resolver tests compare 483 requests across three configurations. Chunk
coherence tests cover queued/completed/in-flight supersession, delayed stale
responses, repeated writes and retirement/reuse. Pipeline comparisons include
capture/publication/read dependencies, multi-chunk wrap and single-buffer pressure.
The default full-loop synthesis profile (eight loops, four read/write buffers)
uses 25,357 LUT4s, 13,764 FFs, 93 DP16KDs and four DSPs on ECP5. Lint and generic
checks pass for ordinary and minimum configurations, without latches or division.
These resource counts describe the default synthesis profile, not the larger
64-loop timing fixture, and exclude a DDR controller and place/route.

The warning-denying workspace build, targeted Rust tests, 19 Python contract tests
and RTL comparisons passed. The remaining full-workspace test run was stopped at
the user's request before pushing; PR CI was not inspected.

## Initial measurements

The following direct-memory measurements precede chunk-memory integration
(commit `59186a5f`). Current interfaces and validation are described in
[chunk memory](../chunk_memory/README.md).

Verilator 5.032 / cocotb 1.9.2, seed 20261110; Yosys 0.52, default capacities:

| Behavioral comparison | Cases | Compared commands/quanta |
| --- | ---: | ---: |
| Standalone store, three configurations | 147 | 4,257 commands |
| State/job/store pipeline, three configurations | 147 | 546 quanta |
| Existing BRAM pipeline regression, two configurations | 432 | 1,287 quanta |

All matched Rust bit for bit. Separate protocol tests cover configuration bounds,
view exhaustion, blocked host traffic, held samples/requests, high address bits,
base/operand latching, memory errors and reset/restart. Both single-case replay
paths passed with waveform recording. The reference Rust tests, 17 Python contract
tests and warning-denying workspace build passed. The complete workspace Rust
suite passed before pushing the shared-range Rust change; it was not repeated
after the RTL/testbench additions.

| ECP5 cell | Store | State/job/store pipeline |
| --- | ---: | ---: |
| LUT4 | 2,963 | 5,847 |
| TRELLIS_FF | 955 | 3,656 |
| DP16KD | 9 | 12 |
| MULT18X18D | 0 | 4 |

These synthesis estimates exclude external samples, a physical DDR controller,
board mapping and placement/routing. They include the shared loop
interfaces and use the default one-cycle response setting.

With requests always accepted and responses on the next clock: a two-chunk slice
takes 15 periods, clearing three chunks 20, and creating an empty recording two.
Those commands issue no sample requests. Appending 96 samples into three freshly
zeroed 32-sample slots takes 684 periods. Counts exclude configuration/readback.

The complete Q=32 Input/Gain/Capture/Playback/Output example takes 725 periods for
initial recording, 626 for playback and 662 for recording into recycled slots.
These include state/sample execution and modeled memory responses, excluding input
loading and readback. They are behavioral counts, not an achieved clock frequency
or a service bound for a real DDR device.

## Rough DDR3 timing estimate

The numerical tables in this section describe the direct-memory baseline above.
The delay model now sits below chunk memory and applies to physical transactions.

The [PYNQ-Z1](https://digilent.com/shop/pynq-z1-python-productivity-for-zynq-7000-arm-fpga-soc/)
has a 16-bit DDR3 interface at 1050 MT/s: 2.1 GB/s raw peak bandwidth. For an initial
estimate, choose a handicap H covering short 32-sample chunks, controller overhead,
read/write changes, other memory users and imperfect scheduling. These are selected
assumptions, not measured efficiency factors for this board.

For a 32-bit sample word and a 100 MHz fabric clock:

```
effective_bytes_per_second = 2_100_000_000 / H
response_cycles = ceil(4 * 100_000_000 / effective_bytes_per_second)
```

| Handicap H | Assumed bandwidth | Cycles/word | Quantized service ceiling |
| ---: | ---: | ---: | ---: |
| 16 | 131.25 MB/s | 4 | 100 MB/s |
| 32 (baseline) | 65.625 MB/s | 7 | 57.14 MB/s |
| 64 | 32.8125 MB/s | 13 | 30.77 MB/s |

`MEMORY_RESPONSE_CYCLES` on `media_audio_pipeline` defaults to 1, preserving the
fast-slave baseline. The shim forwards one request and holds its acknowledgement
until this many clock periods have elapsed. It delays both reads and writes and
respects longer downstream stalls, response backpressure and errors. The value is
the minimum total response time, rather than additional cycles beyond a response.
Reset must also reset/flush the downstream slave, as for the store itself.

The current store issues only one word at a time. Its own FSM cycles, metadata
lookups and job arithmetic add to the service time. Thus the bandwidth column is
a ceiling for the modeled memory service, not the complete processor's sustained
bandwidth. Samples pass through this shim; on-chip metadata and scratch accesses do
not. A future DDR-backed chunk-table implementation would need to include its
transactions too. There is no modeling of bursts, bank/row state, refresh or latency
tails, and these estimates do not establish a hard real-time guarantee.

Run the deterministic timing case, with Rust comparisons enabled:

```sh
python fpga/media_store/simulate.py --pipeline --config default --timing \
  --memory-model legacy --response-cycles 7 --output work/fpga-ddr-estimate/c7
```

`default/memory-timing.json` records total/state/job cycle counts and memory
read/write counts. The test disables random stalls, checks every response takes
exactly the configured delay, and compares all architectural results with Rust.
Omit `--timing` to run the normal directed, randomized and protocol tests with the
same delay. At 100 MHz each measured cycle represents 10 ns; this clock is an
assumption, not a placement/routing result.

Measured complete quanta for the mono Q=32 Input/Gain/Capture/Playback/Output case:

| Memory profile | Fresh recording | Playback | Recording into reused slots |
| --- | ---: | ---: | ---: |
| Fast slave, 1 cycle | 7.25 µs | 6.26 µs | 6.62 µs |
| H=16, 4 cycles | 9.17 µs | 7.22 µs | 7.58 µs |
| H=32, 7 cycles | 11.09 µs | 8.18 µs | 8.54 µs |
| H=64, 13 cycles | 14.93 µs | 10.10 µs | 10.46 µs |

Fresh recording issues 64 writes (32 zero-fill plus 32 samples); reused recording
issues 32 writes; playback issues 32 reads. Each extra service cycle adds exactly
one period per transaction. At H=32, the job processor alone takes 10.78 / 7.84 /
8.23 µs respectively; the table also includes state execution and wrapper cycles.
Input/configuration loading, control actions between quanta and result readback
are excluded. These are single-path measurements, not a full-session estimate.
For context, Q=32 at 48 kHz has a 666.67 µs audio deadline.

All four timing profiles matched Rust bit for bit. The normal directed/randomized
pipeline comparisons and protocol tests also passed with 7-cycle responses, and
with 4-cycle responses where the independently stalled slave can take longer than
the configured minimum. Warning-clean lint and generic/ECP5 synthesis passed.

### 64-loop playback at Q=32

The `8x8` timing workload described above gives these results at the assumed
100 MHz clock. Each of four successive quanta has identical timing within its
profile, with 192 jobs, 2,048 sample reads (8 KiB) and no sample writes.

| Memory profile | State cycles | Job cycles | Total cycles | Time | Q=32/48 kHz deadline used |
| --- | ---: | ---: | ---: | ---: | ---: |
| Fast slave, 1 cycle | 1,408 | 31,297 | 32,708 | 327.08 µs | 49.1% |
| H=16, 4 cycles | 1,408 | 37,441 | 38,852 | 388.52 µs | 58.3% |
| H=32, 7 cycles | 1,408 | 43,585 | 44,996 | 449.96 µs | 67.5% |
| H=64, 13 cycles | 1,408 | 55,873 | 57,284 | 572.84 µs | 85.9% |

Totals include three wrapper cycles in addition to state and job execution.
The baseline H=32 profile leaves 216.71 µs of the 666.67 µs quantum; H=64 leaves
93.83 µs. Each extra memory service cycle adds 2,048 periods (20.48 µs). The fast
slave comparison indicates that processor execution is still a substantial part
of the cost, independently of the selected DDR penalty.

All 16 measured quanta match Rust bit for bit. The existing mono timing case also
retains its previous results. These measurements cover 64 simultaneously playing
mono loops, including gains/mixing into eight mono track outputs. Chunk metadata
remains on chip; DDR-backed tables, longer loops, recording, transition execution,
I/O transport and an achieved board clock require separate evaluation.
