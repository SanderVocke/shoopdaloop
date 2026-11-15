# RT communication: design and working plan

## Purpose and status

Move the application toward complete state snapshots and ordered command groups,
with a simple communication model inside `rt_runtime`. This is the sole working
document for that migration. It describes the implemented system, the remaining
legacy areas, and the next extension to implement.

Snapshot observation and ordinary UI command wiring are implemented. The next
step adds optional expiry deadlines and an ordered response FIFO between control
and RT. That extension is planned, not implemented. It supersedes the restriction
that the new internal command path must never return individual results.

This step delivers deadline capability only. All existing application commands
on the new path must continue with no deadline. Assigning deadlines to specific
application commands is separate future work, outside this milestone.

There are two distinct boundaries:

```text
UI/application <-> control <-> real-time processing in the audio callback
                       snapshot ring: RT -> control
                       command FIFO: control -> RT
                       response FIFO: RT -> control (next step)
```

Networking, browser messaging, UI pacing and application workflows belong on the
control/UI side. Generic rings and handoff mechanisms belong in `rt_runtime`;
loop, strip and bus semantics belong in `shoop_rt_engine` and `shoop_loops`.
Implementation is Rust-only. The logical model should map readily to ARM/FPGA
later; hardware interfaces, drivers and RTL are outside this work.

## Design rules

- Preserve complete publication, FIFO order and grouped execution. RT never
  waits for an unfinished group or chases commands published after its callback
  cutoff. Groups run at one processing boundary, in their written order.
- Commands and snapshots share the same configuration and object identity scheme.
  Do not introduce another object registry or request-ID system.
- Trust the application to construct sensible groups. Retain framing, opcode,
  length and memory-safety checks, without whole-group semantic preflight or
  rollback. Individual command failures leave their targets unchanged and allow
  later commands to proceed. Duplicate and opposing commands execute in order.
- Prepare storage and resources off RT. RT handlers avoid heap allocation,
  destruction of owning resources, locks, waits and I/O. Return owners reliably
  to control for retirement or reuse.
- **Execution is best effort. Do not introduce checks or bookkeeping to prove
  that callback work is bounded.** Do not add per-command cost estimates,
  processing-time accounting, callback work quotas or time-budget admission.
  Pursue reliable audio through efficient handlers and off-thread preparation.
  The optional deadline check below is an expiry check, not a CPU-work budget.
- Fixed queue sizes, message limits and complete-group admission remain storage
  safeguards. The callback-entry cutoff defines which input to consume; it is
  not a processing-time allowance. Process that published prefix without
  introducing a separate work quota.
- The UI keeps drawing with its latest accepted snapshot. Control may maintain
  dependencies and response bookkeeping without adding pending requests,
  cancellation tickets or execution reordering to RT.
- Preserve existing functionality through clearly separated legacy paths until
  each replacement is implemented. Never silently retry an executed or failed
  new-style command through the legacy protocol.

## Shared configuration and addressing

The configuration version is the graph/topology epoch pair already used by
snapshots. Within one active version, a typed slot address denotes the same object
incarnation. Creation, removal, replacement or reuse that changes this mapping
activates a new version. Preparing a candidate must preserve the active mapping.

Put the version once in a command-group header. Commands use existing typed
configuration-local indices; internal ownership handles and snapshot identities
may retain their generations. RT checks the header once. A stale version skips
the whole group, without retargeting it or waiting for another configuration.
Safe lookup handles an unavailable address as an individual failure.

Control retains active and pending configuration descriptions. Snapshots contain
live state for a known configuration, without repeating the topology. A pending
UI graph is distinct from the graph actually running. Activate the displayed
configuration only when RT observation confirms it and matching context is
available. Reject unknown versions and delayed observations of retired versions.
New clients obtain configuration context through resynchronization.

## Implemented snapshot path

RT captures coherent state after changes and same-frame settlement, before
processing the next audio interval. Capture occurs at the first settled processing
tick of every audio callback. Callbacks that only buffer a partial quantum retain
the previous publication. UI delivery settings never reduce RT capture frequency.

| State | Included fields |
| --- | --- |
| Header | Schema, capture frame, graph/topology epochs, object counts and payload length |
| Ordinary loops | Identity/generation, mode and pending transition, recording flags, cursor, length, gain, balance, repeat-sync, content/timing revisions |
| Loop media metadata | Audio/MIDI lengths and channel playback windows, gain, offsets, alignment and preplay; no media bytes or owning leases |
| Loop meters | Per-channel peaks over the interval since the preceding capture |
| Strips | Input/output gain and balance, output mute, monitoring policy and demand, peaks, MIDI activity and latest short input message |
| Buses | Identity/generation, gain, balance, mute and per-channel peaks |

