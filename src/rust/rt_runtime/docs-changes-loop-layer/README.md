# Loop-layer implementation: documentation change-set handoff

These are **change instructions**, not replacement book chapters. The implementation
worker does not edit `docs/`, `docs-previous/` or the documentation worker's drafts.
The separate documentation worker applies the reviewed changes in its final pass.
See `PLAN_LOOP_LAYER.md` for the complete acceptance contract.

## Baseline and status

Initial handoff slice: Stage 6 generic staged-graph activation and held-request
recovery, based on implementation parent `95a42c09`. `baseline.json` records hashes
of the current book/archive/README inputs and relevant implementation sources.
At this review, chapter 7 contains the approved example-led text; the other eighteen
chapters are empty. The archive includes the staged-graph guide written before
this handoff policy. The live `DOCS_REWRITE_PLAN.md` is a separate worker's plan,
not modified by this work. Before this slice's commit, concurrent edits to chapters
1–4 and7 were detected. Chapter7 was re-read (source-excerpt metadata added; no
semantic rewrite needed); chapters1–4 remain in the pending backfill review.
`rechecks.json` records this separately without replacing the original baseline.

The later [loop-facade live-graph slice](loop-facade-live-graphs.md), based on
`b071ffc4`, adds fixed-role live routing and paired graph/timing publication.
Its reviewed destination/source hashes are in `live-graph-baseline.json`;
verification is in `docs/loop_layer/evidence/live-graph-stage6{,-provenance}.json`.
It supersedes the earlier claim that the loop Controller cannot change routes,
not the still-pending general role-generation/media transaction requirement.

The [media-owner activation prerequisite](loop-media-owner-activation.md), based
on `229e1a13`, adds borrowed whole-batch owner/initial-media publication while
retaining old exports and pins. `owner-activation-baseline.json` records its reviewed
media guide and source inputs. General facade generation integration is still pending.

**This is not yet a complete Stage 9 handoff.** The current notes cover the graph/
recovery prerequisite, fixed-role facade integration, media-owner publication and
their cross-cutting effects. Full review/backfill of prior
loop-layer stages remains explicit below. Do not infer that empty chapters need no
changes, that existing archived prose is authoritative, or that notes have already
been applied. No fresh Miri, completed Stage 6, loop AudioWorklet, final CI or review
approval is implied. Use implementation evidence for verification status, not the
archived guide's milestone counts.

Before applying a note, re-read the destination and compare its baseline: the user
may have already incorporated some changes. Preserve those edits, resolve conflicts
against current tested public behavior, and update this index rather than duplicating
prose. Stage/progress counts belong in evidence, not teaching chapters.

## Coverage ledger

Paths below mirror the eventual destinations under `docs/`. “Slice reviewed” means
only this milestone's changes have been reviewed; it does not close full coverage.

| Chapter | Initial change note / remaining review |
| --- | --- |
| 1 — Runtime and host | Pending full Stage 3–6 backfill: timing authority, generic/loop/host boundaries, no deadline guarantee. |
| 2 — First graph | Pending: startup graph preparation, no warm-up render, NoPlanner compatibility. |
| 3 — Processors | Pending: decision/observation capabilities, required render access, guard and error obligations. |
| 4 — Callback | Pending: callback versus interval lifecycle, zero blocks, exact ceilings and non-rollback faults. |
| 5 — Ownership | [Slice reviewed](part-2-control-a-running-graph/05-commands-completions-and-ownership.md); backfill typed scheduling/resource ownership. |
| 6 — Live edits | [Slice reviewed](part-2-control-a-running-graph/06-routing-and-live-graph-edits.md); review all legacy editing/archive topics. |
| 7 — Exact parameters | [Reviewed, no semantic rewrite for this slice](part-2-control-a-running-graph/07-changing-parameters-at-an-exact-sample.md); recheck final API/example and navigation. |
| 8 — MIDI | [Slice reviewed](part-2-control-a-running-graph/08-processing-and-scheduling-midi.md); backfill reserved recovery and required rendering. |
| 9 — Resources | Pending: required/background access, indexes, versions and scoped snapshot ownership. |
| 10 — Record/export | Pending: exact cuts, truthful prefixes, snapshots and teardown. |
| 11 — MIDI events | Pending: page indexes, complete packets, half-open ranges, equal-time events and EOF. |
| 12 — MIDI state | Pending: projection/checkpoints, strict pre-target state, unsupported state/restoration policy. |
| 13 — Planning | [Slice reviewed](part-4-coordinate-processors-and-changes/13-planning-processing-intervals.md); backfill full planner/observation lifecycle. |
| 14 — Typed operations | [Slice reviewed](part-4-coordinate-processors-and-changes/14-application-specific-operations.md); backfill pending result/cancellation semantics. |
| 15 — Graph activation | [Detailed prerequisite notes](part-4-coordinate-processors-and-changes/15-staging-and-coordinating-graph-activation.md); revisit after general loop integration. |
| 16 — Capture groups | Pending: first-interval reservations, common-prefix outcomes, scoped authority, cancellation and claims. |
| 17 — Capacity | [Slice reviewed](part-5-configure-and-operate-the-runtime/17-capacity-work-budgets-and-diagnostics.md); backfill all changed formulas and render counters. |
| 18 — Prepared ingress | Pending: reusable metadata, prepared flags/payloads, ownership-preserving refusal and callback restrictions. |
| 19 — Shutdown | [Slice reviewed](part-5-configure-and-operate-the-runtime/19-stopping-disconnecting-and-reclaiming-ownership.md); backfill resource/domain extraction and permanent loop quiescence. |

## Archive coverage still to reconcile

- `getting-started.md`, `integration.md`: chapters 1–6, 13–15, 18–19.
- `scheduling-midi.md`, `changing-parameters.md`: chapters 5, 7–8, 14, 17–19.
- `resources.md`: chapters 9–11, 16–19.
- `midi-indexing.md`, `midi-state.md`: chapters 11–12, 16–17.
- `capture-groups.md`: chapters 10, 13, 16–19.
- `planning.md`: chapters 3–4, 13–17.
- `graph-activation.md`: chapters 5–6, 8, 13–15, 17–19; this slice reviewed below,
  but evidence claims and future integration limitations must be refreshed separately.
- `performance.md`: chapters 17–19 and supporting reference/measurement pages.

[Cross-cutting and other-document follow-ups](cross-cutting.md) cover README,
rustdoc, references, architecture and loop-specific attribution. Full loop-library
user-guide changes and earlier-stage audit deltas remain to be added; this index
must not be marked complete until each pending row has actionable notes or a
source-backed, explicitly justified no-change finding.
