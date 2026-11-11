# Chunk-memory subsystem

The initial design is implemented in Rust and RTL, including standalone tests and
full-loop integration. See [implementation and protocol](fpga/chunk_memory/README.md).
The following defines the subsystem boundary and its logical contract.

## Purpose and boundary

Introduce a chunk-memory subsystem separate from the job processor. It hides
BRAM buffering, cache lookup, writeback, prefetch scheduling, arbitration and
chunk-ID-to-address translation. DDR burst handling can be added internally later.
The existing job scratch RAM continues to hold intermediate arithmetic results.

Media metadata remains responsible for views, ranges, allocation and ownership.
A media frontend resolves view/frame ranges into chunk spans before requesting
sample access. The chunk service does not interpret graph topology or loop modes.
Future DDR-backed metadata transfers must share the memory scheduler.

## Logical interface

Use commands, sample streams and completion responses with request IDs. Keep BRAM
buffer handles private. Packet widths and physical port organization remain open.

| Operation | Request | Result |
| --- | --- | --- |
| Read | Chunk ID, sample offset, count | Sample stream and completion/status |
| Write | Chunk ID, sample offset, count, sample stream | Completion when buffered and logically visible |
| Prefetch | Chunk ID | Advisory hint; no completion dependency |
| Fence | Ordered after earlier writes | Completion when those writes are acknowledged at the external memory interface |
| Retire | Chunk ID | Completion when the allocation can safely be reused |

Commands, streams and completions use ready/valid flow control. Accepting a write
command reserves resources to receive its payload. Completion follows receipt of
all samples into authoritative local storage; DDR writeback can proceed later.
Writes preserve samples outside the requested span. Allocation and initialization
policy remain with the media frontend.

Prefetch hints may be ignored or displaced. Accepting a hint does not promise
residency; a later demand read works on both a hit and a miss. Future hints may
include a needed-by quantum without changing their advisory nature.

## Internal organization

- A read pool holds raw chunks for demand reads and read-ahead. Cached data can
  be shared across views; gains and ramps remain outside the cache.
- A write pool combines partial recording writes and queues completed chunks for
  background DDR transfer. It can retain active recording tails.
- A shared directory coordinates both pools and forwards newer buffered writes
  to readers, including while writeback is pending.
- A chunk-transfer scheduler arbitrates reads, writebacks and prefetches. A DDR
  adapter maps each logical transfer to the controller's transactions or bursts.

Provide bounded service for demand reads and writes approaching buffer exhaustion;
prefetch uses spare capacity. Exact arbitration, pool sizes and replacement policies
remain open. Resolve chunk spans once and stream samples within each span, rather
than repeating metadata lookup for every sample.

## Ordering, lifetime and errors

The subsystem may reorder independent chunk transfers. Conflicting operations
preserve their defined order, and reads issued after write completion observe the
new data even before DDR writeback. Fence covers prior writes in its ordered
request stream.
With the AXI endpoint, external completion means B/R acceptance and ordered
visibility, not physical DRAM-cell completion. The simulation's physical model
drain is a separate operation. See [AXI memory](fpga/axi_memory/README.md).

The metadata owner requests retirement once logical references are gone. It waits
for acknowledgement before reusing the allocation. Retirement accounts for cached
data and outstanding transfers so an old transfer cannot affect a new allocation.

Return per-request errors and expose asynchronous subsystem faults: a DDR failure
may occur after a buffered write has completed. Buffer exhaustion applies
backpressure. Real-time bounds require explicit capacity and memory-service bounds.

## Initial prototype and reference

Start with one demand operation at a time while prefetch and writeback run in the
background. Expand concurrency through request IDs as measurements justify it.
Read-ahead can first use upcoming jobs, with next-quantum hints added later.

Keep the Rust reference cache-independent: reads/writes retain their logical
semantics, prefetch can be a no-op, and fence/retire can complete immediately.
Compare logical media contents; drain writes before comparing raw backing DDR.
Benchmark loops larger than cache capacity, including cold reads, seeks and write
backpressure, so small fully resident fixtures do not overstate the benefit.
