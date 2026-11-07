# Audio chunks: index, borrowing, and cost design

This note develops `IDEA_CHUNKS.md`. It is a proposed architecture with executable
checks, not an implementation of the production engine or a deadline guarantee.

## Decisions for a first implementation

- Exactly one quantum per payload chunk. Use planar samples and a channel-group
  width fixed when a pool is prepared. If several widths are required, prepare
  separate fixed-slot pools; do not introduce a variable-duration allocator.
- Keep sample storage and index storage separate. Both use dense indexed slots.
- Use a fixed-height, immutable radix index over chunk positions. Start with
  four-way nodes; retain eight-way nodes as a measurement candidate.
- Make every append install a fresh chunk and build one bounded index path,
  regardless of the old path's sharing. No payload copy-on-write branch exists.
- Resolve borrowed connection references through a scoped processor context.
  Persistent ownership belongs to indexes and resource owners, not graph edges.
- Reset the render arena per graph quantum. Host callbacks and adapters may span
  several quanta and must recover outputs before the arena expires.
- Retire streaming replacement paths within their explicit small bound. Reclaim
  arbitrary dropped media through a separate budgeted queue.
- Keep MIDI storage unchanged, while preserving coordinated audio/MIDI commit.

History-ring specialization, including custom source-local indexing and physical
arenas, and arithmetic descriptors are optional, deferred follow-up ideas. Neither
is part of the initial implementation. Their status and semantics are recorded
in the optional-follow-up section below.

## Persistent index

### Representation

Use a fixed-height radix tree whose leaf entries identify complete immutable
chunks. Internal entries identify child nodes. A missing entry means silence or
an absent region, as distinguished by the owner's logical bounds.

Nodes and chunks reside in separate prepared slabs. Private tree entries can be
32-bit indexes, with a reserved null value. They are owning edges: a target slot
cannot be recycled while an owning edge remains. These private IDs must not be
exposed as arbitrary weak handles to clients.

Borrowed externally usable chunk handles need slot-reuse validation. Use a wide
generation or another explicit non-reuse rule; never silently wrap a generation
into a live stale identity. Arena epochs should be 64-bit ordinary values. A
32-bit epoch at 1500 quanta/second would exhaust in about 33 days.

A possible node header contains a reference count, level/kind, retirement cursor,
and free-list state. With a 16-byte header and 32-bit entries, both four- and
eight-way nodes fit in a 64-byte aligned slot. This layout compiles in the Rust
experiment. The slot alignment is a modeling choice, not a universal cache-line
size assertion for every supported target.

The tree's address limit is fixed during preparation. Its height is
`H = max(1, ceil(log_B(address_slots)))`, where `B` is the fanout. Owner mappings
translate frame ranges to chunk positions. History can wrap modulo its actual
ring capacity; that capacity need not be rounded up to a power of the fanout.

For a 30-second stereo history at 48 kHz and Q=32, use 45001 chunk slots including
a guard slot. A four-way index has height eight; an eight-way index has height
six. Do not let unrelated unused payload capacity inflate every history's index
height. Imported/longer media and growing tails require their own admitted
address limit or a bounded prefix/tail mapping.

### Appending and editing

An append preflights one fresh payload slot and H fresh index slots, plus the
admitted reference-count bounds. The payload writer fills and seals its chunk.
The index builder creates H nodes on the addressed path, retaining unchanged
sibling entries and transferring ownership of the new path. Publication swaps
the owner's root only after the operation is ready.

The same path length and payload-write count apply when many snapshots retain
the old root. Sharing can change reference counts and resource occupancy, but
cannot introduce old-payload copying into this operation.

An edit creates replacement payloads for changed chunks and uses the same index
primitive. Reserve all affected payloads and index paths before publishing a
multi-chunk edit. Large edits need incremental preparation with atomic final
publication, not an unbounded callback loop.

### Grabs and snapshots

An unchanged whole-view snapshot retains one root. A grab of a subrange creates
only boundary paths and retains wholly covered subtrees. A circular range has at
most two intervals and four boundaries. A conservative allocation bound is 4H
nodes, with O(4BH) entry work, independent of the number of chunks in the grab.

