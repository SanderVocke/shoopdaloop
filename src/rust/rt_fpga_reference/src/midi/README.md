# Rust reference MIDI components

`MidiSession` is the programmatic MIDI runtime within `rt_fpga_reference`.
It follows SP loop timing and runs after the audio jobs of each quantum. Shared
app graph compilation, control endpoint, host drivers, and SystemVerilog targets
continue to refuse MIDI; this component does not enable them.

## Scope and ownership

The managed subset remains complete MIDI 1.0 channel packets and System Reset.
Parameter-editing CCs 6, 38 and 96–101, SysEx, and other system messages return
explicit `Unsupported` results. Recordings retain original bytes and equal-time
order. The interpreter handles held/pedal-latched notes, sustain/sostenuto/Hold 2,
pressure, pitch, bank/program selection, channel modes, and resets.

| Owner | Stored information |
| --- | --- |
| `ContentStore` | Fixed event blocks, view tables, immutable state snapshots, references, and deferred retirement. |
| Input/history service | One input interpreter per source, rolling checkpoints, retained event prefix and prepared starting-state comparisons. |
| Output service | Current accepted output state per destination. |
| Playback context | One loop MIDI data channel's binding, cursor, activity, and acknowledged transition progress. |
| `DifferenceService` | Fixed difference bitmaps keyed by snapshot generation and destination. Identical target/destination pairs share one map. |
| SP | Loop mode, position, length, synchronization, and content revision. |

Comparisons use generation-checked weak snapshot references. Views, history
checkpoints, recording writers, and exports own the strong references. Reusing a
snapshot slot cannot make an old comparison valid; avoiding a comparison-to-target
strong reference also avoids an ownership cycle.

## Storage and history

There is one allocation level: blocks of timestamped short events, filled by event
count. Silence occupies duration metadata. One view describes a chronological
range, its time origin, duration, and starting snapshot. Several views can retain
the same physical blocks. Appending into an unpublished tail cannot change a
retained prefix. There is no playlist or semantic search tree.

The default store has 256-event blocks, 4,864 blocks, 512 views, 768 snapshots,
and 256 retirement records. Each ordinary sequence admits 8,192 events. History
writers additionally reserve two blocks of event-count headroom for checkpoint
prefixes and arrivals before old data is discarded; grabbed ranges still admit
at most 8,192 events. Capacities can be configured independently at preparation.

History checkpoints occur every 32 ordinary messages and after expensive state
updates. A range start uses the preceding checkpoint and replays at most 31
ordinary messages. Exact checkpoint starts share the existing immutable snapshot.
Other starts copy one state and derive its differences from the checkpoint's
maps and the changed fields. This preparation includes leading silence and every
message before the start frame, while excluding all messages at that frame.

`set_history_retention(input, 0)` before processing retains only the rolling prefix
needed for future recording-start state. It is useful for many inputs when only
some need retrospective grabs. Full histories use their configured retention;
input observation continues if history capacity is exhausted. The failure is
explicit and existing retained views remain valid.

Clear/replacement releases a handle, not all its events. Final view release queues
its block table. `maintenance(budget)` releases at most that many table/reference
steps; processing supplies a fresh configured allowance each quantum. Snapshots
and descriptor slots return only after their final owners have ended. There is
no synchronous collection fallback on capacity failure.

The small `Take` API is a compatibility storage facade over the same content
store, using one block and a pinned cursor. It has no independent event storage
implementation or runtime scheduler.

## State differences and transitions

`Interpreter::begin`/`step_observed` publish changed cells as a message progresses.
Partial interpreter state remains private. An ordinary note update takes one
step, a pedal release 129, and System Reset 4,226. Each changed destination cell
updates prepared comparisons, including comparisons still being initialized.
Values returning to the target clear their difference bits. Unknown target
controls are skipped, and stable bits are not rewritten.

`StateTransition` visits sparse differences and affected dependency groups.
Ordinary matching controls are not resent. A changed program can require bank
selection before loading the program and restoration of pending bank selectors
afterwards. Changed notes/pedals can require rebuilding that channel's note/pedal
relationship; pressure-only differences do not retrigger matching notes. Historical
reset markers do not cause synthetic resets. Original reset packets still play.
The older `Emission` full-projection helpers remain available for component tests;
they are not the session playback restoration path.

State progress, output-state updates, and transition acknowledgement follow sink
acceptance. Ordinary playback/restoration refusal enters reserved destination
recovery and suppresses overdue events. Recovery sends All Sound Off then All
Notes Off for every channel, retaining progress through refusal. Every context at
that destination is coordinated, and its pedal ownership survives until stop or
restoration resolves it. Ordinary output resumes at a later SP loop beginning;
missed notes are not emitted late. Stop cleanup can resume with one accepted
message per callback. A completed-state query returns `Busy` during an unfinished
transition or recovery.

