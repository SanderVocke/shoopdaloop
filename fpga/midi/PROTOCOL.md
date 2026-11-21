# MIDI data-job protocol

This interface replaces the former phase-based MIDI service. Reload compiled
programs and format a fresh arena; old control commands and memory images are not
compatible. There is no instrument-state ABI.

## Jobs and compiled instructions

All jobs use the existing 256-bit shared JP packet. Bits 7:0 hold the kind, 23:8
operand A, 39:24 operand B, 63:40 reserved zero, and 127:64/191:128/255:192 the
64-bit payloads P0/P1/P2.

| Kind | Job | A | B | P0 | P1 | P2 |
| --- | --- | --- | --- | --- | --- | --- |
| 0x20 | Input | input port | destination list | 0 | 0 | 0 |
| 0x21 | Empty | destination list | 0 | 0 | 0 | 0 |
| 0x22 | Read view | sequence | destination list | position in frames | packed interval/read options | reader owner |
| 0x23 | Begin capture | sequence | 0 | 0 | 0 | 0 |
| 0x24 | Append | sequence | source list | recording position | frame count, low 32 bits | 0 |
| 0x25 | Finish capture | sequence | 0 | 0 | 0 | 0 |
| 0x26 | Merge | left list | right list | destination list | 0 | 0 |
| 0x27 | Output | source list | output port | 0 | 0 | 0 |

Read P1 contains frame count in bits 31:0, cursor in 39:32, channel mask in 55:40,
and restart in bit 56. The upper seven bits are zero. P2 contains owner slot in
31:0 and generation in 63:32; it is a cache identity, not loop state for JP to
interpret. Read and append frame counts equal the admitted quantum size.

The 96-bit SP instructions are:

| Opcode | Instruction | Operands after opcode |
| --- | --- | --- |
| 0x0e | MIDI input | port u16, destination u16 |
| 0x0f | MIDI playback | loop slot u16, generation u32, sequence u8, cursor u8, channel mask u16, destination u8 |
| 0x10 | MIDI capture | loop slot u16, generation u32, sequence u16, source u16 |
| 0x11 | MIDI merge | left u16, right u16, destination u16 |
| 0x12 | MIDI output | source u16, port u16 |

Playback uses all 96 bits. Other unused instruction bits are zero. Playback's
sequence/cursor/destination IDs are limited to 0–255 by this encoding; this does
not limit message counts. The target rejects wider operands. Logical program
validation prevents conflicting writers and cursor reuse within one program.

SP loop flags use bit 57 for MIDI writer-open and bit 58 for the current quantum's
seek/reposition indication. These are transport bookkeeping, not instrument state.
Keep these bits when updating only plans. SP queues capture begin/finish jobs in
a preliminary work-program scan, then performs the ordinary audio/MIDI data pass.
Capture append jobs are emitted only while recording; inactive playback produces
an empty list. Musical position and mode remain exclusively in SP.

## Quantum and memory interfaces

The connected top accepts one quantum through the existing `run_valid/run_ready`
handshake. It latches run parameters, validates the complete input array, then
starts the audio/SP pipeline. After JP finishes, bounded MIDI retirement completes
before `done`. A surrounding callback driver splits multi-quantum callbacks and
rebases MIDI input/output offsets for each quantum.

The isolated functional unit exposes `prepare_valid/ready/done/error`, job
ready/completion, and `finish_valid/ready/done/error`. Admission priority is prepare,
finish, job, then control. Only the connected wrapper should arbitrate an active
engine's control and quantum requests. Do not reuse input memory until the quantum
finishes.

Each input record contains four u32 words: input port, quantum-relative frame
offset, packed message, ignored padding. Input records are globally ordered by
offset; each offset is less than the quantum size. The caller supplies a 32-bit
record count, without a 256-message cap. Input storage is 16-byte aligned and lies
after the MIDI arena, within the configured memory transport window.

All output and memory ports use ready/valid handshakes. Fields remain stable while
stalled. `packet_refused` is sampled only when ready accepts a packet. Successful
output packets are irrevocable. A failed quantum is invalid, including its audio
buffers; its caller must not publish those buffers as successful audio output.

