# Lower-level APIs

[Book](../README.md) · [Reference](README.md)

Use GraphRuntime unless the host intentionally owns the coordination obligations below. Standalone primitives are public building blocks, not a second hidden application engine or permission to bypass validation.

## Ownership transport

`transport::channel<T,R,S>` creates single-producer control and single-consumer render endpoints with separate normal/recovery credit ranges, reliable owning returns and Copy-only telemetry. Preparation and final endpoint destruction belong outside callbacks.

For each real host callback, the render owner calls `begin_cycle(frame, epoch)` once. It snapshots eligible ingress and resets work/publication counters. `next_operation()` lends at most one mutable request at a time. If a request cannot finish within the current allowance, retain it in transport and retain its validation/progress state keyed by sequence; another borrow of that request is not another dequeue/cancellation observation.

Call `complete(outcome, retired)` only when that current request really terminates. It returns ownership of `retired` in `CompleteError` on misuse/budget failure; preserve that payload rather than dropping it on-render. A completion can return a prepared retirement bundle. Filling telemetry cannot steal reliable-return space. Do not free a refused/retired bundle or manufacture an outcome just to empty a queue.

Eligible recovery can park the held normal owner inline; after recovery, that head resumes before later normal traffic. Operation/publication limits still apply, and normal fairness is not unconditional. `set_epoch` updates acknowledgement metadata only; it does not reset a cycle. `publish` is bounded/lossy Copy telemetry, not ownership transfer.

`prepare_submissions` prepares reusable flags only with no outstanding commands. `submit_prepared` scans its fixed lane flag range; Full admission and Busy held flag owners differ. Every SubmitError retains the original value. Tickets are pre-dequeue cancellation flags, not general scheduler cancellation. See [prepared ingress](../part-5-configure-and-operate-the-runtime/18-submitting-without-allocation.md).

## Descriptor construction is not certification

`compile`/`compile_planned` are off-render convenience compilers against a catalog. A catalog describes prepared physical identities, signatures and optional planning access; a plan separately describes active signal order and bindings.

`PreparedPlan::from_descriptor`/`from_planned_descriptor` allocate/check storage envelopes. They do not certify an arbitrary incoming descriptor for execution. A compact wrapper may have no spare row capacity for future pushes.

`PreparedPlan::with_capacity(config)` prepares reusable inactive storage. Exclusively owned inactive plans support `begin_descriptor`, bounded `push_step`, `push_connection`, `push_binding` and `set_planning_step`. Capacity refusal leaves the owner available for correction/recycling; these operations do not resize vectors. Stamped membership avoids clearing every slot on each begin; exhaustion does not wrap.

For manual validation, call `PlanValidator::begin()` for a new candidate, then `advance(candidate, catalog, allowance)` incrementally. Charge its reported steps against the enclosing callback allowance; do not restart it every callback or refresh the allowance per interval. Preserve candidate/catalog/epoch while it is Pending and inspect terminal Valid/Invalid. A bad input range, stale generation, cycle or binding must never execute merely because its sender called it prepared.

Actual retained capacities matter separately from validation. `storage_bytes` includes spare vector capacity; staged admission and displaced-owner retirement have additional checks. A borrowed metadata view is not an inactive owner or permission to mutate it. Runtime staging is opt-in once before the first nonempty callback; disabled standalone graph access cannot create runtime authority.

## Scheduler and receipts

Standalone schedulers retain their own prepared event/payload/result stores. The caller must validate targets/envelopes against the appropriate catalog and maintain the same callback-boundary recycling, stable delivery order and one-terminal-result obligations as GraphRuntime.

Immediate parameter/MIDI completion differs from typed delivery receipts. A typed token's scope/sequence/state is checked on terminal completion; Pending application data must retain the token until resolved or explicitly abandoned after stopping. Queued typed cancellation is not ticket cancellation. Reading a result sends recycle credit for the next callback; it does not replenish the current callback through an internal interval.

Recovery execution capacity is explicitly prepared; it is not an unlimited side channel. Safe classification, current delivery-frame termination and structural restrictions remain caller/domain contracts. [Execution rules](execution-rules.md) define frames, sort keys and cancellation cuts.

## ResourceStore and lent authority

A standalone ResourceStore owner calls `begin_callback()` once per real host callback and uses bounded `reclaim()` with remaining validation work. Do not reset accounting per command, node or internal interval. Preparation, structural owner setup and final destruction stay outside rendering.

`ResourceIo` lends resource metadata, lease and legacy optional access. `render()` lends required data operations with independent counters; it cannot replenish budgets, reclaim or manufacture structural authority. Opaque indexes/cursors/events validate store scope and lease lifetime; raw IDs still require same-store discipline.

`PlannerResourceIo::capture()` adds frame/ceiling-stamped group lifecycle authority. Positive reservation must fit that planning window; activation/freeze match the current stamp and commit matches the reserved end. Ordinary consumers can stage complete data but cannot reserve/publish the shared group. Direct `store.planner_io(frame, ceiling)` hosts own those stamp/time guarantees; copying arbitrary numbers into a prototype context does not make it a runtime callback.

`GraphAccess::disabled()` similarly supports standalone prototypes without runtime publication rights. Borrowed graph/resource/decision/observation views cannot escape their permitted scope or survive the mutation they constrain.

## RenderRuntime is only the clock/host-block layer

`RenderRuntime::new` retains prepared limits and starts at frame zero. Its `process(AudioBlock)` validates nonempty shapes/expected frame, clears bounded output and advances the empty-graph clock. Zero calls do nothing. It has no processor DAG, transport, scheduler, resource store or planner hidden inside it.

GraphRuntime composes those layers and advances the validated clock before graph processing. A later error can leave real time/state/publication advanced despite cleared output. Neither layer supplies rollback, fake finish or universal fault continuation.

Stopped extraction preserves only the owners explicitly returned. `into_resources` drops planner/other owners; `into_planner_and_resources` additionally keeps its cleanup ledger. Both require stopped callbacks and settle no requests/device state. See [shutdown](../part-5-configure-and-operate-the-runtime/19-stopping-disconnecting-and-reclaiming-ownership.md).
