# Generic interval planning

Previous guide. The current [planning task](../docs/part-4-coordinate-processors-and-changes/13-planning-processing-intervals.md) demonstrates two guarded consumers; [typed operations](../docs/part-4-coordinate-processors-and-changes/14-application-specific-operations.md) demonstrates retained Pending receipts.

[Overview](../README.md) · [Host integration](integration.md) · [Scheduling](scheduling-midi.md)

`GraphRuntime<P, L = NoPlanner>` can own one prepared `Planner`. The default keeps ordinary `Processor` implementations usable without planning. A planner is not a signal node: it publishes immutable decisions before each signal traversal and consumes observations afterward.

## Prepare and bind

1. Prepare the planner's initialized, pre-touched decision storage and private state. Its `PlannerSpec` and decision-slice length must remain fixed. Associated decisions are `Copy`; observations are `Copy + Default`. Neither owns heap objects. Third-party implementations must still uphold the nonblocking/no-heap callback contract.
2. Implement `Processor<L>` for consumers. `planning()` declares whether a decision is required, the maximum observations produced per interval, and the reserved operation-payload signature. This metadata is queried only during preparation.
3. Wrap processors in `PreparedNode<P, L>` and call `GraphRuntime::prepare_with_planner`, or `prepare_with_planner_and_resources`. The runtime checks typed storage arithmetic and initializes/touches observation storage. Alternatively, `prepare_with_initial_graph(config, nodes, planner, resources, graph, bindings)` compiles the initial signal/planning graph off-render and returns epoch one at frame zero. It does not execute a warm-up callback, use ingress/result credits, or invoke the planner/processors during preparation. Invalid graphs consume/drop preparation inputs off-render. Later live commits retain normal validation, epochs and owning receipts.
4. Call `compile_planned` with explicit `DecisionBinding` records. Each consumer has at most one binding; several consumers can share a slot. Ordinary FX nodes need no binding. The planning prefix, generations, signatures, membership, aggregate observation reservations, and independent signal DAG are validated before activation.
5. Submit the plan through `Command<P, L>` and drain `Control<P, L>` completions. Replaced/rejected plans retain the ordinary owning-return contract.

The runnable [planned gain example](../examples/planned_gain.rs) checks 32 samples, five internal intervals, and an immediate typed processor gain operation at frame 13. It has one consumer and no heap guard of its own; it is not a multi-consumer or pending-receipt demonstration:

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

`PlanContext` and planner `OperationContext` include `graph_epoch`, the active
runtime graph identity, and `callback_start`, the enclosing nonempty host callback's
absolute start. `frame` still denotes the current internal/delivery boundary.
The callback origin remains unchanged across every internal interval and operation;
planners can use it to reset private background allowances once, not per interval.
These are runtime-provided identities, not a new musical clock. Manual context
construction in standalone prototypes supplies these fields and inert
`graphs: GraphAccess::disabled()` authority. Runtime contexts additionally lend
[staged graph activation](graph-activation.md) authority. A prepared permit lets a
planner finish domain preflight before an infallible graph/epoch swap; borrowed
views cannot survive that mutation. The copied `graph_epoch` is the context's
entry identity; `commit()` returns a newly published epoch. Recovery operation
contexts cannot activate. Planning-time removal of a node with an already-delivered
MIDI injection is refused before domain publication.

Zero-length host callbacks do nothing. A boundary at callback end settles in the next nonempty callback, not in `finish`. Invalid planner intervals or changed decision-slice length are explicit render errors before consumers run. As with processor errors, a propagated failure is not rollback: outputs are cleared and the outer clock has already advanced. Hosts must not replay the failed frame. A domain that already published state may require stopping callbacks and extracting its planner/store for cleanup rather than resuming an unfinished committed interval.

## Accounting and current limits

Ingress, event-credit recycling, resource accounting, validation, telemetry publication, and host MIDI recovery remain one host-callback lifecycle. Returning results or traversing many intervals does not refresh those allowances. `Budgets::subranges` is ignored, including zero. Positive frame progress bounds interval count by actual frames, not an arbitrary subdivision quota. Finite same-frame settlement is the planner's responsibility. Application-dependent expensive work may miss a host deadline.

Normal external event/result capacity is `min(scheduled_events, budgets.events)`. A configured planner also has one independent recovery receipt. Planner boundaries consume no scheduler slots. Diagnostic traversal counters saturate and set `Diagnostics::counter_overflow`; diagnostic exhaustion cannot refuse rendering.