AXI addresses are translated from logical `arena_base` through the configured
physical base/window. The transport supports 32/64-bit data, word write strobes,
backpressure, and error responses. The audio and MIDI AXI masters have separate
public ports, as before; system-level DDR arbitration is external.

## DDR layout and small BRAM tables

Let V=views, T=table entries per view, B=blocks, E=events per block, R=retirements,
and C=shared scratch-event capacity. Offsets below are relative to `arena_base`.

| Region | Offset | Bytes |
| --- | --- | --- |
| View descriptors | 0 | V × 64 |
| View block-ID tables | V × 64 | V × T × 4 |
| Block references | end of tables | B × 4 |
| Free block stack | end of references | B × 4 |
| Stored event records | end of free stack | B × E × 16 |
| Retirement entries | end of records | R × 8 |
| Padding | end of retirements | up to 15, to align scratch to 16 bytes |
| Shared scratch lists | aligned end of retirements | C × 16 |

`arena_bytes` includes all these regions. The caller's memory window additionally
covers the input array. Default T is ceil(MAX_EVENTS / BLOCK_EVENTS) + 1, allowing
a partial first block. No histories, instrument states, snapshots, difference
maps, or comparison directories are allocated.

A stored record is timestamp u64, insertion order u32, packed message u32. A
scratch record is quantum offset u32, source ID u32, insertion order u32, packed
message u32. The packed message contains bytes in bits 23:0, length 1–3 in 25:24,
and zeros in 31:26. Unused message bytes should be zero. Playback source IDs are
cursor IDs; live input source IDs are 0x10000 OR input port. Merges compare offset,
source, then order. They preserve duplicates and deterministic equal-time order.

View descriptors contain sixteen u32 words:

| Words | Meaning |
| --- | --- |
| 0–2 | generation, references, status |
| 3–5 | block count, first-event offset, event count |
| 6–7 | time origin u64 |
| 8–9 | duration u64 |
| 10–11 | zero/reserved |
| 12–13 | next insertion order, event capacity |
| 14–15 | zero/reserved |

Status: empty=0, recording=1, complete=2, incomplete=3, retired=4. Handles contain
slot in low 32 bits and nonzero generation in high 32 bits. Range views select
[start,end), share referenced blocks, and rebase timestamps. Release queues a view
for bounded retirement. Retirement is bounded work; event processing has no such
quota. Batch append updates the descriptor once for the whole source list and
preserves an incomplete prefix if event/block capacity is exhausted.

Only sequence handles, list start/count pairs, cursor lookahead, and validity flags
are held in small BRAM/register structures. Scratch payload capacity does not
multiply BRAM use per loop. The store's current descriptor and block pointer are
working registers, not a general DDR caching subsystem.

## Control interface

Commands are admitted between quanta. Once admitted they complete before a new
quantum; the caller schedules sufficiently small transfers during active rendering.
Format and large initial preparation belong outside rendering. Reserved commands
return Unsupported. Arguments not listed below are ignored.

| Op | Action | Arguments |
| --- | --- | --- |
| 0 | Format content and reset sequence/cursor tables | none |
| 5 | Replace sequence's completed view | sequence slot, view |
| 6 | Duplicate sequence's completed view | source sequence, destination sequence |
| 7 | Clear sequence | sequence slot |
| 8 | Begin import | none; returns an owned writer handle |
| 9 | Append import event | view, frame, packed message |
| 10 | Finish import | view, duration |
| 11 | Retain/export sequence view | sequence slot; returns owned handle, duration, event count |
| 12 | Read view event | view, index; returns rebased frame, packet/length, insertion order in response_count |
| 13 | Release owned import/export/range handle | view |
| 15 | Create range view | source view, start, end; returns owned handle |
| 20 | Maintenance | maximum steps, capped at 64 |

Imports can be slow and incremental. They allocate independent content; replacing
or duplicating shares content rather than copying it. Retain/export only accepts
completed views. Never release a sequence's borrowed handle as if it were a
separately retained export.

