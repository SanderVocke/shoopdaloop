# Bounded RTL media store

Simulation now uses the complete [Rust/Marlin testbench suite](../../src/rust/rt_fpga_testbench/README.md):

```sh
cargo run --locked -p rt_fpga_testbench --features sim -- media
```

The Python simulator entry names are aliases for this runner and accept its
current options. The native suite guide describes profiles, replay, VCDs and
reports. Historical measurements are marked separately.

The current allocator owns groups of 32 chunks; see
[block allocation, sharing and interfaces](BLOCK_ALLOCATION.md). Sample transfers
remain chunk-sized. Legacy one-chunk regression runs and blocked runs are both
available; select `--block-chunks 32` for the current allocation geometry.

This hardware service implements mono fixed-point chunks, shared read-only
ranges, per-block references and deterministic LIFO reuse. Samples pass through
[chunk memory](../chunk_memory/README.md) to the external memory port. Small
view descriptors use synchronous on-chip RAM. Block-ID vectors, reference counts
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
- `rtl/media_metadata.sv`: compact radix-indexed vectors, detached roots and serialized ownership metadata.
- `rtl/media_reclaimer.sv`: budgeted reference release and detached-table cleanup.
- `rtl/metadata_cache.sv`: persistent writeback cache used by tables, counts and free IDs.
- `rtl/media_audio_pipeline.sv`: exclusive store ownership between jobs and host commands.
- `rtl/memory_delay.sv`: optional minimum completion delay for a memory transfer.
- `rt_fpga_testbench`: standalone ownership/protocol checks and complete sample quanta.
- `check.sh`: warning-clean lint and generic/ECP5 synthesis for both tops.

Defaults: 32 issued view IDs, 1,024 physical chunks, 32 samples per chunk,
and 32 chunks per allocation block (32 allocation blocks). Each view allocates
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
Metadata requirements are registered after a phase or configuration-request
change. The frontend waits for this preparation before advancing or transferring
samples; successive samples in the same phase retain their original cadence.
A phase-transition token identifies prepared controls without comparing copies
of the complete phase register.
Physical chunk commands pass through a one-entry register before cache admission.
Fence drains sample buffers before a separate metadata-flush phase.
Append capacity, slice bounds/offsets and terminal-sample decisions have separate
register boundaries. Metadata captures root RAM results and tag matches before
decoding; tree capacity and trim arithmetic are staged before their decisions.

| Opcode | Operation | Operands | Result |
| --- | --- | --- | --- |
| 0 | Create | None | `result_view`: next recording ID |
| 1 | Slice | `cmd_view`: source, `cmd_start`, `cmd_length`: frames | `result_view`: read-only range ID |
| 2 | Clear | `cmd_view` | Retain ID/kind; detach the old table for deferred release |
| 3 | Remove | `cmd_view` | Detach the old table and invalidate ID |
| 4 | Reclaim | `cmd_view` | Release at most one tail reference; `result_empty` |
| 5 | Append | `cmd_view`, `cmd_count`: input words | Updated recording |
| 6 | Read | `cmd_view`, `cmd_start`: position, `cmd_length`: loop length, `cmd_count`: output words | Sample stream |
| 7 | Fence | None | Earlier writes acknowledged by backing memory; does not drain cleanup |
| 9 | Maintenance grant | `cmd_count`: optional block ceiling, `cmd_start`: cycle budget; zero selects defaults | Grant bounded cleanup work and acknowledge immediately |
| 10 | Drain cleanup | None | Wait for all retired tables; offline operation |

Slices copy IDs and retain references, with no sample-memory transactions. Nested
ranges retain their own references. Partial boundary chunks stay allocated while
reads remain bounded by logical frame ranges. Sources may append, clear or be
removed without changing retained slices. Read-only views reject append, including
empty append and append after clear.

