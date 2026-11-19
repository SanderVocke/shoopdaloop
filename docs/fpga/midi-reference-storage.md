# MIDI shared-storage reference contracts

Implementation branch: `feat/fpga-reference-midi-storage`, based on `17c06dcd`.
This document records the Rust reference profile; it does not freeze an RTL ABI.

## Capacity and work model

The session profile has 128 loops, eight controlled destinations, one history
source, 8,192 events per sequence, 256 events per allocation block, 4,864 event
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

Baseline: 99 reference/compiler tests passed. Existing unrelated native-suite
failures are recorded by the first milestone. Stage results, resource accounting,
complete grab timings, and final native results will be appended as implemented.
CI results and review feedback are excluded unless explicitly requested.
