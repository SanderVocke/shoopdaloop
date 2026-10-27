# Change instructions — chapter 19: staged, retired and parked owners

Destination: the matching chapter under `docs/`. Baseline: empty; archive sources
`integration.md`, `resources.md` and `graph-activation.md`. The initial generic
graph/transport owner slice is retained below; the later section adds resource,
prepared-ingress, planner and loop quiescence backfill. Publication remains pending.

List Ready candidates and Taken retired graphs among owners that survive control
stalls and must be accounted for. Explicit normal-lane ReleaseGraph returns them
through reliable owning completions. Taking/reading a metadata view does not release
an owner. Taken can legitimately have no plan after activating the first graph into
an empty runtime; release still retires its identity. Disconnect is not an implicit
activation, rollback, graph Stop or publication of an unfinished candidate.

The transport may simultaneously retain a current recovery command and a parked
normal command, plus queued commands/returns. A stopped runtime/endpoint can destroy
those owners off-render. Do not require the callback to discard either owner to
service recovery, or suggest dropping the receiver while audio still uses it.
Late cancellation of the already-dequeued parked command does not undo partial work.

Keep graceful drain versus intentional abandonment explicit. Inspect whether an
owner is an unapplied candidate, a displaced graph, a pending scheduled result or a
resource lease; these have distinct release mechanisms. Graph metadata release does
not automatically clean up processors, held notes, capture groups or application
media. Generic stopped extraction and explicit domain cleanup must retain their
separate explanations; do not transplant the loop Resolver's permanent quiescence
API into the generic runtime contract.

References: guarded `tests/graph_activation.rs` unread-result/disconnect and
NoPlanner owning-return cases; `tests/lifecycle.rs::recovery_preempts_held_normal_without_rechecking_cancellation_or_bypassing_fifo`
checks off-render cleanup with both current and parked owners. The final example
should assert complete owner disposal and resource recovery, not merely that
rendering stopped without an error.

## Stage3–6 shutdown and extraction backfill

See `../operation-baseline.json` for reviewed sources, unchanged empty destination
and archive hashes, full native rt_runtime debug/release checks, bounded benchmark
smokes and canaries. Add a small executable shutdown/abandonment example. Do not
make a whole application tutorial or claim that dropping every handle is a graceful
musical transition.

### Two explicit host policies

**Graceful:** stop admitting new work, cancel/resolve future domain requests under
its policy, stop/freeze captures at valid boundaries, complete required output/MIDI
cleanup, and collect reliable command/execution results while nonempty callbacks
still run. Bound the demonstration by known scheduled work or explicit cancellation.
A countdown on a stopped clock need not ever complete without a policy action; do
not recommend an unconditional render-until-idle loop or a blocking flush. Zero
callbacks make no progress. Late ticket cancellation is not a universal scheduler
cancel; typed queued/domain cancellation has separate receipts/outcomes.

Then stop/join callbacks before destroying endpoints/processors/planner/resource
storage. Drain and dispose/recycle owning returns outside render. A live-render
owner cannot be borrowed concurrently to inspect its store/catalog/diagnostics;
use coherent bounded publication while it is active.

**Abandonment/fault:** stop callbacks first, then consume the runtime off-render.
Queued/held/parked/returned owners are retained for that teardown; abandoned future
work is not promised a fabricated terminal completion. Published graph/domain cuts
and actual captured prefixes are not undone. A stopped device cannot receive reset
messages merely because Rust owners were destroyed. Separate device cleanup from
internal MIDI state/resource recovery.

### Owner ledger required by the example

Track and assert recovery of each applicable category, not just command_slots:

| Owner or credit | Required accounting |
| --- | --- |
| Queued/held/parked commands, returned candidates/processors/plans | Owning completion or stopped endpoint/runtime destruction off-render; retain refused payloads. |
| Scheduled future actions/pending typed receipts/unread executions | Explicit cancellation/completion while running, or declared abandonment on stopped teardown; no implicit acknowledgement. |
| Prepared cancellation flags, tickets and completion flag owners | Return admission credits by consuming completions; flag reuse waits for every external owner. Keep final destruction off RT. |
| Uploads and page reservations | Finish/commit intentionally or cancel; reclaim retired data under bounded maintenance. |
| Snapshot/export/playback pins | Release each acquired lease exactly once; copying metadata/IDs is not a new lease and Drop does not release it. |
| Group interval/take/final-lease promises and claims | Cancel interval before prepared take, or freeze active take at its actual common prefix, release returned leases, then release Idle group claims. |
| Active resource content and retired pages/versions | Explicit remove/reclaim if reusing the store, or final whole-store destruction off-render. Metadata release does not instantly reclaim all pages. |
| Ready/Taken graph/topology and staged media/source owners | Release/dispose through the appropriate owning path; do not confuse candidate readiness with active publication or rollback permission. |

Choose a compact generic fixture with an upload, retained snapshot, future scheduled
action and an owning graph/processor change; assert refusal or abandonment behavior
for each branch and complete reclaim. Link chapter16 for the prepared-group branch
rather than duplicating all its setup. Existing `bounded_runtime` shutdown proves
its own finite pending work, capture/export validation, upload/snapshot release,
empty-plan/remove sequence and full pool recovery; it is not a general cancellation
recipe for arbitrary future or clock-relative domain requests.

### Extraction is not automatic domain cleanup

`GraphRuntime::into_resources()` consumes a stopped runtime and drops other owners.
It cannot discover leases/take reservations represented only by plain IDs inside a
processor/planner. `into_planner_and_resources()` retains the planner ledger with
the store so application cleanup can release those promises; other owners still
are destroyed off-render. Both operations are ownership extraction, not command
settlement, a fake final render interval or device MIDI cleanup. A processor with
hidden leases needs its own returned/external ledger if the store must be reused.

For loop-specific references only: `Engine::into_stopped()` retains the loop planner,
store and observer. `Stopped::cleanup()` permanently quiesces the resolver, cancels
unpublished ownership/intervals, freezes actual active prefixes, releases media and
source/export/pin leases, then releases group claims/resources. Repeating successful
cleanup is safe. This handles a fault after publication but before finish without
rollback, replay or a fake finish/Stop. Published time/state/revisions/epochs remain
what occurred; quiescence is permanent and does not authorize resumed execution.
Old generation exports cannot retarget a new owner. Keep this attribution separate
from the generic runtime and from full downstream synth/device state reconciliation.

### Verification and limits

Current188 native cases/profile include lifecycle two-thread/model/refusal/disconnect
and prepared-ingress tests; capture-group disconnect and planner extraction are also
covered by their dedicated suites. Stage5/6 facade tests verify every owner phase,
post-publication faults, retained old exports and cleanup twice with full resource
recovery. Cite their source-backed contracts rather than historic counts in the
teaching chapter. The new small chapter example still needs separate-worker execution
in both profiles, scoped heap guards and source/excerpt checks. No new Miri, actual
loop-host, cross-platform performance or final-acceptance result follows from this
review. Keep the parked-normal recovery correction above, with finite budgets and
no unconditional fairness claim.
