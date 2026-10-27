# Change instructions — chapter 6: ordinary versus coordinated graph edits

Destination: the matching chapter under `docs/`. Baseline: empty; archive sources
are `integration.md`, `getting-started.md` and `graph-activation.md`.

For this slice, qualify any blanket statement that all graph changes activate
only at callback boundaries. **Legacy `Command::Commit`** retains that behavior;
opt-in staged plans can activate at a planner operation/internal interval boundary.
Keep the ordinary NoPlanner walkthrough simple and link chapter 15 for the latter.

Add a short distinction between validating a candidate and making it active.
`StageGraph` Ready status does not alter routes or advance the graph epoch. Catalog
edits and other graph commits invalidate earlier staged validation; expected epoch
checks are not replaced by visually matching routes. Removing/reinstalling an
inactive processor changes its generation, so a staged reference cannot silently
retarget the replacement. Installed nodes and active graph membership differ.

Retain deterministic DAG/fan-in/fan-out explanations and legacy owning return rules.
Do not imply that a graph permit constructs new processors, changes prepared port
shapes, guarantees MIDI note cleanup, or performs arbitrary resource transactions.
Staging's fixed metadata slots and actual-capacity refusal are introduced in
chapters 15/17, not by complicating the first routing example.

References: `tests/graph_activation.rs` physical-catalog-generation case and
`no_planner_legacy_commit_and_128_staging_returns_keep_the_original_lifecycle`.
These verify legacy compatibility and owner recycling, not a complete loop
role-generation migration. The backfill below completes this handoff's routing
review, not Stage9 publication or final acceptance.

## Ordinary routing walkthrough backfill

Use the initial-graph preparation from chapter2, then a live edit using the public
compiler and Command::commit. A small two-gain graph makes fan-out and fan-in
observable: input1 feeds gains0.5 and0.25, both feed output0, and the second also
feeds output1. Assert outputs0.75 and0.25. Canonical signal DAG order must not depend
on description row order. Explain audio summation and MIDI stable timestamp/source
ties separately; a signal cycle is rejected, unlike an application's valid cyclic
sync relations. Planning bindings do not create signal routes.

Prepare the replacement off-render. With a small validation allowance, prove old
output continues while validation is incomplete; assert the new routes only after
the owning Applied completion. For an ordinary commit this is a callback ingress
cut, not a scheduled sample-exact command. Match the completion's sequence and read
the actual current epoch rather than predicting how many callbacks validation takes.
Keep all temporary compiler/catalog/descriptor construction outside the guard.

Commit a graph excluding the first node before removing it. Recover the retired
plan and processor via owning completions, then reuse the physical slot only with
the new checked generation. A future action addressed to the old NodeId must fail,
not mutate the replacement. Installed-but-inactive processors remain distinct from
active membership: parameters may address them; scheduled MIDI may not. Processor
port signatures and prepared maxima cannot be resized by swapping a graph.

Place failures where used: compiler rejects malformed/cyclic descriptions; runtime
validation and expected-epoch rejection still matter for prepared descriptors and
concurrent catalog edits. Rejection preserves the old graph and returns the candidate
owner. Never treat successful off-render compilation as permission to bypass runtime
validation. Show stale epoch explicitly, then recompile/restage as appropriate rather
than editing the expected number to evade a changed catalog.

Do not leave finite retirement slots unread or free their contents in process().
Guard the callback, recycle returned owners off-render, and check final outstanding
ownership. The existing gain_graph example exercises a simpler one-node lifecycle;
its passing execution does not claim this proposed two-node example exists.
Review/input hashes and fresh gain_graph runs: ../control-baseline.json.
