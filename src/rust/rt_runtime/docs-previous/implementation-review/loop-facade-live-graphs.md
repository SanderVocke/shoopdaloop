# Change instructions — loop facade live routes and paired configuration cuts

Destinations for the documentation worker: `src/rust/shoop_loop_engine/docs/runtime.md`,
`docs/topology-activation.md`, `docs/program-changes.md`, the loop README, and
repository `docs/loop_layer/{RUNTIME_INTEGRATION,FACADE_DESIGN,TOPOLOGY_ACTIVATION_DESIGN}.md`.
This is a follow-up to the generic graph-permit notes, not a replacement generic
book chapter. Do not imply that generic rt_runtime understands loop modes or roles.

## Superseded statements

The reviewed loop runtime guide says general graph replacement and changed-running
iteration-zero activation remain work. The topology-activation guide correctly
explains the already-implemented owning running-program operation, but still says
the Controller cannot change the signal graph after preparation. Update these
statements precisely:

- Owning `ProgramActivate` already implements a single program's stopped/zero cut.
- The facade now exposes **validated live routing with fixed logical identities,
  physical processor identities/signatures and role bindings**.
- It also publishes a staged timing topology and staged graph together at operation
  delivery, provided changed programs are already stopped.
- Arbitrary logical/physical role-generation replacement, new media claims/selections
  as one configuration transaction, and combined running-program/graph publication
  are not supplied by these new operations. Do not relabel this as complete Stage6.

## Preparation and owning graph admission

Add `Engine::prepare_graph_staging(candidates)` as opt-in allocating preparation,
once before the first nonempty callback. Existing constructors still leave graph
staging disabled; timing-topology staging remains a separate `TopologySettings`
facility. Zero candidates and late/repeated preparation are refused.

Preparation now retains a separate control-side Catalog and immutable decision
bindings. `Controller::compile_graph(&GraphDescription)` allocates/compiles using
that copy off-render; it cannot manufacture new processor slots, signatures or
logical roles. This has preparation storage cost, not callback work. Do not run
compilation in a worklet process or MessagePort handler as an allocation workaround.

`Controller::stage_graph(slot, expected_epoch, plan)` and `release_graph(id)` use
prepared normal-lane owning transport submissions, not unchecked command access.
Admission/validation and owning return semantics are the generic graph-bank API:
StageGraph Applied means Ready, not active; rejection/cancellation returns the
original owner; ReleaseGraph returns unused or displaced metadata for off-render
reuse/disposal. Graph IDs are re-exported as `GraphStageId` and `GraphStageStatus`;
these are distinct from the timing receiver's StageId and Status.

No wire receiver or cross-instance pointer transport is introduced by this surface.
A real independent worklet host still needs prepared graph materialization/transport.

## New typed operations and replies

| Operation | Result / meaning |
| --- | --- |
| `GraphStatus { id }` | `Reply::GraphStatus { id, status }`, using generic Ready/validated_epoch or Taken semantics. |
| `GraphActivate { id, graph_epoch, topology_epoch }` | `Reply::GraphActivated` with new graph epoch and unchanged topology epoch. Preserves current domain/media ownership. |
| `TopologyGraphActivate { id, graph, graph_epoch, topology_epoch }` | `Reply::TopologyGraphActivated` with both new epochs and both candidate identities. `id` is the timing candidate, `graph` is the graph candidate. |

All three require empty payloads and are not recovery-safe. Graph stage/release
use normal-lane commands. Keep transport admission, generic execution and typed
`Reply::Failed` distinct; a delivered operation is not automatically domain success.

Before publication, the facade checks expected topology epoch and obtains the
exclusive generic graph permit. It validates that every prepared primitive's
playback and capture nodes remain active and are bound to their exact planner and
decision slots, with no extra/missing bindings. Generic graph validation alone
cannot prove that application-role relationship: a wrong decision slot can be a
valid generic descriptor and must still be refused here.

For the paired operation, timing activation is the final fallible step. It checks
exact logical IDs/kinds and stopped-program compatibility and retains its displaced
owner. Only after success does the code commit the already-preflighted graph permit
and update the facade's epoch. No ordinary graph failure remains after timing
publication. Failed checks preserve both Ready owners and the working configuration.
The generic permit still rejects stale validation and pending-injection conflicts.

