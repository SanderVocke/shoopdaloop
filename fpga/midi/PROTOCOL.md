# Event-only MIDI RTL protocol

This revision intentionally has no instrument-state ABI. It is not a compatible
reader for earlier state-capable RTL memory images. Reformat/reload the arena.
The Rust FPGA reference retains its independent, state-capable interface.

## Composition and handshakes

`midi_audio_pipeline_axi` is the connected SP/mixed-queue/JP top. The isolated
`midi_job_service` and `midi_job_service_axi` expose the same service commands for
testing. Commands, responses, memory transactions and output packets use
ready/valid handshakes. A packet remains stable while the receiver is not ready.
A receiver reports `packet_refused` with its ready acknowledgement; the affected
playback loop then suppresses subsequent events until retrigger. No recovery or
cleanup packets are generated. A memory error invalidates the prepared image;
a new format/load is required.

SP remains responsible for loop modes, positions and transitions. The service
uses those settled values. Host publication must update corresponding SP rows
and acknowledge `midi_publication_commit` before another quantum can start.

## Memory layout

All offsets below are relative to `arena_base`. Let V be views, T table entries
per view, B event blocks, E events per block, R retirement entries, L loops and I
inputs.

| Region | Offset | Bytes |
|---|---:|---:|
| View descriptors | 0 | V × 64 |
| View block-ID tables | V × 64 | V × T × 4 |
| Block reference counts | end of tables | B × 4 |
| Free block stack | end of references | B × 4 |
| Event records | end of free stack | B × E × 16 |
| Retirement queue | end of records | R × 8 |
| Reserved logical loop-descriptor window | end of queue | L × 128 |
| Input history metadata | end of descriptor window | I × 96 |

Loop descriptors are primary BRAM in the generated service; their reserved
logical address window is not accessed in DDR. `arena_bytes` reports the entire
logical window above. No state, snapshot, checkpoint, difference-map or directory
regions are allocated. Staging buffers must lie after `arena_bytes` and within
the caller-provided memory window.

A record holds timestamp u64, insertion order u32 and packed message u32.
Packed message bits 23:0 contain bytes in transmission order, bits 25:24 contain
length (1–3), and bits 31:26 are zero.

A view has sixteen u32 words:

| Word | Meaning |
|---:|---|
| 0–2 | Generation, references, status |
| 3–5 | Block count, first-event offset, event count |
| 6–7 | Time origin u64 |
| 8–9 | Duration u64 |
| 10–11 | Reserved zero; no starting-state handle |
| 12–13 | Next insertion order, event limit |
| 14–15 | Reserved zero, writer owner (0 public, 1 history, 2 capture) |

Status is empty=0, recording=1, complete=2, incomplete=3, retired=4. Handles combine
slot in bits 31:0 and nonzero generation in bits 63:32. Completed views can share
blocks. Range views select events in [start,end), rebase timestamps by their
origin and need no checkpoint or replay. A retained export survives replacement.
Final release enqueues bounded retirement of blocks and then the view.

## Public commands

Commands preserve their existing numbers where practical. Arguments not listed
are reserved and ignored. Removed state commands return Unsupported (2) on a
prepared image. `response_state` and the SNAPSHOTS capacity parameter are removed.

| Op | Action | Arguments |
|---:|---|---|
| 0 | Format | none |
| 1 | Bind loop context | slot, loop identity, input, destination, channel mask |
| 2 | Unbind stopped context | slot |
| 4 | Grab history into group | input, start, end, target-slot-list address, count |
| 5 | Replace loop content | slot, view |
| 6 | Duplicate loop content | source slot, target slot |
| 7 | Clear loop content | slot |
| 8 | Begin import | none; returns recording view handle |
| 9 | Append import event | view, timestamp, packed message |
| 10 | Finish import | view, duration; returns ready=1 on success |
| 11 | Retain/export loop view | slot; returns handle, duration, event count |
| 12 | Read exported record | view, event index |
| 13 | Release exported/import view | view |
| 14 | Send direct output | destination, packed message, sample offset |
| 15 | Obtain history range view | input, start, end |
| 16 | Configure history retention before processing | input, frames |
| 17 | Removed state recovery | Unsupported |
| 18 | Invalidate image after engine fault | none |
| 19 | Removed instrument-state query | Unsupported |
| 20 | Bounded maintenance | maximum work, capped at 64 |
| 21 | Set stopped loop's recording limit | slot, events |
| 22 | Atomically clear group | target-slot-list address, count |
| 23 | Atomically replace group | view, target-slot-list address, count |

Imports no longer read a starting-state pointer or prepare comparison maps.
Transfer grants replenish per quantum: 64 events, 64 metadata operations and 128
published bindings. Capture/history reservations preserve admitted recording
capacity when UI imports allocate blocks. No comparison-cell grant remains.

## Jobs and errors

Internal ops 24–34 retain the compiled Rust job encoding: begin, finish capture,
begin capture, observe input, append capture, prepare window/release playback,
claim (compatibility no-op), read until, output, complete and maintain. The shared
JP dispatches these alongside audio jobs. Claim jobs neither inspect nor update
instrument state or channel ownership.

Error codes remain invalid event=1, unsupported=2, capacity=3, invalid state or
handle=4, busy/desynchronized playback=5, overflow=6, memory fault=7 and incomplete
content=8. Here “invalid state” concerns service lifecycle or handles, not MIDI
instrument state. Input ordering, density limits, duration alignment, generation
checks and atomic publication validation remain enforced.
