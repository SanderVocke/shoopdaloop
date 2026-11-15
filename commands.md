# Immediate real-time commands

## Goal

Simplify the first experimental command subset end to end: UI to control, then
control to hard real-time execution inside `rt_runtime`. The UI sends complete
groups on a dedicated fire-and-forget path. Control publishes admitted groups
through a bounded single-producer, single-consumer FIFO; RT consumes and applies
them at a processing boundary.

Commands on this path carry no future execution deadline, request ID, cancellation
ticket, or individual admission/execution reply on either application bridge.
They bypass the existing request/reply bookkeeping rather than hiding its results.
RT does not retain dequeued commands to match them with later arrivals. Current
state returns through snapshots; bounded diagnostic counters report refusals.
Command admission and snapshot observation are separate. Transport-level delivery
mechanisms do not become application request IDs or execution acknowledgements.

This document records the completed experiment, coexistence extension and command
simplification. The remaining command
protocol stays available. Rust is the implementation target; the logical FIFO and
publication rules should later map readily to hardware. ARM/FPGA code, register
interfaces, and drivers are outside this work.

## Status

| Work | Status |
| --- | --- |
| Immediate UI/control/RT path, scalar controls and synchronized playback | Implemented and smoke-tested |
| Coexistence with legacy graph, media and recording operations | Implemented and smoke-tested |
| Larger FIFO storing each group's actual command count | Implemented and smoke-tested |
| Configuration version guarantees stable object identities; configuration-local command addresses | Implemented and smoke-tested |
| One-pass RT execution with individual command failures | Implemented and smoke-tested |
| Migration of ordinary UI controls to the immediate API | Implemented and smoke-tested for the supported subset |

The current FIFO has **256 fixed-size records**. Each group occupies one header
plus its actual command count, from 1 through 32. One callback can therefore
consume 128 singleton groups or up to 248 commands in larger groups. Commands
carry configuration-local indices without expected generations. RT checks each
header once and attempts commands in order; individual failures do not suppress
later effects.

The initial and coexistence checklists preserve the historical implementation
steps. The simplification checklist and latest validation record describe the
current implementation. The subsequent UI integration automatically routes the
supported subset through the immediate API; its scope is recorded below.

## Design principles

### Callback-boundary execution

At callback entry, snapshot the FIFO's published write position. Consume the
complete batches visible at that cutoff before rendering their first affected
audio interval. Later publications wait for the next callback. Do not drain an
unbounded moving tail while control continues producing commands.

Commands execute at the first eligible processing boundary, rather than at a
timestamp supplied by control. Account explicitly for hosts that buffer partial
quanta: apply commands before the next unrendered processing quantum, never
retroactively to audio already rendered or buffered. Stamp resulting snapshots
with the actual processing frame.

### Complete publication and same-callback execution

A synchronized UI action must reach control as an explicit complete group. Thirty
independent UI messages cannot imply one synchronized group: a callback or network
delivery gap could divide them. In the first milestone, each group fits one
bounded application message, with room for at least thirty supported commands.
Choose a compact encoding and verify its maximum encoded size against the existing
transport's message limit. There is no application Begin/Append/Commit or multipart
assembly protocol for this subset. Underlying network fragmentation is handled by
transport framing before control sees the complete message.

For example, control prepares thirty playback-start commands and publishes them
in one step. RT sees either the complete group or none of it. Successful starts
take effect before audio processing at the same frame; execution need not be
parallel on the CPU to be simultaneous in audio time.

Use a larger ring of fixed-size records, with a group header containing its
configuration version and command count, followed by that many command records.
A group occupies its actual record count instead of a maximum-sized batch slot.
Control reserves space for the complete header and commands, writes them, and
advances the published write position once. Only the producer writes that
publication position; only the consumer releases consumed space. Handle wrapping
without allocating. A separately published Begin/End pair is insufficient if RT can consume the
beginning before the group is complete.

RT never waits for missing members, accumulates partial groups, or splits a group
across callbacks. Every supported group fits the prepared FIFO and one bounded
application message by application/API contract. Groups larger than the FIFO are
not supported by this design. RT never rounds its cutoff into unpublished data.

RT checks the configuration version once at the group header. If it matches, RT
consumes and attempts commands sequentially, directly from the ring, then releases
their storage. It needs no separate whole-group buffer and no preliminary semantic
scan. Grouping guarantees one processing boundary and command order; it does not
guarantee that every command succeeds or that earlier effects are rolled back.

### Capacity and worst-case work

Prepare all FIFO storage and bounds before callbacks start. Support at least
thirty playback-start commands in one batch. Also define maximum batch count,
commands per batch, total admitted commands, and payload bytes.

