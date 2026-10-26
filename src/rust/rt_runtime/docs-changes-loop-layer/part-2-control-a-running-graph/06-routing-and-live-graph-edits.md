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
role-generation migration. Full archive-to-chapter routing coverage still needs
its Stage 9 review.
