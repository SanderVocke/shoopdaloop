---
title: "ShoopDaLoop FPGA"
subtitle: "Audio and MIDI RTL technical reference"
lang: en
---

# System architecture

This reference describes the RTL at `feat/backend-replacement` revision
`cc410f27`. For each processing quantum, the **state processor (SP)**
determines which loops are playing or recording and where they are in their
content. It writes an ordered list of jobs for the **job processor (JP)**,
which processes samples and packets using the on-chip audio buffers and
media services. The host, device controller, or testbench supplies input,
configuration, and the signal to start each quantum.

![Figure 1. The SP keeps its program and loop state on the FPGA. Audio and MIDI reach external DDR through separate hardware services.](figures/system.svg)

## Where to read the RTL

A **top-level module** is the outermost SystemVerilog module selected for a
simulation or synthesis build. The smaller modules inside it can also be
simulated on their own. Start with the build that has the interfaces you want
to follow.

- **Audio and MIDI together.** Start at `midi_audio_pipeline_axi`. Inside
  it, `media_audio_pipeline_axi` contains the audio media store and
  `audio_pipeline`, which in turn contains the SP and JP. The MIDI data-job unit attaches to JP's MIDI job port. It moves
  events between staged input, DDR lists, stored views and output ports.
- **Marlin or ARM using the audio mailbox.** Start at `host_pipeline_axi`.
  It adds an AXI-Lite mailbox and command dispatcher around an audio-only
  configuration of `media_audio_pipeline_axi`.
- **The loop algorithm itself.** Start at `state_processor` and then
  `sp_engine`. This module can run in isolation using on-chip RAM and
  testbench-supplied configuration.

## One processing quantum

1. The host, audio bank controller, or testbench supplies input and requests
   processing for the next quantum. In the mixed core, the wrapper validates
   the staged MIDI batch before starting SP.
2. SP updates loop state at the quantum boundary. It queues MIDI writer
   transitions before the regular work pass, then writes the data jobs
   into an on-chip FIFO. When SP queues an audio playback job, wires named
   `read_intent_valid` and `read_intent_data` also expose a one-clock copy
   of that descriptor. The audio store can use this hint to start fetching
   samples before JP reaches the job.
3. After SP finishes, JP reads and executes the jobs in FIFO order. A job
   such as Gain reads samples from an on-chip scratch buffer written by an
   earlier job, then writes its result for a subsequent job such as Output.
   Audio media jobs and MIDI jobs can also wait for memory or I/O.
4. The mixed wrapper runs bounded MIDI retirement after a successful JP pass.
   Pipeline completion reports whether the quantum succeeded. The output bank
   can then be handed back to the audio device using its own interface. Audio
   writes can still be sitting in the FPGA's caches. A separate media
   **fence** waits for the memory interface to acknowledge
   those writes before it reports success.

| Unit | Meaning |
| --- | --- |
| Frame | One sample time. A stereo frame contains two channel samples. Musical positions and MIDI offsets use frames. |
| Quantum (Q) | Frames processed by one run. Managed sync supports 1, 16, 32, 64 and 128, subject to the instantiated limits. |
| Audio chunk | Fixed power-of-two count of 32-bit mono samples. The run quantum must be an integer number of chunks. |
| Audio allocation block | Contiguous group of physical chunks with one ownership count. Default geometry uses 32 chunks per block. |
| MIDI event block | Fixed capacity measured in events. Event timestamps and view duration remain measured in frames. |