Memory capacity is only one constraint. The maximum published work at the cutoff,
including header handling, per-command execution/fallbacks, and boundary
settlement, must fit the chosen callback work allowance alongside audio processing. Size that allowance
against the smallest supported callback period and the supported operations'
worst-case work, not average queue occupancy.

For this experiment, provision service to consume the entire configured published
queue at the cutoff. Do not inherit a smaller legacy dequeue/publication budget
that could split a group. Larger capacities require revisiting the work bound;
available memory alone is not a reason to increase them. If later designs need to
defer work, they must defer whole queued batches with explicit admission semantics.

Publication, dequeue, validation, and application must perform no heap allocation,
deallocation, locks, waits, or I/O on RT. Prefer fixed-layout value commands with
bounded payloads and ordinary object identities; avoid owned heap objects needing
a return path. Use a sound Rust SPSC implementation with explicit publication
ordering. Hardware memory ordering remains a future platform-specific concern.

Size the ring for many singleton groups as well as larger groups. Account for
header space and overhead: merely enlarging fixed maximum-sized batch entries
does not meet the storage goal. Select and document concrete record/byte capacity,
maximum published work, and any retained per-message command limit during the
simplification milestone. A group may initially remain limited to 32 commands;
that is independent of how many groups the RT FIFO can hold. The current
1,280-byte application-message envelope still applies unless explicitly changed.

### Admission and observation

Fire-and-forget means no per-group or per-command application acknowledgement, not
unbounded buffering or silently publishing a prefix. The UI submission API is
nonblocking: it either accepts a complete group into bounded local storage or
returns a local capacity/validation refusal. Local acceptance does not certify
remote admission or RT execution.

Control's internal submission API similarly admits the whole batch or refuses
it locally. If the RT FIFO is full, control retains complete groups in bounded
storage and slows further transport intake. Once that storage is full, stop
accepting additional input and let transport backpressure reach the sender. Keep
the drawing loop nonblocking and preserve group order. Do not silently drop an
admitted start/stop group to make room for newer work.

Control may retry a batch that has not yet been admitted to the RT FIFO. Such
retries stay outside RT and carry no execution deadlines. RT does not wait, retry,
or keep a future-work list. A stale configuration skips the whole group without
effects. A command with unmet preconditions leaves its target unchanged, records
a bounded diagnostic, and does not prevent later commands from being attempted.
There is no correlated failure reply for a group or an individual command.

There is no snapshot wait between consecutive immediate groups. The existing
snapshot reconciliation barrier applies when handing control back to legacy
operations that follow an immediate prefix. Preserve that coexistence mechanism.
Latest-state feedback does not promise a separate historical outcome for every
command.

### Addressing within the snapshot configuration

Commands and snapshots share the same versioned configuration and object identity
scheme. Do not introduce an independent command-object registry or identifier
namespace. Put the active configuration version once in each group header;
currently this is the existing graph/topology epoch pair used by snapshots.
Individual commands use existing typed slot indices as addresses within that
configuration, so loop 7 and strip 7 are unambiguous distinct targets. Do not
introduce another object registry or repeat expected generations in each command.

Use the retained configuration's mappings between logical application objects and
runtime processors. A logical loop can map to playback, capture, and fader nodes;
its identity must not be reinterpreted as an unrelated processor-slot index.
Control may resolve these mappings before publication, but the resulting RT
addresses must remain tied to the group's configuration.

Establish this invariant before removing command-level generation checks:
**within one active configuration version, an address always denotes the same
object incarnation.** Every creation, removal, replacement, or reuse that changes
that mapping activates a different graph/topology version. Candidate preparation
must preserve the active version's mapping; candidate addresses become active
only with the corresponding version. Keep that mapping stable while applying a
group. Audit loop, strip, bus and relevant processor lifecycle paths, including
staged and whole-session replacement.

RT compares the header with the full active graph/topology version once. A
mismatch skips the complete group; never retarget it to a newer configuration or
wait for its expected version. A match establishes object identity for the
configuration's addresses. Ordinary safe lookup still handles a nonexistent or
out-of-range address as an individual command failure. Rust ownership handles and
snapshot identities may retain their internal generations; this change removes
redundant command-level generation fields and validation, not ownership safety.

The publication sequence and audio frame of an individual snapshot are not the
configuration version and are not command correlation tokens. Commands need not
reference a particular snapshot publication. Snapshot-based observation must
interpret resulting state using the same configuration and target identities.

### Trusted groups and individual command outcomes