The range operation must remove entries outside the admitted range, including
unselected entries in a boundary leaf. Merely storing a short logical window on
an untrimmed full-history root would unnecessarily pin unrelated history.

Logical bounds mask unused samples at partial chunk boundaries. These samples
must never become visible through padding or growing an owner. Imported audio,
silence extension, and continuing recordings need explicit mappings and tests.

Snapshots and continued history share immutable payloads. A grabbed owner can be
edited later without changing its source history, peers, or earlier exports.

## Ownership and reclamation

Tree nodes hold reference counts on owning edges. Leaf nodes own chunks. Graph
connections borrow through these owners for the duration of one quantum and do
not repeatedly acquire/release ownership along each edge.

All pool and index mutation remains on the render owner. Control-side exports
use the existing bounded request/streaming boundary. Direct concurrent reads
from a globally mutable pool are not part of this design. These rules avoid
atomic payload reference counts, locks, and `Arc` per chunk.

### Streaming replacement paths

Replacing one leaf can displace at most H old index nodes: all unchanged siblings
remain owned by the newly published tree. If a snapshot owns the old path, fewer
of those nodes become reclaimable.

Retire this path before the next streaming update. A conservative budget is
`H * (B + 1)` units: B entry visits and one finalization per node. A unit includes
bounded reference-count/free-list operations; it is not a CPU-cycle estimate.
The algorithm uses explicit prepared work storage, not recursive destructors.

This immediate but bounded path retirement limits the transaction workspace and
avoids a bulk deletion blocking the recycling required by ongoing history.

For U admitted single-chunk index updates per quantum, reserve at least UH path
slots for simultaneous staging and budget `UH(B+1)` retirement units. Actual
multi-target transaction concurrency and callback command staging can require
more workspace and must be added explicitly.

### Bulk deletion

Dropping a long owner only releases its root. A zero-reference node enters a
prepared retirement queue. Process one child edge at a time, preserving the
cursor, and finish the current node before switching to another. Its finalization
returns the node slot to the free list. Leaf-edge release can return a final
payload slot in bounded time.

Give this queue a separate fixed quota, for example K complete-node equivalents
per quantum, or `K(B+1)` units. Returning child references may enqueue more nodes,
but cannot recursively consume the rest of the graph in the current operation.
An intrusive queue can use retirement state already reserved per node.

Retired owners keep their capacity reservation charged until resources have
actually been reclaimed. A delete command must not immediately advertise all
its memory as reusable. Admission must bound both retirement backlog and command
arrival rate. Repeated deletions/recreations cannot borrow the streaming reserve.

For a unique 30-second four-way history, roughly 15003 nodes are reachable. At
32 node equivalents per quantum, draining that whole index takes about 313 ms at
48 kHz/Q=32. This is acceptable reclamation latency only if the deleted owner's
capacity remains charged throughout. It is not a 313 ms render operation.

## Safe Rust borrowing interface

The experiment in `borrow_probe.rs` is a standalone `#![no_std]` library using
`alloc`, with `#![forbid(unsafe_code)]`. It allocates typed `f32` arrays during
preparation. Processing performs no general heap allocation, byte-to-float cast,
or global mutable access.

The central API shape is:

```rust,ignore
produce_transient(|inputs, destination| { /* fill destination */ })
produce_persistent(|inputs, destination| { /* fill destination */ })
produce_transient_batch(count, |inputs, destinations| { /* fill outputs */ })
```

The closure receives a read-only handle resolver and a distinct mutable output.
Its higher-ranked lifetime prevents borrowed samples from escaping the call.
No method that resets or mutates the pool is available through the read view.

For a transient destination, split the arena at its allocation cursor. The reader
can see the immutable produced prefix, while the writer receives only the new
destination in the suffix.

For a persistent destination, reserve one unpublished slot and split persistent
storage into the regions before, at, and after it. The reader can resolve live
chunks on either side; it cannot resolve the destination. After successful
production, seal the slot and return a validated handle. Failed production does
not publish it. Repeated inputs and fan-out are ordinary immutable borrows.

Use typed sample storage. Persistence serialization, interleaving, and endian
conversion belong at explicit adapters, not in every graph edge.