## Boundary semantics, media and pending programs

These configuration operations execute during delivery, before musical preview.
A same-frame queued Stop does not make an already-running changed program stopped
for immediate topology replacement. Same-frame Play uses the successfully published
configuration when settlement occurs, regardless of whether its request was
registered before the configuration operation. Deferred State/Export queries expose
the post-settlement graph/topology cut, not the operation context's old entry epoch.
Zero callbacks do nothing; callback-end activation waits for the next nonempty call.

Routing can change while a take records. Keeping the same capture/playback consumers
preserves take identity, accumulated prefix and MIDI input tracking; samples from
that frame follow the new routes. It does not restart the take or replace old media.
Capture allocation/exhaustion handling remains the existing boundary preflight.
This is not a promise to roll back a published configuration if later capture/DSP
work fails. Faulted engines must be consumed/stopped, not replayed. Retaining the
physical consumers preserves their cursors/projections, not arbitrary downstream
synthesizer state: changing MIDI routing/effects supplies no automatic note or
plugin-state reconciliation. Application/device cleanup remains explicit; general
old-role cleanup is part of the still-pending role-generation transaction work.

The old planning-time graph-epoch recheck for pending ProgramActivate is now tested
against an actual intervening route change: the pending program fails at that cut,
its Ready owner returns after finish, no new zero-actions execute, and the old
program continues on the new signal routes. ProgramChanged reports the settlement's
new graph epoch and unchanged topology epoch. To retry, use current epochs; a graph
candidate validated before another graph activation must be released/restaged.

## Ownership, recovery and shutdown

Graph and topology retirement are separate: ReleaseGraph returns displaced graph
metadata; TopologyRecycle returns the timing bank to Idle. Unread external results
and retained retirement slots do not gate automatic clocks. Immediate graph
activation needs no extra retained loop-domain request slot; normal runtime admission
credits still apply. Recovery cancellation remains available with all normal domain
slots occupied. It cannot be used to force graph publication.

Ticket cancellation before dequeue prevents publication; cancellation is not undo.
Stale scopes/generations and invalid role bindings retain owners. Disconnect can
still allow already-queued work to execute; it does not invent Stop or rollback.
Stopped extraction destroys generic graph owners off-render and explicitly cleans
loop topology/media owners. A post-publication render fault preserves the published
epochs while permanent resolver quiescence recovers owners without fake finish.

## Concrete evidence and example work

Six guarded cases in `tests/runtime_topology.rs` verify:

1. Paired route/program configuration: old program refused while running, then Stop
   at6 and paired activation/restart at7; independent complete stereo samples, MIDI,
   every public State field and journals across partitions, reversal and journal
   capacities0/1/1024; coherent export and both retirement paths.
2. Continuous recording through a graph cut: two-frame prefix `[2,4]`, six-frame full
   audio `[2,4,12,16,4,8]` (right channel times10), complete captured MIDI byte stream,
   immutable exports and unchanged take revision across the route change.
3. Pending program delivered4, graph changed5, program fails5 before its old zero7;
   independent original-program audio/MIDI prove no phantom candidate actions.
4. Disabled/late preparation, generic-valid/application-invalid role bindings,
   epoch/scope/generation errors, payload/recovery rejection, ticket cancellation,
   returned candidate equality and successful subsequent paired reuse.
5. Sixteen occupied domain slots, unread results,130 one-sample intervals, independent
   cancellation, stale graph restaging and a second activation while retired owners
   and unresolved requests remain retained.
6. Disconnect before/after delivery and a processor fault after paired publication
   but before finish, with complete resource/reference recovery.

See `docs/loop_layer/evidence/live-graph-stage6{,-provenance}.json` for verification
status. Six Miri executions are registered for the consolidated stage run, not
independently interpreted. Generic hosts are unchanged historical regressions;
actual independent loop-host acceptance remains pending.

The existing `coordinated_loop` public example still verifies its original lifecycle,
not this new route-edit protocol. A future documentation example should separately
exercise compile → stage → Ready → paired activation → coherent query → both owning
retirements, with exact output assertions and guarded rendering. Do not present
integration-harness helpers as library APIs or claim that example already exists.