Trust the application to construct sensible groups. Remove whole-group semantic
checks in the application adapter, protocol builder/codec, control endpoint and
RT handler. Do not reject duplicate, opposing or otherwise self-inconsistent
instructions as a group; execute instructions in their written order. Keep only
the framing, length, opcode and bounds checks needed to decode/store records
safely and publish complete groups. Those checks do not become a semantic scan.

Command handlers decide their own current-state behavior as they execute. The
default failure behavior is no effect, a bounded diagnostic update, and continued
execution of the remaining commands. No handler may wait, retry, retain a command,
or dispatch through the legacy path to satisfy a precondition.

| Situation | Behavior |
| --- | --- |
| Group configuration does not match the active version | Skip the whole group; record configuration refusal |
| Valid scalar command and available target | Apply the value normally |
| Missing target, invalid scalar value or unsupported processor condition | Leave state unchanged; count command failure; continue |
| Start/restart on an empty, recording or otherwise unsupported loop | Leave state unchanged; count command failure; continue |
| Stop on an already stopped supported loop | Successful no-op; continue |
| Stop requiring recording finalization or other excluded ownership work | Leave state unchanged; count command failure; continue |
| Several commands address the same object | Attempt each in order against the state left by earlier commands |

Prefer these no-op failures to new implicit conversions or recovery workflows.
Document any command-specific fallback that differs from them. Snapshots describe
the state actually reached, including partial success within a group; diagnostics
remain aggregate and never become per-command acknowledgements.

### When the application needs execution confirmation

If integration reveals that the application must know whether an action has
executed, first use a simple observation of incoming snapshots. Check the relevant
object identity, configuration, and resulting state to determine whether the
application's intended condition has been reached. Do not introduce admission IDs,
request IDs, echoed command tokens, execution acknowledgements, or a complicated
correlation framework to make this work.

Keep the distinction between observing a condition and proving a command's
history. A desired value already present in a snapshot can satisfy a state-setting
action, but does not prove that a particular newly submitted command executed.
Likewise, latest-state snapshots can skip a transient action or an intermediate
state. Do not claim stronger confirmation than the observation provides.

If the application's actual requirement cannot be satisfied simply and correctly
through snapshot observation, the implementer may defer that command from the
initial experimental set. Deferral means do not implement or migrate that command
yet. Document the application dependency and reason for exclusion, and continue
with the remaining subset. Do not implement an alternative acknowledgement path,
wrap the command in the old request/reply machinery, or add a fallback to make it
appear supported by the experiment. Existing functionality outside the experiment
may remain unchanged.
This is an explicitly authorized narrowing of scope; preserving the simple
communication model takes precedence over including every candidate command.

### Connection failure and synchronization guarantees

Use the existing ordered, reliable carriers for UI-to-control delivery. Connection
failure can leave the sender uncertain whether its last group was admitted or
executed. Do not add automatic application replay or retry across reconnect.
Discard connection-local unsent work, resynchronize configuration and current
state, and let the caller choose any fresh action. A group already admitted to RT
may still execute; disconnect does not introduce cancellation semantics.

This matters for playback restart: replaying the same start-from-zero command can
restart a sounding loop again. The first milestone does not promise exactly-once
execution across connection failure or historical confirmation of every action.

Network latency changes which callback receives a group. It must not separate
the commands within an accepted group. Guarantee synchronization at one RT audio
frame, not execution at a preselected wall-clock time or simultaneous execution
on several machines.

## Delivered command subset

The initial milestone delivered a separate immediate-command transport and
execution path, a control submission API, and a narrow UI/application adapter
entry point exercising both bridges. A singleton command is a batch of one.
Selected operations bypass the outer legacy request/correlation tables and
`Command::Schedule`, scheduler reservations, tickets, and terminal receipt streams
inside RT. Discarding legacy replies behind an adapter is not this milestone.

The supported subset is:

- Loop gain and balance.
- Strip input/output gain and balance, plus audio output mute.
- Bus gain, balance, and audio mute.
- Immediate start/restart and stop of playback on ordinary, already-loaded,
  audio-only primitive loops. Define start/restart position explicitly, initially
  frame zero. Stop means stopping playback, not finalizing a recording.
- Mixed scalar batches and a group starting thirty distinct prepared loops at
  the same processing frame.

This opcode subset stays subject to the execution-confirmation rule above. Do not
add unsupported command types or silently fall back to the old request/reply path
inside a group. Unmet runtime preconditions for a supported command use its
individual no-op/failure behavior; they do not exclude the other group members.

Keep click-suppression ramps as normal processor state. Setting their targets at
the same boundary does not require retaining commands or scheduling future work.

