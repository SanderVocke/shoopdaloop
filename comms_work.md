# RT communication: design and working plan

## Purpose and current status

Move the application toward coherent state snapshots and ordered value commands,
with simple transport inside `rt_runtime`. This is the sole working document.
Generic rings and handoffs belong in `rt_runtime`; loop/strip/bus behavior belongs
in `shoop_rt_engine` and `shoop_loops`. Networking, browser messaging and UI pacing
remain outside the audio callback. Implementation is Rust-only; straightforward
future ARM/FPGA mapping is a design aim, not hardware work to implement now.

```text
UI/application <-> control <-> RT processing in the audio callback
                       snapshot ring: RT -> control
                       command FIFO: control -> RT
                       response FIFO: RT -> control
```

**Delivered:** coherent snapshots and push/pull UI delivery; ordinary scalar UI/Lua
controls; complete loop transition-plan replacement and selective clearing;
ordinary audio recording, quantized transitions and fixed-cycle finishing;
completed audio Grab and Grab-and-continue with explicit or automatic musical
finishing; ordered same-callback responses and optional expiry capability.
The transition-plan milestone below is complete. Remaining operation families are
listed separately; full legacy-operation coverage remains the overall goal.

All application commands continue to use `deadline = None`. Musical delays
belong to loop behavior; assigning transport expiry deadlines to application
commands remains separate future work.

## Coverage goal and permitted tradeoffs

The migration's end goal is to support **all application operations currently
supported through legacy communication** through the new command/snapshot model.
The milestone scopes and remaining-area list describe sequencing, not permanent
feature exclusions. Leaving an operation on legacy preserves it during migration;
it does not count as delivering that operation through the new model.

**Communication simplicity takes priority over reproducing all legacy correctness
bookkeeping.** If the different model introduces limitations, the implementer may
make small relaxations to behavioral requirements, safeguards, edge-case guarantees
or capacity limits to keep the communication layer simple. This is explicit user
authorization to make those limited tradeoffs without asking again, including
corresponding adjustments to detailed acceptance expectations. Prefer simple
processor behavior and snapshot observation over more request/acknowledgement,
cancellation or timing machinery. Keep Rust safety and resource ownership intact.

Document each relaxation, its reason, the before/after behavior and any remaining
limitation here as it is made. **After completing implementation, report those
tradeoffs in this chat**, including their user-visible effects and any operations
still unmigrated. Say explicitly if no relaxations were needed; a document or PR
update alone does not satisfy that reporting requirement. This permission does
not authorize silently dropping an operation, declaring an omitted required
milestone feature complete, or abandoning the simple communication model.

## Design rules and configuration identity

- Publish complete snapshots and command groups. RT captures the readable command
  prefix at callback entry, consumes it in FIFO order and finishes every group it
  starts in that callback. Later publications wait for the next callback.
- Trust the application to construct sensible groups. Keep framing, opcode,
  length, value and memory-safety checks; add no whole-group semantic preflight
  or rollback. Individual failures preserve their targets and allow later members
  to proceed. Duplicate/opposing commands execute in order.
- **Execution is best effort. Do not introduce bookkeeping to prove callback work
  is bounded:** no processing-time accounting, command-cost estimates, CPU-time
  admission or callback work quotas. Fixed storage/message limits and the input
  cutoff remain storage and ordering rules, not processing-time guarantees.
- Prepare backing storage off RT. Handlers avoid heap allocation, destruction of
  owning resources, locks, waits and I/O. Return owners reliably for control-side
  retirement or reuse. Capture/history storage stays opaque to control.
- Use the existing graph/topology epoch pair and configuration-local typed slots.
  The same version denotes the same object incarnations. Creation, removal or
  replacement/reuse that changes the mapping activates a new version; preparing
  a candidate leaves the active mapping intact. No separate object registry or
  request-ID system is introduced.
- Check the configuration once per group. A stale version refuses the whole group;
  an unavailable target fails individually. Internal ownership handles and
  snapshot identities may retain generations. Content/history revisions remain
  necessary: unchanged graph identity does not imply unchanged media.
- Control retains active/pending configuration descriptions. Snapshots contain
  live state for that context, without repeating topology. A requested UI graph
  becomes active only after RT observation and matching context are available.
  Reject unknown/retired versions; resynchronize new clients before displaying them.
