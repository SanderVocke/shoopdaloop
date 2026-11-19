# MIDI component protocol

All interfaces use one clock. Configuration (`arena_base`, `quantum_frames`, AXI
mapping and arena limits) stays stable during commands. One runtime command is
active at a time. Admission is `command_valid && command_ready`; responses remain
valid until `response_ready`. Pools are positive and bindings number at most 128.

The memory interface issues one aligned byte address, read/write flag and 32-bit
payload, then waits for acknowledgement. Writes must be visible before dependent
state/ownership changes. The AXI wrapper uses narrow single-word transactions,
with correct lanes on both bus widths. A response follows all acknowledged writes.

Output proposals stay stable while `packet_valid && !packet_ready`. A handshake
with `packet_refused == 0` accepts a packet; one with `packet_refused == 1` refuses
it. Destination and quantum-relative sample offset accompany each packet. Only
acceptance advances musical state.

## Values and lifetime

Handles are `{generation:u32, slot:u32}` in 64 bits. Zero means no handle. Preparing
an image starts a new lifetime and invalidates old handles. Bindings are immutable
within that lifetime. Logical loop identities use the same shape but are distinct
from finite binding-slot indices.

Packed packets contain status/data bytes in bits 0–23, length in bits 24–25, and
reserved zeros above. Unused bytes have canonical zero padding. Input-window
records have four words: input index, sample offset, packed packet, reserved.
Offsets are globally ordered; equal-time order is retained.

Event payloads have four words: backing timestamp low/high, insertion order and
packed packet. A view subtracts its time origin when returning a record. Start
is inclusive, end exclusive; silence occupies duration metadata.

Import/export/history-range handles are owned and must be released. Publication
handles name content owned by bindings; export before retaining an independent
version. Starting-state addresses remain valid while the view is owned. Current
input/output-state query addresses are borrowed: inspect while commands remain
quiescent.

## Commands

Arguments are 64 bits; unused fields are zero. Unlisted response fields have no
meaning. State/event staging addresses must be aligned and accessible in the
mapped arena. Caller staging buffers stay immutable until the command completes
and must not overwrite core-owned regions.
These are memory operations, not file formats.

| Op | Operation | Arguments 0–4 | Result |
| --- | --- | --- | --- |
| 0 | Prepare | zero | Fresh image, states and histories. |
| 1 | Bind | slot, logical identity, input, destination, mask | Immutable binding; duplicate identity refused. |
| 2 | SP record | slot, mode, position, length, begin-capture | Record for next processing command. |
| 3 | Process | start frame, quantum frames, input address, count, 0 | Output stream; response frame is interval end. |
| 4 | Group grab | input, start, end, target-list address, count | Shared view owned by bindings; duration/state address. |
| 5 | Replace | slot, view, 0, 0, 0 | Retains complete, prepared content. |
| 6 | Duplicate | source slot, target slot, 0, 0, 0 | Shares source content. |
| 7 | Clear | slot, 0, 0, 0, 0 | Releases content/writer ownership. |
| 8 | Begin import | starting-state address, 0, 0, 0, 0 | Owned private writer. |
| 9 | Import event | writer, frame, packet, 0, 0 | One event transfer. |
| 10 | Finish import | writer, duration, comparison budget, 0, 0 | Count 1: prepared; 0: call again. Budget capped at 256. |
| 11 | Export | slot, 0, 0, 0, 0 | Owned view, duration, event count, starting-state address. |
| 12 | Export event | view, index, 0, 0, 0 | Frame, order in count, packet/length; length 0 at end. |
| 13 | Release | owned view, 0, 0, 0, 0 | Releases one reference. |
| 14 | Send | destination, packet, sample offset, 0, 0 | Explicit tracked output. |
| 15 | History range | input, start, end, 0, 0 | Owned view, duration, count, starting-state address. |
| 16 | Retention | input, retained frames, 0, 0, 0 | Before processing only; zero keeps a rolling prefix. |
| 17 | Recovery | sample offset, 0, 0, 0, 0 | Resumable; no time advance. Works after DDR failure. |
| 18 | Pipeline fault | zero | Marks destinations/contexts for recovery. |
| 19 | State query | kind (0 input, 1 output), index, 0, 0, 0 | Borrowed address; output can be busy. |
| 20 | Maintenance | budget, 0, 0, 0, 0 | Steps in count; budget capped at 64. |
| 21 | Record limit | slot, event limit, 0, 0, 0 | Stopped binding only; 1 through `MAX_EVENTS`. |
| 22 | Group clear | target-list address, count, 0, 0, 0 | Atomic clear. |
| 23 | Group replace | view, target-list address, count, 0, 0 | Atomic retained-content publication. |

