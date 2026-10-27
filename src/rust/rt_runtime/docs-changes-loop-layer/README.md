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
semantic rewrite needed); chapters1–4 were then left pending. `rechecks.json`
records that historical recheck without replacing the original baseline. The later
Part I review below now covers those four populated chapters.

The later [loop-facade live-graph slice](loop-facade-live-graphs.md), based on
`b071ffc4`, adds fixed-role live routing and paired graph/timing publication.
Its reviewed destination/source hashes are in `live-graph-baseline.json`;
verification is in `docs/loop_layer/evidence/live-graph-stage6{,-provenance}.json`.
It supersedes the earlier claim that the loop Controller cannot change routes,
not the still-pending general role-generation/media transaction requirement.

The [media-owner activation prerequisite](loop-media-owner-activation.md), based
on `229e1a13`, adds borrowed whole-batch owner/initial-media publication while
retaining old exports and pins. `owner-activation-baseline.json` records its reviewed
media guide and source inputs. That slice alone did not integrate facade generations.

The [coordinated configuration slice](loop-configuration-activation.md), based on
`f510466d`, now combines graph/timing/media publication over prepared physical
roles with logical generation edits, source lease staging, input policy and capture
claim checks. `configuration-baseline.json` records reviewed runtime/topology guides.
Physical shapes/node IDs remain fixed. At that milestone, combined running-program/
graph publication and full Stage6 verification remained pending.

The [paired running-program/graph slice](loop-program-graph-activation.md), based on
`b5b240d0`, now holds graph publication authority across selected program/capture
preflight and publishes both configurations at one zero/stopped cut. Its baseline
reviews runtime/topology/program guides; independent graph retirement and cancellation
remain explicit. Broader lifecycle coverage and full Stage6 verification remain open.

The [nested lifecycle slice](loop-nested-lifecycle.md), based on `9203bc43`, adds
an independently predicted integrated seek/stop/restart/capture example, explicit
same-frame priority and countdown outcomes, and immutable old/new exports. Its
baseline rechecks unchanged runtime/program guides. The operational
`docs/loop_layer/STAGE6_READINESS.md` maps accumulated implementation coverage;
consolidated Stage6 Miri remains pending, not replaced by the native exact-filter
mirrors or the expanded228-execution registration.

The subsequent **Part I backfill review** fully read chapters1–4, their three new
examples, the separate-worker plan, and archived getting-started/integration/planning
plus the approved parameter-chapter style reference. All four populated chapters
still match their prior drift hashes. [Part I notes](part-1-process-audio/01-the-runtime-and-the-host.md)
preserve the working examples and identify small authority/interval/resource/fault
clarifications. `part-one-baseline.json` records source/destination hashes, unchanged
11-file archive integrity,23 checker-test passes, a drafting-only book check, and
all three examples executed in debug/release using isolated output. No authored
chapter, example, archive or separate-worker plan was changed or staged by this
worker. This is actionable review, not separate-worker publication or full-book
acceptance. The consolidated Stage6 Miri job runs independently from `faec78df`;
see the operational `MIRI_JOBS.md`, not these notes, for its status.

The **stored-media/MIDI/capture backfill** now supplies actionable example and
failure/ownership requirements for chapters9–12 and16. `media-baseline.json` records
the five still-empty destination files, reviewed archives and implementation hashes,
unchanged11-file archive integrity,63 existing resource/index/state/group tests per
native debug/release profile across15 targets, and a drafting-only book check.
These are reviewed contracts and tested implementation behavior, **not newly authored
chapter examples**. Publication, example creation/execution and final acceptance
remain the documentation worker's work. The notes correct raw-ID versus scoped-view
preconditions, physical resource versus musical revision clocks, strict pre-target
state versus restoration, and the current prepared-first-interval capture path.

The **operation/capacity/teardown backfill** expands chapters17/19 and adds chapter18
prepared ingress: `operation-baseline.json` records the still-empty destinations,
source/archived-guide review, unchanged archive/input hashes,188 rt_runtime cases
per native debug/release profile, warning-denied rustdoc, and the existing seven-
workload `bounded_runtime --iterations 1024` smoke in both profiles. All six deliberate
allocation/reallocation/deallocation canaries produced the expected guard failure.
These are documentation smoke checks, not the full performance campaign, new
prepared-ingress/shutdown examples or actual loop-host evidence. Cross-cutting notes
also identify the stale “split credits”/all-resource-quota crate rustdoc summary.
All19 chapters now have notes, but some earlier slices still require full backfill,
and **none of this asserts separate-worker publication or full-book acceptance**.