- Preserve unsupported functionality through clearly separated legacy routes,
  selected before submission. Never silently retry a submitted command through
  legacy, split a synchronized action to fit, or replay it after reconnect.

## Implemented snapshots

RT captures after same-frame settlement, before the next audio interval, at the
first settled tick of every callback. Buffer-only callbacks retain the preceding
publication. UI delivery settings never reduce RT capture frequency.

| State | Included information |
| --- | --- |
| Header | Schema, capture frame, graph/topology epochs, counts and payload length |
| Loops | Identity/generation, mode, nearest transition/countdown plus both new-plan entries and failure count, recording flags, cursor, length, gain, balance, repeat-sync, content/timing revisions |
| Media metadata | Audio/MIDI lengths and channel windows, gain, offsets, alignment and preplay; no media bytes or owning leases |
| Meters | Per-channel loop/strip/bus peaks; independent accumulators preserve legacy readers |
| Strips | Input/output gain and balance, output mute, monitoring policy/demand, MIDI activity and latest short input message |
| Buses | Identity/generation, gain, balance and mute |

Control values describe applied targets; ramps remain processor state.

The internal ring contains ten preallocated slots of equal payload capacity,
chosen from prepared session limits. Each publication has an actual length.
Reject a configuration that cannot fit before activation; do not resize live or
publish truncated success. Failed capture preserves the last complete snapshot.

Only RT writes slots. It marks a slot in progress, fills it, marks it complete and
advertises its sequence. RT can overwrite old publications and never waits for
control. Control reads the sequence, copies into private storage, then rereads it;
only an unchanged complete sequence is accepted. Otherwise it keeps its last copy.
No finite ring guarantees non-overlap if control pauses indefinitely. Rust uses
atomic payload words and ordered metadata: sequence validation alone cannot make
concurrent ordinary-memory copying safe. Sequence exhaustion stops publication
with an error instead of wrapping ambiguously.

Control validates configuration before updating its timing cache or forwarding
state. The UI assembles and validates a complete publication before applying it
to existing objects. Incomplete, unknown or obsolete publications never partially
change the display. Redundant timing queries and the legacy display selector are
removed. Snapshot age remains observable.

Every carrier supports both delivery modes:

- Native in-process defaults to pull: serve the newest complete snapshot without
  waiting for RT, with at most one adapter pull outstanding.
- Browser/network delivery defaults to push at 60 Hz. Delivery is configurable;
  control supersedes obsolete undelivered state and avoids catch-up bursts.

The UI draws on its own schedule using its latest accepted state. Network/browser
pulls remain asynchronous. Policy changes preserve complete assembly. Delivery
requests/settings stay outside the RT command FIFO.

## Implemented commands, responses and clock

Control publishes a whole group: one header followed by its actual commands.
RT executes directly from readable FIFO records, including wrapping. Each command
and response FIFO has 256 records. Application groups contain 1–32 commands within
the 1,280-byte carrier envelope; control can stage four waiting groups and the UI
service queues up to 256 messages. Independent groups can execute in one callback
without waiting for an intervening snapshot. Thirty synchronized starts use one
group and one processing frame.

Every received group is attempted or explicitly refused, with its complete ordered
response published before that callback returns. Execution precedes its first
unrendered quantum; buffer-only callbacks apply at the next unrendered frame.
Already rendered audio is unchanged. Snapshot capture still requires a settled
tick. Tick setup failure produces `CannotApply`; an invalid host block is rejected
before capturing a command prefix. RT never retains a transport command for a
later callback, retry, musical event or another group's arrival.

### Records and reliable results

| Value | Explicit little-endian layout |
| --- | --- |
| Header, 48 bytes | Five `u64`: graph, topology, deadline-present, deadline, attempt frame; then `u32` result and reserved zero `u32` |
| Command/result, 32 bytes | Eight `u32`: opcode, target, five argument words, result at byte offset 28 |

