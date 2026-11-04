# Continuous MIDI index design

Design investigation for F2/F3 of [the application plan](../../PLAN_FINISH_GRAB_REPLACE.md).
This is not an implementation-complete contract or application parity evidence.
The existing indexed candidate API remains available. Interval-summary and full
projection query code now have linear packet-oracle tests. Fresh CI must validate
them. The bounded ordered production store, work accounting and writer integration
still need implementation and verification.

## Why the continuous writer needs another index

A checkpoint contains the result of every earlier packet. Replacing an early
controller or note-off invalidates later checkpoints, including an unchanged
suffix. Repeatedly rebuilding that suffix cannot guarantee every incoming tick
and arbitrary exit cuts within the prepared callback allowance. Increasing a patch
queue does not solve the dependency. The production MIDI history is also much
larger than a small fixture: the legacy port reserves 43,690 short messages and a
nominal 30-second time horizon. Simply lowering that density to fit full checkpoint
projections would change application behavior.

An ordered semantic index can answer state queries without materializing an
absolute state at every chunk. Changes update affected search-tree paths; unchanged
suffix summaries remain valid functions of their incoming state. This must be
proved for the complete supported projection before using it in production.

## Ordered media and ownership

Keep originals in order by `(u64 frame, u64 order)`. Distinct equal-frame packets
retain their order. Payload bytes live in bounded append-only slabs, with explicit
record lengths and immutable leases; interval erasure changes references rather
than overwriting bytes held by another owner. Loaded immutable bodies can remain
ordinary resource bases. A logical wrap splits an interval in order; it does not
insert restoration messages.

Use a persistent ordered event tree plus category indexes for note on/off, key
pressure, individual controllers, program/channel pressure/pitch and channel/global
resets. A COW radix table can hold category roots: retaining a complete root is
constant work and changing a category copies only a bounded path. Ordered trees
should mutate exclusive staging paths after their initial copy. Stage owned input
and reserve the whole write before graph execution, then commit after all other
fallible tick work succeeds. If mutation happens earlier, retain a complete
rollback root for that in-flight tick. A failed graph tick must preserve the previous
complete audio/MIDI cut, not merely the content before writer entry.

Preparation globally bounds descriptors, payload slabs, tree/table nodes, roots,
retirements and writer scratch. Required capture/writer credits are separate from
ordinary exports and imports. Preflight the whole tick before mutations. Deferred
retirement must not allocate or steal ongoing write workspace.

## Pedal interval summaries

Sustain and Hold2 together have four possible boolean input states. Each subtree
stores, for each input, its output state and whether both pedals were ever up
after an update. Composition evaluates the second subtree using the first's
output and combines that observation. Reset Controllers sets both pedals up.
This answers whether an already unheld, unlatched note lost its final hold during
an interval without scanning the interval's packets.

Sostenuto has two possible input pedal states. Each subtree stores its output
pedal state, last rising edge, and first/last latch-clear points for each input.
All Sound Off clears latches while retaining the pedal value; Reset Controllers
also puts the pedal up. The distinction changes whether a later unchanged
pedal-down packet is a new rising edge. Composition retains these conditional
effects, including equal-frame packet order.

The prepared summaries are fixed-size functions. Their composition requires four
or two cases, independent of suffix length. Current tests compare sustain/Hold2
composition against actual protocol note release for all initial hold states and
verify unchanged sostenuto suffixes after deletion and Clear/Reset replacement.

## Complete state query reconstruction

All queries exclude originals at the requested frame. Internal queries at a
specific packet use the complete ordered point so earlier equal-frame originals
remain included. Initial state is the projection strictly before logical zero.

- Held status follows the latest key note-on/off or applicable All Notes Off,
  channel-mode, All Sound Off or System Reset. Reset Controllers alone leaves
  held status intact.
- A sostenuto latch depends on the last rising edge after its last clear and
  whether that key was held at that edge. If there is no such edge/clear, use the
  initial latch. A note-on while the pedal is already down does not create a latch.
- Velocity starts with the last note-on, or the initial velocity if none exists.
  A later All Sound Off/System Reset kills it. Otherwise find the first key-off
  or applicable All Notes Off after that note-on, or logical zero for an initially
  unheld note. If latched then, advance to its first latch clear. With no later
  note-on, held status cannot resurrect; the hold summary determines whether both
  remaining pedals became clear. This preserves natural suffix effects.
- Key pressure follows its last packet or controller-reset baseline; retain known
  zero separately from unknown. System Reset restores unknown projection.
- Controllers use the latest applicable value, reset baseline or counterpart
  exclusion (124/125, 126/127). Preserve the exact supported reset-retention set.
- Program bank is the bank at the last program change, not the current pending
  bank. Reset Controllers retains bank selectors; System Reset clears them.
- Pitch/channel pressure and reset markers follow their existing baselines.
  Parameter selector family follows its latest selector/reset. The untracked
  parameter-history diagnostic survives Reset Controllers and clears on System
  Reset. Preserve the global System Reset observation marker.

Reconstruct the existing opaque channel projection inside the state module;
public callers continue using the same getters and ordinary media interface.
Verify every field against independent linear packet/state oracles, including
pedal overlap, repeated controls, reset/channel modes, initial latches, unknown
values, all keys/channels and long random edit sequences. A proof for pedals alone
does not validate this reconstruction. The query implementation now has a linear
test timeline used only as an oracle adapter; it is not the production index.
Tests compare the complete opaque projection across equal-frame/wide positions,
initial state, all supported message families and deterministic edited streams.

## Writer and publication requirements

A complete semantic root is usable after every committed tick. Updating the
logical-zero initial projection on a wrapping write does not invalidate summaries
that describe functions for every relevant initial pedal state. Stop or Playing
must not wait for a suffix checkpoint repair. Prefix continuation appends after
the retained prefix and may outlive the input-history horizon.

Growing and fixed-period writers need explicit event/byte/density limits and
reserved final roots, results and retirement. A loaded loop's affected-quantum
density must be checked during admission so removing a dense interval does not
turn into an unbounded per-record callback scan. Input above admitted capacity
requires a truthful terminal outcome; it cannot disappear while an index builds.
Large retained intervals can retire through bounded work after an atomic tree cut.

Use the same source/loop/topology generations, complete-cut watermark and media
revision fences as the existing foundation. Publish audio/MIDI and resolver state
as one admitted result. Old exports and siblings retain their roots and initial
state. Full-loop imports, ordinary playback/get/save/load and teardown must pass
the public-route plan gates before either capability is enabled.

Report prepared memory and complete callback work at actual production pools.
Bound state-query work by category count and tree height; preserve all deadline
misses and overflow diagnostics when measuring. No physical-device certification
is inferred from a fixed finite operation count.