Controls describe applied targets, while ramps remain processor state. Meters
use independent accumulators so legacy readers cannot consume snapshot data.

The internal ring has ten preallocated slots of equal payload capacity. Capacity
is selected from prepared session limits; each publication uses an actual length.
Control rejects configurations that cannot fit before activation. There is no
live resizing, allocation during capture or truncated successful snapshot.
A failed capture leaves the last complete publication available and reports an
error.

Only RT writes slot metadata and payload. It marks a slot in progress, fills it,
marks it complete and advertises the latest complete sequence. It may overwrite
old publications and never waits for control. This deliberately loses intermediate
state; it is not a reliable event queue.

Control reads the sequence, copies into private storage, then reads the sequence
again. It accepts only a matching complete publication and otherwise keeps its
previous accepted copy. A finite number of slots cannot prevent overlap if
control pauses indefinitely. The Rust implementation uses atomic payload words
and ordered metadata accesses; sequence validation alone would not make a
concurrent ordinary-memory copy safe. Sequence exhaustion reports an error and
stops publication while retaining the last snapshot; sequences must not wrap
into an ambiguous read.

Control validates configuration context before updating its timing cache or
forwarding state. The UI assembles and validates a complete publication before
applying it to existing objects. Incomplete, obsolete or unknown-configuration
data never partially changes the display. Redundant timing queries and the
temporary legacy display selector have been removed.

Both delivery modes are available on every carrier:

- Native in-process defaults to pull. Control serves the newest complete snapshot
  without waiting for RT. The adapter permits one outstanding pull and the UI
  keeps drawing if no newer snapshot is available.
- Browser and encoded/network carriers default to push at 60 Hz. Delivery can be
  configured or switched at runtime. Control forwards recent complete state,
  supersedes obsolete undelivered push data and avoids catch-up bursts.

Network/browser pulls remain asynchronous. A delivery-policy change preserves
whole-publication assembly. Snapshot requests and delivery settings stay outside
the RT command FIFO. Snapshot age remains observable.

## Implemented command path and UI integration

The current command FIFO stores fixed-size records with variable-length groups:
one configuration/count header followed by the actual commands. Control reserves
and publishes a whole group at once. RT captures the published record count at
callback entry, executes directly from readable records across wrapping and
releases each group afterward. Current execution occurs at the callback's first
unrendered processing quantum; no tick currently means no dequeue.

The FIFO contains 256 records. Each application group contains 1–32 commands and
fits the 1,280-byte transport envelope. Control can stage four waiting groups;
the UI service has an ordered queue of up to 256 messages. These are existing
capacity limits, not callback processing-time guarantees.

Ordinary UI controls and eligible Lua operations use the new path automatically:

- Loop gain and balance.
- Strip input/output gain and balance, and output mute.
- Bus gain, balance and mute.
- Immediate restart at zero and stop for prepared, ordinary audio-only primitive
  loops, including selected playback and solo stops in one group.

Eligible playback has no recording finalization, pending musical work,
repeat-sync, MIDI or processed/composite behavior. A synchronized action such as
starting 30 loops reaches RT as one complete group. Independent singleton groups
may execute in the same callback; they need no intervening snapshot.

The service advertises playback eligibility from observed state. Unsupported,
mixed, scheduled or oversized UI actions retain their entire existing route,
chosen before submission. A group is never silently split to make it fit. Track
and bus decibel controls are converted to linear amplitude by the backend adapter.

Current new-style commands have no deadlines or individual responses. Snapshots
show resulting state; diagnostics count failures. `RtBackend::immediate_group`
and the shared backend immediate interface do not advance legacy mutation cursors
or reserve legacy outcome credits. Explicit tracked client commands and ordinary
backend mutation methods remain available for compatibility and other workflows.

Local acceptance does not certify RT execution. Full transport retains complete
groups and later UI work in order, propagating backpressure. On disconnect,
discard connection-owned unsent work; work already accepted by the backend may
still execute. Never replay automatically after reconnect.

Control preserves order across new and legacy paths. Immediate work waits for a
preceding legacy prefix to settle. Later legacy work waits for FIFO retirement
and a configuration-matched snapshot covering the execution frame. Control
reconciles scalar values before releasing graph/media work, so a later graph edit
cannot restore old controls. Successful legacy completions also wait for fresh
observation where needed; failures and disconnect outcomes remain available.

## Remaining migration areas

