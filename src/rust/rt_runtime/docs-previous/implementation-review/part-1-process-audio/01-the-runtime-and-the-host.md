# Chapter 1 review: keep the overview; distinguish timing/dependency ownership

Destination: `docs/part-1-process-audio/01-the-runtime-and-the-host.md`.
Reviewed the complete populated draft, `DOCS_REWRITE_PLAN.md`, and the archived
getting-started/integration/planning guides. Reviewed hashes and executable checks
are in `../part-one-baseline.json`. This supersedes the earlier empty/pending review
for this chapter, not the documentation worker's publication responsibility.

## Apply narrowly

- Keep the existing control/render split, lifecycle diagrams, prepared-signature
  replacement explanation, no-device/no-thread boundary and no-deadline statement.
  They agree with current APIs; do not replace them with an implementation log.
- Qualify the acyclic-graph statement as the **signal graph**. When introducing the
  optional planner, link chapter13 and explain that its decision dependencies are
  separately validated. Application sync/action/composite relationships are not
  audio routes. Do not infer that a valid loop sync cycle is a signal feedback path,
  or make `rt_runtime` responsible for interpreting loop music.
- Add only a short timing-authority bridge: the runtime owns absolute sample frames;
  an optional prepared planner settles domain decisions before the signal traversal;
  processors render their assigned interval from borrowed decisions. The loop
  library supplies its own resolver/media consumers on that generic interface.
  Ordinary `NoPlanner` processors remain supported.
- Preserve the distinction between configured storage and callback allowances.
  Internal intervals do not renew ingress/result/resource budgets. Leave sizing
  formulas and termination details to chapters13/17 rather than duplicating them.
- In shutdown references, distinguish ownership disposal from graceful domain
  settlement. Disconnect does not synthesize a loop Stop, finish an interrupted
  interval, release undiscoverable copied lease handles, or deliver device cleanup.
  Chapter19 should provide the full extraction/quiescence sequence.

## Checks and source anchors

`GraphRuntime::prepare_with_initial_graph`, `run_scheduled`, `Planner::{plan,finish}`
and `ProcessBlock` establish these boundaries. The first graph/processor/callback
examples retain their `NoPlanner` path and ran in debug/release during this review.
Do not add loop-library or browser dependencies to a generic-runtime example.
Historical archive claims that held normal validation blocks every recovery command
are obsolete; the existing chapter5/19 handoff describes the parked-normal fix.
No new code or new example is required solely for this overview refinement.
