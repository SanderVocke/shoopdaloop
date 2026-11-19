# MIDI shared-storage reference contracts

Implementation branch: `feat/fpga-reference-midi-storage`, based on `17c06dcd`.
This document records the Rust reference profile; it does not freeze an RTL ABI.

## Capacity and work model

The default session profile has 128 loops, eight controlled destinations, one input
with full history, 8,192 events per sequence, 256 events per allocation block, 4,864 event
blocks, 512 views, 768 state snapshots, and 256 queued retirements. It reserves
32 blocks per live full sequence (4,096 total), leaving 768 blocks for history,
transfers, old retained versions, and recording headroom. Reservations make these
budgets explicit; unlimited retained exports are not promised.

Event payload capacity is 19 MiB at 16 bytes per record. Snapshot payload capacity
is about 6.6 MiB. Comparison storage is separately sized for the configured
snapshot/destination relationships. Report actual prepared sizes, including
tables, free lists, context state, transfer slots, and comparison summaries.

A range containing at most 8,192 events intersects at most 33 blocks. Grabbing it
into 128 loops therefore retains at most 33 block references and publishes 128
bindings. It creates one starting snapshot and one comparison per distinct
configured destination; duplicates share those comparisons. Fixed snapshot and
bitmap copies are charged explicitly.

History checkpoints occur at most 32 ordinary messages apart and immediately
after messages that can affect a channel's notes/controllers or the entire state.
A range start uses the checkpoint preceding its first included event and replays
at most 31 ordinary events. Range endpoints are absolute sample frames in a
half-open interval. Initial state precedes every event at the starting frame.
Duration remains independent of event count; loop durations obey SP quantum
alignment. A range can start at any retained frame consistent with that duration.
Checkpoint exhaustion explicitly freezes capture of new history, preserving its
accepted prefix and existing grabs; input observation continues. Ordinary dense
CC recordings in the required retention profile must not exhaust the pool.

Destination comparison maps remain current for retained checkpoints. A new target
can clone a checkpoint's maps and update only fields affected by the bounded
prefix. Cold imports initialize comparisons incrementally before publication.
Replay, map copies, snapshot copies, reference retention, and binding publication
are all included in the grab work counter and timing measurement.

## Ownership and operations

The content store owns blocks, views, and immutable snapshots. Recording writers,
loop bindings, history checkpoints, exports, and active comparisons retain their
required handles. Recycled handles carry a new generation. Published prefixes
remain stable; writers append only beyond them. Releasing the last view owner
queues its table for bounded cleanup; it never walks the table on that call.

The engine owns input and destination interpreters. Inputs are interpreted once
per source. Each accepted output message updates its destination and the affected
comparison fields before a ready state can be observed. A comparison is shared
only for the same target, destination, and mapping. Channel ownership is explicit;
independently playing contexts cannot overlap channels at one destination. General
same-channel voice merging is outside this reference milestone.

Stop releases the context's owned notes/pedals; starting/repeating then transitions
the destination toward the saved state. Ordinary equal controls are not resent.
Saved reset markers are historical information, not instructions to reset again.
Recorded reset packets still play unchanged. Program/bank and pedal dependencies
may require several ordered messages even when only one difference group is set.
Generated transition progress advances only for accepted output; recovery affects
all bindings sharing the failed destination and cannot replay overdue notes.

Content changes are reserved first and committed at an SP boundary. One group
grab either replaces every requested binding or leaves all of them unchanged.
Replacing playing content waits for acknowledged cleanup without exposing a
half-switched binding. Import/export are bounded transfer steps; export pins a
coherent version. Control workers do not mutate live store arrays.

## Validation evidence

The normal stress fixture uses 128 inputs and loops, with one full 30-second
history and 127 inputs retaining only a recording-start prefix. Its input limit is
one event per input per quantum. Eight destinations each host sixteen disjoint
channel owners. It records and replays 3,000 CC events per loop at 100/second;
the full-capacity release run records/replays 8,192 events per loop over 30 seconds.
Both validate all original bytes/timestamps, passthrough audio, concurrent export
and import, duplicate/replacement, atomic 128-loop grab, and eventual reclamation
of all loop owners. The remaining 128 history writers are intentional live owners.

Prepared session memory in that profile is **34,457,072 bytes (32.86 MiB)**,
including event payload capacity, snapshot/difference tables, context/input state,
free lists, checkpoint queues, and retirement/transfer metadata. Caller audio,
input/sink arrays, and profile samples are excluded. One input with history can
serve many loops; 128 full retrospective histories are not implicitly provisioned.
History writers reserve two additional blocks of event-count headroom for rolling
prefix/arrival handling. Published sequences still admit at most 8,192 events.

### Work bounds

| Operation | Accounted limit |
| --- | --- |
| Ordinary view event range | At most 8,192 records and 33 intersecting blocks. |
| History state derivation | At most 31 ordinary messages from a ready checkpoint; exact checkpoint starts share state. |
| Group publication | Up to 128 bindings; fixed per-binding metadata, without walking old event/block tables. |
| New target comparisons | Copy one ready bitmap per configured destination and update replay-changed cells. |
| Changed output cell | Probe live target snapshots, at most the snapshot pool capacity; skip irrelevant targets and unchanged bits. |
| Ordinary note update / pedal release / System Reset | 1 / 129 / 4,226 interpreter steps; a bulk command can notify many changed fields. |
| Online transfers between quanta | 64 event transfers, 256 setup cells, 64 handle/metadata operations, 128 binding publications. |
| Automatic retirement | At most the configured number of block/descriptor release steps per quantum; default 64. |

Snapshot copies, replay, lookup, comparison fanout, reference retention, publication,
and deferred release are counted and included in complete operation timings.
`storage_work` combines bookkeeping visits and snapshot-copy bytes; separate
`state_copy_bytes` makes copy volume explicit. Counts are functional software units,
not RTL clocks. The declared protocol/pool limits bound heavy reset/pedal work;
the CC timings below do not establish its deadline on every configured topology.

### Local checks

Baseline: 99 focused reference/compiler tests passed. The latest all-feature run
passes 122; an extra cold-grab test passes separately, bringing current coverage
to 123. All thirteen original pipeline scenarios were migrated to the new runtime.
Tests include full-comparison oracles after 700 mixed updates; independent pedal,
bank/program and exact event traces; one-message cleanup/refusal; sync waits;
fatal audio faults; generation/capacity failures; import grants/cancellation;
background admission reservations; and a guarded 128-loop end-to-end session.

The extra cold-grab fixture replaces 128 distinct views containing 8,192 events
and derives state by replaying 31 distinct controls, while allocating no heap and
without advancing the SP frame. Source/target lifetimes survive collector progress.

Warning-denying workspace build, formatting, test-policy, and capability-parity
checks pass. Rust 1.100 emits existing Cargo manifest unused-dependency warnings;
those are separate from Rust compiler warnings. Full native-suite results and
final CPU timing distributions will be recorded below when complete. The earlier
milestone reproduced unrelated native startup/capacity and shutdown failures on
its untouched baseline; do not attribute those to reference MIDI without evidence.

CI results and review feedback are excluded unless explicitly requested. No RTL
simulation, synthesis, physical-device behavior, or hard hardware deadline is
claimed. The production native MIDI backend and frontend/control protocol were
not changed.