Input results are `Unprocessed`; RT copies commands and fills result codes.
The generic FIFO supplies counts/framing; its Rust enum layout is not a hardware
ABI. RT protocol version is 35, UI/application version 40 and snapshot schema 2.
Transition opcode 2 uses all five argument words: optional immediate mode, First
mode/delay, Second mode/delay. Modes are Stopped (0), Playing (1), Recording (2);
`u32::MAX` marks an absent mode, with zero delay for an absent entry. Opcode 15
clears First (1), Second (2) or Both (3). Grab/window and Grab/gesture use opcodes
13 and 14; the gesture flags also carry synced/continue intent.
Unknown modes, noncanonical fields and removed Stop/Record opcodes are rejected.
Maximum current Grab requests use 821 bytes without a deadline (829 with one);
maximum responses use 1,076 bytes, within the carrier envelope.

Results include `Applied` (also successful no-ops), `Scheduled` for installed future
work, `DeadlineMissed`,
`StaleConfiguration`, `TargetUnavailable`, `InvalidValue`, `Unsupported`,
`CannotApply`, `CaptureUnavailable` and `HistoryUnavailable`. Header `Applied`
means header checks passed; each member has its own result. Completed responses
contain no `Unprocessed`. Feature-specific result codes may be added.

Control reserves response space for the entire group before publishing input,
and retains that reservation until it consumes the response. Releasing command
records does not release response credit. RT never waits for a response reader,
drops results or needs a separate whole-group buffer. Exhausted storage creates
control-side backpressure while audio continues. FIFO order provides result
correspondence without request IDs or cancellation tickets.

Control forwards responses reliably and the backend drains them automatically.
`RtBackend::set_immediate_response_handler` optionally observes them while pumping.
Ordinary UI commands remain fire-and-forget; snapshots drive display and aggregate
failure notices report problems without UI operation IDs. A response can precede
the snapshot covering its frame. Control waits for fresh matching observation
only when a dependent workflow requires it.

### Processing clock and expiry

`ControlEndpoint::command_clock()` atomically exposes the next unrendered
processing frame, published at callback entry/exit independently of polling.
Responses stamp the attempt frame on that same snapshot timeline. The host's
consumed-frame counter can differ during quantum buffering; loop seeking does
not change this clock. Reconnecting preserves it; a fresh engine resets it.

An optional group deadline is an inclusive latest execution frame. RT skips an
already-late group with `DeadlineMissed` before attempting members. A future
deadline never delays execution or schedules work. It applies to the synchronous
command step, not the eventual end of a take or musical plan. The atomic clock
offers observation, not a delivery-latency guarantee or a CPU reservation.

**All application groups use `deadline = None`.** Explicit expiry is exercised
only by synthetic transport checks. No default timeouts, automatic deadlines,
command-specific policies or UI deadline settings belong to this work.

### Ordering, legacy handoff and disconnect

Local admission does not prove RT execution. Backpressure preserves complete
groups and later UI work in order. New-path calls do not advance legacy mutation
cursors or reserve legacy outcome credits. Earlier legacy work must settle before
handoff; later graph/media work waits for ordered results and an appropriate
snapshot. Reconcile applied scalar values so a later graph edit cannot restore
old controls. Preserve legacy failure and disconnect outcomes.

Disconnect drops connection-owned unsent staging and marks the exact outstanding
response prefix for discard. Published work may execute and still releases its
response reservations. Active capture can continue. New connections receive no
replay and no old results.

## Implemented loop, recording and Grab behavior

`Transition(loop, TransitionPlan)` is used throughout UI, shared backend, adapter and RT;
Grab shares its destination-mode enum. Eligible UI/Lua controls also include loop
gain/balance, strip input/output gain/balance and output mute, and bus gain/balance/
mute. The adapter converts UI decibels to linear amplitude. Selected/solo actions
use a complete group when supported and within capacity.

- **Playing:** start/restart loaded audio at zero, or seal an active take and
  select it for playback as one action.
- **Recording:** acquire ready capture storage and begin recording. Repeating it
  while recording succeeds without restarting or replacing the take.
- **Stopped:** seal active capture and stop. Repeated Stop succeeds harmlessly.

RT chooses behavior from its actual state, including before a fresh UI snapshot.
Playing an empty active take fails and preserves recording; Stop can seal it.
These are capture-aware actions, not assignments to a public mode field.

