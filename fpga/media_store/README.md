# Bounded RTL media store

This prototype mirrors Rust `MediaStore`: mono fixed-point chunks, shared read-only
ranges, per-slot references and deterministic LIFO reuse. Samples use an external
memory port. Small bounded metadata uses synchronous on-chip RAM.
`media_audio_pipeline` connects the store to state/job execution, enabling ReadView
and Capture alongside arithmetic jobs.

The memory port can attach to a DDR controller or cache. This milestone includes
an independently stalled simulation slave, without a vendor DDR controller or
burst/cache implementation. The Rust model remains elastic; RTL has physical limits.

## Structure and capacities

- `rtl/media_store.sv`: serial commands, allocation, ownership and range translation.
- `rtl/media_ram.sv`: synchronous metadata RAM, one read and one write port.
- `rtl/media_audio_pipeline.sv`: exclusive store ownership between jobs and host commands.
- `rtl/memory_delay.sv`: optional minimum response delay for each sample-memory word.
- `simulate.py`, `tests/`: Rust comparisons for standalone storage and complete audio quanta.
- `check.sh`: warning-clean lint and generic/ECP5 synthesis for both tops.

Defaults: 32 issued view IDs, 32 slots, 32 samples per chunk. Each view has a fixed
table block of `SLOTS` entries. This `VIEWS * SLOTS` metadata layout is deliberately
small and bounded; a large session needs a different table allocation/storage
implementation. Deleted view IDs remain holes and are not reissued within a reset
epoch. View exhaustion is an additional hardware refusal; Rust's table is elastic.

CHUNK_FRAMES is a compile-time power of two, `1..65536`. VIEWS/SLOTS can each be
`1..65536`, with the table address space fitting u32. Non-power-of-two view/slot
capacities are tested. Positions, lengths and external byte addresses are 64-bit;
samples, IDs and reference counts are 32-bit. Samples retain raw Q8.24 bits.

## Command interface

Commands transfer on `cmd_valid && cmd_ready`; operands and `arena_base` latch at
admission. `done` pulses once; `error_code` remains latched. One command executes
at a time. Configuration takes priority, and commands/configuration block while busy.

| Opcode | Operation | Operands | Result |
| --- | --- | --- | --- |
| 0 | Create | None | `result_view`: next recording ID |
| 1 | Slice | `cmd_view`: source, `cmd_start`, `cmd_length`: frames | `result_view`: read-only range ID |
| 2 | Clear | `cmd_view` | Retain ID/kind; release all references |
| 3 | Remove | `cmd_view` | Release references and invalidate ID |
| 4 | Reclaim | `cmd_view` | Release at most one tail reference; `result_empty` |
| 5 | Append | `cmd_view`, `cmd_count`: input words | Updated recording |
| 6 | Read | `cmd_view`, `cmd_start`: position, `cmd_length`: loop length, `cmd_count`: output words | Sample stream |

Slices copy IDs and retain references, with no sample-memory transactions. Nested
ranges retain their own references. Partial boundary chunks stay allocated while
reads remain bounded by logical frame ranges. Sources may append, clear or be
removed without changing retained slices. Read-only views reject append, including
empty append and append after clear.

Clear/remove release in table order. Reclaim releases from the tail and truncates
logical length, accounting for the first-chunk offset. Only a zero reference count
returns an ID to the free stack. Allocation pops that stack before growing the slot
high-water mark; storage does not shrink. Append preflights capacity before changing
media. New slots are zeroed, including unpublished tails, matching Rust snapshots.
Reused slots retain old unpublished samples. Descriptors/lengths publish at completion.
Zero-word reads still validate the view, position and loop length.

### Streams and memory

Append consumes `cmd_count` words on `write_valid && write_ready`. Read emits
`cmd_count` words on `read_valid && read_ready`. Valid/data remain stable while
stalled. A refused command terminates its stream via `done`.

`memory_valid && memory_ready` transfers one 32-bit read/write request. Addresses
are bytes: admitted base plus `(slot * CHUNK_FRAMES + offset) * 4`. The base must
be four-byte aligned and leave room for the physical arena without u64 overflow.
Request fields remain stable under backpressure. At most one request is outstanding;
both reads and writes require a response. The slave retains response valid/data/error
until `memory_response_ready`. No fixed response latency is assumed.

Commands are serialized and hold audio admission until completion. Command sizes
and external-memory service must be bounded before claiming an audio deadline.
This word interface establishes correctness; efficient DDR bursts and arbitration
remain separate work.

### Errors and reset