| Area | Why it still uses legacy communication |
| --- | --- |
| Compatibility and larger/mixed actions | Tracked APIs support existing workflows and compatibility backends. Actions exceeding the current group envelope or including unsupported members retain their whole route. |
| Other scalar controls | Monitoring policy, playback defaults, smoothing and some effect parameters need explicit command semantics and failure behavior. Some are straightforward additions. |
| Graph and processor lifecycle | Creation, removal, routing and replacement require preparation, activation, configuration changes and safe retirement of displaced owners. |
| Media and persistence | Loading, exporting and saving need reliable bulk delivery, content revisions, stable read cuts and resource lifetimes. Snapshots carry no media bytes. |
| Recording and capture | Recording, replacing, grabbing history and clearing content manage writers, reserved storage and finalized takes. A mode change alone does not complete these workflows. |
| Musical timing | Quantization, repeat-sync, seeking and timing edits require exact musical behavior and interaction with existing processor state. |
| Composites, processed playback and MIDI | Nested programs, dry/wet processing, processor-state restoration and MIDI note/state cleanup need feature-specific behavior. |
| Additional observation | Composite/plugin state, processor metadata, preview, profiling, memory and engine health retain separate streams. Decide which need coherent snapshots and which remain independent. |
| Setup and resynchronization | Capability negotiation and structural context are needed before interpreting live snapshots; connection recovery must avoid replay. |

Ownership return is a separate reliable responsibility. Graph plans, processors,
media buffers and other owners must eventually reach control for reuse or
destruction. Neither lossy snapshots nor byte-copying an owning Rust object can
provide that lifetime protocol. Preserve the existing return paths until their
replacements are designed.

## Next extension: ordered command responses

### Clock and expiry deadlines

RT owns a global audio-frame index that control can read atomically at any time.
Define its publication boundary and relationship to snapshot and command frames.
Audit and reuse the existing atomic `next_frame` where its semantics match;
avoid an unrelated clock. The index is independent of loop seeks. Engine restart
establishes a fresh time domain so old queued deadlines cannot be reinterpreted.

A group header may carry an optional latest execution frame. At its command
boundary, RT checks configuration and deadline before attempting any member.
Execution at or before the deadline is permitted. A later boundary skips the
whole group with `DeadlineMissed`; there are no partial deadline refusals within
a group. A future deadline never delays execution, reorders groups or creates a
pending command. A deadline applies to the synchronous command step, not the
eventual completion of a recording or musical program.

This is a best-effort expiry policy. An atomic read gives the latest published
frame, not guaranteed delivery latency. A stopped audio clock cannot produce
callback responses. There is no per-command timer, execution-cost check or
attempt to reserve CPU time against a deadline.

Do not introduce command-specific deadline policies, default timeouts, automatic
deadline assignment or UI deadline settings in this step. Existing production
callers must send `deadline = None`. Exercise explicit deadlines using synthetic
transport test groups; these tests do not establish application policies. Existing
legacy command behavior remains unchanged.

### Records and response publication

Define an explicit internal command record layout with space reserved for a
response code. Initialize that field to an unprocessed value. Specify widths,
encoding, initialization and versioning; ordinary Rust enum layout is not a
hardware or binary ABI.

Add a preallocated SPSC response FIFO alongside the command FIFO inside
`rt_runtime`. RT copies the command stream directly into its reserved response
records and fills codes as it attempts commands. Preserve input order and group
boundaries. Publish the complete response group only after processing it. Control
must never see a partial response, and RT needs no separate whole-group buffer.

Every consumed group produces exactly one response group, including stale
configurations, expired groups and individual failures. Preserve the input header
and record the attempt's processing frame so control can interpret the results.
Initial codes should cover applied/no-op success, deadline missed, stale
configuration, unavailable target, invalid value and unsupported preconditions.
No returned record may remain unprocessed. More specific codes can be added as
features migrate.

There are no request IDs, cancellation tickets or reordered results. FIFO order
provides correspondence. Control may track outstanding groups and dependencies;
RT only consumes commands, attempts them and publishes their results. Ordinary
UI actions can remain fire-and-forget, with control draining responses and
snapshots providing display state. The extension does not require forwarding an
individual reply to the UI for every action.

### Same-callback execution

RT receives a group for execution when it belongs to the published prefix captured
at callback entry. It must attempt or explicitly refuse that group and publish
its complete response before the callback returns. Later publications wait for
the next callback. Every started group finishes in that callback; RT never
retains a command to retry or match with later input.

The current no-tick/no-dequeue behavior must be addressed explicitly. The next
implementation needs a safe command boundary even when a callback only services
buffered audio, or an explicit cannot-apply response in that callback. Respect
the first unrendered audio frame and never modify audio already rendered. Do not
silently leave a received group pending or report success for an unapplied step.

### Response capacity and backpressure

Control reserves enough response capacity for a whole group before publishing
its commands. Account for response headers and every possible result record.
Keep that reservation until control consumes the response; freeing command FIFO
space does not release response credit.