Control prepares capacity and backing pools off RT and handles final reclamation.
It does not manage pages, write positions or per-take preparation. RT acquires
existing capture banks and media/finish reservations before changing state;
ordinary finishing uses reserved resources. No per-click prepare/status/commit
exchange is needed. Acquisition failure preserves previous state/content. Capture
exhaustion retains an observable, exportable incomplete prefix. Input gain,
automatic/manual monitoring, overlapping demand and older readers/exports remain
valid. Media bytes and ownership use existing reliable paths.

`LoopPlanner::grab_immediate` chooses history using current RT phase, including
natural wraps and inherited sync pulses. It protects shared pages, installs a
completed take or attaches growing capture in that callback, with no missing or
duplicated samples at the history/live join. Explicit Playing/Stopped finishes
both capture-bank takes and history-backed growing takes. Missing/expired/gapped
history or unavailable storage preserves the old take; startup padding retains
its existing rules. Grab against an already Recording target fails individually
and preserves its prefix. Successful continuing capture retains finish resources
and publishes its usable prefix on exhaustion.

Grab retains separate eligibility and supports primitive repeat-sync behavior.
New plans coexist with immediate controls and other loops' pending actions.
Eligible quantized and fixed-cycle recording gestures and automatic Grab finishing
use the new path. MIDI, processed/composite, unsupported/mixed/oversized actions,
longer/incremental plans and no-clock fixed-cycle recording retain legacy routes.
Both plan entries and their cumulative per-incarnation failure count reach the
UI in `snapshot.immediate.plans`; existing nearest-transition displays still work.

## Remaining migration areas

| Area | Status and remaining difficulty |
| --- | --- |
| Musical plans | Two-action replacement, clearing and ordinary automatic finishes are migrated. Longer/incremental plans, no-clock fixed-cycle recording, seeking, repeat-sync changes and timing edits retain legacy routes. |
| Advanced capture/Grab | Ordinary audio Grab with automatic musical finishing is migrated. Replacing, MIDI, processed/composite capture, active-recording Grab targets and clearing content need later migration of their ownership/behavior. |
| Graph/processors | Creation, routing, replacement and removal need preparation, versioned activation and retirement of displaced owners. |
| Media/persistence | Loading, exporting and saving need reliable bulk delivery, stable cuts, content revisions and resource lifetimes. Snapshots carry no media bytes. |
| Composites/processed playback/MIDI | Nested programs, dry/wet behavior, processor restoration and MIDI note/state cleanup need feature-specific migration. |
| Other controls/observation | Monitoring policy, playback defaults, smoothing/effect controls; composite/plugin state, preview, profiling, memory and health need suitable semantics/streams. |
| Compatibility/setup | Tracked APIs, larger/mixed gestures, capability negotiation and resynchronization remain supported; preserve complete routes and avoid replay. |

Ownership return remains reliable and separate from lossy snapshots. Never copy
an owning Rust object as a value packet or remove its return path before a safe
replacement exists.

## Completed milestone: complete loop transition plans

### Goal and scope

Extend the new interface for ordinary audio-only primitive loops on direct,
unprocessed strips: install up to two future musical transitions together, replace
the complete plan, clear individual entries or both, and observe the complete
plan through snapshots. Wire eligible quantized Play/Stop/Record, fixed-cycle
recording, and existing ordinary audio Grab-and-continue gestures with automatic
musical-boundary finishing through it. Include Stop at the boundary and Playing
when the existing play-after-record setting requests it. Scheduled finishing must
handle ordinary recordings and history-backed growing takes through the existing
capture machinery. **Future-transition planning alone is not milestone completion:
the Grab -> continue recording -> finish at a boundary flow must also use the new
model end to end.**

This stage does not migrate processed/MIDI/composite modes or add transport
scheduling; these remain part of the overall coverage goal. Other unsupported
actions keep their whole legacy route. The legacy engine permits eight planned
transitions per loop; the new interface carries two. Preserve longer legacy plans
and incremental APIs that cannot safely express full replacement; do not truncate
them or claim their migration is complete. Deliberate replacement/clearing must
also disarm old legacy follow-ups for that target, with reliable retirement;
they must not fire later over a new plan. Leave other targets' plans intact.

### Command contract

Extend the logical transition value to:

```text
Transition {
    loop,
    now: optional mode,
    first: optional (mode, sync delay),
    second: optional (mode, sync delay),
}
ClearPlannedTransitions(loop, First | Second | Both)
```

