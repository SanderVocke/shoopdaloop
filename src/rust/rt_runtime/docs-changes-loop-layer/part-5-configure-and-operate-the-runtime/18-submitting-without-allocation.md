# Chapter 18: prepared flags, payload recycling and inactive plan storage

Destination currently empty. `../operation-baseline.json` records source/archive
review and current checks. Teach one small guarded ingress/recycling task with
explicit ownership and observable refusal; ordinary `submit` remains the simpler
control-thread API. This is a chapter/example specification, not new authored prose.

## Example to implement from public APIs

1. Prepare the runtime/control endpoint and owning payload buffers outside render.
   Call `prepare_submissions()` with no outstanding commands. Calling it with
   outstanding work is Busy; `submit_prepared` without preparation is InvalidState.
   It prepares reusable cancellation flags, not arbitrary future command payloads.
2. Submit a preallocated resource Write or similarly owning command using
   `submit_prepared`, preserving the original value on every SubmitError. Render
   bounded callbacks and collect its reliable completion. Extract/reuse the same
   Box buffer from the returned request. Assert pointer/capacity identity, exact
   written bytes and no heap operations in the chosen submit/process/recycle scope.
   Construction, unrelated destruction and final teardown remain outside that scope.
3. Use one normal slot to distinguish failure causes. While its command/completion
   owns admission credit, another normal submit is Full. After receiving completion,
   deliberately retain its flag owner (ticket or completion) and assert Busy despite
   the now-free admission credit. Recovery uses its own flag range/credit and cannot
   be borrowed to satisfy normal submission. Release finished handles and retry only
   at an appropriate later opportunity, not in an unbounded callback polling loop.
4. Demonstrate pre-dequeue ticket cancellation versus late cancellation after
   dequeue. An old ticket cannot affect a subsequently reused flag: reuse requires
   only the endpoint's base flag owner to remain. Guard repeated reuse with fixed
   prepared buffers and check all credits/owners return at shutdown.

The existing `tests/prepared_ingress.rs` gives direct public-API patterns:1000 flag
reuse iterations,64 owning-buffer writes, and100 descriptor-materialization cycles.
Do not replace its full regression workloads with a short documentation smoke. The
new book example can remain small and independently executable, with its own source
excerpt markers and assertions.

## Reusable graph materialization as a focused extension

Use `PreparedPlan::with_capacity(config)` off-render to allocate/touch descriptor,
membership and planning-binding storage. While exclusively owning an **inactive**
plan, use `begin_descriptor`, bounded `push_step`/`push_connection`/`push_binding`
and `set_planning_step` calls. Stamped membership makes begin_descriptor constant
work without clearing every node slot; counter exhaustion is explicit, not wrap.
Check capacity refusal leaves ownership available for correction/recycling.

Materialization is **not validation**. Feed a descriptor with a deliberately bad
input_end or stale node identity through incremental PlanValidator (or the runtime's
normal validated commit/staging path). Assert it is rejected and current output is
unchanged, while the candidate returns with retained capacity. Hold the catalog and
candidate unchanged during validation. Storage wrappers from_descriptor/
from_planned_descriptor allocate/check envelopes and do not certify execution safety;
a compact descriptor may have no spare capacity for later pushes.

Normal commit, staged Ready activation and Taken/displaced owner return retain their
separate contracts. Only mutate a returned exclusively owned inactive plan; a metadata
view is not ownership permission. Actual retained vector capacity matters to graph-
stage admission. Never grow/compile/destroy an arbitrary plan in the bounded ingress
scope, or bypass validation because a compiler or remote sender called it prepared.

## Guarantees and limits to state

- submit_prepared scans at most that lane's fixed flag count. It is not a retry loop
  chasing another producer; the control endpoint still has a single producer.
- Payload/refusal/retired objects retain their own allocation/destructor obligations.
  A generic completion or ticket drop is not automatically RT-safe merely because
  submission was prepared. Only a demonstrated ownership-preserving recycling scope
  can avoid last-owner destruction; endpoint/final payload destruction stays off RT.
- Read both command completion and later scheduled execution results where applicable.
  Result credit and prepared flag availability are independent; admission is not
  application or domain success. Retain/refuse future timestamps explicitly.
- Recovery remains finite, classified and payload-restricted in GraphRuntime. Its
  ability to park held normal validation does not remove publication/work budgets,
  allow structural recovery activation or promise unlimited normal-lane fairness.
- Reusing local Rust storage is not a cross-instance wire protocol. Independent Wasm
  hosts need checked encoding, bounded ingress, local ownership and generation maps;
  no pointer/Vec/trait-object transfer, shared-memory assumption or MessagePort bypass.

## Reviewed evidence, not publication

All188 rt_runtime tests passed in native debug/release in this review, including the
three prepared-ingress cases and transport/graph/typed-lifecycle regressions. The
bounded_runtime example also ran its prescribed1024-iteration documentation smoke
in both profiles, with six deliberate heap-canary failures correctly detected.
Those runs do not implement this new chapter example or certify arbitrary payload
code, JavaScript callbacks, Miri or independent loop-host behavior. Preserve the
current example/guard inventory and have the separate worker verify published prose,
new examples, source excerpts and ownership cleanup before final acceptance.
