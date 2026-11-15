# RT communication: design and working plan

## Purpose and status

Move the application toward complete state snapshots and ordered command groups,
with a simple communication model inside `rt_runtime`. This is the sole working
document for that migration. It describes the implemented system, the completed
response extension, and the remaining legacy migration areas.

Snapshot observation, ordinary UI command wiring, and the ordered RT response
extension are implemented. The command transport supports optional expiry
deadlines and a reliable response FIFO. Feature migrations listed below remain
separate work; this milestone supplies their communication foundation.

The next milestone is immediate recording for ordinary audio loops, described
below. It is planned, not implemented. Grabbing and quantized recording remain
outside that milestone.

The response extension delivers deadline capability only. All application
commands on the new path, including the planned recording commands, must continue
with no deadline. Assigning deadlines to specific application commands is
separate future work, outside both milestones.

There are two distinct boundaries:

```text
UI/application <-> control <-> real-time processing in the audio callback
                       snapshot ring: RT -> control
                       command FIFO: control -> RT
                       response FIFO: RT -> control
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
releases each group afterward. Execution occurs before the callback's first
unrendered processing quantum, or before return at the next unrendered frame if
the callback only buffers audio. Every captured group produces its complete
response in that callback. Already rendered audio is unchanged; snapshots still
publish only at settled processing ticks.

Each command and response FIFO contains 256 records. Each application group
contains 1–32 commands and fits the 1,280-byte transport envelope. Control can
stage four waiting groups; the UI service has an ordered queue of up to 256
messages. These are existing
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

Existing application commands have no deadlines. RT returns individual results
in order; ordinary UI actions remain fire-and-forget. Snapshots show resulting
state, and diagnostics count failures. `RtBackend::immediate_group`
and the shared backend immediate interface do not advance legacy mutation cursors
or reserve legacy outcome credits. Explicit tracked client commands and ordinary
backend mutation methods remain available for compatibility and other workflows.

Local acceptance does not certify RT execution. Full transport retains complete
groups and later UI work in order, propagating backpressure. On disconnect,
discard connection-owned unsent work; work already accepted by the backend may
still execute. Never replay automatically after reconnect.

Control preserves order across new and legacy paths. Immediate work waits for a
preceding legacy prefix to settle. Later legacy work waits for ordered responses
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
| Recording and capture | The next milestone moves immediate ordinary audio recording and finishing to the new path. Recording is still legacy today. Replacing, grabbing history, quantized/processed/MIDI recording and clearing content remain separate migrations. |
| Musical timing | Quantization, repeat-sync, seeking and timing edits require exact musical behavior and interaction with existing processor state. |
| Composites, processed playback and MIDI | Nested programs, dry/wet processing, processor-state restoration and MIDI note/state cleanup need feature-specific behavior. |
| Additional observation | Composite/plugin state, processor metadata, preview, profiling, memory and engine health retain separate streams. Decide which need coherent snapshots and which remain independent. |
| Setup and resynchronization | Capability negotiation and structural context are needed before interpreting live snapshots; connection recovery must avoid replay. |

Ownership return is a separate reliable responsibility. Graph plans, processors,
media buffers and other owners must eventually reach control for reuse or
destruction. Neither lossy snapshots nor byte-copying an owning Rust object can
provide that lifetime protocol. Preserve the existing return paths until their
replacements are designed.

## Implemented ordered command responses

### Clock and expiry deadlines

`ControlEndpoint::command_clock()` returns an atomic read handle for the next
unrendered processing frame. RT publishes it at callback entry and after processing,
independently of control polling and snapshot delivery. Responses record the frame
at which the group was attempted. These use the same `Frame` timeline as snapshots.

The host's existing `next_frame` tracks consumed callback frames and can differ
while buffering a quantum, so it cannot substitute for this processing-frame
observation. A buffer-only callback can leave the processing clock unchanged.
Loop seeks do not change it. A fresh engine creates fresh FIFOs and a fresh clock;
reconnecting to the same engine preserves the clock and never replays queued input.

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

The internal value layouts are explicit and little-endian:

| Value | Layout |
| --- | --- |
| `Header`, 48 bytes | Five `u64` words: graph, topology, deadline-present, deadline, attempt frame; then a `u32` header result and a reserved zero `u32` |
| `CommandRecord`, 16 bytes | Four `u32` words: opcode, configuration-local target, argument bits, response code |

Input result fields are `Unprocessed`. RT overwrites them in the response copy.
The generic ring supplies group counts and framing; its Rust enum layout is not
a hardware ABI. The RT carrier protocol is version 31; the UI/application protocol
remains version 36. Existing production batch constructors default to no deadline.

The preallocated SPSC response FIFO sits alongside the command FIFO inside
`rt_runtime`. RT streams command copies into reserved response records with
their result codes, preserving input order and group boundaries. Only the complete
response group is published. Control never sees a partial response, and RT needs
no separate whole-group buffer.

Every consumed group produces exactly one response group, including stale
configurations, expired groups and individual failures. Preserve the input header
and record the attempt's processing frame so control can interpret the results.
The result codes are `Applied` (including successful no-ops), `DeadlineMissed`,
`StaleConfiguration`, `TargetUnavailable`, `InvalidValue`, `Unsupported` and
`CannotApply`. `Unprocessed` never appears in a completed response. Header
`Applied` means its checks passed; individual member results determine which
commands succeeded. More specific codes can be added as features migrate.

There are no request IDs, cancellation tickets or reordered results. FIFO order
provides correspondence. Control may track outstanding groups and dependencies;
RT only consumes commands, attempts them and publishes their results. Ordinary
UI actions can remain fire-and-forget, with control draining responses and
snapshots providing display state. The extension does not require forwarding an
individual reply to the UI for every action.

Control forwards `EngineMessage::ImmediateResponse` reliably. The backend drains
these automatically. Callers that need each result can install
`RtBackend::set_immediate_response_handler`; it runs on control while pumping,
including during configuration resynchronization. Aggregate RT and local
admission failures feed the existing UI notice, without creating UI operation IDs.

### Same-callback execution

RT receives a group for execution when it belongs to the published prefix captured
at callback entry. It must attempt or explicitly refuse that group and publish
its complete response before the callback returns. Later publications wait for
the next callback. Every started group finishes in that callback; RT never
retains a command to retry or match with later input.

Buffer-only callbacks apply their prefix at the next unrendered frame before
returning. Fresh state is captured when processing next reaches a settled tick;
commands do not modify previously rendered audio. If tick setup fails before
command application, the prefix receives `CannotApply` results. Host blocks are
validated before capturing a command prefix, so a rejected block does not receive
groups for execution. There is no silent deferral or success for an unapplied step.

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

On disconnect, control drops unsent staging and marks the exact outstanding
response prefix for discard. Already RT-published commands may execute, and their
responses still release reservations, but they are not exposed to the replacement
connection. Fresh submissions retain FIFO correspondence without replay.

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

## Next milestone: immediate audio recording

### Goal and scope

Move immediate recording of ordinary audio-only primitive loops on direct,
unprocessed strips to the new UI/control/RT command path. Support starting a take,
finishing into Stopped, and finishing into Playing. Preserve the familiar
record -> record audio -> stop or play behavior, with snapshots showing the
actual state and ordered results reporting command success or failure.

This milestone excludes grabbing, replacing, quantized or armed starts,
fixed-cycle automatic finishes, MIDI recording, processed/dry-wet recording and
composites. Those actions keep their existing routes and remain available in the
UI. This is a recording migration; it does not introduce scheduling behavior.
All new recording commands use `deadline = None`, without new timeout policies,
request IDs, cancellation tickets or callback-work budgets.

### Capture storage is opaque to control

"Prepared capture resources" means recording storage ready for RT to acquire:
audio pages from a preallocated pool, a recording slot with its write position
and frame count, and the handles/reservations needed to seal and publish a take.
Audio pages can be acquired from that pool as recording grows; there is no
assumption of unlimited recording storage or allocation of a maximum-length take
for every loop.

Control configures capacities, allocates backing storage outside the callback,
and handles final reclamation. It does not select pages, advance write positions,
manage per-take preparation steps or understand the recording storage layout.
RT manages those details behind the loop command interface. Reuse existing
capture banks, snapshot reservations, media ownership and retirement mechanisms
where suitable. Resource handles are ownership handles, never command IDs.

Prepare ready resources during startup/configuration or safe recycling. A Record
command with no available ready storage fails individually in its receiving
callback, leaving the loop unchanged. Do not hold it pending or introduce a
per-click prepare/status/commit exchange. A successful start reserves the resources
needed to finish, so ordinary stopping does not depend on finding spare memory.

### Commands and observable behavior

The logical operations are `Record(loop)` and
`FinishRecording(loop, after_mode = Stopped | Playing)`. Exact opcodes may reuse
or extend existing Play/Stop commands. The required behavior is:

- **Record:** acquire ready capture resources, enter Recording and start a new
  take at the first sample of the next unrendered processing block. Later
  callbacks append input without further commands. Success means capture is
  attached and the recording state has changed, not that a completed take exists.
- **Finish into Stopped:** seal the captured prefix, publish it as completed
  media, release recording demand and enter Stopped at the command's processing
  frame.
- **Finish into Playing:** perform that same finish and select the completed
  take for playback from its beginning as one semantic action. Do not implement
  this as separate stop/play commands with an asynchronous preparation gap.

RT decides from its current state whether Play/Stop must finish a recording or
perform ordinary playback control. Snapshot lag must not route a real recording
through a playback-only handler that rejects it. Define repeated Record and
empty-take behavior explicitly, preserving existing ordinary behavior where
possible; an invalid repeat must not accidentally discard the captured prefix.

Use the existing command-consumption point, not a future execution timestamp.
Apply received groups and publish responses in the same callback, including
buffer-only callbacks. Recording changes affect the next unrendered processing
block and never rewrite audio already rendered. The RT command context needs
safe access to capture/resource operations and the corresponding state settlement;
changing only the public mode field is insufficient.

Finish success means the take is sealed and available to the requested next mode.
Keep loop lengths, positions, content revisions and capture flags consistent in
snapshots. Preserve existing input gain, monitoring intent and automatic monitoring
behavior, including overlapping recorders on the same strip. Completed takes
must also remain accessible through existing media/export APIs; a scalar snapshot
does not transfer their ownership.

### Failure and preservation rules

- Multiple Record commands in one group take effect at one processing frame.
  They may succeed or fail independently. Do not require a batch-wide preflight,
  all-or-nothing commit, rollback or a global input-transaction lock.
- Keep each loop's audio channels coherent as one take. Failed acquisition leaves
  the previous mode/content unchanged; successful replacement of playback media
  must preserve older exports and readers until their owners are released.
- Retain configuration identity, resource ownership, capacity checks and any
  content-revision checks needed to prevent stale prepared media overwriting a
  newer take. An unchanged graph version does not imply unchanged content.
- If storage is exhausted while recording, preserve the captured prefix, stop
  further capture safely and expose the incomplete/error state. Spontaneous
  capture errors use snapshots/diagnostics, never an unsolicited response group
  that would break command/response FIFO correspondence.
- Add recording-specific result codes where useful, such as unavailable capture
  storage. A failed command does not suppress later members. Never silently
  replay or retry a submitted recording command through the legacy path.

### Implementation stages

1. **Recording contract and resource preparation**

   - [ ] Map the existing capture/finish/ownership operations to the logical
     commands, define repeat/empty-take handling and identify the ready resources
     and finish reservations each take needs.
   - [ ] Prepare storage and readiness behind the RT interface. Verify failure
     to acquire a recording slot leaves the old loop intact, and finishing uses
     already reserved resources. Do not add control-side per-take polling.

2. **Capture-aware RT handlers**

   - [ ] Provide safe resource access at the immediate execution point and
     implement Record plus finish-to-stop/play with real capture settlement.
     Keep application-specific behavior outside generic FIFO code.
   - [ ] Smoke-test actual recorded samples and start/finish frames, buffer-only
     callbacks, independent group failures, empty/repeated actions, resource
     exhaustion and retained old media. No processing-time budget checks.

3. **UI and control wiring**

   - [ ] Route eligible recording, stop and play actions through complete groups,
     including selected-loop and solo gestures that fit the existing envelope.
     Base recording finalization on RT state, not a possibly stale UI mode.
   - [ ] Keep unsupported/mixed/oversized actions on their complete legacy route,
     selected before submission. Verify snapshots, monitoring and existing
     recording exports remain correct, without UI request IDs or deadlines.

4. **Focused end-to-end validation and completion**

   - [ ] Exercise Record -> audio capture -> finish-to-stop/play through direct
     and encoded delivery. Check a grouped start, Play/Stop before a fresh UI
     snapshot, unavailable storage, disconnect without replay and an unchanged
     legacy recording variant.
   - [ ] Run relevant smoke tests, formatting and warning-denying builds. Update
     this document and the existing draft PR; leave broad CI in the background.

### Recording milestone acceptance criteria

- Eligible ordinary audio recording uses the new command/result transport end
  to end. Every command returns a meaningful result in its receiving callback;
  no legacy transition is merely queued behind a reported success.
- Recorded samples and take length match the RT start/finish frames. Finish into
  Playing makes the completed take playable as one action; finish into Stopped
  leaves it available for later playback/export.
- A grouped start shares one processing frame among successful members, while
  failed members preserve their previous state. Snapshot lag cannot make Play
  or Stop bypass recording finalization.
- Capture resources remain opaque to control. No per-command preparation
  protocol, RT pending-request table, new request IDs, application deadlines,
  arming/scheduling or processing-time accounting is added.
- Capacity exhaustion, incomplete takes, monitoring and media ownership are
  handled explicitly. Unsupported recording variants remain functional through
  their existing routes. No automatic replay or legacy retry is introduced.
- Focused direct/encoded/UI smoke checks and the relevant build/format checks
  pass. No broad integration matrix or CI completion wait is required.

The execution and verification policy below applies to this milestone: keep its
checkboxes current, commit meaningful milestones on the existing branch, document
supported revisions to implementation/design, and obtain explicit user approval
before changing the goals or acceptance criteria. Do not merge the draft PR
automatically.

## Completed response-extension stages

1. **Clock and record contract**

   - [x] Define clock publication/reset semantics, optional deadline encoding,
     response fields/codes and layout versioning.
   - [x] Resolve callback/quantum buffering against the same-callback rule.
     Verify deadline equality, expiry and unchanged group ordering with focused
     clock/codec examples. Do not add execution-cost or time-budget machinery.

2. **Paired FIFOs and response reservations**

   - [x] Add generic response storage and control-side reservation accounting;
     copy command records and publish complete result groups.
   - [x] Verify wrapping, a paused control reader, exhausted response capacity
     and credit release on response consumption. RT must never wait for space.

3. **RT handler integration**

   - [x] Apply header checks once, attempt commands in order and fill every code;
     return all results before the receiving callback exits.
   - [x] Smoke-test successful, expired and stale groups, individual failures,
     cutoff races, buffered callbacks and a group of 30 aligned starts.

4. **Control and application integration**

   - [x] Drain results in order, preserve legacy handoff and reconnect rules,
     and expose useful failures without requiring UI request IDs. Keep deadlines
     absent for all existing application commands; add no deadline policies.
   - [x] Verify that existing snapshot-driven controls, dependencies and push/pull
     delivery keep working. Preserve unmigrated features through their routes.

5. **Focused end-to-end validation and documentation**

   - [x] Exercise UI -> control -> RT -> response -> control with both direct
     and encoded delivery, stalled consumption and disconnect without replay.
   - [x] Run relevant smoke tests, formatting and warning-denying builds; update
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

### Validation record

All 27 selected smoke checks pass: five generic FIFO/publication checks, three
protocol checks, eleven engine command/coexistence checks, three backend command
checks, three snapshot delivery checks and two UI service checks. They cover
complete publication under a paused producer, response-space exhaustion and
wrapping, inclusive expiry and future deadlines, clock lifetime, buffer-only
callbacks, individual failures, graph/media ordering, 30-loop alignment, encoded
delivery under backpressure, disconnect without replay and unchanged UI deadlines.

Formatting, test-attribute policy and a native UI build with Rust warnings denied
pass. The build disables optional native audio-provider features. No broad suite,
live browser run, callback timing benchmark or CI completion wait was used.
