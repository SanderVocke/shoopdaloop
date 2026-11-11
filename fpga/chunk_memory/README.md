# Chunk memory

This implements the [chunk-memory design](../../FPGA_CHUNK_MEMORY.md). The Rust
functional implementation is `rt_fpga_reference::chunk_memory::ChunkMemory`.
Media metadata owns allocation, views and references; chunk memory owns raw
samples, buffering, coherence and external transfers. Arithmetic scratch stays
in the job processor.

The default simulation memory path now uses the shared synthesizable AXI
endpoint. See [AXI memory and DRAMsim3](../axi_memory/README.md) for detailed
timing, bus-width options, and the retained legacy word-port mode.

## Command and data interfaces

Commands transfer on `cmd_valid && cmd_ready`. Each carries a 32-bit `cmd_id`,
32-bit chunk ID and sample offset/count. Chunk IDs address physical slots in the
configured arena; the caller validates logical ownership. One demand command is
outstanding, independently of the background transfer engine.

| Opcode | Operation | Completion |
| --- | --- | --- |
| 0 | Read a span within one chunk | All requested samples delivered |
| 1 | Write a span within one chunk | All supplied samples buffered and logically visible |
| 2 | Prefetch one chunk | Hint consumed; residency is not promised |
| 3 | Fence | Earlier writes acknowledged by external memory; active transfer drained |
| 4 | Retire one allocation | Its transfers drained, dirty data written and cache entries invalidated |

Read/write operands satisfy `offset <= CHUNK_FRAMES` and
`count <= CHUNK_FRAMES-offset`. Empty spans validate their chunk and offset but
transfer no data. Fence ignores span/chunk operands; prefetch/retire ignore spans.
Retire retains backing samples, including unpublished tails needed on reuse.

Writes stream through `write_valid/ready/data`. Reads stream through
`read_valid/ready/data/last`. The accepted count defines the write payload length.
Results use `response_valid/ready/id/error`, retaining ID/status while stalled.
Read data/last and downstream requests remain stable under backpressure.

An accepted nonempty write reserves a matching or available write buffer. A full
dirty pool backpressures a different chunk's command until writeback frees capacity.
A write waits for required fills/writebacks of that chunk. A full overwrite can
supersede an active speculative read: old responses drain without writing RAM or
publishing a valid read line, whose buffer stays reserved until drainage finishes. Reads after
write completion see the new samples, whether or not DDR has been updated.

The independent `prefetch_valid/ready/chunk` input admits hints while a demand is
active. A single pending hint may be dropped by a fence or retirement. Neither
prefetch interface creates a demand dependency or reserves residency.

Error codes are 0 success, 1 invalid chunk, 4 invalid shape/base and 5 memory fault.
Invalid commands leave samples unchanged. A sticky `fault` reports failures after
a posted write has already completed. An active demand receives a failure unless
its completion is already held; completed writes cannot be retroactively failed.
Further commands require reset.

## Buffering and transfers

Read and write pools have separate sample RAMs and a shared associative directory.
Replacement is round-robin. Read lines remain pinned while samples are streamed.
Dirty write lines forward reads directly. Partial write misses fill the old chunk
before modification; full overwrites need no old-data fetch. Conflicting fills,
writes, writebacks and retirements are serialized by chunk identity.

The demand and transfer FSMs run concurrently. Demand fills have priority. Dirty
writeback runs after a quiet interval of `CHUNK_FRAMES+8` periods, or earlier for
buffer pressure, fence or retirement. The interval combines adjacent writes,
including fresh-slot zeroing and capture. Unclaimed write buffers can drain while
another receives samples. Prefetch uses spare transfer capacity. A transfer owns
the memory port for a complete chunk, bounding interference to one transfer at
this arbitration level. Latency also depends on memory and client backpressure;
these are not hard real-time guarantees.

