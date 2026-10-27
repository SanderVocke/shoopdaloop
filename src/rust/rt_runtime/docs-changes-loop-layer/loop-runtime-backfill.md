# Loop facade, state/export and repository contract backfill

Companion to [loop foundations](loop-foundations-backfill.md). Destinations are
`shoop_loop_engine/docs/runtime.md` and the three repository loop documents reviewed
in `loop-guides-baseline.json`. These are change instructions; do not apply them by
rewriting authored guides during this implementation review.

## Current facade surface and recovery

Preserve Engine::prepare/prepare_with_observer and the fixed-graph
prepare_with_staging walkthrough. Add the opt-in prepare_with_configuration_staging
path and separate prepare_graph_staging call before rendering. Inactive prepared
physical templates may be outside the initial graph; only assigned primitive roles
belong in its required bindings. Controller::compile_graph is allocating control-side
work, not worklet ingress. stage_graph/release_graph use prepared submission and
reliable owning completions. Distinguish controller-side compilation from receiver
materialization, validation, activation and retirement.

Controller::submit/submit_payload/read/parameter also use submit_prepared. Do not
copy the generic Control::submit allocation warning as though this facade allocates
a new cancellation flag for each call. Payload buffers and compiled plans still need
preparation/recycling; retained tickets/completions can exhaust the prepared flag
pool independently of queue/result admission. An empty Box is not an owning nonempty
payload allocation. Validate these distinctions in the eventual host adapter rather
than assuming a prepared queue makes arbitrary producer code allocation-free.

Correct runtime.md's sentence that only cancellation and immediate Stop are recovery
safe. Current LoopPlanner::recovery_safe accepts:

- Cancel;
- TopologyCancel;
- RoleCancel;
- RoleSourceRelease;
- Transition with Timing::At and Transition::Stop.

They still require payload-free at-or-before-current-frame delivery, finite reserved
transport/result capacity and a bounded semantic operation. They must resolve at the
delivery frame, either during delivery or that frame's planning, before advancing a
positive interval. The runtime checks recovery_pending immediately after plan; merely
finishing sometime before callback end is insufficient. This precision also corrects
the chapter14 handoff wording. Graph/role/configuration activation and ordinary
source validation are not emergency-lane bypasses. ReleaseGraph remains normal-lane.

Use a small operation/result table rather than making readers infer the surface from
an enum dump. Separate request acceptance from terminal Reply and both from ownership
return. Four frame identities remain explicit. Query saturation and unrelated result
stalls may refuse new requests but cannot stop admitted automatic music. Recovery's
own unread result must be consumed/recycled before another recovery admission.

## Teach each configuration transaction separately

Apply the existing notes in order:

1. [Live graphs](loop-facade-live-graphs.md): fixed-role graph routing, matching
   physical decision bindings, GraphActivate and immediate TopologyGraphActivate.
2. [Media-owner permits](loop-media-owner-activation.md): exclusive whole-batch
   owner/initial-media publication, preserved exports/pins, no speculative reuse of
   metadata expected from retirement.
3. [Configuration activation](loop-configuration-activation.md): source snapshot/
   incremental validation, role-edit banks, matching topology/graph candidates,
   stopped edited owners and idle capture claims, explicit input preserve/reset.
4. [Paired program/graph](loop-program-graph-activation.md): one selected running
   program changes at a real zero/stopped cut; graph remains Ready/unlocked while
   pending, is rechecked at the cut and publishes only after genuine family preflight.
5. [Nested lifecycle](loop-nested-lifecycle.md): seeks through delayed/play/record
   spans, direct priority, countdown cancellation, restart and immutable revisions.

The configuration note's original warning that paired publication is unavailable
is superseded by the next note; preserve milestone history but do not copy that
warning into current guides. Scope remains prepared physical templates, not runtime
processor construction, changing port shapes or arbitrary live node replacement.
For immediate stopped-owner changes, a Stop only queued for the same frame is not
already committed state at delivery. For a pending program change, zero eligibility
is a real entry, not an idle cursor repeatedly observed at zero.

Graph-stage, topology-stage, role/source and media/export IDs name distinct scoped
pools. LoopId and standalone RequestId instead contain slot/generation only; the
caller must route them to the correct resolver. Runtime Sequence is not standalone
RequestId or an authentication token. No common integer shape makes these IDs
interchangeable or authentic across independent Wasm instances. Ready timing IR is
not Ready graph compatibility. Candidate graph
release/reuse or independent activation can invalidate a pending pair without
rolling back the independent operation. Retire graph, topology and role metadata
through their separate APIs; a single musical terminal result does not recycle them.

## State/export task: one truthful settled cut

Retain the existing post-settlement/pre-render F query contract. Demonstrate Export
submitted before same-frame Record→Play yet reporting the new Playing state and
frozen prefix excluding F. The facade Cut contains frame, graph_epoch, topology_epoch,
state, playback/latest borrowed IDs, gain and capture/MIDI-overflow diagnostics. It
has no public AfterInterval enum field. Standalone Resolver::state after finish can
show position==length with a wrap pending; that is not a settled facade query cut.
Do not instruct a user to read an imaginary phase member or implicitly settle twice.