This makes response space available even if control pauses. RT never waits for
space, drops a response or executes a group whose response cannot be stored.
When capacity is exhausted, control stops admitting groups and propagates
backpressure while audio continues. Reservation/accounting belongs on control;
RT writes into the guaranteed space using normal FIFO ownership rules.

Reconnection must preserve correspondence for already admitted work, or establish
an explicit fresh command/response session. Never pair an old response with a new
submission or automatically replay a group whose outcome is uncertain.

### Advanced workflows and observation

A response confirms the synchronous step represented by its code. For example,
starting recording can complete now while recording continues; arming a prepared
musical program can return an explicit armed result while snapshots later show
playback beginning. Do not describe arming as completion of a future action.

Prepare graph, processor and media resources on control, then use commands to
activate, swap or freeze them. Keep musical behavior in processor/program state,
without a future-deadline scheduler in the transport. Define ownership transfer
and reliable retirement explicitly when adding handles or result data. Those
feature migrations are subsequent work, not delivered by the response FIFO alone.

Snapshots remain the source of current display state. A response can arrive before
a snapshot covering its frame; control must wait for matching fresh observation
when a dependent workflow needs that state. Preserve ordering and reconciliation
with legacy work while migrating dependencies to the new results.

## Implementation sequence for the next step

1. **Clock and record contract**

   - [ ] Define clock publication/reset semantics, optional deadline encoding,
     response fields/codes and layout versioning.
   - [ ] Resolve callback/quantum buffering against the same-callback rule.
     Verify deadline equality, expiry and unchanged group ordering with focused
     clock/codec examples. Do not add execution-cost or time-budget machinery.

2. **Paired FIFOs and response reservations**

   - [ ] Add generic response storage and control-side reservation accounting;
     copy command records and publish complete result groups.
   - [ ] Verify wrapping, a paused control reader, exhausted response capacity
     and credit release on response consumption. RT must never wait for space.

3. **RT handler integration**

   - [ ] Apply header checks once, attempt commands in order and fill every code;
     return all results before the receiving callback exits.
   - [ ] Smoke-test successful, expired and stale groups, individual failures,
     cutoff races, buffered callbacks and a group of 30 aligned starts.

4. **Control and application integration**

   - [ ] Drain results in order, preserve legacy handoff and reconnect rules,
     and expose useful failures without requiring UI request IDs. Keep deadlines
     absent for all existing application commands; add no deadline policies.
   - [ ] Verify that existing snapshot-driven controls, dependencies and push/pull
     delivery keep working. Preserve unmigrated features through their routes.

5. **Focused end-to-end validation and documentation**

   - [ ] Exercise UI -> control -> RT -> response -> control with both direct
     and encoded delivery, stalled consumption and disconnect without replay.
   - [ ] Run relevant smoke tests, formatting and warning-denying builds; update
     this document to distinguish delivered behavior from remaining work.

### Acceptance criteria

- Every complete received group gets exactly one complete ordered response in
  the same callback, with a meaningful code for every command.
- Optional deadlines reject an already-late group as a whole and never schedule
  work for the future. An absent deadline preserves immediate FIFO execution.
- All existing application commands on the new path use `deadline = None`.
  Explicit deadlines are exercised only in synthetic transport tests. No
  command-specific policy or automatic/default deadline assignment is delivered;
  applying deadlines to application commands requires separate future work.
- Thirty successful starts in one group share one processing frame. Individual
  failures preserve subsequent effects and do not introduce rollback.
- Slow control cannot force RT to wait or lose responses; admission reserves
  response capacity before command publication.
- Configuration identity, current snapshots, existing UI behavior, legacy handoff
  and ownership lifetimes remain correct. No automatic replay or legacy retry.
- No request IDs, RT pending-request table, execution reordering, cancellation,
  processing-time bounds checks, cost accounting or callback work budgets are
  introduced. RT makes its best effort using efficient command handlers.

### Execution and verification policy

Keep this document current and check off completed work. Commit meaningful
milestones on the existing branch and update the existing draft PR. Implementation
steps may change with new evidence; document supported design changes. Goals and
acceptance criteria require explicit user approval to change.

Use focused smoke tests for each stage and one focused end-to-end pass. Existing
smoke coverage includes snapshots/configuration changes, push/pull, scalar UI
controls, slider bursts, individual failures, 30-loop alignment, Lua observation,
recording and disconnect without replay. Do not run broad integration matrices
or add a callback-cost certification exercise. Keep CI in the background rather
than waiting for a large suite as a completion gate. Address relevant review
findings within scope; leave the PR in draft and do not merge automatically.