Exclude recording/replacing, musical quantization, delayed transitions, composite
programs, MIDI delivery/restoration, processor/plugin lifecycle, graph changes,
media transfer, and ownership reclamation. Those paths retain their current
protocols until separately designed and migrated.

Keep prepared media as a requirement for immediate playback. Graph edits, media
operations, recording and other legacy commands remain available in the same
session. Control serializes the handoff between the two command paths so neither
can overtake earlier work on the other path. The coexistence milestone below
replaces the first implementation's permanent stable-session lock.

## Acceptance criteria for the agreed target

These supersede the initial milestone's whole-group refusal criteria and are
verified by the focused simplification checks below.

1. Every complete group visible at the callback cutoff is consumed at that
   callback's eligible processing boundary, before affected audio renders. A
   matching configuration causes commands to be attempted once in written order;
   a stale configuration skips the group without effects. RT performs no
   whole-group semantic preflight and never rolls back earlier commands.
2. Starting thirty prepared, eligible audio loops produces aligned playback at
   the same audio frame, including publication racing the callback cutoff: the
   group runs at this boundary or the next, never partly at each. A member with
   unmet preconditions fails individually; valid members still run at that frame.
3. The dedicated UI-to-control and control-to-RT paths have no application
   deadlines, request IDs, cancellation, or individual admission/execution replies.
   RT has no pending execution records for these commands.
4. Each group fits one bounded application message. Full queues refuse complete
   groups locally or propagate bounded backpressure; neither UI drawing nor RT
   waits. Header mismatch leaves the whole group without effects; individual
   failures leave their own targets unchanged and do not suppress other commands.
   Failures are visible in bounded diagnostics. Reconnect never replays a group.
5. Capacity and callback work are explicitly bounded; RT remains heap-free and
   nonblocking. Snapshots describe applied state after boundary settlement.
6. Existing push/pull snapshot delivery and unmigrated command/media workflows
   continue operating. Ordering across graph/media edits and immediate groups is
   enforced rather than assumed.
7. Application dependencies on execution confirmation are met through simple,
   sound snapshot observations or documented exclusion of the affected command.
   No confirmation requirement reintroduces request correlation or acknowledgements
   into the experimental path.
8. Snapshots' full graph/topology version fixes the identity behind every
   configuration-local address. Identity/address changes activate a different
   version; preparing a candidate preserves the active mapping. A group header
   check replaces repeated expected-generation fields/checks in commands. Safe
   lookup handles an invalid address individually. No new identity registry,
   version scheme or command correlation is introduced.
9. The ring stores actual group lengths, supports many singleton groups, and
   publishes only whole groups. RT executes directly from its readable records
   without a separate group buffer. Concrete capacity, worst-case callback work
   and outer message-size bounds are documented. No snapshot is required between
   consecutive immediate groups; legacy handoff reconciliation still works.

The implementer may narrow the candidate command set under that rule without
additional approval. This does not waive complete publication, same-callback
execution, boundedness, or the protocol simplicity criteria. If playback start
itself must be deferred, record it as unimplemented and the thirty-loop validation
as not applicable. Do not claim that
the synchronized playback case was delivered or tested.

## Completed initial implementation stages

These checked stages record the original implementation, including its
whole-group preflight. They do not mark the new acceptance criteria as delivered.

- [x] **1. Define the boundary and representation.** Inventory exact command
  variants, preflight costs, conflict rules, capacities, and the cutoff's placement
  relative to existing ingress, planner settlement, snapshot capture, and rendering.
  Choose the single-message UI encoding and RT batch representation. Define local
  submission results, bounded backpressure, reconnect behavior, and reuse of the
  snapshot configuration/object mappings in the group header and target encoding.
  Verify these
  choices cover the acceptance criteria. Identify any application dependency on
  execution confirmation; specify a simple snapshot observation or document and
  defer the affected command.
- [x] **2. Implement the bounded transports.** Add the dedicated UI message,
  nonblocking submission API, control buffering, and immediate FIFO with atomic
  whole-batch publication. Verify full-queue refusal/backpressure, producer
  publication racing the cutoff, FIFO ordering, and bounded draining. Confirm
  that neither bridge allocates application request/cancellation records.
- [x] **3. Connect scalar and playback execution.** Add complete-batch preflight,
  immediate application and settlement, diagnostics, and application submission.
  Enforce graph/media lifetime safety and ordering between command paths. Smoke-test
  scalar changes, stale configuration and generation rejection, invalid-batch
  rejection, snapshot feedback, and heap-free RT use.
