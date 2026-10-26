# Change instructions — chapter 5: owning commands and recovery

Destination: the corresponding chapter under `docs/`. Baseline: empty; consult
`docs-previous/integration.md` and `graph-activation.md`. This note covers the new
graph/recovery slice; earlier resource and typed-result coverage remains pending.

## Add or qualify

- Include staged graphs in the owning-command lifecycle: normal-lane `StageGraph`
  validation success returns the command with `plan: None, id: Some(id)`, meaning
  Ready, not active. Failure/pre-dequeue cancellation returns the original plan.
  `ReleaseGraph` returns an unused candidate or displaced graph via `Retired::Plan`.
  Link chapter 15 for the permit rather than introducing its entire protocol here.
- Recovery has reserved admission and priority over a held normal command at the
  next callback's ingress snapshot. The receiver now parks the held normal owner
  in one fixed inline slot, services eligible recovery, then resumes that same
  normal head before later normal requests. No allocation, extra admission credit,
  owning destruction or producer-chasing loop is introduced.
- Parking is **not another dequeue**. A ticket cancelled after the original dequeue
  cannot cancel partially executed work when it resumes. Preserve the original
  cancellation observation, owning return reservation and normal FIFO order.
- Recovery still obeys configured operation/publication limits. Cancelled recovery
  dequeues consume those limits too. If a callback spends its publication allowance
  on recovery, resume normal work in a later callback; do not overdraw results.
  Recovery priority is not a promise of normal-lane fairness under endless recovery
  that consumes every configured slot.
- Retained normal validation progress survives unrelated recovery completions.
  Normal requests may still wait for validation/background allowance; this does not
  authorize blocking the independent recovery lane behind a held normal owner.
- Distinguish transport owning completion from a scheduled operation's execution
  result and from domain success. Reading the former does not discharge the latter.
  The full typed/pending-result explanation belongs in chapter 14.

## Demonstration and evidence

Show a held normal request, a later recovery request, and a second normal request.
Assert recovery first, then the original normal head, then the later normal request.
Cancel the held ticket after its first dequeue and verify it still finishes rather
than being re-cancelled on resumption. Test publication capacities one and two,
including cancellation of recovery itself. Keep returns unread until the ownership
checks, then dispose outside the guarded scope. Include stopped/disconnected cleanup
with a current recovery owner and a parked normal owner.

`tests/lifecycle.rs::recovery_preempts_held_normal_without_rechecking_cancellation_or_bypassing_fifo`
is the guarded behavior reference. `tests/graph_activation.rs::incremental_validation_and_malformed_or_cancelled_envelopes_never_publish_early`
also verifies continuous recovery during one-step staged **and legacy** validation.
Source: `transport::RenderEndpoint::{begin_cycle,next_operation,complete}` and
`GraphRuntime`'s validation-sequence handling. The regression initially reproduced
missing recovery execution; the final verification report retains that failure.
Do not substitute an unguarded toy queue for the public behavior when demonstrating
callback-safety claims.