Modes remain Stopped, Playing and Recording. `now = None` leaves current playback,
cursor and capture untouched. Every successful Transition replaces the entire
future plan; omitted entries remove the corresponding old actions. An immediate
transition without future entries therefore clears the old plan. A Transition
with no immediate or future actions cancels the plan without stopping audio.

The immediate action and replacement are one command. If the immediate action
fails, retain the old state/content and old plan. Installing future actions is not
a promise that their later resource-dependent execution will succeed. Preserve
per-command coherence without adding group-wide preflight or rollback.

The five argument words hold `now mode`, `first mode/delay`, `second mode/delay`
within the existing 32-byte record using the versioned canonical encoding above;
there is no new handle or request identity. Thirty
complete two-action plans still fit one 30-command group and the current envelope.
Use only ordinary format/safety checks; trust the app to supply sensible plans.

### Grab-and-continue with automatic finishing

Migrate the existing ordinary audio gesture, including its sync/target and cycle
choices. Resolve both the history window and intended finish boundary from
authoritative RT state at the cut, reusing the current gesture/window logic.
Preserve Stop/Play after-mode selection and supported selection/solo behavior,
subject to the documented small-relaxation policy above.

Attach protected history, start continuation capture and install its musical
finish as one coherent per-loop operation in the receiving callback. Extend the
Grab value/handler as needed and use the same loop-owned plan machinery; add no
separate transport scheduler or request lifecycle. Installation success confirms
both capture and its finish plan. A failed Grab must not leave a finish armed
against the previous take; preserve the previous content/state/plan and release
temporary resources. Do not depend on a later UI snapshot to decide whether to
arm the finish.

At the intended boundary, seal the growing take and enter Stopped or Playing
using existing capture reservations and monitoring/ownership handling. Snapshots
show the active continuation, its planned finish and the resulting completed take.
Replacement, selective clearing, future failure and disconnect obey the same plan
rules below. Clearing the finish alone leaves current recording running; an
explicit immediate Stop/Play finishes it. Keeping this entire gesture on legacy
does not satisfy this milestone.

### Musical timing and execution

- Both future points count actual sync pulses from the same RT installation
  boundary. Keep the existing skip convention: 0 means the next eligible pulse,
  1 the second, and so on. The second delay is relative to installation, not to
  the first action. Thus Playing at pulse two then Stopped four cycles later is
  `first = (Playing, 1)`, `second = (Stopped, 5)`.
- Keep pending musical state in the loop/planner. Consume and answer the command
  in its receiving callback, including buffer-only callbacks; do not retain a
  transport group or allocate a legacy request lifetime until a future event.
  Installation success reports its synchronous meaning, e.g. `Scheduled`.
  Future execution/failure is observed through snapshots/diagnostics, without an
  unsolicited response that breaks FIFO correspondence.
- Apply the callback's command prefix before musical settlement at its next
  unrendered frame. A pulse resolved there counts consistently for installed
  entries; cancellation before settlement can prevent a due action. Already
  executed actions are never undone. Preserve alignment across grouped loops.
- Use sync-source/self-clock wraps and meaningful musical boundaries, with the
  documented restart difference below;
  do not translate cycle delays into audio-clock deadlines or wall-clock timers.
  Preserve supported no-sync UI behavior through the appropriate route. A paused
  source pauses its countdown; target removal or a changed sync source must
  invalidate the affected plan safely. An unrelated graph edit must not retarget it.
- Immediate Playing keeps its current restart-at-zero behavior. Scheduled Playing
  preserves legacy position when already playing; finishing recording into Playing
  still seals and selects the take from its beginning. Recording repeats retain
  the active take; scheduled Stop/Play performs real capture finalization.
- Acquire capture storage and update monitoring through existing RT/planner
  resource operations. On a failed future action, preserve valid current state/
  media and clear that loop's remaining plan, so a failed recording start cannot
  later play an older take through its planned finish. Other loops and later
  command-group members remain independent. Preserve incomplete-prefix handling.

### Cancellation and observation

