# Change instructions — chapter 13: graph authority in planner contexts

Destination: the matching chapter under `docs/`. Baseline: empty; refer to archived
`planning.md` and `graph-activation.md`. Full Stage 3 planner lifecycle backfill is
still required; this note covers the newly extended context.

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
