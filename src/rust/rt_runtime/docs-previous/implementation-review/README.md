# Archived implementation review

This directory was moved from `src/rust/rt_runtime/docs-changes-loop-layer/` after the correctness merge. It preserves the original notes and JSON evidence as historical source material, not an active handoff queue. The user's final-input instruction supersedes the old worker, future-stage and validation-workflow directions below.

Use the [documentation completion reference](../remaining-documentation.md) for the consolidated authoring requirements and [DOCS_REWRITE_PLAN.md](../../../../../DOCS_REWRITE_PLAN.md) for the execution and completion record. No further handoff notes or implementation changes are expected. Do not refresh baselines, rerun documentation accounting, wait for missing hosts/benchmarks or infer that an old pending statement is current. Original destination paths/hashes and verification records describe their historical snapshots; they are not current integrity requirements.

The original review follows, including its chronological index and source links.

# Loop-layer implementation: documentation change-set handoff

These are **change instructions**, not replacement book chapters. The implementation
worker does not edit `docs/`, `docs-previous/` or the documentation worker's drafts.
The separate documentation worker applies the reviewed changes in its final pass.
See `PLAN_LOOP_LAYER.md` for the complete acceptance contract.

## Current verification checkpoint

Stage6's consolidated Miri run is now collected and verified:81commands/228executions
(128runtime+100loop), immutable implementation faec78df. See repository
`docs/loop_layer/evidence/miri-stage6{,-collected,-provenance}.json` and the raw-log
bundle. Current scoped implementation/configuration hashes match. Stage6 is complete;
the user requires a goal pause before Stage7. Earlier pending statements below record
historical review checkpoints, not current interpreter status. Documentation notes
remain unapplied handoff instructions; publication and final acceptance are not
completed by this verification milestone.

## Historical baseline and review status

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
All19 chapters now have notes; the subsequent control/planning review below
completes their earlier-slice backfill. **None of this asserts separate-worker
publication or full-book acceptance**.

The **control/planning backfill** completes chapter5–8/13–15 review against the
archives and current APIs. It covers owning returns, ordinary routing, preservation
of the approved parameter chapter, packet/recovery MIDI, exact planner intervals,
retained typed receipts and coordinated graph publication. `control-baseline.json`
records six empty destinations and unchanged chapter7, four fresh warning-denied
example executions (gain_graph/planned_gain × debug/release), and the drafting-only
book check. All123 scoped Rust inputs,10 configuration/runner inputs and11 archives
remain unchanged; the previous188-case native/profile evidence and raw log hashes
were verified and reused, not rerun or relabeled as fresh. Existing planned_gain is
one-consumer and unguarded: the notes explicitly require the proposed guarded
multi-consumer/pending-operation tutorials, rather than claiming those exist.
All19 chapter backfill reviews are now actionable. Full loop-guide/architecture
backfill, reference tables, authored examples/publication and final acceptance
remain open.

The **loop-guide backfill** now reviews all11 existing loop README/guide destinations
and the complete repository SEMANTICS/RUNTIME_INTEGRATION/FACADE_DESIGN documents.
[Loop foundations](loop-foundations-backfill.md) gives destination-specific stale-
claim corrections and preparation/command/composite/media walkthrough requirements;
[facade/state/export follow-ups](loop-runtime-backfill.md) covers current recovery,
configuration, coherent cuts, ownership and shutdown. `loop-guides-baseline.json`
records destination/source hashes and fresh coordinated_loop debug/release plus
warning-denied loop rustdoc. The example has sample/content/ownership assertions but
no allocator guard itself; guarded integration evidence is separately attributed.
The review also clarifies delivery-frame recovery, conceptual versus actual Cut
fields, caller-routed LoopId/RequestId, and explicit off-render import padding.
Full repository runtime architecture/audit/performance/fixture review, later-stage
host/benchmark changes and separate-worker publication remain outstanding.

The **repository/architecture review** now covers all seven specified runtime
contract/guide/audit/host/performance/evidence documents, three root proposal/design
documents, the governing inter-loop decision, current runtime README and generic
fixture README. [Repository follow-ups](repository-runtime-followups.md) distinguish
stale current bounds from historical evidence; [architecture follow-ups](architecture-followups.md)
map proposed APIs to implemented scope without changing the decision or exclusions.
`repository-baseline.json` records warning-denied rustdoc for both crates, the draft
book check, unchanged protected dirty drafts, nine historical reports, the golden
journal and DONE_INITIAL_CRATE. This is document review, not the required new full
callback audit, benchmark campaign or final-head CI check.