Sources. [Architecture index](https://github.com/SanderVocke/shoopdaloop/blob/cc410f27/FPGA_ARCHITECTURE.md) · [Mixed top](https://github.com/SanderVocke/shoopdaloop/blob/cc410f27/fpga/midi/rtl/midi_audio_pipeline_axi.sv) · [Pipeline controller](https://github.com/SanderVocke/shoopdaloop/blob/cc410f27/fpga/job_processor/rtl/audio_pipeline.sv)

# State processor

The program has a sync prefix and a work section. The prefix executes one
`ProcessLoop` instruction per loop. After the prefix, a capture prepass
scans the work section for MIDI Capture instructions and queues writer
transitions. SP then walks the work section again to build the audio and
MIDI data jobs. All three scans use the same `sp_engine` hardware, including
its instruction controller, working registers and 65-bit adder.

![Figure 2. One engine runs the sync prefix, capture prepass and data pass. The loop record keeps the render position for this quantum and the position for the next one in separate words.](figures/state.svg)

## What lives on the FPGA

The program, node-state and loop RAMs in Figure 2 are **all on-chip
synchronous memories**. The job FIFO also uses on-chip RAM and a registered
head. These RTL arrays can infer FPGA block RAM (BRAM). The synthesis result
depends on the target and memory geometry. Consult the post-synthesis
utilization for the target profile to see the BRAM mapping.

| Storage | What it holds |
| --- | --- |
| Program RAM | 96-bit instructions. Live loading can provide a second program region. |
| Node-state RAM | 129-bit entries for values such as view IDs, enables and gain ramps. |
| Loop RAM | Sixteen 64-bit words per loop. They hold loop identity, mode, positions, capture length, revisions and two plan slots. |
| Job FIFO | 256-bit job descriptors for the current run. |
| Engine registers | Working operands while the engine processes one instruction or loop. The sync event shares a descriptor register with job emission. |

SP has **no DDR master or DDR-backed state RAM**. A view ID stored in SP
identifies audio content. The audio store, reached through JP, uses that ID
to find sample and metadata pages in DDR. A MIDI sequence ID selects
a view handle in the data-job unit. SP supplies the playback interval,
recording position and writer transitions. The data-job unit keeps the
view handles and event cursors and accesses its own DDR arena. In a
standalone fixture without a sync prefix, the work pass can read explicit
loop records supplied by the testbench on the `loop_read_*` ports.

## When a loop can change mode

**The sync pass makes loop decisions once per quantum, at its starting
frame.** A quantum contains one or more whole audio chunks. A chunk is a
storage and transfer size. Even if the quantum contains several chunks, SP
does one sync pass for the entire quantum.

Each loop has two slots for commands that can stop, play, record or seek.
For an **absolute-frame plan**, SP tests whether the requested frame is at or
before the current boundary. If frame 80 is already in a slot when a Q=32
run starts at frame 64, it is still pending at 64 and becomes eligible at 96.
The renderer processes the entire interval from 64 through 95 under one
settled loop mode. For a **source-pulse plan**, a pulse from the configured
sync source can make the plan eligible at the current boundary, after any
requested pulse skips and once the effective frame is due. A pulse is a
logical notification such as a loop restart or wrap. The compiler orders
source loops before dependent loops so the latter can see the source's pulse
from the same boundary. Each loop can deliver at most one pulse per quantum.
The newer eligible plan takes precedence where plans compete.

MIDI packets can carry sample offsets within the quantum. Those offsets tell
the receiver when to play an event. SP loop decisions still occur at the
boundaries.

![Figure 3. With Q=32, a request stored for frame 80 waits until the frame-96 boundary. The loop record and FIFO hold different parts of the state handoff.](figures/quantum.svg)

## How the render state reaches JP

SP owns loop timing. During `ProcessLoop`, it loads one loop into working
registers, applies eligible plans, resolves wraps and pulses, and performs
these writes to the **same loop record**:

1. **Save the value this quantum should use.** Write the current render
   position to word 8 and stamp the current boundary in word 9.
   Set the begin-capture flag in word 0 if the loop is recording and its
   capture length is still zero.
2. **Save the values the next quantum will start with.** For playback, advance
   position by Q and write it to word 2. For recording, grow the capture length
   in word 4 by Q. Word 3 is the content length used for playback. It can be
   shared by both passes because the advancement step leaves it intact.
3. **Read the saved render values to build jobs.** After the sync pass, the
   work instructions read the mode, begin-capture flag, render position and
   content length from the record and check its boundary stamp. SP then
   writes the required operands into the job FIFO. For example, ReadView
   carries a view ID, chunk position and length. First capture can emit
   BeginCapture ahead of Capture.

On the first Q=32 recording run, word 4 is already 32 when the work pass
reads the loop. The begin-capture flag still describes this run as the first,
so SP can queue BeginCapture followed by Capture.

JP finds everything it needs about the loop in the FIFO descriptor. It
processes samples or packets and updates the relevant media storage. SP
already wrote the next loop position or capture length during the sync
pass. JP completion lets the pipeline start another quantum using those
values. A failure can leave the earlier SP writes in place.

## Where sync events go

`sync_event_valid/ready/data` is a hardware output carrying 256-bit records
for loop transitions, wraps, pulses and plan results. The record includes a
loop slot, a boundary frame and a type-specific payload. The frame describes
the musical boundary. Transferring the record can take additional FPGA
clocks. This output lets a testbench or host observe the loop decisions. JP gets its work separately
through the job FIFO, and dependent loops get their pulse from loop RAM.

A Marlin test can accept the stream directly. The audio host wrapper routes
it into the mailbox's event queue for the Marlin host owner or Zynq ARM to
read. A wrapper with `EXPORT_SYNC_EVENTS=0` accepts events internally without
waiting for an external sink. When exported, SP holds the current event and
waits if `sync_event_ready` is low. The host mailbox instead accepts the
stream during a live session and flags a fault if its event queue overflows.

## Work pass and admission

A packed instruction has an 8-bit opcode, five 16-bit operands and eight
reserved bits. A job descriptor carries an 8-bit opcode, two 16-bit ID fields,
24 reserved bits and three 64-bit payload words. The work pass resolves
state addresses into the job operands that JP consumes.
A gain instruction emits an immutable ramp snapshot and advances the
persistent counter once per quantum. Audio playback jobs encode position and
length in chunks. MIDI jobs address ports, sequences, cursor slots and
event lists. Read also carries the loop identity to validate cursor reuse.
The capture prepass uses the writer-open flag in loop RAM. Jobs stay in
the FIFO until SP finishes.

The compiler supplies the **maximum number of job descriptors one run can
enqueue** as `run_job_budget` or, with live loading, in the switch descriptor.
The budget is measured in FIFO entries. For example, a budget of 20 needs
room for 20 descriptors even if a particular loop mode produces only 12.
A single instruction can emit more than one job. For example, a MIDI retake
can emit FinishCapture, BeginCapture and Append. Since JP starts only after SP finishes, the FIFO must
hold the whole list at once. The bound says nothing about execution time
or how many clock cycles those jobs take.

`run_valid && run_ready` starts admission checks for geometry, instruction
and state counts, an empty FIFO and enough FIFO slots for the budget. SP
reports a queue-full error when it cannot fit the declared budget.
A fault can leave earlier state writes and a partial queue. A caller using
the standalone core can inspect and drain that queue. The combined controller skips the normal JP pass on
SP failure. Reset invalidates execution and FIFO state while retaining RAM
payloads. Reload required records before resuming managed execution.

| Port family | Use |
| --- | --- |
| `cfg_*` | Read/write program or persistent state, with `cfg_loop` selecting loop RAM. Observe the response beat. Live loading permits writes to the inactive instruction region under its dedicated rules. |
| `run_*`, `done`, `error_code`, `instruction_pc` | Admit a run and identify success or the failing instruction. |
| `job_*`, `queued_jobs` | Drain the FIFO after the run. SP holds the descriptor stable under backpressure. |
| `read_intent_*` | Advisory pulse on playback enqueue, with no ready input. |
| `sync_event_*` | Ready/valid events, held until accepted. |

Sources. [SP RTL](https://github.com/SanderVocke/shoopdaloop/blob/cc410f27/fpga/state_processor/rtl/state_processor.sv) · [Executor](https://github.com/SanderVocke/shoopdaloop/blob/cc410f27/fpga/state_processor/rtl/sp_engine.sv) · [Loop RAM layout](https://github.com/SanderVocke/shoopdaloop/blob/cc410f27/fpga/state_processor/rtl/sp_loop_defs.svh) · [Instruction and port specification](https://github.com/SanderVocke/shoopdaloop/blob/cc410f27/fpga/state_processor/README.md)

# Job processor

`audio_pipeline` runs SP first, then JP. Each JP job finishes before JP
accepts the next one. The compiler uses numbered **scratch buffers**, which
are regions of on-chip sample RAM. For example, Input writes buffer 0, Gain
reads buffer 0 and writes buffer 1, then Output reads buffer 1. The dependency
is a concrete RAM read of the samples written by the earlier job. JP checks
per-buffer readiness bits that it clears at the start of each run.

**`MAX_FRAMES` defaults to 32** in `job_processor` and the main
pipeline wrappers. It limits the runtime quantum and sets the reserved size
of each scratch buffer, in samples. The address is
`buffer_id * MAX_FRAMES + frame_in_quantum`. With the default stride,
buffer 1 starts at word 32 even during a 16-frame run. A 64-frame run needs a
build with `MAX_FRAMES` at least 64. Chunk size is configured separately.

![Figure 4. An example FIFO and the scratch RAM it drives. Buffer IDs in each job select the physical sample addresses, with a stride of 32 words.](figures/jobs.svg)

| Job family | Activity | Duration depends on |
| --- | --- | --- |
| Input, Silence, Constant, Output | Copy or produce quantum data and accumulate output | Quantum frames and RAM/I/O scheduling |
| Gain, Arithmetic, Mix, Smoothing | Read scratch, apply signed fixed-point math, write results | Operand reads, pipeline fill and ramp initialization |
| ReadView, BeginCapture, Capture | Resolve playback or recording view, exchange sample words, wait for store completion | Cache and metadata residency, memory service and stream readiness |
| MIDI | Submit a data-job descriptor on `midi_job_*` and wait for the result | List length, DDR responses and downstream output acceptance |

Audio words are signed Q8.24. Mix saturates the sum. Gain multiplies, truncates
toward zero, then saturates. An active ramp uses a shared divider up front and
incremental quotient/remainder stepping across samples. Restart smoothing
retains per-history state across runs. Unstalled gain emits one result per
clock after pipeline fill. Mix reads two operands on one scratch read port and
can produce one result every two clocks. Those rates describe the inner sample
loops.

For playback or capture, JP translates chunk-based job operands to frame-based
store arguments. The store then resolves authoritative view metadata. JP's
late look-ahead of a following playback job can also supply an advisory hint.
`monitor_valid/ready/data` shows the SP FIFO head and its consumption, so a
trace can distinguish a stalled head from an accepted descriptor. `state_done`
ends SP execution. `jobs_done` ends JP execution. Check the pipeline
`error_code` together with `error_stage` on completion and examine the
`midi_error_code` for the cause of a MIDI failure. A failed MIDI job ends
the JP pass with `INVALID_JOB` and invalidates the quantum.

Sources. [JP RTL](https://github.com/SanderVocke/shoopdaloop/blob/cc410f27/fpga/job_processor/rtl/job_processor.sv) · [Audio job arithmetic and interfaces](https://github.com/SanderVocke/shoopdaloop/blob/cc410f27/fpga/job_processor/README.md)

# Audio media and caches

The media command layer accepts logical frame ranges within views. Metadata
maps a view's block ordinal to a physical allocation block. The low bits of
the position within that block select a chunk and then a sample. The sample
cache operates on physical chunk tags. At 32 frames/chunk and 32
chunks/block, a chunk is 128 bytes and a mono block is 4 KiB. For an aligned
view, block ID 7 and chunk index 3 within that block select physical chunk
`7 * 32 + 3 = 227`, starting `227 * 128 = 29,056` bytes into the sample arena.

![Figure 5. A worked audio address translation beside the sample and metadata layout in DDR.](figures/audio-storage.svg)

## Views, allocation and reclamation

On-chip view descriptors and roots identify dense radix-16 trees in DDR.
Leaves contain allocation-block IDs. The root state also caches a leaf
translation per view. Reference counts belong to physical blocks. A grabbed
range retains the blocks it intersects, including partial boundary blocks.
A writer can append beyond the published prefix. Clearing or replacing a
view can detach its old tree into the retirement FIFO and publish the new
view without walking the old recording.

The bounded collector walks detached trees from the tail. The final reference
to a block triggers grouped chunk-cache retirement. This drains matching dirty
writes and in-flight traffic, cancels speculative work, and invalidates
matching entries before the block ID is published on the free stack. Pending
retirements therefore consume capacity. The view pool, block pool, table page
pool and retirement queue each have independent capacity limits.

## Sample-cache operations

The sample cache has separate read and write RAM pools with associative tags,
round-robin victim selection and free-line preference. Defaults are four lines
in each pool. A line contains one fixed-size physical chunk. Directory state
tracks valid, loaded/dirty and transfer ownership. Read lines stay pinned until
the last sample is accepted.

| Operation | What the cache does |
| --- | --- |
| Read hit | Streams the resident chunk or forwards from the authoritative write line, including dirty data. |
| Read miss | Reserves a line, issues a demand fill, and streams after successful acknowledgement. |
| Full-chunk write miss | Installs the new data directly in a write line. |
| Partial write miss | Fills the previous chunk first, then overlays changed words. |
| Writeback | Drains dirty lines after a quiet interval of `CHUNK_FRAMES+8` clocks or sooner under pressure, fence or retirement. |
| Prefetch | Uses spare transfer capacity. Drops or invalidates superseded speculative data on conflicting writes and retirement. |

The audio pipeline can buffer a copy of SP's `read_intent_*` descriptor as
an early playback hint. JP can offer a second hint by looking ahead at a
queued playback job. The hint resolver looks up the view mapping and can
ask the sample cache to fetch the physical chunk. The hint only changes
when fetching can start. JP still executes the original playback job. Foreground work takes priority over new speculative
metadata lookups. A queued hint can be cancelled. An already accepted memory
fill can drain, with its superseded result discarded. The actual playback job
always re-resolves the view at execution time.

![Figure 6. In this example, a write supersedes a prefetch of the same chunk. A subsequent read takes the newer data from the dirty write line.](figures/cache-flow.svg)

Consider a FIFO sequence of read, capture and read. The first read can use a
prefetch. Capture may change the logical mapping or replace the bytes of an
existing chunk. The second read resolves the mapping anew and can forward
updated dirty data immediately. DDR writeback can follow later.

## Metadata cache and DDR layout

Metadata has separate fully associative writeback caches for table/directory
pages, block reference counts and the reusable-ID stack. A line is 16 words
(64 bytes). The defaults of 8, 4 and 2 lines total 896 bytes of metadata cache
payload. A playback hit can require only the table path. Ownership updates
can touch all three caches. Lines persist across normal quanta. On a miss, a
dirty round-robin victim is written back before the new line fills. A word
write updates the resident line and marks it dirty. Allocating a fresh table
page can install a zeroed line directly, with no read of its old bytes.

Let `A = round_up(SLOTS * CHUNK_FRAMES, 16)` and
`S = round_up(SLOTS / BLOCK_CHUNKS, 16)`. These are counts of 32-bit words,
and `SLOTS` counts physical chunks.

| Region | Word offset | Length (words) |
| --- | ---: | ---: |
| Audio samples | 0 | `A` |
| Block reference counts | `A` | `S` |
| Reusable block stack | `A + S` | `S` |
| Radix pages | `A + 2*S` | `16 * TABLE_PAGES` |
| Retired roots | `A + 2*S + 16*TABLE_PAGES` | `16 * RETIRED_TABLES` |

`shared_axi` gives the sample and metadata DMA clients one physical master.
It alternates competing clients at completed burst boundaries and holds
ownership through RLAST or B. There is one outstanding AXI transaction.
INCR bursts have up to 16 beats and stop at physical 4 KiB boundaries. A
32-sample chunk takes one full 64-bit burst or two full 32-bit bursts when
aligned and contained in a page. Device and controller backpressure affect
latency, so the burst bounds alone give no quantum deadline.

A fence is a command to the audio media store. It tells the sample cache
and the three metadata caches to write back dirty lines and waits for the
AXI write responses. For example, a Capture job can finish while its samples
still reside in a write buffer. A later JP read can use those samples
immediately from the buffer. If a host or simulator needs a coherent image
from the backing memory, it issues a fence and waits for success first.
That success means the AXI endpoint acknowledged the writes. The DDR timing
model tracks physical completion separately. A failed fill or writeback
latches a memory fault. Reset loses cache and allocator state while
external bytes can remain. Coordinate endpoint reset with the response source
and reload a coherent image. A failed transfer may already have changed some
physical bytes.

Sources. [Allocation blocks](https://github.com/SanderVocke/shoopdaloop/blob/cc410f27/fpga/media_store/BLOCK_ALLOCATION.md) · [Metadata layout](https://github.com/SanderVocke/shoopdaloop/blob/cc410f27/fpga/media_store/DDR_METADATA.md) · [Sample cache RTL](https://github.com/SanderVocke/shoopdaloop/blob/cc410f27/fpga/chunk_memory/rtl/chunk_memory.sv) · [Prefetch coherence](https://github.com/SanderVocke/shoopdaloop/blob/cc410f27/FPGA_CHUNK_PREFETCH.md) · [AXI endpoint](https://github.com/SanderVocke/shoopdaloop/blob/cc410f27/fpga/axi_memory/README.md)

# MIDI execution and storage

MIDI uses the same SP, job FIFO and JP as audio. SP chooses when a loop
records, plays, wraps, stops or seeks. JP dispatches MIDI jobs to
`midi_data_jobs`, a directly sequenced functional unit with a content-store
controller. MIDI data jobs read and write lists of complete timestamped
messages. A Stop changes the loop mode and can finalize a take. It sends
no automatic Note Off, pedal release or controller correction. The stored
message bytes are what playback sends.

![Figure 7. SP puts recording transitions before data jobs. JP moves timestamped event lists through a shared DDR scratch arena and keeps list descriptors and reader cursors on chip.](figures/midi.svg)

## Order of work within a quantum

1. **Validate input.** The mixed wrapper latches the input address and
   record count, then checks the entire batch before starting SP. Each
   16-byte staging record has four u32 words: input port, offset within
   the quantum, packed message, and ignored padding. The array is
   16-byte aligned after the declared MIDI arena, inside the AXI window.
   Sort the array by offset, keep offsets below Q, and preserve it until
   completion.
2. **Settle loop timing and queue writer transitions.** SP runs its sync
   prefix. It then scans the work program for MIDI Capture instructions
   and queues FinishCapture or BeginCapture as needed. The writer-open
   bit in SP loop RAM records the intended writer lifecycle. This pass
   completes before any regular data job is queued.
3. **Queue and execute the data jobs.** SP walks the regular work pass in
   program order. JP then dispatches each queued descriptor and waits for
   its result. On playback, SP supplies the loop-relative starting
   position, Q, a channel mask and a restart flag. The MIDI unit resolves
   the sequence's view and moves the messages in that interval. It gets
   no loop mode input.
4. **Run bounded retirement.** After a successful JP pass, the mixed
   wrapper waits for MIDI retirement before asserting `done`. The
   default retirement budget is 64 steps per quantum.

| JP job | Reads | Updates or produces |
| --- | --- | --- |
| MIDI Input | Staged batch and selected input port | One scratch list with that port's messages |
| MIDI Empty | Destination list ID | A zero-length list |
| MIDI Read | Sequence handle, cursor and stored view | A scratch list for the interval, plus the updated cursor |
| MIDI BeginCapture | Sequence slot and any old view | A new writer view and its sequence handle |
| MIDI Append | Source scratch list and recording interval | Timestamped records and the writer's new duration |
| MIDI FinishCapture | Open writer and its accumulated duration | A completed view that a later Read can use |
| MIDI Merge | Two ready scratch lists | One sorted list, or an alias of the nonempty input |
| MIDI Output | One ready scratch list | Packet handshakes for the selected output port |

## Variable event counts and the job budget

An audio quantum fixes a sample count. A MIDI quantum fixes a time
window. It can carry zero messages or many messages at the same offset.
The input count is supplied with the run as a 32-bit record count. The
RTL checks input shape and memory access. The size of a storage block
places no per-quantum event limit on the run.

The compiler reserves one job for each MIDI Input, Playback, Merge, or
Output instruction, and up to three for each MIDI Capture instruction.
Three slots cover a retake that finishes an old writer, begins a new
one and appends a list. This sizing depends on the graph. A burst makes
the jobs run longer without adding FIFO descriptors. The RTL component
tests record and play 3,000 messages at one sample offset and separately
merge 1,200 messages. At the default block geometry, 3,000 stored events
take twelve blocks. Deadline measurements use their stated graph, memory
delay and event distribution.

| Capacity | Mixed-top default | What uses it |
| --- | ---: | --- |
| Sequence slots and reader cursors | 128 each | Sequence-to-view handles and independent playback progress |
| List descriptors | 256 | Start/count pairs in BRAM |
| Scratch records | 131,072 | Shared DDR space for Input and Read lists and intermediate Merge results |
| Events per view | 8,192 | Lifetime capacity of an individual take or imported view |
| Event blocks | 4,864 × 256 records | Stored takes and shared views, 19 MiB of event payload |
| Views and retirement entries | 512 and 256 | Shared ranges, retained exports and pending releases |

## What stays in BRAM and DDR

A **sequence** is a stable on-chip index holding a 64-bit view handle.
The view descriptor in DDR gives its event count, duration, time origin,
first-event offset and block-ID table. Every stored event is 16 bytes:
a 64-bit timestamp, a 32-bit insertion order and a packed message.
Blocks fill by event count. Batch Append advances time even for a list
of length zero and allocates blocks as messages arrive. Completed views
and range views can share blocks. A retained export keeps an old take's
blocks alive across a retake.

A **scratch list** is an on-chip start/count pair pointing to a contiguous
run of records in the shared DDR scratch arena. Each 16-byte record holds
quantum offset, source ID, insertion order and packed message, all u32.
The arena's allocation cursor resets for each prepared quantum. Merging
two nonempty lists copies their events to a new list in offset, source
and insertion order. Equal-time messages and duplicates remain distinct.
An empty merge can reuse the other list's start/count pair. Account for
intermediate lists when sizing scratch memory.

A **reader cursor** lives on chip and retains the active view and owner
identity, next event index and one fetched future event. Starting,
wrapping or seeking can reset this cursor. A nonzero restart position
uses binary search, followed by ordinary forward reads. If the next
event is already held and falls beyond the current interval, Read
returns an empty list without fetching from DDR. The store also keeps
the current view descriptor and block lookup in working registers.
MIDI uses acknowledged 32-bit word accesses through its own AXI adapter.
The separate audio path keeps its sample and metadata caches.

## Output and fault handling

The MIDI unit holds destination, offset, length and packet bytes stable
until `midi_packet_valid && midi_packet_ready`. A high
`midi_packet_refused` on that transfer faults JP. Previously accepted
packets remain delivered. There is no instrument-state tracking or
restoration traffic.

Malformed staged input reports an error before SP advances and can be
corrected and retried. A memory fault during validation, a failed job,
or a retirement failure latches a fault in the connected pipeline.
The caller must discard the failed quantum's audio buffers. The wrapper
holds off future runs until reset. Storage exhaustion can leave an
incomplete recording prefix, and an incomplete view cannot be published
as a successful take. Use `midi_error_code` for the MIDI cause alongside
the shared `error_code` and `error_stage`.

## Content publication and live loading

Between quanta, the MIDI control port can import, export, release,
create a range, replace, duplicate or clear a sequence. Successful
replace, duplicate or clear raises `midi_publication_valid` and fences
run admission. The caller must have identified the matching stopped
SP loop and completed any writer. It updates the loop's mode, position,
length, revisions and plans as required, then asserts
`midi_publication_commit`. The command response can then complete.
A compatible live-loading switch preserves the sequence and cursor
IDs it carries forward. The loading caller is responsible for preserving
an open writer's binding across the handoff.

Sources. [MIDI protocol](https://github.com/SanderVocke/shoopdaloop/blob/cc410f27/fpga/midi/PROTOCOL.md) · [Mixed wrapper](https://github.com/SanderVocke/shoopdaloop/blob/cc410f27/fpga/midi/rtl/midi_audio_pipeline.sv) · [Data-job RTL](https://github.com/SanderVocke/shoopdaloop/blob/cc410f27/fpga/midi/rtl/midi_data_jobs.sv) · [Current validation](https://github.com/SanderVocke/shoopdaloop/blob/cc410f27/docs/fpga/midi-data-jobs.md)

# Marlin and ARM interfaces

Marlin can exercise the component ports directly or run host software through
the AXI-Lite mailbox. The Zynq audio integration uses the mailbox register
contract. The mixed core independently exposes MIDI packet, command,
publication and memory ports. Connecting them to application device routing,
ARM configuration, and SoC DDR arbitration is a separate integration step.

![Figure 8. The direct Marlin test path exposes component signals. The audio host path carries control through AXI-Lite and storage traffic through a separate AXI port.](figures/host.svg)

## Marlin component and mailbox tests

The Rust/Marlin testbench drives the synthesized-design interfaces in
simulation. It can load packed program/state entries, initiate runs, monitor
job acceptance and events, and exchange media and MIDI traffic. The functional
AXI responder exercises RTL and real backing bytes in simulation. Use the
DRAMsim3 agent to explore memory timing, and the fixed-delay or fault-injection
modes to exercise stalls and error paths. Rust provides expected logical
results independently of the FPGA cache timing.

The `shoop_rt_host_marlin` owner and mailbox tests exercise the software
side through MMIO and the actual mailbox RTL. They cover a different boundary
from direct mixed-core tests. The mailbox drives the audio-only specialization
of `media_audio_pipeline_axi`.

## Zynq ARM audio bridge

The Z-turn block design routes the PS GP0 host aperture to AXI-Lite and the
fabric's 64-bit sample/metadata AXI master to PS HP0. GP0 carries commands and
results. HP0 carries audio storage traffic to the reserved DDR aperture.
`host_mailbox` holds request, response and event transport.
`host_dispatch` drives configuration, switch, snapshot and audio control.
The compiled-audio bring-up can use a deterministic source or the external
I²S adapter, based on the build profile.

| Mailbox concern | Version 6 register anchors |
| --- | --- |
| Discover the image | `ID` 0x000, `VERSION` 0x004, `STATUS` 0x008, `EPOCH` 0x00c, geometry at 0x010–0x03c |
| Submit command | `REQUEST_EPOCH` 0x040, `REQUEST_OP` 0x044, payload from 0x048, `REQUEST_PUSH` 0x078 |
| Consume response | `RESPONSE_TAG` 0x080, `RESPONSE_CODE` 0x084, payload from 0x088, `RESPONSE_POP` 0x0b8 |
| Consume events | `EVENT_COUNT` 0x0c0, tag/data from 0x0c4, `EVENT_POP` 0x0f8, revision at 0x0fc/0x100 |
| Observe service | Completed quanta 0x104, peak quantum cycles 0x108, audio xruns 0x10c, abort 0x110 |

The register schema defines payload widths, operations and result codes.
The owner checks the epoch and capabilities, stages a request when room is
available, then reads and pops the result. Program upload, request/response
polling and the device's audio cadence have separate roles.

## Live program handoff

The allocator preserves addresses for continuing instances and initializes
new slots before activation. The loader writes the inactive instruction
region. A revision-qualified switch selects it at a quantum boundary.
The pipeline waits for old job and hint queues and the media handoff fence
before returning completion and ownership. SP state remains live across that
switch. The returned first frame identifies the new program's boundary.
The host must wait for completion before reclaiming the previous region or
the retired state bindings.

## Audio device clock boundary

The board audio path uses two banks between the sample clock and core clock.
The I²S adapter serializes codec traffic at the device side. The core is
paced by bank handoff and uses external sample I/O on JP. The bank controller
coordinates ownership across the clock domains. Completion releases output
for the device, while watchdog and xrun accounting indicate missed service.
For the provisional Z-turn I²S2 profile, the supplied external 12.288 MHz
reference drives 48 kHz stereo I²S. The current graph consumes the left
input and duplicates mono output to both DAC channels.

Sources. [Marlin runner](https://github.com/SanderVocke/shoopdaloop/blob/cc410f27/src/rust/rt_fpga_testbench/README.md) · [Mailbox schema](https://github.com/SanderVocke/shoopdaloop/blob/cc410f27/fpga/host_bridge/registers.json) · [Z-turn bridge](https://github.com/SanderVocke/shoopdaloop/blob/cc410f27/fpga/host_bridge/README.md) · [Live loading](https://github.com/SanderVocke/shoopdaloop/blob/cc410f27/LOADING_DESIGN.md) · [Audio bank RTL](https://github.com/SanderVocke/shoopdaloop/blob/cc410f27/fpga/audio_io/rtl/audio_io.sv)

# Verification guide

The most direct way to follow a quantum is to capture run acceptance,
`instruction_pc` and sync events, job acceptance, and pipeline completion
on the same clock axis. For a slow audio job, add the view translation,
sample cache tag and relevant DMA handshakes. For a slow MIDI job, add the
input validation and retirement phases, the list start/count pairs, job
completion, packet acceptance and the word-memory response.
For an issue at the ARM boundary, record the mailbox epoch, active
revision, event count and the audio bank release.

| Question | Starting point |
| --- | --- |
| Did loop mode and render position settle as expected? | Marlin `sync` test and SP sync-event stream |
| Did the compiled sequence drive the intended jobs and produce the right samples? | `state`, `audio`, `pipeline`, and mixed `midi` tests |
| Did the second read observe intervening writes? | `chunk`, `metadata`, `ownership` and `deferred` tests |
| Did ownership survive a switch at the intended frame? | `loading` and `shoop_rt_host_marlin` mailbox tests |
| Did a memory or output stall cause a problem? | `axi` and `midi` tests with delayed responses and refusal cases |
| Is the implementation fast enough for the target profile? | Routed timing report, cycle counts per tested quantum, then measured device deadlines |

Run individual suites from the repository root.

```sh
cargo test --locked -p rt_fpga_testbench --features sim --test sync
cargo test --locked -p rt_fpga_testbench --features sim --test midi -- --test-threads=2
cargo test --locked -p rt_fpga_rtl --test midi_target
cargo test --locked -p shoop_rt_host_marlin --features sim --test mailbox_endpoint
```

Functional simulation establishes behavior for the tested cases. Static
timing reports establish clock timing for a routed profile. End-to-end
deadlines also depend on the job and event distribution, memory delays,
backpressure and device cadence. At 48 kHz, Q=32 offers about 667 µs between
audio quanta. Compare the available time to the full service interval,
including bank handoff, under the intended workload.

Sources. [Testbench and reproduction commands](https://github.com/SanderVocke/shoopdaloop/blob/cc410f27/src/rust/rt_fpga_testbench/README.md) · [Current MIDI RTL validation and Vivado measurements](https://github.com/SanderVocke/shoopdaloop/blob/cc410f27/docs/fpga/midi-data-jobs.md) · [64-loop quantum measurements](https://github.com/SanderVocke/shoopdaloop/blob/cc410f27/docs/fpga/midi-data-jobs-benchmarks.md)