Clear/remove transfer the existing DDR table to a fixed-size retirement record
and publish the empty/invalid view without walking old entries. Cleanup later
releases entries from the tail and recycles empty metadata pages. Reclaim remains
a synchronous one-entry command and truncates
logical length, accounting for the first-chunk offset. Only a zero reference count
retires the allocation before returning its ID to the free stack. Allocation pops that stack before growing the slot
high-water mark; storage does not shrink. Append checks capacity before changing
media. Fresh one-chunk allocations zero unpublished tails in legacy mode.
Block allocations do not erase the whole block; only published samples are
accessible. Reused allocations retain old unpublished samples. Descriptors/lengths publish at completion.
Zero-word reads still validate the view, position and loop length.

### Physical view reuse

`REUSE_VIEWS=0` (default) preserves append-only physical descriptor IDs: deleting
a view does not make that raw ID available for a new view during the same reset
epoch. `REUSE_VIEWS=1` lets Create/Slice scan for an invalid descriptor before
growing the high-water mark. Chunk reference counting and allocation are unchanged.
Enable this only behind a host registry that checks logical handle generations
and media/export/cursor ownership before accessing raw views. The Marlin mono
runtime adapter supplies that registry and opts in, keeping writable view zero
stable while reclaiming immutable take descriptors. Raw IDs have no hardware
generation field and cannot by themselves detect stale use in reuse mode.

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
| 3 | Slot, physical view, metadata page or retired-record capacity exhausted |
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
pulse after request/validation, the variable-latency metadata operation and a
registered result stage. Sample data on `cfg_response_valid`. Data is 160 bits;
IDs/counts use 32 bits and diagnostic totals can use 64 bits.
Write/error responses return zero. Full 32-bit addresses are checked before
narrowing. Load only trusted, checked images while execution is stopped;
this debug/loading port does not implement the complete snapshot validator.

| Space | Address | Value |
| --- | --- | --- |
| 0 | 0 / 1 / 2 / 3 | Issued views / block high-water mark / free-block count / logical block limit |
| 1 | View ID | Descriptor: valid bit 0, read-only bit 1, offset `[63:32]`, frames `[127:64]`, table count `[159:128]` |
| 2 | `(view << SLOT_BITS) \| ordinal` | Block ID; SLOT_BITS = max(1, clog2(BLOCKS)) |
| 3 | Block ID | Reference count |
| 4 | Stack index | Block ID, top at `free_count - 1` |
| 5 | 0, write | Fence and invalidate metadata caches/leaf translations |
| 6 | 0..14, read | Cleanup and capacity diagnostics; see below |
| 7 | `record * 16 + word`, read | Coherent retired-record words; diagnostic only |

Descriptor bits `[31:2]` are reserved. Logical slot limit defaults to SLOTS and
supports snapshots with smaller `chunk_limit`. Snapshots without a limit compare
identically while operations fit physical capacity. Restore must supply mutually
consistent descriptors, tables, counters and free entries.
Write each view's table as a dense ordinal prefix during restore. Reads beyond
that prefix and sparse writes are refused. Space 5 is a cold-cache diagnostic;
normal quantum boundaries do not flush metadata.

## Deferred ownership and maintenance

CLEAR, REMOVE and automatic BeginCapture share a root-detachment transaction.
A nonempty old table moves to one of `RETIRED_TABLES` fixed-size DDR records;
its existing pages and references remain owned by that record. No block-ID list
or audio payload is copied. An empty view needs no record. Queue-full refusal
occurs before the live view is changed. New takes still need free sample blocks
and metadata pages; detached storage is not immediately reusable.

The 256-record default costs 16 KiB and provides slots for eight retired
32-channel batches when the other pools have enough headroom. Exhaustion tests
use a two-record queue. A FIFO collector has one resumable context. Reference updates/free-stack changes
are serialized against foreground ownership work. On the final reference, a
separate grouped-retirement path waits for dirty writes and accepted sample
transfers while unrelated sample commands can continue. Only then is the block
made reusable. Each consumed tail entry also releases newly empty tree pages.

