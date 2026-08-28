# Scalar latency expected-failure inventory

This file tracks transferred tests whose production behavior is not implemented yet. Entries are removed only after the named subsystem is implemented and the complete lower-layer suite is rerun. Compilation failures, missing-scaffolding panics, hangs, and ignored tests are not acceptable entries.

## Shared latency domain

Recorded after transferring the 11 reference tests into `shoop_latency`. `RUSTFLAGS="-D warnings" cargo check -p shoop_latency --all-targets` passes. `cargo nextest run -p shoop_latency --profile ci` discovers 11 tests: 3 pass and the following 8 fail through their intended behavioral assertions against an explicit `Unsupported` recipe warning.

| Test | Expected result after Stage 2 | Current explicit gap |
|---|---|---|
| `checked_recipe_summation_rejects_limits_and_capacity_overflow` | `500_000 + 500_000` is unresolved with `TotalOverflow`; component count above 16 is rejected | Resolver reports `Unsupported`. |
| `disabled_unknown_is_zero_but_enabled_unknown_is_unresolved` | Enabled unknown is unresolved; disabled unknown resolves to zero | Disabled recipe remains unresolved. |
| `every_component_toggle_and_mode_resolves_independently` | Exact selected totals for each enabled mode/range/trim; disabled contributes zero | All applicable enabled components are unresolved. |
| `grab_and_replacement_follow_their_channel_roles` | Direct/dry total 6 frames; wet total 11 frames | Recipes are unresolved. |
| `operation_recipes_enforce_component_and_cue_semantics` | Direct-world 6, direct-cue 13, wet-cue 18, dry render 8, dry-into-wet 8 frames | Recipes are unresolved and render provenance is not applied. |
| `overlapping_automatic_intervals_are_not_double_counted` | Shared automatic interval is unresolved with overlap warning and zero contributions | Resolver reports only `Unsupported`. |
| `path_aggregation_distinguishes_equivalent_ranged_unknown_and_ambiguous` | Equivalent exact, ranged path, unknown, and mixed-rate ambiguity remain distinct | Aggregator returns `Unknown` for every input. |
| `take_snapshot_is_frozen_and_detects_later_revision_changes` | Frozen total remains 4 frames while revision change sets `changed` | Scaffolding marks the unresolved snapshot incomplete with total zero. |

Already-green lower-layer assertions cover observation certainty validation, range selection, and individual manual/automatic/trim bounds. They must remain green when Stage 2 resolves the eight gaps.
