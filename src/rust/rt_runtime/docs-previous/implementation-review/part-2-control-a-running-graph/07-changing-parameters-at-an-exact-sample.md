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

## Full backfill review result

The current chapter was read completely again alongside gain_graph.rs, the approved
archive, scheduler frame/credit fields, and the typed/planner context. No semantic
rewrite is needed. The ordinary parameter example correctly uses actual_frame for
its immediate outcome; do not reuse that simplification for deferred typed results.
Execution::late_by() measures delivery minus requested time, not musical waiting
until terminal completion. The callback ingress admission description is correct;
internal planning intervals do not create additional admission opportunities.

Fresh warning-denied debug/release runs pass, including the example's scoped heap
guards, exact64/64 sample assertions and owning plan/processor teardown. The approved
chapter's rust-source marker and nested links pass the drafting checker. Hashes are
in ../control-baseline.json. The checker still allows14 empty chapters, so this is
local preservation/verification, not book publication acceptance. The helper's finite
64-callback validation bound belongs to this fixed offline demonstration; it is not
a runtime subdivision cap or a general deadline/fairness guarantee.
