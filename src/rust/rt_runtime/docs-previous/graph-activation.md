# Staged graphs and coordinated boundary activation

Previous guide. Start with the current [staged graph task](../docs/part-4-coordinate-processors-and-changes/15-staging-and-coordinating-graph-activation.md) and [execution reference](../docs/reference/execution-rules.md).

The opt-in graph-bank API separates **validated ownership** from **permission to
publish**. A planner can preflight its domain against a candidate graph, then
publish domain state and swap graph owners at one operation/planning boundary.
The final graph swap is infallible; dropping an unused permit publishes nothing.
This is not arbitrary domain rollback, processor construction or a resource bundle.
The existing `Command::Commit` and `NoPlanner` lifecycle remain available unchanged.

## Preparation and owning admission

Call `GraphRuntime::prepare_graph_staging(candidate_count)` once, off-render,
before advancing the render clock. Zero count is invalid; existing constructors
leave staging disabled. The method allocates/pre-touches fixed owner slots. It does
not allocate plan descriptors on behalf of a producer. Prepare a `PreparedPlan`
off-render; for reusable materialization storage use `PreparedPlan::with_capacity`.
Compilation is not callback work. Returned owners keep their own vector capacities;
staging never enlarges them and does not promise all returned plans are full-capacity.

Submit `Command::stage_graph(slot, expected_epoch, plan)` on the normal lane.
The generic receiver validates it against the live catalog using the same
incremental validator and callback-wide validation allowance as legacy commits.
The old graph continues rendering. Partial validation does not publish readiness,
change epochs or expose a candidate view. A stale epoch, busy slot, malformed
plan or insufficient storage budget returns the original owner in the command's
`plan: Some(...)` with a rejected owning completion.

On successful admission the owning completion has `Outcome::Applied` and:

```text
Command::StageGraph { slot, expected_epoch, plan: None, id: Some(stage_id) }
```

This means **Ready**, not active. `StageId` contains scope, slot and generation;
its three u32 parts are plain IDs, never pointers. Generations and scopes never
wrap. Rejected/cancelled admission does not issue an ID. Pre-dequeue ticket
cancellation returns the original command/owner. These process/instance-local IDs
are not host authentication or a cross-instance routing protocol.

Normal-lane `Command::ReleaseGraph { id }` releases either a Ready candidate or a
Taken retirement slot. An owner, if present, returns as `Retired::Plan` through the
reserved owning command completion. Releasing an initially empty active graph's
retirement slot can legitimately return no plan. Release ends that stage identity;
stale/double releases fail. Reuse increments the generation. Ready and Taken slots
cannot be overwritten. Control disappearance does not implicitly activate or
free them; final runtime destruction/extraction disposes remaining owners off-render.

## Scoped activation authority

Planner `OperationContext` and `PlanContext` contain `graphs: GraphAccess`.
It provides borrowed `active()` / `candidate(id)` views and `status(id)`:

- `Ready { validated_epoch }`: the complete plan was validated at this epoch;
- `Taken`: activation has consumed the candidate and reserved the displaced owner.

A Ready plan is not automatically current after another graph/catalog edit.
`prepare_activation(id, expected_epoch)` rechecks the stage identity, readiness,
current epoch, validation epoch, next-epoch availability, displaced-owner storage
budget and pending MIDI compatibility. Even a catalog-preserving intervening
commit makes older validation stale. Release and restage the owner to revalidate it.
Recovery operation contexts cannot obtain an activation permit. Stage/release
commands are also normal-lane only; trusted planner recovery classification still
has to obey its documented semantic contract.

A successful call returns an exclusive `GraphActivation` borrowing runtime state.
It exposes `current()`, `candidate()` and `next_epoch()`. Domain validation and
reservation may use these immutable views. Do **all fallible work before publishing
any domain state**. The intended order is:

```text
permit = context.graphs.prepare_activation(stage_id, expected_epoch)?
prepared_domain_cut = validate_and_reserve_domain(permit.current(), permit.candidate())?
publish_preflighted_domain_infallibly(prepared_domain_cut, permit.next_epoch())
epoch = permit.commit()
```

This is an ordering sketch, not a generic domain implementation. A complete
compile/run example using the public production API is in
[the lifetime prototype](../../../../docs/loop_layer/prototypes/graph_activation_lifetime.rs).

`commit(self)` swaps the candidate into the active slot, keeps the displaced owner
in the stage slot, marks Taken and advances the epoch. It does not allocate, free,
scan or validate. There is no ordinary graph failure after domain publication.
An unused permit has no rollback/destructor action because it changed nothing.
The exclusive borrow prevents another graph mutation between preflight and commit;
views cannot survive that mutation or escape the callback. `GraphAccess::disabled()`
is inert, for standalone planner contexts; it cannot manufacture authority.

A planner-operation activation happens during delivery; an activation in `plan`
happens after due external actions and before affected rendering. The selected
plan supplies the new signal routes and decision bindings for the interval.
Later operation contexts use the new epoch. The context's copied `graph_epoch`
is its entry identity, while the permit's return is the newly published identity.
Diagnostics report the final published epoch even if a later render step faults.
Zero-frame callbacks do not admit, activate or settle a boundary. An activation at a callback's end belongs to the next nonempty callback; `finish` does not settle that next boundary.