`ClearPlannedTransitions` clears only the selected fixed entry positions. First
executing or being cleared never renames Second. Clearing an absent entry succeeds
harmlessly; clearing future work leaves current audio/capture untouched. Clearing
First preserves Second; the app chooses Both to cancel a whole linked gesture.
Use groups to coordinate cancellation across supported loops.

Selective clearing operates on RT's current plan and retains the other entry's
exact countdown. Do not reconstruct the surviving entry from an old UI snapshot
and restart its delay. Cancellation addresses the loop's current entry positions;
it introduces no cancellation ticket or historical request ID. FIFO order decides
which replacement/clear wins.

Publish both entries, their presence/modes and remaining delays coherently with
current loop state and configuration. Keep positions stable after execution, and
make plan failure/cancellation observable through existing state/diagnostics as
appropriate. The UI must not infer execution merely from local admission or a
plan-installation result. Current nearest-transition displays can be derived from
the complete observed plan.

Control/UI must assemble a complete intent before submission. Translating the
old separate start/finish calls one-for-one would erase the start plan. New plans
must coexist with immediate controls and unrelated loops' pending work; remove
the blanket rejection of immediate transitions whenever any plan is pending.
Preserve ordered legacy handoff without waiting for a musical plan to finish
before admitting ordinary later new-path commands.

### Implementation stages

1. **Value protocol and snapshot contract**

   - [x] Extend UI/shared/backend/RT values with complete replacement and selective
     clearing, canonical encoding and synchronous result meaning; expose both
     future entries in snapshots and bump affected versions. Carry automatic
     Grab-continuation finish intent through the new interface as well.
   - [x] Verify absent fields, all three modes, maximum groups, stable entry
     positions, skip counting and distinction from expiry deadlines with focused
     codec/state examples.

2. **RT musical state and capture integration**

   - [x] Install/replace/clear loop-owned plans synchronously, reuse pulse and
     capture machinery, preserve repeated-action semantics and remove global
     pending-plan restrictions without retaining transport requests. Integrate
     history attachment, continuation start and automatic finish installation.
   - [x] Smoke-test exact pulse/frame execution, two-step plans, immediate-action
     failure preserving an old plan, future failure clearing its remainder,
     recording/Grab-continuation finishing, monitoring, source changes, buffer-only
     callbacks and ordinary controls while other loops have plans.

3. **UI/control integration and compatibility**

   - [x] Send eligible complete scheduled/fixed-cycle recording intents and Grab
     gestures that continue recording and finish at a boundary through the new
     UI/Lua path; wire clearing and both-entry displays over direct/encoded delivery.
     Keep transition plans as one command per loop and deadline fields absent.
   - [x] Check stale-snapshot cancellation without moving the survivor, replacement
     before a fresh snapshot, legacy/new handoff without leftover follow-ups,
     selected/solo grouping and unchanged unsupported/longer-plan legacy behavior.
     Never silently split, retry or replay.

4. **Focused end-to-end validation and completion**

   - [x] Exercise 30 grouped loops sharing a sync source, starting at the same
     boundary and stopping four cycles later; plan replacement and clearing,
     scheduled recording/finishing and Grab -> continued input -> automatic
     Stop/Play with real samples, resource failure and disconnect without replay.
     Verify snapshots, exact history/live joins and same-callback results.
   - [x] Run relevant smoke checks, formatting, test-attribute policy and a
     warning-denying build. Update this document and the existing draft PR;
     commit/push completed work and report behavior relaxations/remaining limits
     in this chat. No broad suite or CI completion gate.

### Acceptance criteria

- A complete two-action plan is installed by one command in its receiving callback.
  Both points share the RT installation origin; successful grouped loops with the
  same sync source and delays execute at identical intended musical boundaries.
- Successful replacement removes the old plan. Failed immediate execution preserves
  old state/content and plan. Selective clearing never shifts or renames the
  surviving entry; clearing Both leaves current audio/capture running unchanged.
- All accepted commands receive their ordered response in that callback, including
  buffer-only callbacks. Future actions live in loop state, with no retained
  transport command, new request IDs, tickets, reordering or execution reply later.
- Snapshots expose both entries and actual state coherently. Future failure cannot
  trigger a stale dependent finish; capture/media ownership and monitoring remain
  correct. Pending plans do not globally block ordinary new-path controls.
