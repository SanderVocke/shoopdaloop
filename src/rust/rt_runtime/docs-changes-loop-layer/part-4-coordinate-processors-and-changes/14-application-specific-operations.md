# Change instructions — chapter 14: retained outcomes and graph activation

Destination: the matching chapter under `docs/`. Baseline: empty; archive sources
`planning.md` and `graph-activation.md`. Full typed-operation/result backfill is
still pending; these additions cover the graph/recovery milestone.

Explain that an application operation may refer to a runtime StageId, but the
operation's Copy payload does not own the candidate graph. Ownership stays in the
prepared graph bank. A planner may activate during operation delivery, or retain a
pending request/token and settle it during later planning. Distinguish requested,
delivered and terminal frames; use the terminal frame for a deferred activation's
result rather than relabeling the original delivery.

The runtime returns the graph capability through OperationContext/PlanContext,
not an unchecked Control handle. Recovery operation contexts explicitly deny graph
activation. Classifying an operation as recovery-safe cannot override that denial;
the planner must still obey the general bounded recovery contract. Stage/release
commands are normal-lane operations, not emergency lane bypasses.

Cross-reference chapter 5's held-normal preemption: recovery admitted in a later
callback is no longer hidden behind unfinished normal validation. Normal validation
retains progress across unrelated completions. This is separate from pending domain
operations whose owning command already completed but whose execution token remains
retained. Do not conflate those two storage lifecycles.

The eventual public example should expose a real pending token, separate command
acceptance and domain outcome, explicit cancellation, and actual terminal frame.
The graph example can then reuse these ideas without teaching loop modes. References:
`tests/typed_execution.rs`, `tests/graph_activation.rs` deferred/cancelled activation
cases, and `tests/lifecycle.rs` held-normal recovery case. A successful generic
execution receipt is not proof of application-level transaction success.