| Store code | Meaning |
| --- | --- |
| 0 | Completed |
| 1 | Invalid view or frame range |
| 2 | Invalid state, including append to read-only views |
| 3 | Slot or physical view capacity exhausted |
| 4 | Invalid opcode or arena base |
| 5 | External-memory error |

Invalid commands and capacity refusal preserve architectural media. A memory error
can leave partial writes/provisional allocation: the controller reports it and
refuses commands until reset. Internal ownership invariant failures also halt.
There is no retry/rollback protocol for these faults.

Synchronous active-high reset aborts work and clears logical counts/control while
retaining RAM/external samples. Old contents are inaccessible until descriptors
and slots initialize. Coordinate reset with the slave so an old response cannot
be mistaken for a new transaction. Reset begins a new store epoch.

## Metadata configuration

The idle-only configuration port transfers on `cfg_valid && cfg_ready`; responses
pulse in the following cycle. Data is 160 bits, with zeros above bit 31 in scalar
spaces. Write/error responses return zero. Full 32-bit addresses are checked before
narrowing. Load only trusted, Rust-validated images while execution is stopped;
this debug/loading port does not implement the complete snapshot validator.

| Space | Address | Value |
| --- | --- | --- |
| 0 | 0 / 1 / 2 / 3 | Issued views / slot high-water mark / free count / logical slot limit |
| 1 | View ID | Descriptor: valid bit 0, read-only bit 1, offset `[63:32]`, frames `[127:64]`, table count `[159:128]` |
| 2 | `view * SLOTS + ordinal` | Chunk ID |
| 3 | Slot ID | Reference count |
| 4 | Stack index | Chunk ID, top at `free_count - 1` |

Descriptor bits `[31:2]` are reserved. Logical slot limit defaults to SLOTS and
supports snapshots with smaller `chunk_limit`. Snapshots without a limit compare
identically while operations fit physical capacity. Restore must supply mutually
consistent descriptors, tables, counters and free entries.

## State/job integration

`media_audio_pipeline` enables the job processor's optional `ENABLE_MEDIA` path.
ReadView validates its destination and streams store output into scratch RAM.
Capture validates its source and streams scratch samples into the store. FIFO order
governs visibility when recording/playback share a view.

Host `store_cmd_*`, `store_cfg_*` and stream ports work between quanta. Host traffic
cannot interrupt jobs or steal memory between transactions. A pending host request
takes priority over another quantum. Program/sample configuration blocks during
store commands. Combined `busy` covers audio/store work; overall `done` is audio
completion and `store_done` reports host-command completion.

Media-enabled job codes add 5 (InvalidView), 6 (MemoryFull), 7 (InvalidState) and
8 (memory fault). Existing codes/stages retain their meanings. Failing jobs are
popped; later jobs remain for drain. Earlier state/sample effects survive. Read
destinations become ready only after the last sample. The ordinary BRAM-only top
defaults to `ENABLE_MEDIA=0` and continues refusing media jobs with code 3.

The wrapper forwards the shared loop-record read port and external-job completion
ports. The media tests use per-node playback/capture state. The separate connected
loop pipeline retains its BRAM-only sample path and ties off the unused media ports.

## Comparison and reproduction

Format-version-1 `media_case`/`media_trace` payloads expose serial commands separately
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

The separate `8x8` configuration has 64 views, 256 slots, chunk/quantum maximum 32,
program/state/FIFO depths 256/512/256, 256 scratch buffers and eight input/output
ports. Select it explicitly; `all` retains the smaller regression configurations.
With `--pipeline --config 8x8 --timing`, the workload is 64 simultaneous mono loop
playbacks, each with a constant 0.5 gain, grouped into eight tracks of eight loops.
Seven binary mixes per track feed one mono output per track: 192 instructions/jobs
per quantum (64 reads, 64 gains, 56 mixes, eight outputs), with 320 state entries.
Each loop has 96 samples in three distinct chunks. Staggered positions and four
successive quanta exercise chunk boundaries and loop wraparound. All media, state,
jobs, scratch samples and output samples compare exactly with Rust. This measures
playback/mixing, without recording or loop-transition processing.

```sh
python fpga/media_store/simulate.py --pipeline --config 8x8 --timing \
  --response-cycles 7 --output work/fpga-ddr-8x8/c7
```

The selected case is saved beside `8x8/memory-timing.json` for replay. Initial
program/state/media setup and per-quantum result readback are outside the measured
audio execution interval. This configuration enlarges RAM capacities, retaining
one state processor, one job processor and one outstanding sample-memory request.

## Initial measurements

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
board mapping and placement/routing. They include the shared loop/external-job
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
  --response-cycles 7 --output work/fpga-ddr-estimate/c7
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
