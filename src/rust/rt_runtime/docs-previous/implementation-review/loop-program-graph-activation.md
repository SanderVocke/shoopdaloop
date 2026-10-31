# Apply: paired running-program and graph publication

Implementation parent: `b5b240d0`. This slice adds the previously pending combined
running-program/graph cut. It does not complete Stage 6 or final documentation.
Reviewed guides are `shoop_loop_engine/docs/{runtime,topology-activation,program-changes}.md`;
the first two still match the configuration slice's reviewed hashes. Record/recheck
`program-graph-baseline.json` before applying edits. Authored guides remain owned by
the separate documentation worker.

## Correct and extend the task-led guides

- Keep standalone `Resolver::stage_program` graph-independent. In program-changes.md,
  replace the statement that facade integration is fixed-graph only with links to
  both `ProgramActivate` and the new paired path. Do not attribute graph authority
  to the standalone resolver.
- In topology-activation.md, introduce a distinct paired running-program workflow
  alongside fixed-graph program activation, immediate graph/timing activation and
  stopped-owner configuration. The new path keeps logical identities, kinds, sync
  sources, triggers, other programs and physical consumer roles fixed. Only the
  selected program/nominal length and signal routing change. Logical owner-generation
  edits remain a separate, already-stopped configuration transaction.
- In runtime.md, extend command/results, boundary ordering, ownership, cancellation
  and fault sections. Replace the pending combined-running-program/graph limitation,
  not the still-pending broader lifecycle/host/final-verification work. Reconcile
  README, architecture and book chapters5–6,13–17,19; this slice does not certify a
  full review of those destinations or backfill earlier-stage omissions.

## Required runnable recipe

1. Compile and ingest a timing candidate with the existing bounded topology receiver.
   It may change one selected composite program only. Prepare a compatible graph
   with the existing control-side compiler/staging path, using exactly the current
   primitive playback/capture consumers and decision slots. Both candidates must
   be Ready at the expected graph/topology epochs.
2. Submit payload-free normal-lane
   `ProgramGraphActivate { id, target, graph, graph_epoch, topology_epoch }`.
   `id` identifies the timing bank; `graph` is the generic graph-stage identity.
   Admission rechecks the graph permit/role compatibility and reserves the usual
   normal domain request plus single resolver program owner. Failure before
   admission leaves both candidates unchanged. The topology becomes Taken while
   pending, but the graph remains Ready: **the graph stage is not locked**.
3. A running program waits for its actual zero entry (or stopped cut), using the
   existing unpublished probe. Idle zero does not qualify repeatedly. The selected
   candidate supplies real decisions without executing old zero-actions first.
   No public new probe authority is added: the resolver's crate-private selection
   accessor rejects absent/committed previews, and is informational only.
4. Recheck graph readiness/generation/epoch while pending. At the selected cut,
   borrow the exclusive graph activation permit before real capture preflight.
   Capture refusal drops that permit and re-previews the old program with the
   request-family refusal. Graph-preflight refusal follows the same family path.
   Required retries add a new latched failure; they are not a fixed subdivision quota.
5. Once the preflight succeeds, settle media and commit resolver state/topology,
   then consume the graph permit before completing replies/queries or rendering.
   The newly published graph processes sample/event F. Callback-end F waits for the
   next nonempty callback; zero-frame calls do not activate anything.
6. Read `Reply::ProgramGraphChanged { id, graph, graph_epoch, topology_epoch,
   completion }` as the reliable terminal outcome, not an admission acknowledgement.
   Requested/effective/delivered/actual frames remain distinct. Failed/cancelled
   outcomes report the actual working epochs; they do not claim activation.
7. After finish, the normal slot and internal program owner are collected without
   waiting for external result consumption. Success leaves separate displaced
   topology and graph owners, requiring `TopologyRecycle` and `release_graph`.
   Failure/cancellation restores the topology Ready; its graph remains whatever
   independent graph operations made it. An idle clock may keep a request pending
   indefinitely without blocking unrelated music.

Use `tests/support/program_graph_scenarios.rs` for an example with old playback at3,
request delivery4, activation7, and a delayed new child beginning8. Show exact stereo
output and MIDI cleanup/restoration, both epoch changes, immutable original export,
and separate retirement. Use `program_graph_capture.rs` for real FX-generated audio
and MIDI recorded at the cut, immediate playback, later recording, initial-state
metadata and whole-family capacity refusal. Do not copy the test harness wholesale.

## Failure/recovery details that must remain explicit

- Sequence cancellation and recovery-safe `TopologyCancel` cancel the retained
  program request through existing semantics. The latter recycles the rejected
  topology after finish; neither silently releases the independently staged graph.
  Ticket cancellation before dequeue publishes neither configuration. The paired
  activation itself is not recovery-safe and rejects unexpected payloads.
- Releasing/reusing the graph stage invalidates its old generation. Independently
  activating a graph changes the epoch and fails the pending pair; do not roll back
  that separate graph operation or retarget the pair to a reused slot. Such failures
  become terminal program `InvalidState`; immediate admission errors retain their
  runtime/domain classification. Bad/missing role bindings fail before staging the
  program owner. Generic runtime graph validation/retirement rules are unchanged.
- Real capture-family refusal preserves both old configurations and capture revision.
  Independently scheduled effect parameters still run: refusing a pair is not a
  rollback of unrelated controls. Do not use synthetic silence as capture evidence;
  the fixture checks actual6/60 stereo FX samples and original MIDI packets.
- A render/interface fault after publication cannot undo either epoch or authorize
  replay. Permanent stopped cleanup recovers the program owner after an unfinished
  interval, plus actual capture prefixes; graph owners are destroyed off-render.
  Disconnect alone invents no Stop and may still allow admitted publication.
- Waiting/status checks scan finite prepared metadata. Real selected-boundary work
  includes existing resolver/media checks. Graph commit is an infallible owner swap,
  not a claim that the whole transaction is constant-time or deadline guaranteed.

## Evidence and limitations

`program-graph-stage6{,-provenance}.json`:19 scoped gates,152 loop tests per native
profile and Node/Chromium,2140 workspace passes/four existing skips,8 positive
prototype executions and23 expected compile failures. Four new guarded facade
cases cover full independent audio/MIDI/state/journals across partitions/order and
journals0/1/1024; real capture versus family refusal; cancellation/stale graph reuse/
independent activation; full16 domain slots, unread outcomes and retained graph/
topology owners through128 clock cycles, recovery, disconnect and post-publication
fault cleanup. Existing probe tests also check private selection-query phases.

The initial capture fixture referred to Endpoint.port_type rather than its actual
kind field (E0609); the failed compiler log is retained in
`target/loop-layer-program-graph-stage6/`. No failed intermediate-tree reproduction,
workload reduction or interpreter pass is claimed. Miri inventory is now
**227 =128 runtime+99 loop**, registered only. Consolidated Stage6 Miri, broader
nested/mixed lifecycle coverage, independent actual loop hosts and final delivery
remain open. The separate worker must publish and verify the final guide changes.