## Pipeline and content API

1. Prepare `MidiSession` with `SessionLimits`, then bind its finite loop slots to
   logical loop identities, input/destination indices, and owned channel masks.
2. Feed ordered callback-relative `InputEvent`s to
   `Pipeline::process_midi_session`, with an `OutputEvent` sink. A callback may
   contain several quanta. The event at an interval's end belongs to the next one.
3. Use existing loop plans for record/play from zero/stop. Input is tracked once
   per source while stopped or playing. There is no automatic input monitoring;
   `send` permits explicit monitored/control output through the destination tracker.
4. Use `Pipeline::{grab_midi,replace_midi,duplicate_midi,clear_midi}` between
   processing calls. These publish content and SP metadata together. A group grab
   reserves all target ownership first, shares one range/state, and changes all
   bindings or none. Replaced active views remain pinned until cleanup finishes.
5. Use `begin_import`, bounded `import_events`, and repeated `finish_import` calls
   to stage new content and prepare comparisons. An unfinished import cannot be
   bound. Cancellation releases its owned view. `export` retains a coherent view;
   `export_events` reads it incrementally, including after the source is replaced.

`history_range`, `begin_import`, and `export` return an owned handle that must
be released. `content(slot)` returns the binding's borrowed handle; retain it with
`export` before keeping it independently. Read duration and starting snapshot
through the view description and immutable store access. Export includes both
those values and the ordered records, not only MIDI packets. This is an in-memory
component API; it does not implement a file format.

After processing starts, all control calls share grants replenished by each
quantum: 64 event transfers, 256 comparison-initialization cells, 64 metadata
operations, and 128 published bindings. Exhausted grants report `Busy` or pending
progress. Initial cold setup before the first processing call is offline. Imports
also preserve recording/history block and snapshot reservations; they cannot use
all capacity promised to admitted capture. Accepted import prefixes remain
inspectable if a later operation fails.

A binding's channel mask explicitly selects input/playback channels without
renumbering them. Independently playing contexts cannot overlap owned channels
at one destination. There is no arbitrary channel mapping or same-key voice
merging. System Reset playback requires ownership of the full destination.
Different outputs can carry independent copies of the same sixteen-channel data.
Bindings are immutable for their logical lifetime, so the supported mapping has
no in-place revision that could invalidate a prepared comparison.

Malformed windows, excess configured input density, stale loop identities,
discontinuous calls, and unsupported seek plans fail preflight. Do not bypass
MIDI processing between calls once attached. MIDI capacity/refusal appears in
`SessionReport::error` while audio proceeds. Fatal pipeline faults silence audio,
mark MIDI destinations for recovery, and permit explicit `drain_recovery` without
advancing the failed pipeline.

## Validation and profiling

All processing pools are allocated during preparation. `SessionWork` counts state
steps, transition work, reads, context visits, copies, comparison probes, and
publication. `storage_work` is a composite bookkeeping counter including snapshot
bytes; `state_copy_bytes` separately reports bytes copied. Neither is a hardware
cycle estimate. `prepared_bytes` includes all session pools and excludes caller
input, audio, sink, and test/profile buffers.

```sh
cargo test --locked -p rt_fpga_reference --all-features -p rt_fpga_compiler
cargo run --locked --release -p rt_fpga_reference --all-features --example midi_session_profile -- 3000
cargo run --locked --release -p rt_fpga_reference --all-features --example midi_session_profile -- 8192
cargo run --locked --release -p rt_fpga_reference --all-features --example midi_session_profile -- grab
```

The session profile records and plays 128 independent inputs/loops, retains one
full history, transfers an exported loop during playback, grabs into all 128
bindings, duplicates/replaces content, and drains retired loop ownership. It checks
all original output values/timestamps and passthrough audio. The corresponding
3,000-event test applies heap guards to processing, grab, and transfer operations.
The profile reports p50/p99/max quantum time and complete logical-grab time on the
running host; it does not establish physical FPGA or MIDI-device deadlines.

The thirteen first-milestone pipeline scenarios were migrated to `MidiSession`,
preserving recovery, refusal, sync-wait, incomplete-recording, all-quantum and
partitioning coverage. Independent state/pedal timelines and a full-comparison
oracle remain separate checks. Provenance: state rules derive from `rt_runtime`
at `1d081bf8`; ordering derives from `shoop_loops` at `9eb71d04`. No production
runtime dependency was added. Local evidence and memory/timing results are in
[the storage milestone](../../../../../docs/fpga/midi-reference-storage.md).