**This is not yet a complete Stage 9 handoff.** The current notes now cover reviewed
Stage0–6 implementation impacts across the book, loop guides and named repository
cross-documents. Stage7–8 host/benchmark/audit/CI changes and subsequent fixes must
still be added, and the publication worker must implement/execute the pending
examples and final book/reference checks. Do not infer that empty chapters need no
changes, archives are current authoritative prose, or notes have been applied. No
collected fresh Miri pass, completed Stage6, loop AudioWorklet, final CI or review
approval is implied. Use current operational evidence, not archived milestone totals.

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
| 5 — Ownership | [Backfill reviewed](part-2-control-a-running-graph/05-commands-completions-and-ownership.md): owning returns, separate credits/receipts, resource buffers, cancellation and recovery preemption; publication/example pending. |
| 6 — Live edits | [Backfill reviewed](part-2-control-a-running-graph/06-routing-and-live-graph-edits.md): ordinary fan-in/out, incremental publication, generations and owner recycling; new two-node example/publication pending. |
| 7 — Exact parameters | [Reviewed, preserve approved prose](part-2-control-a-running-graph/07-changing-parameters-at-an-exact-sample.md): fresh guarded gain_graph debug/release, source excerpts/navigation verified; final publication gate remains. |
| 8 — MIDI | [Backfill reviewed](part-2-control-a-running-graph/08-processing-and-scheduling-midi.md): complete packets, stable ties, bounded ingress and reserved host recovery/backpressure; new examples/publication pending. |
| 9 — Resources | [Backfill reviewed](part-3-work-with-stored-media/09-loading-and-reading-resources.md): upload/ownership, required versus background access, raw-ID preconditions and bounded cleanup; chapter/example publication pending. |
| 10 — Record/export | [Backfill reviewed](part-3-work-with-stored-media/10-recording-and-exporting.md): immutable truthful prefixes, capture revisions, exhaustion and single-resource versus group atomicity; chapter/example publication pending. |
| 11 — MIDI events | [Backfill reviewed](part-3-work-with-stored-media/11-seeking-and-reading-midi-events.md): page versus event positions, complete packet retry, ties/EOF and view lifetimes; chapter/example publication pending. |
| 12 — MIDI state | [Backfill reviewed](part-3-work-with-stored-media/12-midi-state-and-checkpointed-seeking.md): concrete checkpoint reference, C×K−1 capacity, pre-target state, unsupported histories and renderer attribution; chapter/example publication pending. |
| 13 — Planning | [Backfill reviewed](part-4-coordinate-processors-and-changes/13-planning-processing-intervals.md): complete plan/process/finish lifecycle, authority, positive intervals and one callback envelope; guarded two-consumer example/publication pending. |
| 14 — Typed operations | [Backfill reviewed](part-4-coordinate-processors-and-changes/14-application-specific-operations.md): four frame identities, retained receipts, queued/pending cancellation, domain outcomes and recovery limits; public pending example/publication pending. |
| 15 — Graph activation | [Backfill and loop integration reconciled](part-4-coordinate-processors-and-changes/15-staging-and-coordinating-graph-activation.md): scoped permits, actual storage, MIDI conflicts and independent retirement; generic tutorial/publication pending. |
| 16 — Capture groups | [Backfill reviewed](part-4-coordinate-processors-and-changes/16-recording-resources-together.md): first-interval preflight, reservations/claims, staged common-prefix publication, stamped authority and explicit cleanup; chapter/example publication pending. |
| 17 — Capacity | [Stage3–6 backfill reviewed](part-5-configure-and-operate-the-runtime/17-capacity-work-budgets-and-diagnostics.md): typed/pool/graph/group/index formulas, callback allowances versus required work, diagnostics and bounded smoke limitations; publication/reference verification pending. |
| 18 — Prepared ingress | [Backfill reviewed](part-5-configure-and-operate-the-runtime/18-submitting-without-allocation.md): distinct flag/admission credits, owning-buffer reuse, inactive plan materialization versus validation, finite recovery and destructor limits; new example/publication pending. |
| 19 — Shutdown | [Backfill reviewed](part-5-configure-and-operate-the-runtime/19-stopping-disconnecting-and-reclaiming-ownership.md): graceful versus abandoned work, complete owner ledger, resource/planner extraction, and separately attributed loop quiescence; new example/publication pending. |

## Archive coverage review and remaining publication

- `getting-started.md`, `integration.md`: chapters1–4 reviewed in the Part I notes;
  startup/processor/host capabilities are accounted for; prepared ingress/extraction/
  teardown is reviewed in18–19. Commands/live edits and planner/coordination
  backfill is now reviewed in5–6/13–15. Do not copy obsolete held-validation/
  recovery ordering from the archive.
- `scheduling-midi.md`, `changing-parameters.md`: backfill reviewed in5/7–8/14/17–19;
  approved parameter prose preserved, packet/reset and pending-receipt examples
  specified but not newly implemented.
- `resources.md`: upload/access/index/capture/export ownership reviewed for9–11/16;
  consolidated formulas, prepared ingress and shutdown backfill now reviewed in17–19.
- `midi-indexing.md`, `midi-state.md`: event/state/initial/checkpoint/ownership policies
  reviewed for11–12/16; chapter17 now supplies the sizing/counter backfill. The
  separate worker must verify the final reference tables and examples.
- `capture-groups.md`: take/interval/first-reservation/common-prefix/lifetime/cleanup
  review covers10/16. Consolidated sizing/ingress/shutdown is reviewed in17–19;
  chapter13 now covers planner integration. Do not copy obsolete blanket claims
  of missing loop integration.
- `planning.md`: introductory processor/callback effects reviewed for chapters3–4,
  capture/accounting/cleanup in16–19; complete planner/receipt/activation backfill
  now reviewed in13–15, with explicit current example limitations.
- `graph-activation.md`: backfill reviewed in5–6/8/13–15/17–19, including current
  prepared-template loop integration attribution. Historical milestone evidence is
  not current final acceptance; preserve its provenance and use operational ledgers.
- `performance.md`: chapters17–19 backfill now reviewed, with current formulas,
  held-normal recovery correction, required/background work and measurement limits.
  Separate-worker supporting reference pages and final benchmark evidence remain.

[Cross-cutting and other-document follow-ups](cross-cutting.md) cover README,
rustdoc, references, architecture and loop-specific attribution. Loop-library
user-guide and named repository document backfill are now covered by the four
indexed notes above. Later Stage7–8 implementation impacts and subsequent fixes
still need review; actual callback audit/performance/host evidence and publication
remain separate. Do not mark the handoff or final documentation complete before
those inputs, new examples and reference/publication checks exist.
