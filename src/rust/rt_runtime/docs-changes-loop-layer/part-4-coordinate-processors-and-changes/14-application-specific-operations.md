# Change instructions — chapter 14: retained outcomes and graph activation

Destination: the matching chapter under `docs/`. Baseline: empty; archive sources
`planning.md` and `graph-activation.md`. The backfill below completes the
operation/result handoff alongside the graph/recovery milestone.

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

## Complete typed-operation walkthrough backfill

Use a small generic operation family: an immediate processor multiplier change,
a planner request that waits for a known future boundary, and a bounded Cancel/Stop
recovery operation. Planner::Operation and Result are Copy family data; payload
bytes are separately bounded and borrowed only during delivery. Copy is not wire
serialization, pointer transport permission, authentication or owned-payload lifetime.
Do not retain OperationContext/payload references in a pending request. Copy only the
necessary validated fields into prepared domain slots and retain its ReceiptToken.

Submit Action::Planner or Action::Processor through the real Command::Schedule API.
Match sequence across transport completion and execution. Distinguish all four
frames with an explicit example: requested1, admitted/effective10, delivered10,
terminal20. Scheduled confirms admission, delivered_frame=Some(10) records delivery,
actual_frame=20 records terminal resolution, and late_by()=9 measures delivery
lateness rather than the musical waiting duration. A cancellation before delivery
may produce delivered_frame=None; never invent a delivery frame for it.

Immediate processor operations return their typed result. A pending planner token
retains its terminal reservation until ReceiptWriter::complete at a later context's
current frame. That writer checks scope/identity/terminal state and supplies the
terminal timestamp; there is no arbitrary backdating API. Report runtime errors in
Execution::result and the domain value in operation_result. A domain refusal can
have generic Ok and an explicit rejected domain outcome. Do not turn that into an
admission failure or pretend a successful command completion proves publication.

Demonstrate three cancellation points separately: a ticket flag before dequeue;
cancel_queued(sequence, outcome) for a not-yet-delivered normal typed action; and
completing a retained token for delivered pending domain work. Duplicate/stale token
completion must not resolve a newer event. Cancellation does not silently retract
already-published domain effects. Pending slot replacement must resolve the displaced
request rather than lose its outcome or recycle its reservation early.

Fill normal action/result capacity and leave outcomes unread. Independent internal
intervals still advance, and a configured planner's additional recovery receipt can
admit a payload-free, current-frame, recovery_safe operation. It can cancel queued
or pending normal work without borrowing a normal pending slot. Its own unread result
still consumes its sole reservation: consume/recycle it before requiring another.
Recovery may settle during planning, but must terminate within its admission callback;
future, payload-bearing, unclassified or nonterminating recovery is rejected/faulted
according to the public API. Classification is a trusted bounded semantic contract,
not an escape from guards, graph authority or admission limits.

Show malformed payload, unsupported operation, stale planner/processor identity and
explicit domain refusal with different observable results. Keep payload capacity
separate from the associated Rust enum's storage size; prepared memory includes
normal/recovery retained values and their owning returns. Use scoped guards across
delivery, planning, completion and saturated-result paths, then drain off-render.

References: tests/typed_schedule.rs and tests/typed_execution.rs (including pending
boundary, recovery cancellation, unread results, order and refusal cases). Existing
planned_gain only demonstrates an immediate typed processor result, not this pending
protocol. ../control-baseline.json records fresh example checks and source-matched
native188-case evidence; a new public pending-operation example is still required.
