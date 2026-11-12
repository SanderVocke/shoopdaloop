# Bounded RTL media store

The current allocator owns groups of 32 chunks; see
[block allocation, sharing and interfaces](BLOCK_ALLOCATION.md). Sample transfers
remain chunk-sized. Legacy one-chunk regression runs and blocked runs are both
available; select `--block-chunks 32` for the current allocation geometry.

This hardware service implements mono fixed-point chunks, shared read-only
ranges, per-slot references and deterministic LIFO reuse. Samples pass through
[chunk memory](../chunk_memory/README.md) to the external memory port. Small
view descriptors use synchronous on-chip RAM. Chunk-ID vectors, reference counts
and the reusable-ID stack use DDR-backed persistent caches; see
[the metadata architecture](DDR_METADATA.md).
`media_audio_pipeline` connects the store to state/job execution, enabling ReadView
and Capture alongside arithmetic jobs.

Chunk memory supplies read/write buffering, forwarding, prefetch and chunk-level
arbitration and AXI chunk bursts. The memory port can attach to a DDR controller;
a vendor controller remains future work. The CPU and FPGA storage layouts/capacities are independent.

The default simulation memory path now uses the shared synthesizable AXI
endpoint. See [AXI memory and DRAMsim3](../axi_memory/README.md) for detailed
timing, bus-width options, and the retained legacy word-port mode.

## Structure and capacities

- `rtl/media_store.sv`: serial commands, allocation, ownership and range translation.
- `rtl/media_ram.sv`: synchronous metadata RAM, one read and one write port.
- `rtl/media_metadata.sv`: compact radix-indexed vectors and serialized ownership metadata.
- `rtl/metadata_cache.sv`: persistent writeback cache used by tables, counts and free IDs.
- `rtl/media_audio_pipeline.sv`: exclusive store ownership between jobs and host commands.
- `rtl/memory_delay.sv`: optional minimum completion delay for a memory transfer.
- `simulate.py`, `tests/`: standalone ownership/protocol checks and complete sample quanta.
- `check.sh`: warning-clean lint and generic/ECP5 synthesis for both tops.

Defaults: 32 issued view IDs, 32 slots, 32 samples per chunk. Each view allocates
only the table/directory pages it uses from a bounded DDR page pool. Cache sizes
are independently configurable and persist across quanta. Deleted view IDs remain holes and are not reissued within a reset
epoch. View exhaustion is a hardware capacity refusal; native resources are sized separately.

CHUNK_FRAMES is a compile-time power of two, `1..65536`. VIEWS/SLOTS can each be
positive, with VIEWS at most 65536 and the packed table address fitting u32. Non-power-of-two view/slot
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
| 7 | Fence | None | Earlier writes acknowledged by backing memory |

Slices copy IDs and retain references, with no sample-memory transactions. Nested
ranges retain their own references. Partial boundary chunks stay allocated while
reads remain bounded by logical frame ranges. Sources may append, clear or be
removed without changing retained slices. Read-only views reject append, including
empty append and append after clear.

Clear/remove release in table order. Reclaim releases from the tail and truncates
logical length, accounting for the first-chunk offset. Only a zero reference count
retires the allocation before returning its ID to the free stack. Allocation pops that stack before growing the slot
high-water mark; storage does not shrink. Append checks capacity before changing
media. New slots are zeroed, including unpublished tails, matching Rust snapshots.
Reused slots retain old unpublished samples. Descriptors/lengths publish at completion.
Zero-word reads still validate the view, position and loop length.

### Streams and memory

Append consumes `cmd_count` words on `write_valid && write_ready`. Read emits
`cmd_count` words on `read_valid && read_ready`. Valid/data remain stable while
stalled. A refused command terminates its stream via `done`. Append completes when
samples are buffered and visible; use Fence before inspecting backing DDR.

The frontend resolves and streams whole spans within each chunk. Job-FIFO logical
read hints and upcoming spans feed the chunk prefetch interface; invalid hints
are ignored. `READ_BUFFERS` and `WRITE_BUFFERS` default to four each.

`memory_valid && memory_ready` transfers one 32-bit read/write request. Addresses
are bytes: admitted base plus `(slot * CHUNK_FRAMES + offset) * 4`. The base must
be four-byte aligned and leave room for the physical arena without u64 overflow.
Request fields remain stable under backpressure. At most one request is outstanding;
both reads and writes require a response. Transfers cover entire chunks and may
continue after command/audio completion. The slave retains response valid/data/error
until `memory_response_ready`. No fixed response latency is assumed. The chunk
service binds the arena base for its reset epoch.

