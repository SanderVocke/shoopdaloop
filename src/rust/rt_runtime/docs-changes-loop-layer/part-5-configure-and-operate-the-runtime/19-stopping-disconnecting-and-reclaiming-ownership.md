# Change instructions — chapter 19: staged, retired and parked owners

Destination: the matching chapter under `docs/`. Baseline: empty; archive sources
`integration.md`, `resources.md` and `graph-activation.md`. Complete resource/loop
quiescence backfill is still pending; this slice adds generic graph/transport owners.

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
