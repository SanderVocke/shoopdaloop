# Change instructions — chapter 5: owning commands and recovery

Destination: the corresponding chapter under `docs/`. Baseline: empty; consult
`docs-previous/integration.md` and `graph-activation.md`. This note covers the new
graph/recovery slice; the later section below completes resource/typed-result
ownership backfill. The destination/example still require separate-worker publication.

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

## Complete ownership walkthrough backfill

`../control-baseline.json` records the empty destination, reviewed archive/source
inputs and current verification. Start with a small public owning-command example:
prepare a graph/processor or resource buffer off-render, submit it, keep callbacks
running, and match its completion by sequence. Assert that a refused submission
returns the original value, and a rejected/cancelled admitted command returns its
post-operation request plus any retirement owner. Do not drop a refused Box/plan/
processor inside the guarded callback merely to report its error.

Show one accepted scheduled parameter with two separate acknowledgements: transport
completion reports Scheduled; later execution reports the processor's return value
and frame. A typed planner request may remain pending still longer. Generic execution
Ok is not domain success; chapter14 supplies the actual pending/outcome example.
Reading an owning completion frees transport credit but not a pending/unread execution
slot; scheduling credit recycles at a later callback boundary. Future timestamps
retain their reservations. Telemetry is lossy Copy data, not an acknowledgement.

For a resource Read, recover the advanced cursor and buffer from the returned
ResourceCommand request, and use written/eof from its reply. Inspect outcome and
reply separately. Budget is refusal, not automatic retry; retain/reconsider the
returned owner outside render. Chapter9 supplies resource bytes/lease details.
Explicitly account for uploaded/captured snapshots: copying a plain handle does not
acquire a lease, and Drop cannot release a lease hidden in application state.

Exercise a full normal lane with unread owning returns and one independent recovery
request, then drain/recycle and prove ownership recovery. Include pre-dequeue ticket
cancellation and later cancellation that cannot undo already-dequeued work. Snapshot
queue availability once per callback; arrivals/recycled execution credits midcallback
wait for the next callback, not the next planner interval. Preserve normal FIFO but
never teach a global FIFO across normal/recovery lanes. Recovery may overtake a
normal read with a release, causing a legitimate stale-handle failure.

Ordinary submit may allocate a cancellation flag. Link chapter18 for prepared
submission and its distinct Busy condition when tickets/results retain flag owners.
Do not make this introductory example an allocation-free-ingress claim without the
corresponding guard and prepared payload/retirement scheme. Final teardown must
stop/join callbacks; abandoning future work does not fabricate terminal results.
The full native188-case evidence in operation-baseline remains source/input-matched;
this review adds no unimplemented example or new full-suite pass claim.
