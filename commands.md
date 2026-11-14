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

This document specifies a first experiment. It does not implement it or replace
the remaining command protocol. Rust is the implementation target; the logical
FIFO and publication rules should later map readily to hardware. ARM/FPGA code,
register interfaces, and drivers are outside this work.

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

### Atomic publication of groups

A synchronized UI action must reach control as an explicit complete group. Thirty
independent UI messages cannot imply one atomic group: a callback or network
delivery gap could divide them. In the first milestone, each group fits one
bounded application message, with room for at least thirty supported commands.
Choose a compact encoding and verify its maximum encoded size against the existing
transport's message limit. There is no application Begin/Append/Commit or multipart
assembly protocol for this subset. Underlying network fragmentation is handled by
transport framing before control sees the complete message.

For example, control prepares thirty playback-start commands and publishes them
in one step. RT sees either the complete group or none of it. All accepted starts
take effect before audio processing at the same frame; execution need not be
parallel on the CPU to be simultaneous in audio time.

Use either a bounded batch per queue entry or reserved queue slots made visible
by one publication of the write position. Select and document one representation
before implementation. A separately published Begin/End pair is insufficient if
RT can consume the beginning before the group is complete.

RT never waits for missing members, accumulates partial groups, or splits a group
across callbacks. Preflight the entire batch before mutating state. A stale target,
unsupported transition, or invalid value rejects the whole batch without partial
application. Validation must cover interactions within a batch; initially reject
conflicting playback transitions for the same loop rather than inventing rollback.

### Capacity and worst-case work

Prepare all FIFO storage and bounds before callbacks start. Support at least
thirty playback-start commands in one batch. Also define maximum batch count,
commands per batch, total admitted commands, and payload bytes.

Memory capacity is only one constraint. The maximum published work at the cutoff,
including preflight, application, and necessary boundary settlement, must fit the
chosen callback work allowance alongside audio processing. Size that allowance
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
or keep a future-work list. Invalid groups are rejected completely and reported
through bounded diagnostics; this first experiment provides no correlated failure
reply for an individual submitted group.

RT still validates identities, generations, and the applicable configuration.
These are object/configuration identities, not request correlation IDs. Reject
stale or invalid batches immediately. Expose bounded refusal counters and a
compact error classification through diagnostics/snapshots. Latest-state feedback
does not promise a separate historical outcome for every command.

### Addressing within the snapshot configuration

Commands and snapshots share the same versioned configuration and object identity
scheme. Do not introduce an independent command-object registry or identifier
namespace. Put the active configuration version once in each group header;
currently this is the existing graph/topology epoch pair used by snapshots.
Individual commands name typed targets using the existing identities belonging
to that configuration, so loop 7 and strip 7 are unambiguous distinct targets.

Use the retained configuration's mappings between logical application objects and
runtime processors. A logical loop can map to playback, capture, and fader nodes;
its identity must not be reinterpreted as an unrelated processor-slot index.
Control may resolve these mappings before publication, but the resulting RT
addresses must remain tied to the group's configuration and be checked on RT.

RT first checks that the group's configuration is active and that every target
belongs to it, then performs the remaining whole-batch preflight. Reject a stale
configuration, missing target, or generation mismatch without applying any part
of the group. Never silently retarget commands to a reused slot, translate them
to a newer configuration, or hold them until an expected configuration activates.

For the first milestone, reuse the existing slot-and-generation safeguards as
part of object identities. They are not new request IDs. Omitting generations
later would require establishing that every relevant object replacement changes
the configuration version and cannot reuse an address within that version.

The publication sequence and audio frame of an individual snapshot are not the
configuration version and are not command correlation tokens. Commands need not
reference a particular snapshot publication. Snapshot-based observation must
interpret resulting state using the same configuration and target identities.

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

## First milestone

Deliver a clearly separated immediate-command transport and execution path, a
control submission API, and a narrow UI/application adapter entry point exercising
the dedicated path across both bridges. A singleton command is a batch of one.
Selected operations must bypass the outer legacy request/correlation tables and
`Command::Schedule`, scheduler reservations, tickets, and terminal receipt streams
inside RT. Discarding legacy replies behind an adapter is not this milestone.