- Eligible UI/Lua scheduled and fixed-cycle recording behavior uses the new path;
  unsupported/mixed/oversized/longer plans remain functional through their complete
  legacy routes. No application deadline assignment or callback-work accounting.
- Ordinary audio Grab-and-continue followed by automatic Stop/Play at its musical
  boundary uses the new UI/control/RT model, with correct take finalization and no
  stale finish after failed acquisition. Implementing future loop transitions
  while leaving that whole gesture on legacy is incomplete.
- Focused direct/encoded/UI smoke coverage and relevant build/format checks pass.
  Report all small requirement relaxations and remaining limitations in this chat
  on completion, as authorized above. Removing required operation coverage or
  changing the core communication model requires explicit user approval.

### Delivered design choices and remaining limits

- New musical state is a preallocated pair of entries per loop in `LoopPlanner`.
  The existing capture-aware immediate handlers execute due actions before tick
  settlement. The old resolver's request/receipt lifetime is not used for new
  plans. Successful installation, cancellation and responses remain synchronous.
- **Timing relaxation:** new countdowns advance on completed primitive sync wraps.
  Starting/restarting a paused source does not itself consume a count, so an
  already armed plan can run one cycle later than legacy's entry-pulse behavior.
  This keeps timing independent of legacy request/completion and event-origin
  bookkeeping. Ordinary running-clock boundaries and grouped sample alignment
  are preserved; a focused paused/restart check documents the difference.
- A single delayed action on an unsynchronized, non-playing loop uses the legacy
  immediate fallback, so the sync loop can start recording before any clock runs.
  A two-step plan requires a usable musical source. UI fixed-cycle recording with
  sync disabled (or on the sync loop itself) retains its complete legacy route;
  no virtual clock, wall-clock timer or deadline policy was introduced.
- The new packet contains two future entries, as agreed; longer and incremental
  legacy plans remain supported on their old route. Existing delayed
  `loop_transition` calls retain incremental semantics. Explicit Lua
  `loop_transition_plan(selector, now, first_mode, first_skip, second_mode,
  second_skip)` sends a full replacement (`nil` for absent values), and
  `loop_clear_planned_transitions(selector, mask)` clears entry mask 1/2/3.
- UI quantization uses the latest observed target position for its requested
  delay; network/snapshot lag is not hidden by extra synchronization. Grab's
  history selection and target-dependent finish are resolved from RT state.
  A continuation installed exactly on a wrap finishes at a later boundary,
  allowing live recording instead of immediately sealing an empty take.
- Scheduled resource failure clears the loop's remaining plan and is visible in
  snapshots as a failure counter. Storage and media ownership remain protected;
  future execution creates no additional command response. Processed/MIDI/
  composite, graph/media lifecycle and other areas above remain unmigrated.

## Execution and verification policy

Keep this document current and check off completed work. Commit each completed
stage or meaningful milestone on `feat/rt-state-snapshots`, push to draft PR #997
against `feat/backend-replacement`, and update the PR. Implementation steps may
change with evidence; design revisions require a documented, supported reason.
The coverage/tradeoff policy above permits small behavioral requirement and
acceptance-detail relaxations for simplicity, with documentation and a final chat
report. Changes to operation-coverage goals or the core model still require
explicit user approval.

Use focused smoke checks per stage and a focused end-to-end pass. Do not run broad
integration matrices or callback-cost certification. Check relevant review feedback
and address findings within scope; keep CI in the background, without waiting for
a large suite. Leave the PR in draft and do not merge automatically.

Latest milestone validation: 52 selected native/offline checks pass: protocol 7,
engine immediate 30 (including seven new musical scenarios), backend immediate 5,
UI immediate 6, UI client 1, existing legacy timing 2 and aligned legacy recording
1. Coverage includes direct/encoded delivery, exact samples and history/live joins,
30-loop alignment, stable clearing, failed-now preservation, future failure,
source changes, legacy replacement, buffer-only callbacks, paused-clock behavior,
startup recording, automatic Grab finish and disconnect without replay.
Formatting, test-attribute policy and a Rust-warning-denying native UI build pass
with optional native audio-provider features disabled. The existing unused
message-port dependency manifest warning remains. No broad suite, callback timing
benchmark, live browser run or CI completion wait was used.