- [x] **4. Validate and publish the milestone.** Exercise thirty-loop synchronized
  playback through the application entry point and actual callback processing;
  compare recorded output/start frames, including a callback-cutoff race. Exercise
  push and pull feedback, encoded asynchronous command delivery, a paused consumer,
  disconnect/reconnect without replay, and one representative unmigrated workflow.
  Verify incomplete transport data never reaches the RT FIFO. Run formatting,
  test-policy checks, and a warning-denying build of affected crates. Record focused
  timing observations without claiming a hard deadline guarantee from a smoke run.
  Commit and push on the agreed work branch, update the existing draft PR (or
  establish a new branch/PR if the agreed destination changes). Focused smoke
  checks are the completion gate. Leave broader CI running in the background;
  do not wait for the full matrix or expand testing to finish this experiment.
  Do not merge autonomously.

Stage 1 precedes transport work; stage 2 precedes execution integration; stage 4
validates the connected result. Keep local testing focused rather than launching
the entire integration matrix. Existing CI may provide broader coverage.

## Execution contract

When implementation is requested:

- Keep this plan updated and check off completed stages.
- Commit each completed stage or meaningful milestone.
- Revise implementation steps when new evidence warrants it.
- Revise design rules only for a documented, well-supported reason.
- Do not change goals or acceptance criteria without explicit user approval.
  The documented command-set deferrals above are already authorized; record them
  explicitly and do not use them to conceal an unmet acceptance criterion.

The current implementation uses these concrete bounds:

- The RT ring contains 256 preallocated records. A group occupies one header plus
  1–32 `Copy` command records. The cutoff contains at most 128 groups, or at most
  248 commands (seven groups of 32 and one of 24). Header/command combinations
  always fit the same 256-record bound. Control stages at most four additional
  groups; the record carrier retains at most one complete blocked message.
- The largest encoded group is 308 bytes (length prefix, message tag,
  graph/topology epochs, count, and 32 commands of at most 9 bytes), within the
  1,280-byte carrier limit. Each target is a configuration-local `u32` index;
  the command variant supplies its type. No command owns an allocation.
- Control reserves all records for a group and publishes them with one cursor
  update, using safe `rtrb` chunk operations. RT freezes the published record count
  before legacy ingress. The first unrendered quantum consumes that prefix before
  planner settlement and snapshot capture. No tick means no dequeue. It borrows
  records in place across wrapping and releases each complete group afterwards.
  There is no whole-group buffer, semantic scan, duplicate-command check or
  per-command expected-generation validation.
- Control's ordered inbox holds up to four waiting immediate groups plus the
  prepared reliable-request credit allowance. A legacy prefix uses its existing
  queue and execution machinery. An immediate prefix waits for that work to
  settle, then publishes to its dedicated FIFO. Later legacy work waits for FIFO
  retirement and snapshot reconciliation. There is no permanent session latch
  and no engine re-preparation requirement.
- Playback uses direct boundary state changes in the loop resolver, with no
  pending request records. Restart's DSP discontinuity is processor state, not
  a retained command. Only stopped/playing primitive audio loops with prepared
  content and no pending musical work qualify. Start means restart at zero.
- Applications observe gain/balance/mute or stopped/playing conditions in the
  existing configuration-matched snapshots. The entry point does not promise
  historical confirmation of a restart or wait for it before returning.

The 256-record service allowance is an explicit experimental bound, not a proven
hard deadline on all supported machines. Deployments must validate their audio
period and session size before raising it. The wire layout is explicit; Rust ring
record layout is not a hardware ABI.


## Implemented API and scope

`shoop_rt_backend::RtBackend::immediate_group(&[ImmediateCommand])` is the explicit
application entry point. It resolves existing application loop/track/bus identities
through the retained mirror and the identities of the last validated snapshot.
An announced configuration does not replace this observed mapping until its
snapshot is accepted. An unknown, future or removed application object becomes an
invalid address and fails individually on RT instead of addressing a reused slot.
These retained identities reuse snapshot generations on the application side;
they are not a separate object registry or command-correlation scheme. Gains use
linear amplitude and balances use -1 through 1. It returns immediately with local
acceptance or refusal; it does not advance the mutation cursor, reserve reliable
request credits, or wait for execution. Ordinary `Backend` mutation methods retain
their legacy behavior outside the experiment.

At the protocol boundary, `ClientMessage::Immediate(Batch)` carries one complete
group. `ProtocolClient::immediate` sends it without a correlation. Custom carriers
must call `Endpoint::try_deliver` to propagate `Full`; the in-process carrier,
record server, and native process wrapper do this. Encoded records use the existing
little-endian codec; Rust object layout is not a hardware ABI. Protocol version is
30. `rt_runtime::immediate` contains only the generic FIFO and processing-boundary
hook; engine and loop semantics live in their own `immediate` modules.