`maintenance_tick` opens a new grant in the standalone store. The loop pipeline
grants maintenance once per processing iteration, before control settlement and
audio processing. Explicit host settlement opens that iteration's grant; its
subsequent audio run does not replenish it. `maintenance_enable` allows new work;
disabling it preserves progress and lets admitted indivisible work finish.
Grant inputs are registered and take effect one cycle later. Accounting retains
the pending work charge in the effective credit and reported cycle count.

The default is 5,000 active cleanup cycles, equivalent to 50 microseconds at
100 MHz, with a 1,024-cycle transaction guard and no block-count ceiling
(`RECLAIM_BLOCKS=0`). This is 7.5% of a 32-frame quantum at 48 kHz. The counter
charges accepted metadata requests, ownership locks, queue/page cleanup and
sample retirement, including their memory-response waits. Idle time and waiting
for foreground work to release a resource do not consume cleanup cycles. A new
grant replaces the previous allowance; unused cycles do not accumulate.

New transactions stop when only the guard remains; an accepted transaction or
atomic reference/free-stack update finishes before yielding. A positive
`RECLAIM_BLOCKS` optionally imposes an additional profiling/defensive ceiling.
Shared-reference drops may return no storage. Simulation defaults derive the
50-microsecond allowance from `--fabric-hz`; hardware integrating a different
clock must set `RECLAIM_CYCLES` accordingly.

Foreground commands win admission at safe boundaries. An already accepted
metadata operation, including one bounded-depth tail trim, can delay foreground
service. A pending processing iteration also suppresses new collector admission,
before the run handshake, so an early audio completion does not force the next
iteration to wait for the unused allowance. The standalone audio pipeline gives
its pending run the same priority. The allowance is a ceiling, not a minimum
iteration duration. The continuous wrapper admits a new iteration only when its
input quantum and an output bank are available; offline back-to-back iterations
can run sooner and each receives a fresh, non-accumulating allowance.
The configured guard must cover the measured non-preemptible tail for
the selected geometry and memory service. Arbitrarily stalled DDR has no finite
latency bound; these defaults do not establish physical audio deadlines.

`reclaim_pending`, `reclaim_references`, `reclaim_active`, `reclaim_released`,
`reclaim_block`, `reclaim_blocks`, `reclaim_freed`, `reclaim_cycles`, page counters
and capacity-failure counters expose progress. `reclaim_quiescent` excludes
pending jobs but includes admitted collector transactions/retirement. Continuous
wrappers combine it with accepted AXI traffic for normal quiescence. Fatal
recovery instead drains accepted AXI transactions and resets the entire arena
state, abandoning partial internal work under the existing reset contract.

Space 6 addresses are: 0 pending tables; 1 remaining table references (64 bits);
2 queue high-water mark; 3 completed releases; 4 blocks returned; 5 active collector
cycles; 6 active block ID; 7 active/released flags in bits 0/1; 8 record capacity;
9/10 FIFO head/tail; 11 pages in use; 12 pages available; 13 command capacity
failures; 14 metadata pages recycled. Counter values may span 64 bits.

Pause maintenance before a consistent physical ownership audit. Its accounting
includes live tables, retired tables and a reference already released by an
in-flight cleanup step. Ordinary Fence drains writes without reclaiming old
tables. The explicit offline drain ignores automatic grant limits until the queue is
empty, without installing an unlimited automatic grant. Drain cleanup and Fence
before exporting a complete restorable physical snapshot. The test adapter marks undrained logical snapshots with
`pending_reclamation` and rejects restoring them. Its functional command helper
can explicitly settle cleanup after measuring the foreground done pulse; timing
and deferred tests use `drain=False`.

Use free-block/page counts and pending ownership when admitting recording work.
Reserve first-block capacity for all channels starting together, metadata paths
for their new tables, and retirement records for the old takes. There is no
synchronous sweep when those resources run low. Queue depth, page capacity and
sample capacity remain independent limits. Cleanup itself allocates no pages.