Commands are serialized and hold audio admission until completion. Command sizes
and external-memory service must be bounded before claiming an audio deadline.
Chunk arbitration is internal; integrated AXI tops transfer chunks in bursts.

### Errors and reset

| Store code | Meaning |
| --- | --- |
| 0 | Completed |
| 1 | Invalid view or frame range |
| 2 | Invalid state, including append to read-only views |
| 3 | Slot, physical view or metadata page capacity exhausted |
| 4 | Invalid opcode or arena base |
| 5 | External-memory error |

Invalid commands and capacity refusal preserve architectural media. A memory error
can leave partial writes/provisional allocation: the controller reports it and
refuses commands until reset. Internal ownership invariant failures also halt.
There is no retry/rollback protocol for these faults.

`memory_fault` also reports writeback errors after logical completion. The full
loop coordinator contains those faults without rolling back completed clocks.

Synchronous active-high reset aborts work and clears logical counts/control while
retaining RAM/external samples. Fence first if buffered writes must survive reset.
Old contents are inaccessible until descriptors
and slots initialize. Coordinate reset with the slave so an old response cannot
be mistaken for a new transaction. Reset begins a new store epoch.

## Metadata configuration

The idle-only configuration port transfers on `cfg_valid && cfg_ready`; responses
pulse after the variable-latency metadata operation. Data is 160 bits, with zeros above bit 31 in scalar
spaces. Write/error responses return zero. Full 32-bit addresses are checked before
narrowing. Load only trusted, checked images while execution is stopped;
this debug/loading port does not implement the complete snapshot validator.

| Space | Address | Value |
| --- | --- | --- |
| 0 | 0 / 1 / 2 / 3 | Issued views / slot high-water mark / free count / logical slot limit |
| 1 | View ID | Descriptor: valid bit 0, read-only bit 1, offset `[63:32]`, frames `[127:64]`, table count `[159:128]` |
| 2 | `(view << SLOT_BITS) \| ordinal` | Chunk ID; SLOT_BITS = max(1, clog2(SLOTS)) |
| 3 | Slot ID | Reference count |
| 4 | Stack index | Chunk ID, top at `free_count - 1` |
| 5 | 0, write | Fence and invalidate metadata caches/leaf translations |

Descriptor bits `[31:2]` are reserved. Logical slot limit defaults to SLOTS and
supports snapshots with smaller `chunk_limit`. Snapshots without a limit compare
identically while operations fit physical capacity. Restore must supply mutually
consistent descriptors, tables, counters and free entries.
Write each view's table as a dense ordinal prefix during restore. Reads beyond
that prefix and sparse writes are refused. Space 5 is a cold-cache diagnostic;
normal quantum boundaries do not flush metadata.

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

The wrapper forwards the shared settled loop-record read port and always uses
its internal executor. Logical capture starts emit BeginCapture (job opcode 7),
which uses internal media opcode 8 over the four-bit command bus. It creates only
the next descriptor or clears/reuses an existing writable view; gaps are invalid,
physical descriptor exhaustion reports MemoryFull, and shared slices retain their
references. Clearing walks the old chunk list, so long takes require bounded
reclamation work before making deadline claims.

The standalone media tests use per-node playback/capture state.
`fpga/loop_pipeline` owns fatal processing policy: any required media error latches
a fault, suppresses output and requires reset/reload. Clock advancement follows
successful jobs. There is no host preparation pause or external executor.

## Verification

Full musical/state/media behavior is compared with the production float and
Q8.24 runtime via [common sessions](../verification/README.md). Static lowering
uses production signal and timing compilation. There is no software FPGA executor.
Standalone tests preserve hardware-specific protocol, configuration, stream,
cache, capacity, fault and reset checks without a duplicate looping backend.

```sh
python fpga/media_store/simulate.py --config all --random-cases 12
python fpga/media_store/simulate.py --pipeline --config all --random-cases 5 --memory-model fixed
python fpga/media_store/resolver_simulate.py
python fpga/media_store/simulate_metadata.py
```

The subsystem's `check.sh` retains lint and synthesis entry points. Current
coverage and local evidence are recorded in [the retirement ledger](../verification/MIGRATION.md)
and [subsystem gates](../verification/subsystem-gates.json). [Historical results](HISTORY.md)
retain the earlier reference-backed measurements; their commands are retired.