Playback, Latest and Recording selections are different. Recording export must
acquire a common immutable audio/MIDI prefix without interleaved rendering; failure
releases provisional leases/metadata and returns no partial export. Optional tail
copy Budget refusal is not capture exhaustion. A held export remains at its original
owner/revision/capture cut after later recording, selection or owner-generation edit.
Its request cut does not relabel when old media was captured.

Use Cursor and Controller::read with a prepared owning byte buffer. Inspect admission,
resource reply written/eof and the advanced cursor returned in the command, then
recycle the buffer off-render. Keep ExportId alive until every read is done. Decode
LEf32 audio and original MIDI frame/length/packet records; synthetic restoration
packets appear in captured media only if they actually traversed the capture graph.

Retrieve MidiInitial for all16channels plus the System Reset marker from that same
immutable export; reconstruct with MidiState::from_channels. Channel0 alone suffices
for the small example's assertion, not a general export codec. Compare held velocities,
pedal state, loaded/pending banks and selector hints against independent expectations.
Release once; stale/double release must not decrement a reused generation's owner.
The resource-handle owner remains responsible for imported resources after cleanup.

## Fault, shutdown and host boundaries

A graceful host may schedule Stop, await outcomes and deliver MIDI cleanup/recovery
before stopping callbacks. An idle countdown is not guaranteed to finish without an
explicit cancel/Stop policy. Disconnect alone is neither Stop nor rollback. Stop/join
callback execution before Engine::into_stopped destroys graph/processors off-render.
Stopped::cleanup permanently quiesces, collects pending/applied configuration owners,
uses the actual committed capture prefix and releases ledger/groups/created resources.
It must not invent finish for an unrendered interval or resume a faulted domain.
Repeat cleanup and drain bounded reclamation; check pages/versions/snapshots, media
references, role sources and all staged/retired owners separately. Remove caller-owned
import resources explicitly. Device resets cannot be emitted after the device stops.

Host integration documentation must show the callback and per-output recovery drain,
prepared input/output buffers, complete MIDI packet parsing and ownership returns.
Neither library owns devices, MessagePort, shared memory or a UI clock. The current
coordinated_loop example is offline Rust execution, package Node/Chromium tests run
Wasm tests, and the existing rt_runtime host is a generic fixture: none is the actual
loop AudioWorklet. Keep tests/loop_engine_host and its wire/journal README marked
pending until implemented and independently verified with actual Web Audio samples,
non-shared memories, fixed-memory/canary evidence and native comparison.

## Repository document corrections

### docs/loop_layer/SEMANTICS.md

- Update the opening implementation status: resolver, runtime/resource/media facade
  and supported Stage6 transactions exist. Independent host/final verification remains
  pending. Preserve INTER_LOOP_PROCESSING decisions, precedence and all exclusions.
- Explain AfterInterval as a conceptual standalone observation phase, not a current
  facade Cut field. All public facade State/Export cuts use the documented settled
  phase. Do not change callback-end or zero-frame semantics to conceal that distinction.
- Replace generic Cancel(request_id) prose with explicit layers: standalone Resolver
  uses RequestId; facade Operation::Cancel names the runtime Sequence and distinguishes
  queued from delivered pending work. Timing::At is paired with a separate frame.
- Qualify channel padding: the current importer requires matching prepared duration;
  a host can explicitly materialize padding off-render, but no implicit facade padding
  API is shipped. Do not silently weaken equal-duration validation.
- Replace blanket general graph/role transaction pending statements with the supported
  prepared-template and paired paths. Preserve family refusal, no old-zero-action
  publication and permanent quiescence. The normative scenario table remains a contract;
  link observed tests rather than declaring every future host obligation satisfied.

### docs/loop_layer/RUNTIME_INTEGRATION.md

- Reconcile opening and later fixed-graph/running-program limitation paragraphs with
  current implementation, without rewriting original Stage5 measurements or partial
  Miri evidence. The callback/interval capability split and four work classes remain.
- Replace the obsolete held-normal-validation-does-not-preempt-recovery statement:
  the fixed parked owner now allows eligible callback-snapshotted recovery first,
  preserves validator sequence/progress, resumes FIFO without rechecking cancellation,
  and obeys operation/publication limits. No fairness under endless recovery is added.
- Keep recovery termination at the delivery frame precise. Do not defer it to an
  arbitrary later interval in the same callback.
- Describe actual Cut fields/phase and the implemented observer, not a proposed
  general phase-tagged publication buffer as though it were an API. Split proposed
  host serialization from shipped core data. tests/loop_engine_host is still a future
  stage deliverable, not an existing adapter proven by generic-host results.

### docs/loop_layer/FACADE_DESIGN.md

- Keep verified Stage5 architecture/evidence intact; add current configuration scope
  and links rather than leaving live topology/program replacement entirely pending.
- Extend role/binding validation to fixed templates, stopped-owner batch publication,
  source ownership and exact-generation observations. Link paired publication order
  and postpublication fault cleanup; no rollback or arbitrary processor construction.
- Keep evidence scopes separate: the public example asserts the lifecycle, while the
  ten integrated Stage5 tests supply scoped allocator guards. Later153-case package
  evidence and still-running consolidated Stage6 Miri do not imply independent-host,
  benchmark or final-head CI/review acceptance.

All three destinations were read completely and hashed, not edited. Remaining
repository runtime architecture/audit/performance/fixture follow-ups still require
their own complete review; this note does not certify them or final LL-A acceptance.