The queue and metadata state are volatile within the current session epoch.
Coordinated reset discards roots, queue indices and collector state together
with endpoint state. It does not resume a collector from old DDR records.

Focused regression commands:

```sh
cargo run --locked -p rt_fpga_testbench --features sim -- deferred
cargo run --locked -p rt_fpga_testbench --features sim -- capacity
cargo run --locked -p rt_fpga_testbench --features sim -- stress --stress-seconds 30
```

The ownership suite also runs with AXI64, single-line metadata caches and
`--memory-model dramsim3`. The long stress preloads actual hardware ownership
tables for 32 mono or 16 stereo loops, each recorded for 30 seconds at 48 kHz.
It checks mass clear, continued recording and complete recovery; old payloads
are deliberately unobserved. CI runs the full 30-second ownership campaigns. [Saved evidence](measurements/deferred/README.md) distinguishes these
ownership campaigns from full production audio comparisons and timing tests.

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
references. Clearing detaches the existing table; its references and pages are
released by budgeted maintenance. Physical capacity must cover pending cleanup
and new captures simultaneously.

The standalone media tests use per-node playback/capture state.
`fpga/loop_pipeline` owns fatal processing policy: any required media error latches
a fault, suppresses output and requires reset/reload. Clock advancement follows
successful jobs. The integrated coordinator also permits host media operations at its explicit
pre-audio settlement pause; there is no external executor.

## Verification

Full musical/state/media behavior is compared with the production float and
Q8.24 runtime via [common sessions](../verification/README.md). Static lowering
uses production signal and timing compilation. There is no software FPGA executor.
Standalone tests preserve hardware-specific protocol, configuration, stream,
cache, capacity, fault and reset checks without a duplicate looping backend.

```sh
cargo run --locked -p rt_fpga_testbench --features sim -- media --random-cases 12
cargo run --locked -p rt_fpga_testbench --features sim -- pipeline --random-cases 5 --memory-model fixed
cargo run --locked -p rt_fpga_testbench --features sim -- resolver
cargo run --locked -p rt_fpga_testbench --features sim -- metadata
```

The subsystem's `check.sh` retains lint and synthesis entry points. Current
coverage and local evidence are recorded in [the retirement ledger](../verification/MIGRATION.md)
and [subsystem gates](../verification/subsystem-gates.json). [Historical results](HISTORY.md)
retain the earlier reference-backed measurements; their commands are retired.

## Storage statistics

Configuration space **6** is read-only and available between commands/quanta.
Address 16 returns five 32-bit words, least significant first: occupied allocation
blocks, configured block limit, occupied metadata pages, metadata-page capacity,
and consumed view slots. This is one coherent counter sample; it does not read
sample DDR, flush caches, allocate, or change media. Address 17 returns arena bytes
(low/high words), chunk frames, chunks per allocation block, and view capacity.
Addresses 0–14 retain the reclamation diagnostics described above. Writes and
addresses 15 and 18 upwards are refused.

Occupied blocks are the allocation high-water mark minus the reusable free stack.
Shared slices retain blocks once physically, and partial blocks remain occupied
until their last reference retires. Metadata pages are independently counted
from their allocator. Retired tables and their blocks remain occupied until
background reclamation releases them; querying counters does not drain cleanup.
With `REUSE_VIEWS=0`, issued IDs include deleted holes that
cannot be reissued. With `REUSE_VIEWS=1`, the view count follows valid descriptors,
including configuration restores; deleting a descriptor recovers that capacity.
Reset clears all occupancy and restores the physical block limit.

Focused counter checks cover metadata exhaustion with spare sample capacity,
retained slices, reset, read-only access, and both view policies:

```sh
cargo run --locked -p rt_fpga_testbench --features sim -- statistics
cargo run --locked -p rt_fpga_testbench --features sim -- statistics --memory-model dramsim3
cargo run --locked -p rt_fpga_testbench --features sim -- reuse
```