The experiment proves this borrowing pattern for one persistent output chunk
and a contiguous batch of transient outputs, each containing a fixed channel
group. For a multi-output processor, reserve the batch before lending inputs;
normal slice splitting gives the processor disjoint mutable output slices.
Resolve input handles once per call, outside sample-processing loops.

Initially, processors needing several outputs at once can use a transient batch
and pay the one admitted capture copy for any retained outputs. Direct persistent
production is an optional prepared path for suitable producers; do not require
an arbitrary persistent multi-output scatter allocator before shipping the
storage change.

The experiment does not implement persistent ownership, reference counting,
descriptor sequences, plugin ABI adapters, or atomic audio/MIDI transactions.
Sealing an output is distinct from publishing a loop change: production code
must publish roots only after coordinated commit.

Processor contracts still require full initialization of published outputs.
Memory is initialized at setup, but an unwritten reused sample could contain
stale audio. Built-in full writers can avoid a clear; mixers initialize their
destination; adapters for processors without a full-write guarantee clear once.
No uninitialized-memory optimization is required.

## Analytic model

`scripts/model_audio_chunks.py` exposes assumptions through CLI flags and emits a
table or JSON. `scripts/report_audio_chunk_design.py` writes the checked-in
scenario tables and full assumptions in `estimates.md` and `estimates.json`.

Let P be the incoming payload size, G the legacy page size, and N the new node
size. For one aligned legacy append fitting a page, the model counts:

- Interleaving: P bytes read and P written.
- Staging the old page: G read and G written.
- Patching samples: P read and P written.
- Publishing the replacement page: G read and G written.

Total: `4G + 4P` logical bytes. For P=256 and G=4096 this is 17408 bytes.
Frame-aligned page crossings are reported separately; the comparison table uses
the lower aligned cost and excludes legacy index work.

For a new direct persistent append, the model counts:

- P read and P written for capture.
- H old nodes read, H new nodes written, and H retired nodes scanned: `3HN`.
- At most `H(2B-1)+1` reference updates, including eventual path retirement.
- Explicit configurable node and payload allocation/free-list bookkeeping.

Reference traffic defaults to eight logical bytes per 32-bit read/write update.
The sensitivity scenario charges 128 bytes per update to explore poor counter
locality. This is a pessimistic envelope, not a measured cache model: actual
cache-line fills, coalescing, write allocation, prefetch and eviction differ.

For the four-way candidate, the default result is 2664 logical bytes per capture
quantum, including index work, versus the legacy comparator's 17408: about 6.5x.
The cold-counter sensitivity reduces that modeled advantage to about 1.8x.
Eight-way nodes slightly improve the ordinary byte count but perform more
reference updates; they lose under the pessimistic counter assumption. This is
why four-way nodes are the conservative starting candidate.

The graph model counts only explicitly eligible copy-only edges as eliminated
copies. Mixes, effects, and gathers remain separately specified materializations.
It does not model source-local physical history allocation or arithmetic
descriptors and does not estimate savings from either proposed extension.
Additional writers in the model share the captured signal payloads and pay for
their own index updates. Independent edited/processed recordings require extra
payload production and are not covered by that same-signal writer scenario.
With many materializations, the benefit shrinks; the sample DSP-heavy scenario
is about 1.6x, not 6.5x. These are selected-component comparisons, not predictions
for the entire old engine or for callback duration on the Raspberry Pi.

### Capacity illustrations

For an interval of W chunk positions, a compact prefix tree has
`sum(ceil(W / B^level), level=1..H)` nodes. Adding 4H covers boundary overhead for
a clipped circular interval in this representation. The tests exhaustively
check small circular ranges against that conservative bound.

The model reserves each retained view as if its entire interval could become
independent. This is safe but can be expensive: 128 separate 30-second stereo
reservations exceed 1.4 GiB of samples even if the current payloads are shared.
Do not confuse that reservation bound with current physical occupancy.

An initial implementation can use conservative owner credits, as the existing
storage already does. More precise sharing-aware admission is a separate
optimization requiring a bounded algorithm. It must not silently promise future
allocation based on the assumption that owners will remain shared.