**This is not yet a complete Stage 9 handoff.** The current notes cover the graph/
recovery prerequisite, fixed-role facade integration, media-owner publication and
their cross-cutting effects. Full review/backfill of prior
loop-layer stages remains explicit below. Do not infer that empty chapters need no
changes, that existing archived prose is authoritative, or that notes have already
been applied. No collected fresh Miri pass, completed Stage 6, loop AudioWorklet, final CI or review
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
| 1 — Runtime and host | [Stage3–6 backfill reviewed](part-1-process-audio/01-the-runtime-and-the-host.md): qualify signal DAG, authority/host boundaries and optional planning; retain current overview. Publication pending. |
| 2 — First graph | [Reviewed; retain startup example](part-1-process-audio/02-your-first-audio-graph.md): no warm-up, NoPlanner, source-checked debug/release assertions and preparation ownership. Publication pending. |
| 3 — Processors | [Stage3–6 backfill reviewed](part-1-process-audio/03-writing-processors.md): borrowed decision/observation access, required rendering, interval MIDI and failure/lifetime obligations. Publication pending. |
| 4 — Callback | [Stage3–6 backfill reviewed](part-1-process-audio/04-connecting-an-audio-callback.md): one callback envelope, half-open boundaries and domain-specific post-fault cleanup; retain guarded adapter. Publication pending. |
| 5 — Ownership | [Slice reviewed](part-2-control-a-running-graph/05-commands-completions-and-ownership.md); backfill typed scheduling/resource ownership. |
| 6 — Live edits | [Slice reviewed](part-2-control-a-running-graph/06-routing-and-live-graph-edits.md); review all legacy editing/archive topics. |
| 7 — Exact parameters | [Reviewed, no semantic rewrite for this slice](part-2-control-a-running-graph/07-changing-parameters-at-an-exact-sample.md); recheck final API/example and navigation. |
| 8 — MIDI | [Slice reviewed](part-2-control-a-running-graph/08-processing-and-scheduling-midi.md); backfill reserved recovery and required rendering. |
| 9 — Resources | [Backfill reviewed](part-3-work-with-stored-media/09-loading-and-reading-resources.md): upload/ownership, required versus background access, raw-ID preconditions and bounded cleanup; chapter/example publication pending. |
| 10 — Record/export | [Backfill reviewed](part-3-work-with-stored-media/10-recording-and-exporting.md): immutable truthful prefixes, capture revisions, exhaustion and single-resource versus group atomicity; chapter/example publication pending. |
| 11 — MIDI events | [Backfill reviewed](part-3-work-with-stored-media/11-seeking-and-reading-midi-events.md): page versus event positions, complete packet retry, ties/EOF and view lifetimes; chapter/example publication pending. |
| 12 — MIDI state | [Backfill reviewed](part-3-work-with-stored-media/12-midi-state-and-checkpointed-seeking.md): concrete checkpoint reference, C×K−1 capacity, pre-target state, unsupported histories and renderer attribution; chapter/example publication pending. |
| 13 — Planning | [Slice reviewed](part-4-coordinate-processors-and-changes/13-planning-processing-intervals.md); backfill full planner/observation lifecycle. |
| 14 — Typed operations | [Slice reviewed](part-4-coordinate-processors-and-changes/14-application-specific-operations.md); backfill pending result/cancellation semantics. |
| 15 — Graph activation | [Detailed prerequisite notes](part-4-coordinate-processors-and-changes/15-staging-and-coordinating-graph-activation.md); revisit after general loop integration. |
| 16 — Capture groups | [Backfill reviewed](part-4-coordinate-processors-and-changes/16-recording-resources-together.md): first-interval preflight, reservations/claims, staged common-prefix publication, stamped authority and explicit cleanup; chapter/example publication pending. |
| 17 — Capacity | [Stage3–6 backfill reviewed](part-5-configure-and-operate-the-runtime/17-capacity-work-budgets-and-diagnostics.md): typed/pool/graph/group/index formulas, callback allowances versus required work, diagnostics and bounded smoke limitations; publication/reference verification pending. |
| 18 — Prepared ingress | [Backfill reviewed](part-5-configure-and-operate-the-runtime/18-submitting-without-allocation.md): distinct flag/admission credits, owning-buffer reuse, inactive plan materialization versus validation, finite recovery and destructor limits; new example/publication pending. |
| 19 — Shutdown | [Backfill reviewed](part-5-configure-and-operate-the-runtime/19-stopping-disconnecting-and-reclaiming-ownership.md): graceful versus abandoned work, complete owner ledger, resource/planner extraction, and separately attributed loop quiescence; new example/publication pending. |

## Archive coverage still to reconcile

- `getting-started.md`, `integration.md`: chapters1–4 reviewed in the Part I notes;
  startup/processor/host capabilities are accounted for; prepared ingress/extraction/
  teardown is reviewed in18–19. Commands/live edits and
  complete planner/coordination backfill remain chapters5–6/13–15.
  Do not copy obsolete held-validation/recovery ordering from the archive.
- `scheduling-midi.md`, `changing-parameters.md`: chapters 5, 7–8, 14, 17–19.
- `resources.md`: upload/access/index/capture/export ownership reviewed for9–11/16;
  consolidated formulas, prepared ingress and shutdown backfill now reviewed in17–19.
- `midi-indexing.md`, `midi-state.md`: event/state/initial/checkpoint/ownership policies
  reviewed for11–12/16; chapter17 now supplies the sizing/counter backfill. The
  separate worker must verify the final reference tables and examples.
- `capture-groups.md`: take/interval/first-reservation/common-prefix/lifetime/cleanup
  review covers10/16. Consolidated sizing/ingress/shutdown is reviewed in17–19;
  full chapter13 planner integration backfill remains. Do not
  copy obsolete blanket claims of missing loop integration.
- `planning.md`: introductory processor/callback effects reviewed for chapters3–4,
  capture/accounting/cleanup in16–19; complete planner/receipt/activation backfill
  remains chapters13–15.
- `graph-activation.md`: chapters 5–6, 8, 13–15, 17–19; this slice reviewed below,
  but evidence claims and future integration limitations must be refreshed separately.
- `performance.md`: chapters17–19 backfill now reviewed, with current formulas,
  held-normal recovery correction, required/background work and measurement limits.
  Separate-worker supporting reference pages and final benchmark evidence remain.

[Cross-cutting and other-document follow-ups](cross-cutting.md) cover README,
rustdoc, references, architecture and loop-specific attribution. Full loop-library
user-guide changes and earlier-stage audit deltas remain to be added; this index
must not be marked complete until each pending row has actionable notes or a
source-backed, explicitly justified no-change finding.