Successful replace/duplicate/clear holds `publication_valid` until the caller
updates the matching stopped SP loop (length, mode, position, revisions, and plans)
and acknowledges `publication_commit`. The caller must also ensure its recording
writer has finished. No new quantum is admitted during this fence. Then the normal
response handshake completes. Compatible program switches preserve sequence and
cursor slots; removing/rebinding an open writer must be refused by the loading
caller, matching the Rust reference's guard.

Error codes: invalid event=1, unsupported=2, capacity=3, invalid handle/lifecycle=4,
busy=5, overflow=6, memory fault=7, incomplete=8, output refused=9. Malformed input
is rejected before SP runs and can be retried. Execution/cleanup faults latch in
the connected pipeline until reset. A memory fault additionally invalidates the
prepared image; reset and format/reload before resuming. There is no instrument
state recovery or automatic correction traffic.

## Optional expression-state extension (ABI 0x45580100)

Elaborate `STATE_SUPPORTED=1` (`MIDI_STATE_SUPPORTED` at the connected top) to
include expression data jobs. The default remains raw events only. Command 31
returns ABI 0x45580100 in response_count or Unsupported. Reload programs and format
when selecting this extension. Bulk state is appended after the scratch lists.

SP opcode 0x13 encodes owner slot u16, generation u32, sequence u8, source input
u8, output mask u8, and channel mask u16 after the opcode. State bindings follow
all capture instructions. The loader prepares the same bindings with command 21.
SP flag bit 59 remembers playback activity; restore is queued at start/repeat/seek
and cleanup when leaving playback. These jobs precede normal event jobs.

| Job | A | B | P0 | P1 | P2 |
| --- | --- | --- | --- | --- | --- |
| 0x28 restore | sequence u16 | 0 | position frames | owner slot u32 + generation u32 | 0 |
| 0x29 cleanup | sequence u16 | 0 | 0 | owner slot u32 + generation u32 | 0 |

| Control | Arguments 0–3 | Result |
| --- | --- | --- |
| 21 bind | sequence, owner u64, packed input u8/output mask u8/channel mask u16 | error |
| 22 seed field | space (0 input/1 output/2 snapshot), input/output ID or view handle, field index, u32 value | error |
| 23 controller policy | output, low mask u64, high mask u64 | error |
| 24 read field | space, ID/handle, field | u32 value in response_count |
| 31 ABI query | none | ABI in response_count |

Seed outputs before binding; changing physical output assumptions after bindings
returns Busy. Snapshot seeds require an empty writer. New snapshot channels are
cleared before their first seed. Completed snapshots are immutable. Policy changes
refresh comparisons outside quantum execution. Source seeds may describe held
input before recording. Control transfers are serialized with rendering.

The state arena contains input, output, per-view and one seek-scratch state, each
32 KiB, followed by (sequences × outputs + 1) comparison slots of 2 KiB. A state
channel occupies 512 u32 words; fields 0–127 are notes, 128–255 CC, 256 pitch and
257 channel pressure. Note bit layout matches the documented Rust contract.
Known is bit 31. Unused words are reserved. View validity/channel masks are small
on-chip metadata tied to the store's validated generations and view lifetimes.

A comparison slot has difference words at bytes 0–1023, eight u32 nonempty-word
summaries at 1024–1055, and owner activity at 1056–1375. Activity has five u32
words/channel: 128 note bits and owned sustain/sostenuto flags. Remaining bytes
are reserved. A zero summary bit makes its difference word invalid/empty, so
retakes need not clear every bulk difference word. Counts and connection masks
are on-chip summaries; they do not store instrument state. The extra comparison
slot is private seek scratch. DDR faults invalidate the engine as before.

Source input jobs observe each input once per quantum, even while stopped.
Accepted ordinary and corrective outputs update one output model and indexed
comparisons. Restore emits directly to the destination, preserving pedal/note
phase order through backpressure. No new processor, instruction interpreter,
MIDI scheduler, or persistent restoration-message lists are introduced.