Playback is included. It restarts at zero or stops without creating resolver
request records. The active fader configuration carries whether its loop belongs
to an audio-only unprocessed strip. RT checks that flag and actual prepared audio,
empty MIDI content, current
stopped/playing mode, and absence of pending musical/program work, recording
storage, failures, or repeat-sync behavior. MIDI-capable, processed/dry-wet,
composite, recording and repeat-sync playback remain outside this first set.
There is no legacy fallback inside a group.

Each immediate prefix waits for preceding legacy work and a validated RT snapshot
of the current configuration, proving deferred processor configuration has settled.
Later legacy messages wait in bounded control storage until the immediate FIFO
has finished applying its published groups and a snapshot covers their processing
frame. Control refreshes loop, strip and bus scalar settings from that snapshot
before allowing graph/media jobs to resume. This prevents a later graph edit from
restoring stale controls. Graph replacement retains its explicitly prepared new
settings, which become authoritative on activation.

The handoff uses a transport retirement cursor and the existing snapshot audio
frame. Only RT writes the shared retirement cursor and frame; control keeps its
own publication cursor. These are bounded FIFO progress, not command IDs, receipts
or a pending-command table. RT checks the header and attempts each command once,
recording individual failures without retaining commands. The cursors survive reconnect; unsent inbox and
record staging do not. Previously RT-admitted work may still execute, and later
legacy edits wait for its snapshot before proceeding.

The application adapter first sends any earlier locally queued legacy messages.
If credits or transport capacity prevent that, immediate submission returns a
local refusal without sending the group, rather than overtaking those messages.
Groups waiting behind a configuration change are refused as stale when their
header no longer matches; they are never silently retargeted or replayed. Snapshot
observation and bounded diagnostics remain the only remote feedback.

Already admitted delayed musical transitions keep their existing scheduling;
control does not wait indefinitely for them before publishing scalar groups.
Immediate playback continues to refuse pending musical/program conflicts during
its command handler. No future scheduling is introduced into the immediate path.

The UI runtime selects the immediate path for supported ordinary controls, while
the low-level backend mutation methods retain their existing tracked semantics
for other workflows. No controls need to be disabled merely because immediate
groups have been used. Unsupported immediate playback states fail individually while
the rest of the group continues; their legacy operations remain available.

## Completed coexistence milestone

This completed follow-up removed the global stable-session restriction. Ordinary
UI controls have not yet been migrated to immediate groups.

- [x] Preserve arrival order across legacy requests and immediate groups in
  bounded control storage, including the adapter's not-yet-sent legacy messages.
- [x] Hold later legacy execution until queued immediate work is retired and
  its scalar state is reconciled from a complete configuration-matched snapshot.
- [x] Keep graph edits, recording, media operations and prepared graph replacement
  available without re-preparing the engine or introducing command correlations.
- [x] Preserve stale-configuration and reused-slot rejection across activation,
  and retain safe handoff after disconnect without replaying unsent commands.
- [x] Smoke-test the connected paths, update the draft PR, and leave broad CI in
  the background.

The focused checks cover graph edits preserving immediate gains/mutes, mixed
legacy/immediate ordering, recording and subsequent immediate playback, media
clearing, prepared graph replacement, stale epochs and generations, application
adapter integration, encoded delivery and reconnect. Empty recorded MIDI streams
are judged by data size, not duration, so ordinary recorded audio qualifies for
immediate playback. The command subset is unchanged; the simplification below
replaces the initial callback storage bounds.

Snapshots remain the only execution observation. `immediate_diagnostics()` exposes
cumulative control and RT refusal counts plus compact classifications (unavailable,
configuration, target, unsupported state, invalid value). Diagnostics are resent
on new callback frames so lossy telemetry cannot permanently hide a changed count.
They carry no command identity and do not establish individual command history.

## Simplification milestone

The fixed batch slots and whole-group refusal rules have been replaced with the
target above. Ordinary UI controls and the opcode subset have not been expanded.
The legacy coexistence path and RT snapshot publication cadence are preserved.

- [x] **1. Establish configuration-owned identity.** Audit every supported target
  lifecycle, candidate preparation, activation, removal and slot reuse against the
  full snapshot graph/topology version. Make the active mapping immutable within
  a version, including during replacement preparation, and advance the version
  with any changed mapping. Reuse existing typed slot indices and mappings; keep
  internal ownership generations as needed. Verify replacement and slot reuse
  with focused tests before removing command-level generation checks.