Include this subset:

- Loop gain and balance.
- Strip input/output gain and balance, plus audio output mute.
- Bus gain, balance, and audio mute.
- Immediate start/restart and stop of playback on ordinary, already-loaded,
  audio-only primitive loops. Define start/restart position explicitly, initially
  frame zero. Stop means stopping playback, not finalizing a recording.
- Mixed scalar batches and a group starting thirty distinct prepared loops at
  the same processing frame.

These are candidate operations subject to the execution-confirmation rule above.
Any exclusions must be explicit and enforced at submission, rather than silently
falling back to the old request/reply path within an experimental batch. An
excluded command is unsupported by the experiment and must be refused before
publication; implementing it waits for a later design that fits the agreed model.

Keep click-suppression ramps as normal processor state. Setting their targets at
the same boundary does not require retaining commands or scheduling future work.

Exclude recording/replacing, musical quantization, delayed transitions, composite
programs, MIDI delivery/restoration, processor/plugin lifecycle, graph changes,
media transfer, and ownership reclamation. Those paths retain their current
protocols until separately designed and migrated.

Use a stable graph and prepared media for the first experiment. Reject structural
or media-lifetime changes while the experiment is active. Define exclusive routing
for the migrated controls/targets so legacy scheduled commands cannot overtake or
override the immediate path accidentally. Other command categories remain clearly
separate. Supporting unrestricted concurrent graph edits is a later milestone.

### Acceptance criteria

1. Every complete batch visible at the callback cutoff that passes RT preflight
   is applied completely at that callback's eligible processing boundary, before
   affected audio renders. A batch failing preflight is rejected completely.
2. If immediate playback start is included, starting thirty loaded audio loops produces aligned playback at the same audio
   frame, including publication racing the callback cutoff: the entire batch runs
   together at this boundary or the next, never partly at each.
3. The dedicated UI-to-control and control-to-RT paths have no application
   deadlines, request IDs, cancellation, or individual admission/execution replies.
   RT has no pending execution records for these commands.
4. Each group fits one bounded application message. Full queues refuse complete
   groups locally or propagate bounded backpressure; neither UI drawing nor RT
   waits. RT rejection leaves all batch targets unchanged and is visible in
   bounded diagnostics. Reconnect never automatically replays a group.
5. Capacity and callback work are explicitly bounded; RT remains heap-free and
   nonblocking. Snapshots describe applied state after boundary settlement.
6. Existing push/pull snapshot delivery and unmigrated command/media workflows
   continue operating. Restrictions on graph/media edits and conflicting legacy
   commands are enforced rather than assumed.
7. Application dependencies on execution confirmation are met through simple,
   sound snapshot observations or documented exclusion of the affected command.
   No confirmation requirement reintroduces request correlation or acknowledgements
   into the experimental path.
8. Command groups reuse snapshots' configuration versions and existing object
   identities. Stale epochs, foreign targets, and reused-slot generation mismatches
   reject the entire group without mutation or retargeting. No separate command
   identity registry is introduced.

The implementer may narrow the candidate command set under that rule without
additional approval. This does not waive atomicity, boundedness, or the protocol
simplicity criteria. If playback start itself must be deferred, record it as
unimplemented and the thirty-loop validation as not applicable. Do not claim that
the synchronized playback case was delivered or tested.

## Implementation stages

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
  Enforce the experiment's graph/media and legacy-routing restrictions. Smoke-test
  scalar changes, stale configuration and generation rejection, invalid-batch
  rejection, snapshot feedback, and heap-free RT use.
- [ ] **4. Validate and publish the milestone.** Exercise thirty-loop synchronized
  playback through the application entry point and actual callback processing;
  compare recorded output/start frames, including a callback-cutoff race. Exercise
  push and pull feedback, encoded asynchronous command delivery, a paused consumer,
  disconnect/reconnect without replay, and one representative unmigrated workflow.
  Verify incomplete transport data never reaches the RT FIFO. Run formatting,
  test-policy checks, and a warning-denying build of affected crates. Record focused
  timing observations without claiming a hard deadline guarantee from a smoke run.
  Commit and push on the agreed work branch, update the existing draft PR (or
  establish a new branch/PR if the agreed destination changes), and inspect CI and
  automated review. Address relevant failures/feedback until checks are green and
  review concerns are resolved. Do not merge autonomously.

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

