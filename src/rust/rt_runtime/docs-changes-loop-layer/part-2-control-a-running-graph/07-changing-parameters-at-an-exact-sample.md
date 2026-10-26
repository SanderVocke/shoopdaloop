# Review note — chapter 7: preserve the approved walkthrough

Destination: the matching chapter under `docs/`. Baseline: the current populated
chapter, plus `docs-previous/changing-parameters.md`; hashes are in `baseline.json`.

A subsequent concurrent draft was re-read: it adds the `rust-source` marker for
`gain_graph.rs` without changing the walkthrough's semantics. Preserve that marker;
`rechecks.json` records the reviewed revision separately from the initial baseline.
`gain_graph` has now also passed debug/release execution against this implementation
(`target/loop-layer-graph-activation-stage6/gain-graph.log`).

No semantic rewrite is required for the generic graph/recovery slice. Preserve the
exact-sample `gain_graph` walkthrough, the distinction between Scheduled acceptance
and execution, callback-end timing, refusal ownership and pre-dequeue cancellation.
The existing `min(scheduled_events, budgets.events)` formula remains correct.

The new recovery preemption does not make normal parameter admission immediate:
normal queue/validation delays are still possible, so the lead-time advice stays.
Nor does parked-request resumption create another cancellation opportunity. Explain
that detail in chapter 5, not by adding a transport digression here. Typed operations
with deferred domain outcomes belong in chapter 14; do not generalize the parameter
example's immediate execution result to every application operation.

Final checks: execute `gain_graph` in debug/release, check snippets against its
source and verify nested chapter links. This note is a justified no-semantic-change
finding for this slice, not an assertion that the full book or later API revisions
have passed final documentation verification. Keep the user's approved prose intact.
