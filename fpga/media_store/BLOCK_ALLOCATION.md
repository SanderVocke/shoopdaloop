# Allocation blocks

Sample buffering and transfers still operate on `CHUNK_FRAMES` samples. Allocation,
view tables, reference counting and the reusable-ID stack operate on blocks of
`BLOCK_CHUNKS` consecutive physical chunks. The RTL default is 32 chunks per block:
1,024 frames, 21.33 ms and 4 KiB per mono block at Q/chunk=32 and 48 kHz.

```text
view ordinal -> block ID -> physical chunk = block * BLOCK_CHUNKS + within
                  |
             one reference count
                  |
         last release -> retire block -> free-block stack
```

Each view retains one reference per block-table entry. Appending within an owned
block changes no ownership counts. On growth, pop a free block or allocate the
next block ID, set its reference count and append its ID to the compact table.
Clearing/removing walks block entries. Reclaim releases one tail block and adjusts
the view's frame length. A frame range retains its intersecting blocks, including
partial boundary blocks; nested ranges have independent block references.

A writable view can append beyond a grabbed prefix without copying. Its existing
samples remain stable. Clearing that writer preserves every block retained by a
range, and the next take allocates other free blocks. Reserved tail chunks become
reusable only when the entire block has no remaining references.

`chunk_memory.RETIRE_CHUNKS` selects grouped retirement. One command checks the
bounded cache tags for every chunk belonging to the block, waits for matching
dirty writes and accepted transfers, cancels queued hints/fills and invalidates
matching read/write entries before acknowledging reuse. It does not issue 32
separate retirement commands or load the block's samples. The transfer scheduler,
sample cache line size and AXI burst size are unchanged.

Allocation does not zero a whole block. Full-chunk writes create sample-cache
entries directly; partial writes preserve existing bytes through the normal
chunk-memory path. Only frames within published view ranges are accessible.
There is no background collector; release stays synchronous and scales with the
number of blocks in the old take.

## Geometry and interfaces

`SLOTS` remains the physical sample-chunk capacity. It must be divisible by the
power-of-two `BLOCK_CHUNKS`. `BLOCKS = SLOTS / BLOCK_CHUNKS` sizes block tables,
reference counts and the free stack. On-chip root/descriptor and cache capacities
remain independent of recording duration. The default sample capacity is 1,024
chunks (32 blocks), retaining room for 32 independent nonempty mono recordings.

The metadata layout in [DDR metadata](DDR_METADATA.md) uses BLOCKS for its two
scalar regions and table geometry; the sample region still uses SLOTS. A sample
address is `arena_base + (physical_chunk * CHUNK_FRAMES + sample_offset) * 4`.
Metadata and samples still share the existing AXI endpoint and fence/fault epoch.

Configuration descriptors retain their 160-bit width: their table count is now
blocks and their first offset is frames within the first block. Space 2 contains
block IDs, indexed by `(view << SLOT_BITS) | ordinal`, where SLOT_BITS derives
from BLOCKS. Spaces 3/4 contain block reference counts/free block IDs. Global
high-water, free-count and limit values count blocks. Host configuration and
snapshots use the same coherent service as execution.

The Rust store offers `MediaStore::with_block_chunks(chunk_frames, block_chunks)`.
`MediaStore::new` retains one-chunk compatibility. Snapshots add optional
`block_chunks` (default 1); their view IDs and free IDs remain expanded physical
chunk IDs. Free blocks are complete aligned groups with ascending chunk IDs inside
each group, and groups follow LIFO stack order. Logical `chunk_limit` and slot-count
APIs continue to count physical chunks. The existing `reclaim_chunk` entry point
releases one allocation block in blocked mode.

For blocked snapshots, sample bytes outside all published view ranges are
unspecified physical storage and serialize canonically as zero. Rust and simulator
snapshot conversion use the same logical boundary; simulator live samples always
come from actual fenced DDR/SV storage. Restore reconstructs block tables and
counts through hardware config. Legacy one-chunk snapshots preserve their exact
representation, including unpublished bytes. Format version 3 remains compatible
because the new geometry field is optional and legacy output omits its default.

## Cost and validation

A take wastes less than one block of reserved tail space per channel. Range
boundaries can retain extra chunks in their first/last blocks. A 64-stereo-loop,
30-second example uses 180,096 blocks / 5,763,072 physical chunks: 703.5 MiB of
samples. Each block-ID/count/stack word array is about 0.687 MiB, plus directory
and page-pool slack. The existing 512 MiB DRAMsim3 profile remains too small for
that complete session; timing fixtures use the unchanged profile at smaller sizes.

`--block-chunks 32` enables blocked simulation. Regression runners otherwise keep
one-chunk mode. Ordinary profiles reserve 32 times their legacy sample slots;
the full-system timing profiles explicitly use 2,048 / 4,096 / 2,304 physical
chunks for original / streaming / mixed, respectively. Initial production images are repacked outside execution. The production Rust
comparison checks logical audio, loop states, transitions, outcomes and exported
media; it does not require physical allocation equality. Directed blocked runs
also cover stereo capture across a block boundary and a retake with a retained
old take. Standalone tests check ownership, ranges, refusal atomicity and cache
retirement without invoking the retiring FPGA Rust executor.

```sh
python fpga/media_store/simulate.py --config all --block-chunks 32
python fpga/media_store/simulate.py --pipeline --config all --block-chunks 32
python fpga/loop_pipeline/simulate.py --directed --random-cases 2 --block-chunks 32 --faults
python fpga/chunk_memory/simulate.py --config default --retire-chunks 32
```

These check ranges across blocks, retained slices, partial appends, capacity
refusal, full-block reuse, restore and capture across allocation boundaries.
The retirement test explicitly holds responses, pauses dirty writeback, drains
in-flight prefetch, checks invalidation of other chunks in the block, injects an
error, and resets/reuses storage. Native SV and DRAMsim3 support both AXI widths.

The [block benchmark comparison](measurements/blocks32/README.md) contains the
original/streaming/mixed replay results, including the storage-reservation cost,
cold-start regression and short-take timing limitation.
