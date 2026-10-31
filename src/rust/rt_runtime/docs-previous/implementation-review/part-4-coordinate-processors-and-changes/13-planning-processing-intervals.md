# Change instructions — chapter 13: graph authority in planner contexts

Destination: the matching chapter under `docs/`. Baseline: empty; refer to archived
`planning.md` and `graph-activation.md`. The backfill below completes the Stage3
lifecycle handoff alongside the newly extended context; authored publication remains
separate.

Describe `PlanContext` and planner `OperationContext` fields `graph_epoch`,
`callback_start` and `graphs`. The frame is the current internal/delivery boundary;
callback_start remains the enclosing nonempty callback's start across intervals.
It can key private callback-wide allowances, not introduce another musical clock.
The copied graph_epoch identifies context entry; a graph permit's `commit()`
returns the newly published epoch. Later contexts observe that identity.

GraphAccess is scoped authority, not an owned runtime reference. Immutable active/
candidate views cannot survive mutation or escape the callback. For standalone
prototype context construction, supply inert `GraphAccess::disabled()`; it cannot
fabricate production graph authority. Runtime-created contexts lend real authority.
Keep the basic planner example focused; link chapter 15 for prepare/commit ordering.

A planner's plan-time activation selects the interval's routes and decision bindings
after external due actions. The planner must complete fallible preflight before
publishing its domain state and consuming the infallible graph swap. Failed
preflight preserves old state; later rendering failure is not rollback. Recovery
operation contexts cannot activate. NoPlanner remains the ordinary default path.

Source: `src/planner.rs`, `src/executor.rs`, `src/graph/activation.rs`. Production
prototypes `runtime_view_lifetime.rs` and `graph_activation_lifetime.rs` under
repository `docs/loop_layer/prototypes/` verify decision/observation and graph-view
borrow boundaries. `examples/planned_gain.rs` demonstrates the basic planner, not
the complete new staged-graph protocol. Preserve that distinction in example claims.

## Full planner walkthrough backfill

Start with planned_gain.rs: one decision alternates gain every8frames, a typed
processor operation changes the multiplier at13, and a32frame callback produces
intervals [0,8), [8,13), [13,16), [16,24), [24,32). Assert every output sample and
the observation's frame count, not just interval count. Its events=1/subranges=0
configuration demonstrates that internal intervals do not consume event receipts
or a legacy subdivision allowance. Both debug/release executions are refreshed in
../control-baseline.json; the example currently has one consumer and NO heap guard.
Do not label that run a guarded two-consumer demonstration. The book worker should
extend/add a small public example with two consumers and the scoped heap guard,
while retaining this useful exact-frame walkthrough.

One render-owned planner is the authority; each consumer declares ConsumerAccess
and obtains its immutable decision through a validated DecisionBinding. compile_planned
checks identity, slot bounds and required bindings against prepared catalog metadata.
Consumers do not query a mutable global planner or settle musical state. Planning
relations differ from signal routes: a binding does not add an audio/MIDI edge.
Explain missing/stale binding refusal where compiling the example, not in an appendix.

For each positive interval: deliver due external actions, plan within the supplied
ceiling, borrow decisions for graph processing, collect bounded node-stamped
observations, and finish exactly that interval. The planner must return an end
strictly after frame and no later than ceiling. Decisions/storage shape must remain
valid; malformed intervals or changed exposed storage fault before consumers.
NoPlanner preserves ordinary event-defined processing without a domain planner.
The runtime owns absolute frame; observations are feedback about completed work,
not permission for consumers to advance the authoritative clock.

A valid finite sequence of one-sample intervals must run in full. There is no
musical quantum or arbitrary propagation quota. The application's structural rules
must establish same-frame termination; costly valid work may still miss a deadline.
Callback end is not settled by finish; the next nonempty callback settles that
boundary once. Zero-frame callbacks admit nothing and do not replay plan/finish.

Queue snapshots, result recycling, graph validation, background resource transfer,
recovery and diagnostics retain one enclosing callback envelope. Do not refresh them
at every plan call. Required RenderIo media access is separately capacity/shape/time
validated; it is not a discretionary quota that may drop mandatory sample work.
Explain receipts/observations/decisions as distinct bounded stores and link chapter17.
A render fault is not transaction rollback; a domain that already published state
may require permanent quiescence/cleanup rather than generic resumability.

For the final example, compare whole and split callbacks, a zero callback at a cut,
reversed consumer declarations, unread external results and a one-sample planner.
Assert independent samples and observations plus untouched callback-wide allowances.
Use tests/planner_execution.rs and tests/planning_graph.rs as behavior references,
not as pasted tutorial infrastructure. Lifetime prototypes cover refusal to retain
decisions/observations across mutation; they do not replace the executable book example.