- [x] **2. Execute commands once with individual outcomes.** Remove semantic
  whole-group scans and conflict rejection from adapter, builder/codec, control
  and RT. Retain memory-safe framing/decoding. Use the handler behaviors specified
  above, with no-op failure, aggregate diagnostics and continued processing by
  default. Remove assumptions/panics that application follows successful group
  preflight. Replace old all-or-nothing tests with valid/failing/valid groups,
  repeated targets in written order, and unsupported recording/playback cases.
- [x] **3. Store actual group sizes in the FIFO.** After stage 1, remove repeated
  expected-generation fields from commands and resolve addresses through the
  configuration's existing mapping. Update the wire version if its layout
  changes. Implement the larger fixed-record ring with count/version headers,
  whole-group reservation and one publication update. Choose/document practical
  capacity including headers, maximum callback work and message bounds; the
  existing 32-command message cap can initially stay. RT freezes the published
  position once, checks each header once, attempts commands in one pass and
  releases consumed space. Preserve control backpressure, reconnect behavior and
  the legacy handoff using FIFO progress plus snapshots. Test wrapping, full
  admission, many singleton groups, mixed sizes and an interrupted producer whose
  unfinished group must stay invisible to RT.
- [x] **4. Smoke-test and publish the connected result.** After stages 1–3, check
  the documented singleton capacity in one callback, mixed-size groups, partial
  success, thirty-loop alignment and cutoff races through the application API.
  Check stale-header rejection and new-version address reuse, encoded delivery,
  disconnect without replay, and representative legacy graph/media coexistence.
  Confirm heap-free, nonblocking RT execution and no snapshot gate between
  immediate groups. Run formatting, test policy and a warning-denying build of
  affected crates. Record focused results, commit/push on the existing PR branch
  and update its draft description. Do not run the full integration matrix, wait
  for broad CI, or merge autonomously.

Stage 1 is the prerequisite for trusting a header instead of individual expected
generations. Stage 2 establishes command failure semantics; stage 3 connects them
to the new storage and addressing. Stage 4 validates the combination. The latest
validation record below covers this implementation; earlier results remain
historical evidence for the initial and coexistence milestones.

Identity audit: loop incarnation changes require node-pair configuration and a
new topology/graph activation; strip identity/processor binding changes require
a new graph; bus identity/processor changes also require a new graph. Whole-session
replacement always advances both epochs. Candidate node configuration is staged
for the next epoch and does not replace the active mapping during preparation.
Focused loop/strip/bus slot-reuse and prepared-replacement snapshot checks pass.
The structural queue also prevents reusing an occupied address within the same
activation: removal settles before a subsequent creation can activate.

The adapter also retains the identities from its last validated snapshot. A new
configuration announcement alone cannot pair a new version with stale local
addresses. Unknown or not-yet-observed application objects encode an invalid
index and fail individually, leaving other commands in the group usable.


## Validation record

### Simplification

All four simplification stages are complete. The focused checks pass:

- Twenty command checks: two generic FIFO tests, one paused-producer test, two
  protocol tests, eleven engine tests, three application adapter tests and one
  observed-configuration address test. They cover wrapping and exact occupancy,
  whole-group publication racing a cutoff, a producer paused before publication,
  partial success and repeated targets, header rejection, slot reuse and candidate
  preparation, recording/media coexistence, encoded backpressure, reconnect,
  partial host callbacks, and heap-free execution at the 248-command allowance.
- The application test submits 128 singleton groups without intervening snapshots
  and observes their effects after one callback. Thirty valid starts remain
  aligned when an invalid member is inserted into the same group, including the
  existing callback race. Submission creates no reliable mutation identity.
- Eleven snapshot smoke tests preserve push/pull delivery, timing cache behavior,
  graph transitions, bounded payloads and legacy success observation.
- Formatting, test-attribute policy, diff checks and warning-denying builds of the
  engine, backend and native process wrapper pass. The process wrapper build uses
  no native audio-provider features. No full integration matrix, live browser run
  or broad CI completion gate was used. These checks do not certify worst-case
  callback deadlines on other hardware or session sizes.

The implementation and this record are committed on the existing draft PR branch.
No automatic merge is authorized. Ordinary UI-control migration, additional command
families and ARM/FPGA implementations remain outside the completed scope.

### Initial and coexistence implementations

The following is the historical validation of the initial and coexistence
implementations, completed on 2026-11-14. The simplification's results are recorded
separately below.

- Nineteen command/carrier smoke tests pass. They cover cutoff ordering and a
  concurrent producer; a 436-byte maximum record and malformed/truncated records;
  scalar snapshots and all-or-nothing target rejection; partial host callbacks;
  bounded staging/backpressure; encoded fragmentation and a paused receiver;
  disconnect without replay; thirty preloaded loops starting/restarting together,
  including a race against an actual callback; and preserved legacy bulk reads.
  The connected RT and encoded control paths are heap-guarded.