## Same-frame MIDI compatibility

Scheduled planner operations precede processor/MIDI actions under the existing
scheduler ordering, independent of command submission order. Planning occurs
after those deliveries. If a MIDI injection has already been delivered to a node
at this boundary, activation cannot remove that node before it consumes the
packet: preflight returns `Busy`, leaving both owners and the domain unchanged.
The old graph then renders that frame. A later request can use the still-Ready
candidate once the input is consumed. This is an explicit conflict refusal, not
silent packet dropping or delayed replay from a retained input buffer.

Conversely, an operation-time activation that already removed a MIDI target makes
the later injection fail `InvalidId`. Nodes that remain active may consume pending
input through the newly selected graph. The compatibility check visits prepared
node/input-port storage and uses indexed membership; it does not chase producers.
It does not perform synth/note cleanup for removed consumers. A musical domain
must preflight and arrange its own role cleanup/resource lifecycle.

## Recovery while validation is unfinished

A held normal command no longer hides the next callback's eligible recovery
commands. The transport parks that owner in one fixed inline slot, services the
snapshotted recovery lane, and resumes the original normal head before later
normal requests. Parking creates no admission credit or owning payload. Existing
operation/publication budgets still apply, including cancelled recovery requests.
Resumption is not another dequeue: it does not recheck a cancellation flag already
observed before partial work. Unrelated recovery completions preserve the normal
validator's sequence/progress. No producer-chasing retry or allocation is added.

A regression test reproduces the previous obstruction and verifies recovery every
callback while both staged and legacy graph validation advance one step at a time.
Separate transport cases check one/two publication credits, cancelled recovery,
late cancellation of the parked normal request, FIFO order, unread owning returns
and disconnected shutdown with both current and parked owners. These guarantee
priority and ownership under the configured allowances, not fairness for normal
work if recovery consumes every available operation/publication slot forever.

## Storage and work accounting

`PreparedPlan::storage_bytes()` includes its descriptor object and every vector's
**capacity**, not merely populated rows; allocator overhead is excluded. Graph-bank
preparation computes a conservative full-config plan allowance from node steps,
connections, membership and decision bindings. It checks all candidate slots plus
one compatible active owner and slot descriptors against i32::MAX with checked
arithmetic. This conservatively counts inline descriptors more than once; it is
not a measured heap footprint. Pending command owners, compiler temporaries,
validator scratch and the rest of the runtime have separate preparation costs.

Staging rejects overallocated metadata exceeding the per-owner allowance even if
its row counts are valid. Activation likewise rejects an oversized displaced plan
introduced through a legacy path; ownership remains unchanged. Background staging
validation consumes the callback allowance once, not once per internal interval.
Activation preflight has finite prepared-size MIDI compatibility work; ownership
commit is a constant number of moves. No universal callback deadline is claimed.

## Application integration

The separate `shoop_loop_engine` facade uses these permits for fixed-role live routing, paired graph/timing publication, stopped-owner configuration changes and selected running-program/graph publication at an eligible zero/stopped cut. See its [activation guide](../../shoop_loop_engine/docs/topology-activation.md) for current configuration and paired program/graph usage, including their distinct stopped/zero conditions and independent retirements.

These interfaces have different preconditions. Logical owner changes require already-stopped edited owners and idle capture claims; they select prepared physical templates, not newly constructed processors or changed port shapes. The running-program path preserves logical/physical roles while changing one program and routes. A pending application request does not lock its Ready graph: release/reuse or independent graph activation can invalidate it, requiring a new check at the actual cut.

Graph, topology, role and media owners retire through their own APIs. Generic graph IDs are not the loop timing receiver's IDs, and a graph permit does not perform media transactions or note cleanup. A later render fault cannot roll back published epochs; stopped domain cleanup must use its actual committed prefix rather than inventing `finish` or replaying samples.

## Historical verification and current status

At the generic graph-permit milestone, seven guarded integration cases covered exact audio/MIDI and domain/epoch cuts across
partitions and declaration orders; operation-time and deferred planning activation;
callback-end/zero calls; domain refusal; stale/busy/scoped/generation rejection;
actual-capacity limits; incremental validation and cancellation; saturated normal
results and disconnect; physical catalog generation changes; same-frame MIDI
removal/no replay; and 128 owning exchanges preserving the NoPlanner legacy path.
A guarded transport case checks recovery preemption, and a private edge case checks
scope/generation/epoch exhaustion and aggregate sizing.

See [graph-activation-stage6.json](../../../../docs/loop_layer/evidence/graph-activation-stage6.json)
and its provenance for native188runtime/131loop, Node/Chromium184runtime/131loop,
workspace2119/four existing skips,25 broad gates, lifetime/authority proofs and
unchanged generic native/actual-worklet regressions/canaries. Nine new selected
Miri executions were registered but not yet interpreted at that milestone. The
[collected Stage 6 verification](../../../../docs/history/pre-consolidation-fffc1faa/docs/loop_layer/MIRI_JOBS.md#collected-stage-6-pass)
records completed consolidated Stage 6 Miri verification for the later implementation;
the counts above retain their original scope. Supported prepared-template loop
transactions are now implemented, but independent loop-host execution, later
benchmark/audit work and final implementation acceptance retain separate evidence obligations, not documentation handoff dependencies.
