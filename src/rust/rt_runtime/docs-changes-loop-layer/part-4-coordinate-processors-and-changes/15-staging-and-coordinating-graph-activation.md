# Change instructions — chapter 15: staged graph activation

Destination: `docs/part-4-coordinate-processors-and-changes/15-staging-and-coordinating-graph-activation.md`.
Baseline: empty chapter; archived `graph-activation.md` provides most of the generic
prerequisite's contract, not a ready-to-copy tutorial. Exact input hashes are in
`../baseline.json` relative to this note's parent directory. Apply these facts in
the approved chapter 7 style, using a small public-API example, not the integration
harness as teaching infrastructure.

## Add: prepare and admit an owner, without activating it

1. Keep the initial graph running. Off-render, opt into fixed stage slots with
   `GraphRuntime::prepare_graph_staging(candidate_count)`. This is a once-only
   preparation operation before the render clock advances; zero slots is invalid.
   Existing constructors leave this facility disabled. The slots reserve owning
   storage; they do not compile graphs or allocate a future descriptor's vectors.
2. Prepare a `PreparedPlan` off-render and submit normal-lane
   `Command::stage_graph(slot, expected_epoch, plan)`. Distinguish the expected
   **runtime graph epoch** from an application/domain version. Validation uses the
   existing incremental validator and callback-wide allowance while the old graph
   renders; a partial descriptor cannot be observed as Ready.
3. Inspect the owning completion. Success is `Outcome::Applied` with
   `Command::StageGraph { plan: None, id: Some(stage_id), .. }`. It means the owner
   was staged, **not** that the signal graph changed. Rejection/cancellation returns
   the original owner in `plan: Some(...)`; dispose/reuse it off-render.
4. `StageId` is a checked scope/slot/generation identity, not an owner, pointer,
   authentication token or cross-Wasm-instance protocol. No scope/generation wrap.
   `StageStatus::Ready { validated_epoch }` records the validation epoch;
   `StageStatus::Taken` means the slot is reserved for the activation's retirement.

Do not imply that this API materializes a graph from wire bytes. `PreparedPlan`
materialization/validation, a domain wire receiver and application transaction
preflight are separate responsibilities. `StageId` is also unrelated to the loop
library's topology receiver StageId.

## Add: preflight before publishing either half of a coordinated cut

`PlanContext` and planner `OperationContext` lend `graphs: GraphAccess`.
`active()` and `candidate(id)` expose borrowed metadata. A call to
`prepare_activation(id, expected_epoch)` checks:

- authority (recovery operations cannot activate), scope/generation and Ready state;
- current graph epoch equals expected, and candidate validation was at that epoch;
- the next epoch can be represented without wrapping;
- the displaced active plan fits the stage's actual-capacity storage allowance;
- no pending injected MIDI target would be removed before consuming its packet.

A successful `GraphActivation` exclusively borrows the graph state. It supplies
`current()`, `candidate()` and `next_epoch()` for application preflight. Finish
**all fallible application work** before publishing any application state. Then
publish the preflighted application state and consume `permit.commit()`, which
infallibly swaps graph ownership/epoch and retains the displaced graph in the slot.
The commit does not allocate, reclaim, revalidate or scan the graph. An unused
permit changes nothing; dropping it does not roll back arbitrary application edits.

Do not label this a general processor/resource transaction or automatic loop role
migration. If application publication still has an ordinary failure path, the
example has not established the required coordinated cut. Processor construction,
media claims and MIDI/note cleanup remain application responsibilities.

## Explain delivery and planning boundaries precisely

- Operation-time activation happens during scheduled operation delivery; planning-
  time activation happens after due actions, before rendering the selected interval.
- The runtime uses the new routes and validated decision bindings for that interval.
  The context's copied `graph_epoch` is its entry identity; `commit()` returns the
  new identity. Later contexts and final diagnostics observe the published epoch.
- A zero-frame callback does nothing. A boundary at the host callback's end settles
  with the next nonempty callback, not one sample late or in a hidden warm-up call.
- Existing scheduler ordering places planner operations before processor/MIDI
  actions at the same frame. Submission sequence alone does not override action
  categories. For an already-delivered MIDI conflict, demonstrate deferred
  planning-time activation, not two operation submissions in the opposite order.
- If an injection is already buffered for a node that the candidate removes,
  preflight returns `Busy`; both configurations remain unpublished/unchanged and
  the old graph consumes that frame's packet. A later request can retry the still-
  Ready owner. Do not silently drop that packet or replay it when the node returns.
  Conversely, operation-time removal makes a later injection fail `InvalidId`.

## Add: release and stale-owner recovery

Use normal-lane `Command::ReleaseGraph { id }` to return a Ready candidate or a
Taken retired plan as `Retired::Plan`. Initial activation with no previous active
plan can legitimately release a Taken slot without a plan. Release invalidates
that identity; reuse increments the generation. Ready/Taken slots cannot be
silently overwritten. Wrong, stale or foreign identities preserve ownership.

A later graph/catalog edit invalidates earlier validation, even if the old
candidate still appears structurally compatible. Release and restage to validate
at the new epoch. Show one stale/refused preflight before the successful swap;
assert the old output and application state remain intact. Drain owning returns
outside guarded render scopes and leave no staged/retired owner hidden at shutdown.
Disconnect alone is not an activation, graph rollback or owner disposal operation.

## Example and verification requirements

Use a small synthetic generic domain (for example a mode/gain plus two routes), not
Shoop loop modes. The example should:

- prepare a candidate and obtain its StageId through a real owning completion;
- refuse a domain preflight with old output intact, then activate at a known frame;
- assert the exact output before/after that frame and matching domain/graph epochs;
- prove old/new decision bindings match the chosen routes, and explicitly release
  the retired owner; assert callback safety with the appropriate scoped guard;
- run whole and split callback partitions for the same absolute cut, including a
  zero callback at the boundary; do not hide a warm-up render;
- keep the example's driving helper visibly separate from the public runtime API.

Production authority/lifetime prototype:
`docs/loop_layer/prototypes/graph_activation_lifetime.rs` (repository-relative).
It compiles/runs as a positive case and rejects held active/candidate views,
consuming a borrowed permit, escaped authority and private construction. It is a
verification prototype, **not yet a complete public book example**.

Source anchors: `src/graph/activation.rs`, `src/executor.rs` graph-stage preparation,
`Command::StageGraph`/`ReleaseGraph`, `PlanContext`/`OperationContext`, and
`PreparedPlan::storage_bytes`. Behavior tests: `tests/graph_activation.rs`, including
`graph_and_domain_publish_at_one_internal_cut_across_partitions_and_orders`,
`refused_permits_stale_catalog_epochs_and_stage_generations_preserve_both_owners`,
`physical_catalog_generation_changes_invalidate_staged_plans_without_losing_returns`,
and `same_frame_injected_midi_prevents_removal_until_delivery_and_never_replays_on_return`.

Use `docs/loop_layer/evidence/graph-activation-stage6{,-provenance}.json` for actual
verification status. Generic host regressions are not a loop AudioWorklet, and
registered Miri cases are not interpreter passes. Do not transfer milestone totals
into the chapter. Revisit this note after general loop graph/media/role transactions
are implemented; that application integration is not supplied by the permit alone.