Execution is authorized. The implementation uses these concrete bounds:

- One FIFO entry owns one inline `Copy` batch of at most 32 commands. Four entries
  bound each callback to 128 commands. Control stages at most four additional
  batches; the record carrier retains at most one complete blocked message.
- The largest batch record is at most 436 bytes (length prefix, message tag,
  graph/topology epochs, count, and 32 commands of at most 13 bytes), within the
  1,280-byte carrier limit. No command owns an allocation.
- The callback freezes the FIFO count before legacy ingress. The first unrendered
  quantum consumes exactly that many entries, before planner settlement and
  snapshot capture. No tick means no dequeue. Preflight examines at most 32
  targets plus a bounded pairwise check for duplicate playback transitions.
- The experimental session latches on the first group accepted while legacy
  work is idle. Thereafter legacy mutation requests are refused (diagnostics and bulk-read cancellation remain allowed); snapshot,
  resynchronization, diagnostics and bulk-read traffic remain available. The
  latch survives disconnect, preventing previously RT-admitted work from racing
  graph/media changes after reconnect. Reprepare the engine to leave this first
  experimental mode. This deliberately excludes live structural/media editing.
- Playback uses direct boundary state changes in the loop resolver, with no
  pending request records. Restart's DSP discontinuity is processor state, not
  a retained command. Only stopped/playing primitive audio loops with prepared
  content and no pending musical work qualify. Start means restart at zero.
- Applications observe gain/balance/mute or stopped/playing conditions in the
  existing configuration-matched snapshots. The entry point does not promise
  historical confirmation of a restart or wait for it before returning.

The four-batch service allowance is an explicit experimental bound, not a proven
hard deadline on all supported machines. Focused release timing will report the
measured cost; deployments must validate their audio period and session size
before raising these limits.


## Implemented API and scope

`shoop_rt_backend::RtBackend::immediate_group(&[ImmediateCommand])` is the explicit
application entry point. It resolves existing application loop/track/bus identities
through the retained mirror and uses the active snapshot configuration. Gains use
linear amplitude and balances use -1 through 1. It returns immediately with local
acceptance or refusal; it does not advance the mutation cursor, reserve reliable
request credits, or wait for execution. Ordinary `Backend` mutation methods retain
their legacy behavior outside the experiment.

At the protocol boundary, `ClientMessage::Immediate(Batch)` carries one complete
group. `ProtocolClient::immediate` sends it without a correlation. Custom carriers
must call `Endpoint::try_deliver` to propagate `Full`; the in-process carrier,
record server, and native process wrapper do this. Encoded records use the existing
little-endian codec; Rust object layout is not a hardware ABI. Protocol version is
29. `rt_runtime::immediate` contains only the generic FIFO and processing-boundary
hook; engine and loop semantics live in their own `immediate` modules.

Playback is included. It restarts at zero or stops without creating resolver
request records. Control checks the retained strip configuration is primitive
and audio-only; RT checks actual prepared audio, empty MIDI content, current
stopped/playing mode, and absence of pending musical/program work, recording
storage, failures, or repeat-sync behavior. MIDI-capable, processed/dry-wet,
composite, recording and repeat-sync playback remain outside this first set.
There is no legacy fallback inside a group.

The first accepted group latches the stable-session restriction. Reprepare the
engine to resume legacy mutations. The latch persists across reconnects; control
staging and a record server's blocked message do not. Previously RT-admitted work
may still execute. This deliberately narrow entry point does not switch every
ordinary application control to the experiment automatically.

Snapshots remain the only execution observation. `immediate_diagnostics()` exposes
cumulative control and RT refusal counts plus compact classifications (unavailable,
configuration, target, unsupported state, invalid value). Diagnostics are resent
on new callback frames so lossy telemetry cannot permanently hide a changed count.
They carry no command identity and do not establish individual command history.
