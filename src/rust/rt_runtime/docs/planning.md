# Generic interval planning

[Overview](../README.md) · [Host integration](integration.md) · [Scheduling](scheduling-midi.md)

`GraphRuntime<P, L = NoPlanner>` can own one prepared `Planner`. The default keeps ordinary `Processor` implementations usable without planning. A planner is not a signal node: it publishes immutable decisions before each signal traversal and consumes observations afterward.

## Prepare and bind

1. Prepare the planner's initialized, pre-touched decision storage and private state. Its `PlannerSpec` and decision-slice length must remain fixed. Associated decisions are `Copy`; observations are `Copy + Default`. Neither owns heap objects. Third-party implementations must still uphold the nonblocking/no-heap callback contract.
2. Implement `Processor<L>` for consumers. `planning()` declares whether a decision is required, the maximum observations produced per interval, and the reserved operation-payload signature. This metadata is queried only during preparation.
3. Wrap processors in `PreparedNode<P, L>` and call `GraphRuntime::prepare_with_planner`, or `prepare_with_planner_and_resources`. The runtime checks typed storage arithmetic and initializes/touches observation storage.
4. Call `compile_planned` with explicit `DecisionBinding` records. Each consumer has at most one binding; several consumers can share a slot. Ordinary FX nodes need no binding. The planning prefix, generations, signatures, membership, aggregate observation reservations, and independent signal DAG are validated before activation.
5. Submit the plan through `Command<P, L>` and drain `Control<P, L>` completions. Replaced/rejected plans retain the ordinary owning-return contract.

The runnable [planned gain example](../examples/planned_gain.rs) checks 32 samples, five internal intervals, and a parameter change at frame 13:

```sh
cargo run -p rt_runtime --example planned_gain
cargo run -p rt_runtime --release --example planned_gain
```

## Interval lifecycle

- `Planner::plan(PlanContext)` settles the current frame and returns `frame < end <= ceiling`. The ceiling is the earlier of the next admitted external action and host end. Due external actions execute before planning at that frame.
- `ProcessBlock<L>::decision` lends only that node's validated slot, tagged with planner identity and interval. Its immutable borrow ends before planner mutation. Renderers do not receive mutable access to planner state.
- `ProcessBlock<L>::observations` stamps the producer identity and enforces that producer's reservation. `push` returns `Capacity` without changing earlier values when its allowance is exhausted. Unused reservations do not let another producer exceed its own allowance.
- `Planner::finish(FinishContext)` receives only values written in this interval and advances through the interval; it must not settle its end boundary. The runtime clears observation length before the next traversal, without clearing or freeing the backing allocation.
- A configured planner advances even with no active signal graph; in that case it receives no consumer observations. A `None` preparation signature selects the signal-only lifecycle and requires empty decisions and ordinary consumer signatures.

Zero-length host callbacks do nothing. Invalid planner intervals or changed decision-slice length are explicit render errors before consumers run. As with processor errors, a propagated failure is not rollback: outputs are cleared and the outer clock has already advanced. Hosts must not replay the failed frame.

## Accounting and current limits

Ingress, event-credit recycling, resource accounting, validation, telemetry publication, and host MIDI recovery remain one host-callback lifecycle. Returning results or traversing many intervals does not refresh those allowances. `Budgets::subranges` is ignored, including zero. Positive frame progress bounds interval count by actual frames, not an arbitrary subdivision quota. Finite same-frame settlement is the planner's responsibility. Application-dependent expensive work may miss a host deadline.

External event/result capacity is `min(scheduled_events, budgets.events)`. Planner boundaries consume no scheduler slots. Diagnostic traversal counters saturate and set `Diagnostics::counter_overflow`; diagnostic exhaustion cannot refuse rendering.

`PlanContext` and `FinishContext` currently expose the existing restricted `ResourceIo`, with its shared callback quotas. Indexed required-render access and transactional capture are not implemented yet. Typed domain operations and pending runtime terminal receipts are also still pending: `operation_bytes` currently participates in signature validation but does not introduce an operation-delivery API. Do not encode domain requests in float parameters as a substitute.
