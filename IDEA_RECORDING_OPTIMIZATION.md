# Prepare recording storage before it is needed

Status: proposal. This optimization has not been implemented or measured.

## Context

The FPGA memory subsystem currently allocates recording storage when an append
needs it. That puts allocation and metadata work in the path of the audio write:
the job processor has samples ready, but must wait until their destination is
prepared.

Our RTL simulation benchmark used 64 stereo loops recording together, a quantum
of 32 frames, and a 100 MHz FPGA clock. Each stereo loop has two recording
channels, so 128 channels need storage at the same time.

| Measurement | First recording quantum | Steady recording |
| --- | ---: | ---: |
| Complete quantum | 989.91 us | 570.19 us |
| Job processor waiting on the memory subsystem | 475.47 us | 81.92 us |
| DDR traffic | 40.50 KiB | 16.00 KiB |
| Audio samples recorded | 16.00 KiB | 16.00 KiB |

The memory-wait measurement includes our caches and metadata handling, not just
DDR access. Of the extra 393.55 us of memory wait at startup, 378.19 us appeared
as write backpressure: the subsystem was not yet ready to accept the samples.

In this configuration, each channel receives allocation blocks containing 1,024
samples, or 4 KiB. The first allocation also builds a four-level block-address
tree. Across 128 channels, that creates 512 metadata pages of 64 bytes each.
The table cache holds 128 pages, so construction displaces dirty pages and causes
DDR writes. The following quantum also incurs cache misses as the cache settles
on the leaf pages used by ongoing recording.

The recording views in this test were already empty. Disposing of old audio was
therefore not responsible for the startup penalty. Whole audio blocks are not
zero-filled in this configuration either. Much of the penalty comes from
serialized allocation, metadata initialization and cache servicing; the DDR data
bus was only about 2% utilized during the first quantum.

## Recurring allocation costs

A block lasts 32 quanta at this quantum size. If all channels start together,
their next block allocations also happen together.

Those boundaries should cost less than startup because the address trees already
exist. Each leaf page holds 16 block addresses, so most allocations update an
existing leaf. Larger spikes can occur when new leaf or directory pages are
needed. The recurring allocation boundaries have not yet been benchmarked; their
cost should not be assumed to equal the measured startup cost.

## Proposed improvement

Give the memory subsystem a way to **reserve recording capacity without extending
the recorded audio**, and prepare that capacity before the write cursor needs it.

There are two parts:

1. **Prepare the start of a recording in advance.** Keep an empty recording
   allocation ready for each channel that may start recording. Prepare its first
   block and metadata before the record command arrives. Starting a take consumes
   that preparation instead of constructing it during the first audio quantum.
2. **Keep one block ahead during recording.** While the current block still has
   room, allocate the next block and prepare any metadata needed to address it.
   Spread this work across successive quanta rather than waiting for every
   channel to reach its boundary.

Use the existing memory engine for this work. A bounded maintenance budget can
allow a few preparations at a time, with audio operations taking priority. The
engine must yield at safe points, and the budget must account for any operation
that cannot be interrupted. Preparation must not become another long blocking
operation immediately before the next quantum.

For the benchmark geometry, a newly entered block provides 32 quanta of lead time
for preparing its successor. The exact reserve depth and maintenance budget
should be chosen from measured preparation costs and the maximum number of
simultaneous recording channels.

## Meaning and lifetime of a reservation

Reserved capacity and published audio length must be separate concepts. Preparing
a block does not record samples, advance a cursor, generate a sync pulse, or make
unwritten samples readable. Only completed recording writes extend the published
audio.

Reserved blocks and metadata are owned resources: they must not be allocated to
another recording. Clear, restart, loop removal and deferred cleanup must account
for them, including reservations that were never used.

The current capture-start path clears the recording view. It must be adapted so
that starting a take consumes prepared empty storage rather than discarding it.
When replacing an old take, its storage can still be detached for deferred cleanup
while prepared storage becomes the destination for the new take. Repeated starts
must also replenish the supply of prepared empty storage.

Allocation and preparation should stay inside the memory subsystem. The control
side may establish reserve targets, but should not need to issue a command for
each block while recording. This also keeps the future ARM/FPGA boundary simple.

## Expected cost and benefit

One extra block per channel costs **512 KiB of DDR capacity** for 128 channels,
plus metadata and reservation bookkeeping. The audio capacity is in DDR, not
BRAM. There will still be some additional control and state storage, but the
proposal does not require a second allocator or a large additional cache.

The expected benefit is moving allocation and metadata-growth work out of the
critical audio-write path. It does not reduce the amount of audio written, and
ordinary cache and DDR latency remain.

Avoiding allocation spikes depends on preparing enough storage in time. An
immediate record command needs an already prepared initial reserve; doing the
preparation inside that command would retain the startup spike. Memory capacity
and maintenance time must cover the supported simultaneous recordings and restart
rate. The behavior when preparation falls behind must be explicit; this proposal
does not yet choose between a slower allocation path and a capacity refusal.

## Why not just enlarge the cache?

A larger metadata cache is a useful, simpler experiment and should reduce the
measured startup penalty. It costs additional RAM and, with the current fully
associative lookup, additional comparison logic. It also leaves allocation work
on the audio path and cannot prevent spikes for every working set.

Preparing capacity ahead of time is the preferred general approach because it
spreads the work without relying on a much larger hardware cache.

## How to evaluate it

Implement the same reservation semantics in the Rust FPGA reference and RTL,
reusing the existing allocation and cleanup mechanisms where practical.

Check that preparation leaves audio length and visible samples unchanged, that
recording uses prepared capacity, and that clear, restart, removal and cleanup
release all owned resources correctly. Exercise cancellation or interruption of
preparation as well as resource exhaustion.

Repeat the 64-stereo-loop RTL benchmark and measure the worst quantum, not only
the steady average. Cover prepared recording startup, synchronized block
boundaries, leaf and directory growth, and repeated recording restarts. Record
job processor memory waits, DDR traffic and maintenance time, and compare audio
results with the Rust reference. Also measure FPGA resource usage to confirm
that the timing improvement preserves the goal of low hardware cost.