- Thirteen snapshot and timing-edit smoke tests pass, preserving push/pull
  delivery, cache behavior, configuration transitions and unmigrated workflows.
- Formatting, test-attribute policy, diff checks and the warning-denying build of
  affected crates pass. The native process wrapper was built without native audio
  provider features. No full local integration matrix or live browser run was used.
- A release timing smoke measured a 16-frame callback with 128 loop-gain commands
  and snapshot publication in a small prepared session: 10,246 ns median and
  36,525 ns maximum across 128 callbacks on this workspace. It excludes UI/control
  time and is not a worst-case execution-time proof or a guarantee at larger
  session capacities.

The staged commits are published on `feat/rt-state-snapshots` in draft PR
[#997](https://github.com/SanderVocke/shoopdaloop/pull/997), targeting
`feat/backend-replacement`. Per the user's scope clarification, broader CI is
informational and is not a completion gate for this experiment. The focused smoke
evidence above is the validation scope. No automatic merge is authorized.


CI comparison exposed a bus-identity defect already present in the snapshot branch:
new bus resets did not preserve their generation, and replacement configuration
used zero. This invalidated complete UI snapshots and would also refuse immediate
commands to additional buses. Resets and replacement configuration now retain the
existing bus generation. A focused adapter smoke reproduces the failure before the
fix and verifies continued snapshots plus immediate controls on an additional bus.

The worklet artifact contract now reads the engine wire version from the Rust
protocol source. Its stale hand-written version 26 handshake would otherwise
reject version 29 before exercising the artifact. This changes the test fixture,
not the production bridge or the independent audio protocol.


Legacy adapter successes now remain in their existing completion queue until a
fresh validated snapshot is applied. Failures and disconnect outcomes remain
available immediately. A reliable legacy result can precede rate-limited push
observation; treating it as current display state made composite capture report
Busy after a confirmed stop. Capture preparation also waits for fresh observation.
This adds no request tracking to immediate groups. The existing hosted composite
file-roundtrip test reproduces the push-ordering issue on native when the fixture
uses push, and passes with the observation barrier. Focused refusal/disconnect
regressions and a one-Hz push smoke also pass.

The coexistence extension was validated with twelve focused immediate-command
tests across engine and application adapter, including the original thirty-loop
alignment/race smoke. Eleven snapshot smoke tests cover the surrounding delivery
and cache behavior. Formatting, test policy and a warning-denying build of the
engine, backend and native process wrapper (without native audio-provider
features) pass. No full integration matrix or CI completion wait is part of this
follow-up.

## Ordinary UI integration

The shared UI runtime now selects `shoop_app_protocol::immediate` groups for loop
gain/balance, strip input/output gain/balance and output mute, and bus
gain/balance/mute. Eligible Lua control operations use the same runtime queue.
The service advertises immediate support and ordinary-audio playback eligibility
in its session snapshot. Unquantized Play/Stop, default loop clicks, selected
playback and solo stops use one complete group when all targets qualify and the
whole action fits the current 32-command envelope. Start restarts at zero.

The UI sends `ClientMessage::Immediate` without reserving an operation identity.
The service maps existing UI identities through `BackendImmediateCommand` into
the existing RT adapter, converting decibel controls to linear amplitudes there.
There are no individual acceptance or execution replies and no new RT tracking.
Snapshots update the display. A cumulative service refusal counter produces an
uncorrelated UI notice; it does not prove an individual command's outcome.

When local transport is full, the service retains the complete group and later
UI messages in an ordered queue of at most 256 messages. Local and encoded
receivers stop taking messages when that queue is full. Consecutive immediate
groups still need no intervening snapshot; the existing barrier applies before
later legacy work. Disconnect discards service-owned unsent work, while work
already accepted by the backend remains with the host.

Unsupported, mixed, scheduled or oversized actions keep their entire existing
route, chosen before immediate submission. Recording stops still finalize media;
composites, MIDI, processed playback and musical timing remain legacy. There is
no fallback after an immediate group is submitted. Explicit `Client::command`
and ordinary `Backend` mutation methods retain their tracked contracts for
compatibility clients and lifecycle, duplication and persistence workflows.

The application protocol is version 36; RT protocol version 30 and callback
publication are unchanged. Focused smoke checks cover scalar controls, slider
bursts and ordering into legacy monitoring, discarding unsent groups on UI loss,
one group starting 30 loops, encoded message bounds/routing, Lua snapshot reads
and fixed-cycle recording. No broad test suite or CI completion wait was used.