The reported capacity includes current owner limits plus one callback of staging.
It deliberately excludes command staging, retirement backlog headroom, root
tables, processor state and MIDI. It is not a complete pool-sizing formula.

## Reproduction and checks

With the repository's Python and Rust environment active:

```sh
python3 scripts/model_audio_chunks.py --sources 16
python3 scripts/model_audio_chunks.py --reference-access-bytes 128
python3 scripts/model_audio_chunks.py --sources 16 --copy-edges 64 --materializations 32 --dsp-inputs 2 --json
python3 scripts/report_audio_chunk_design.py
python3 -m unittest discover -s scripts/tests -p test_audio_chunk_model.py -v
python3 scripts/check_audio_chunk_design.py
```

The Python checks cover hand-derived costs, alignment, parameter scaling, fanout
sensitivity, immutable range snapshots through history wrap, fixed path counts,
bounded maintenance, exact range trimming, streaming retirement while bulk
deletion is stalled, conservative resource counts, and complete final reclamation.

The Rust checks compile a no_std library with warnings denied, execute a native
harness, and require compiler rejection of escaped input/output borrows and arena
reset during a borrow. The harness covers both destinations, multi-output batches,
duplicate immutable input borrows, reuse of a middle persistent slot, stale
handles, failure and capacity.
Only the installed x86_64 target was compiled; embedded cross-target validation
remains outstanding.

## Optional follow-up ideas

The main proposal describes these ideas in
[the history-ring and arithmetic-descriptor sections](../../../IDEA_CHUNKS.md).
Neither has been added to the model, prototype, or tests in this design pass.

### History-ring specialization: deferred

The baseline already maps circular history onto persistent immutable chunks.
Keep the common persistent pool and index machinery for the initial implementation.
Custom source-local indexing and physical arenas are optional experiments to
consider only after the core design is implemented and measured. Dedicated
physical circular segments might improve locality further, but retained grabs
break FIFO reclamation of payload storage. Segments must remain pinned until
their final owners release them, with bounded capture headroom and fragmentation.
Avoid any requirement to copy pinned samples out of a segment to recycle it.
Neither custom indexing nor physical segmentation is an implementation gate;
adopting either requires a later, separate decision.

### Arithmetic descriptors

This is an optional feature for separate evaluation. The chunk storage and
borrowed-routing implementation must not depend on it.

An eligible consecutive gain stack can carry `(start, end)` envelope gains over
one fixed graph quantum. Compose corresponding endpoints by multiplication and
define a linear envelope between the resulting endpoints. This intentionally
approximates the interior curve of the ideal stacked ramps; endpoint matching is
the contract. Constant gains have equal endpoints; copy is identity. Derive the
per-sample step once when materializing the result, and use incremental gain
updates in the sample kernel.

Chunk-span boundaries and host callback partitions must not change the
approximation segment. A one-sample quantum evaluates its single gain without
division. Keep the descriptor size fixed and determine eligible regions during
graph preparation. General expression trees and descriptor growth through mixing
are outside the initial experiment. Effects, capture points, and meter taps
retain their defined signal-observation points.

The baseline behavior-preservation gates below apply to the core chunk change.
The optional experiment instead needs explicit acceptance of interior-curve and
rounding differences after its own correctness, audio, and worst-case performance
evaluation. Its adoption is not decided here.

## Remaining implementation gates

1. Set numeric per-quantum/callback work budgets from Pi and embedded-target
   measurements, including reference counters under cache interference. No
   analytic byte count proves a hard real-time deadline.
2. Complete pool admission using configured maximum writers, command staging,
   owner lengths, retirement backlog, and graph arena/descriptor requirements.
3. Specify growing-prefix/tail and imported partial-span mappings, with bounds
   on gathers and root updates. The simple circular-history model does not
   establish all of those bounds.
4. Extend scoped borrowing to actual multi-output processor adapters and staged
   multi-target audio/MIDI transactions. Keep existing MIDI representations.
5. Add production equivalence and stress tests before replacing the backend:
   capture cuts, smoothing, gain, exports, edits, host callback partitioning,
   allocation failure and queued deletion while capture continues.

Defer variable-duration chunks, arbitrary lazy expressions, precise shared-credit
accounting, and arena lifetime reuse until measurements show they are needed.
