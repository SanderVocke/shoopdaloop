# Chunk memory

This implements the [chunk-memory design](../../FPGA_CHUNK_MEMORY.md). The production Rust backend supplies logical audio/media behavior; cache and DMA
behavior is verified with independent hardware properties.
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

Resident reads pipeline RAM requests, returned samples and delivery. A valid bit
tracks the synchronous RAM output and another tracks the held output register;
each stage advances only when its successor has room. After pipeline fill, an
unstalled reader receives one sample per clock. Issued and consumed indices are
separate, so arbitrary stalls preserve data/last and completion follows the last
accepted sample. A line stays pinned throughout the stream. Shared write-pool
port arbitration also protects a pending RAM result while the output is stalled.

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

The production CPU path uses its prepared resource/page storage. It does not
model BRAM caches, physical chunk IDs or DMA timing.

## Verification

Full musical/state/media behavior is compared with the production float and
Q8.24 runtime via [common sessions](../verification/README.md). Static lowering
uses production signal and timing compilation. There is no software FPGA executor.
Standalone tests preserve hardware-specific protocol, configuration, stream,
cache, capacity, fault and reset checks without a duplicate looping backend.

```sh
python fpga/chunk_memory/simulate.py --config all --memory-model fixed --axi-data-width 64
```

The subsystem's `check.sh` retains lint and synthesis entry points. Current
coverage and local evidence are recorded in [the retirement ledger](../verification/MIGRATION.md)
and [subsystem gates](../verification/subsystem-gates.json). [Historical results](HISTORY.md)
retain the earlier reference-backed measurements; their commands are retired.