Planner operation/plan/finish contexts expose `PlannerResourceIo`, which dereferences to restricted `ResourceIo` and adds `capture()` boundary authority. Its direct methods retain shared callback quotas; `render()` borrows required read/capture access counted independently of those quotas. Neither reborrowing nor internal intervals reset allowances or diagnostics. `ReadIndex` supplies store-scoped logarithmic byte positioning with separate index-work diagnostics. Opt-in MIDI checkpoint rows provide bounded event/state positioning; their work counters share the same callback envelope. [Capture groups](capture-groups.md) reserve take/index/final-lease credits, stage all members before publication, and commit coherent prefixes. The boundary capability checks runtime frame/ceiling; processors get staged writes but cannot acquire lifecycle authority. Generated ownership/content models and disconnect-at-lifecycle-cut tests cover the generic mechanisms. The separate `shoop_loop_engine` facade now integrates capture, media and supported prepared-template configuration/program-graph changes; those domain transactions are not implemented by generic `PlannerResourceIo` or graph permits. Typed operations and retained receipts are described below; delivery does not refresh any resource allowance.

## Typed operations and terminal receipts

A planner family defines associated `Operation` and `Result` types as compact `Copy + Send + 'static` plain data. Submit `Action::Planner { planner, operation }` or `Action::Processor { node, operation }` through `Command::Schedule`. Targets are distinct generational identities. `operation_bytes` bounds borrowed payloads per planner/consumer; payloads also must fit prepared scheduler stride and callback transfer headroom. A larger typed payload arena does not enlarge the MIDI packet limit.

`Processor::operation` returns an immediate typed result. `Planner::operation` receives an `OperationContext` with requested/delivery frames, a runtime-scoped `ReceiptToken`, borrowed payload/resources, and a restricted receipt writer. It returns `Disposition::Complete(result)` or `Disposition::Pending`. Pending delivery leaves the due queue but **does not release its result credit**. Retain only the token and prepared plain-data state, never the borrowed payload/context. Complete retained tokens with `context.receipts.complete(token, result)` during operation, planning, or finishing. Exactly one terminal write is accepted; stale, foreign-runtime, or duplicate tokens are rejected. The writer supplies its current context frame as the terminal timestamp; callers cannot backdate completion.

`Execution<Operation, Result>` separates:

- `requested_frame`: external scheduled timestamp;
- `delivered_frame: Option<Frame>`: actual delivery, or `None` for cancellation before delivery;
- `actual_frame`: terminal completion/cancellation frame;
- `result`: generic runtime delivery/receipt success or error;
- `operation_result`: the domain's terminal value, which may itself say applied, refused, cancelled, or superseded.

Generic `result == Ok(())` is **not** proof of domain application: inspect `operation_result`. `late_by()` measures delivery lateness, not time spent legitimately pending. Normal parameter/MIDI results keep `operation_result == None`. Both command completions and execution results must be drained; the former returns original owning payloads, while the latter releases receipt credit at a later callback boundary.

## Recovery and cancellation

Ticket cancellation retains its existing pre-dequeue cut. After delivery, a planner cancels pending domain work by completing its retained receipt with a domain cancellation outcome. `ReceiptWriter::cancel_queued(sequence, outcome)` can cancel a normal typed operation before delivery; ordinary parameter/MIDI automation retains its existing cancellation contract. Queued cancellation reports `delivered_frame == None` and prevents later delivery.

A recovery-safe planner action may use `Lane::Recovery`. `Planner::recovery_safe` must classify only payload-free, nonstructural cancellation/stop-like operations. Future timestamps, payload bytes, ordinary parameter/MIDI actions, and unsafe operations are refused on that lane. One separately prepared recovery receipt remains available even when every normal receipt is pending or unread. Its terminal result must be produced immediately or during planning at the delivery frame, **before any interval advances**; leaving it unresolved is an explicit interface fault. A domain must likewise reserve its own recovery state independently of normal pending storage.

Unread recovery results retain that one credit. At the next callback snapshot, eligible recovery can park a held normal validation command and execute within the configured operation/publication allowances. The original normal head then resumes before later normal requests with its validation progress, return reservation and original cancellation observation intact. Parking is not a fresh dequeue. Recovery cancellation also consumes allowances, and endless recovery can starve normal work. See [host integration](integration.md#graph-edits).

A pending domain receipt has a different lifetime from a held transport command: its owning command may already have completed. A pending operation's reference to a Ready graph neither owns nor locks it; revalidate at the actual cut. Per-lane monotonic admission sequences permit recovery before older normal traffic; transport still assigns globally unique submission sequences. Receipt scope identities are local to a Rust/Wasm instance, not transferable capabilities across separate Wasm instances. Host codecs must preserve instance/generation boundaries and encode domain operations explicitly, never Rust pointers or float parameter IDs.