With `BURST_MEMORY=1`, the DMA submits one descriptor per whole chunk through
`memory_valid/ready/write/address/count`; count is in 32-bit samples. Write data
uses `memory_write_valid/ready/data`, and read data uses
`memory_read_valid/ready/data`. A separate `memory_response_valid/ready/error`
acknowledges the complete transfer. Cache lines become loaded, or cease being
dirty, only after successful completion. A transfer error can arrive before all
samples, and aborts the DMA through the existing sticky-fault path.

Integrated AXI tops enable this mode by default. Their adapter packs two samples
per 64-bit beat, limits bursts to 16 beats, and splits at physical 4 KiB boundaries.
A 32-sample chunk takes two 16-beat bursts on a 32-bit bus, or one on a 64-bit bus.
Only one burst is outstanding, and chunks never interleave. Sample RAM remains
32 bits wide. Narrow transfers handle an upper word lane or a one-sample chunk.
The adapter drains failed read bursts before reporting the error; successful
write completion waits for every B response. No vendor controller is included.

Direct cores default to `BURST_MEMORY=0`, preserving the original acknowledged
word port. `--word-transfers` selects it through the AXI wrappers for comparisons;
`--memory-model legacy` selects the original word slave and delay floor.
With AXI, fences drain interface responses and guarantee visibility rather than
physical DRAM completion; the simulator has a separate native-model drain.

`arena_base` binds on the first valid command and remains fixed until reset. It
must be four-byte aligned and leave room for `SLOTS*CHUNK_FRAMES*4` bytes without
overflow. Each command latches its base before validation. Reset aborts requests
and invalidates tags while retaining RAM data. Reset/flush the downstream response
source together, and fence before planned reset if posted writes must be preserved.

Capacity parameters are `SLOTS`, power-of-two `CHUNK_FRAMES`, `READ_BUFFERS` and
`WRITE_BUFFERS`; defaults are 32/32/4/4. Samples use synchronous dual-port RAM.
Write-buffer fills and foreground writes share one physical write port. Normal
capacities infer BRAM; a single-word pool uses registers. Tags/control use registers.
Buffer handles are private; request IDs do not yet imply multiple demand execution.

## Media and full-loop integration

The media frontend resolves each view/range chunk span once and streams samples
through this service. Fresh slots receive a full zero write. Last-reference
release retires the chunk before publishing its ID on the reusable free stack.
Shared read-only views retain their ownership and sample semantics.

State-processor enqueue events feed a bounded hint FIFO. A separate media resolver
walks each actual read window through shared descriptor/table RAM ports, giving
foreground accesses priority. Next-job hints remain a fallback. Invalid or
unpublished hints are ignored, while actual reads keep their execution-time
lookup. State/job media positions and lengths use chunk units; the resolver
reuses the frame-based metadata interface below that boundary. No graph topology,
loop transitions or next-quantum prediction are involved. See
[early prefetch](../../FPGA_CHUNK_PREFETCH.md).

Both `media_audio_pipeline` and the transition-coordinated `loop_pipeline` use
this path for real recording/playback. The host media interface adds opcode 7,
Fence. Snapshots fence before inspecting backing memory; audio completion does
not wait for writeback. `memory_fault` propagates through both wrappers. The loop
coordinator stops work and reports stage 3/code 8 after quiescence. Late writeback
failure freezes clocks at their current value; a completed quantum is not rolled
back. Reset is required for recovery.

Rust uses an elastic arena with direct reads/writes. Prefetch is advisory and
fence/retire complete synchronously; no cache or handshake simulation is added to
the CPU path. MediaStore and the full Rust pipeline delegate through this component
while retaining the existing media snapshot format.
Replacing an external backing image requires draining and resetting cache tags
before loading samples/metadata. Fence alone does not invalidate clean read lines;
ordinary host edits use the media/chunk interfaces.

## Validation and reproduction

`chunk_case` / `chunk_trace` are additive format-version-2 payloads. Requests contain
an ID and tagged operation; results contain ID, status, read samples and logical
arena contents. Rust supplies expected values. Cocotb drives independently stalled
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

Verilator 5.032, cocotb 1.9.2 and Yosys 0.52; seeds 20261110/20261111:

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