Modes are 0 stopped, 1 recording, 2 playing. SP supplies position and timing.
Processing preflights the input window; calls are contiguous and span one
configured quantum. Playback starts/restarts at position zero. Target lists hold
distinct 32-bit binding slots. Published lengths are quantum multiples. Publication
stops/resets affected loop metadata; the embedding commits the same content
lengths/revisions to SP before its next execution.

Control grants are shared across calls. Import comparison initialization resumes
and stays current as output changes. Binding requires ready destination maps.
History ranges have positive duration but can contain zero events. Direct
content-store ranges additionally permit zero duration.

Errors: 0 success, 1 invalid packet/window, 2 unsupported, 3 capacity/output
refusal, 4 invalid/stale/incomplete state, 5 busy, 6 overflow, 7 memory fault.
Capacity/refusal leaves the image usable. Memory failure invalidates it; recovery
then uses on-chip acceptance counters without DDR. Queries/processing require a
new image. Recovery does not claim unreadable state was restored.

## DDR layout

Offsets are relative to `arena_base`. Let `V` be views, `T` table entries, `B`
event blocks, `E` events per block, `S` snapshots and `R` retirements.

| Content region | Offset | Bytes |
| --- | --- | --- |
| View descriptors | 0 | `64 V` |
| Block tables | previous end | `4 V T` |
| Block references | previous end | `4 B` |
| Free-block stack | previous end | `4 B` |
| Snapshot slots | previous end | `16916 S` |
| Event payload | previous end | `16 B E` |
| Retirement records | previous end | `8 R` |

A view has sixteen words: generation, references, status, blocks, first-event
offset, event count, origin low/high, duration low/high, snapshot slot/generation,
next order, event limit, reserved, owner. Status: 0 unused, 1 writer, 2 complete,
3 incomplete, 4 retired. Owner: 0 imported/external, 1 history, 2 recording.
Capture ownership prevents UI import edits. Each snapshot has four header words
(generation, references, two reserved), followed by 4,225 state words.

The runtime reserves a private window for 128-byte loop descriptors. These live
in primary BRAM (at most 16 KiB); the corresponding DDR address space is unused.
Following that window, DDR contains input/output/activity states
(16,900 bytes each); 96-byte history descriptors; checkpoint rings (16 bytes × `S`
per input); 532-byte changed-cell maps per input; 532-byte comparison maps per
snapshot/destination; 48-byte comparison directory entries; and one 532-byte
replay-change map. Loop-activity states remain in DDR; their size differs greatly
from the small loop descriptors. Other on-chip data is temporary working values
and finite counters.

Comparison entries contain twelve words: valid, reserved, target address low/high,
current-state address low/high, bitmap address low/high, snapshot-header address
low/high, expected generation, initialized-cell count. Output updates check live
matching-generation targets and only initialized cells. Derived maps publish
count 4,225 after copying their base and patching changed cells.

## State image

Sixteen channels each occupy 264 words, followed by System-Reset-seen. This is an
explicit hardware representation, not a Rust structure dump.

| Channel cell | Value |
| --- | --- |
| 0–127 | Velocity in bits 0–7, held/sostenuto flags in bits 8/9, pressure in bits 16–23. |
| 128–255 | Last known controller value. |
| 256 | Loaded program. |
| 257–258 | Bank MSB/LSB used when loading that program. |
| 259 | Fourteen-bit pitch bend, initially 8,192. |
| 260 | Channel pressure. |
| 261 | Reset Controllers seen. |
| 262–263 | Basic-state reserved fields, zero. |

Optional seven-bit values use 128 for unknown, distinct from known zero. Notes
start with zero velocity/flags and unknown pressure. Sustain/sostenuto/Hold 2
start known zero. Parameter-editing controllers are unknown. Reset-history flags
are retained but do not request synthetic reset packets during restoration.
