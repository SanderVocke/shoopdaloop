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

Functional correctness and bounded operations determine acceptance. Per the user's
clarification, simultaneous 128-target grabs are useful stress measurements, not a
required support level; cloud wall-clock timings are informational, not gates.

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

Baseline: 99 focused reference/compiler tests passed. The final all-feature run
passes **124**. This includes the cold-grab, channel-handoff, established-owner
note-off, and matching-note continuation regressions. All thirteen original pipeline scenarios were migrated to the new runtime.
Tests include full-comparison oracles after 700 mixed updates; independent pedal,
bank/program and exact event traces; one-message cleanup/refusal; sync waits;
fatal audio faults; generation/capacity failures; import grants/cancellation;
background admission reservations; and a guarded 128-loop end-to-end session.

The extra cold-grab fixture replaces 128 distinct views containing 8,192 events
and derives state by replaying 31 distinct controls, while allocating no heap and
without advancing the SP frame. Source/target lifetimes survive collector progress.

Warning-denying workspace build, formatting, test-policy, and capability-parity
checks pass. Rust 1.100 emits existing Cargo manifest unused-dependency warnings;
those are separate from Rust compiler warnings. The complete native suite ran 3,495 tests: **3,482 passed, 13 failed, 11 skipped**
in 1,047.957 seconds. Its thirteen failing test names exactly match the earlier
first-milestone local run. The earlier
milestone reproduced unrelated native startup/capacity and shutdown failures on
its untouched baseline; do not attribute those to reference MIDI without evidence.

CI results and review feedback are excluded unless explicitly requested. No RTL
simulation, synthesis, physical-device behavior, or hard hardware deadline is
claimed. The production native MIDI backend and frontend/control protocol were
not changed.

### CPU timings

Measured on a shared five-vCPU x86-64 Intel Xeon Platinum 8573C host with
Rust 1.100.0, release optimization, Q=32, and a 48 kHz timeline. Each normal
profile simulates 30 seconds of capture and 30 seconds of playback. The quantum
samples include the whole SP/audio/MIDI processing call; import/export checks
also run between calls under their transfer grants. Values describe this host.

| Events per loop | Capture wall time | Playback wall time | Quantum p50 | Quantum p99 | Largest quantum sample | One complete grab |
| --- | --- | --- | --- | --- | --- | --- |
| 3,000 | 1.193 s | 2.382 s | 19.9 µs | 375.5 µs | 24.7 ms | 158.3 µs |
| 8,192 | 1.584 s | 5.391 s | 18.1 µs | 796.1 µs | 20.5 ms | 147.9 µs |

Both grabs deliberately use a 31-message prefix, share one derived state, and
commit all 128 bindings without advancing SP time. Their comparison work is
544 units (eight bitmap copies plus one changed controller per destination);
state copies and binding/reference work are measured separately. An earlier
3,000-event run had a 2.026 ms grab outlier and a 28.1 ms largest quantum; an
earlier 8,192-event run had a 72.3 ms largest quantum. These remain part of the
observed scheduling limits, even though the final run above was less interrupted.

A separate **101-sample** grab profile replaces **128 distinct full 8,192-event
views** each time, deriving the target from **31 distinct controllers**. It runs
while the local native suite is active. Complete foreground operation timings:

| Minimum | Median | p99 | Maximum | Maximum replay | Maximum comparison work | Maximum storage work |
| --- | --- | --- | --- | --- | --- | --- |
| 20.9 µs | 21.6 µs | 54.2 µs | 93.5 µs | 31 steps | 784 units | 9,489 units |

The additional run during native-suite load measured 21.7 µs median, 137.5 µs
p99, and 196.6 µs maximum. The old 8,192-event tables are detached, not walked
in either interval. Cleanup
between trials uses the explicit budgeted collector. Run with:

```sh
cargo run --locked --release -p rt_fpga_reference --all-features --example midi_session_profile -- grab
```

For context, Q=32 at 48 kHz spans 666.7 µs. The 3,000-event p99 and repeated-grab measurements fit that interval. At
8,192 events, p99 varied from 594 to 796 µs across runs and can exceed it; the
largest scheduling outliers exceed it substantially. The functional one-quantum guarantee is an atomic, bounded publication
with no deferred event/state copying; these shared-host results do not prove an
OS or future FPGA hard deadline. Output-message bandwidth remains a separate
start-playback obligation.

Before the relevant-target/stable-bit optimization, the same CC profiles had
roughly 0.94 ms / 1.56 ms quantum p99 at 3,000 / 8,192 events. The runtime now
skips targets that never established the changed control and avoids rewriting
unchanged difference bits. Work remains incremental, with no whole-state
comparison at a ready playback boundary.

### Native baseline comparison

The failure-name sets were compared directly with the first milestone's complete
local log (`midi-native-suite-small.log`) and are identical:

- Twelve application startup/recovery cases fail while creating the native engine
  with `Runtime(Capacity)`. A representative is
  `native_recovery_tests::a_new_connection_cancels_old_inputs_and_cannot_consume_old_file_callbacks`.
- `shoop_host::recovery::siblings_survive_the_launcher_exit_and_later_control_needs_no_child_handle`
  fails in its shutdown/recovery endpoint handling.

Representative failures were also reproduced on the untouched first-milestone
base, as recorded in the earlier milestone evidence. The production native
implementation was not changed to address them here. The full native run preceded
the final reference-only note-continuation refinement; the complete 124-test
reference/compiler suite and warning-denying build were rerun after that change.
This separates current reference evidence from the wider unchanged native code.

Raw local evidence files are `midi2-final-focused-latest.log`,
`midi2-final-build-latest.log`, `midi2-native-suite-final.log`, and
`midi2-profile-{3000,8192,grab}-final.log` in the execution workspace. The commands
and representative results above make the validation reproducible without those
local files. Existing Cargo build-script output lookup warnings appeared during
the native run; no physical backend availability is inferred from allowed skips.
