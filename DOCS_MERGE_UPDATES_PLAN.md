# Merge loop-layer documentation updates

Completed historical plan. The user subsequently requested consolidation of all remaining documentation work against the existing implementation, with no further handoffs/code inputs. The original handoff is now archived under `src/rust/rt_runtime/docs-previous/implementation-review/`; its preservation requirement below records the merge's original scope, not a prohibition on the later authorized relocation. See `DOCS_REWRITE_PLAN.md` and `docs-previous/remaining-documentation.md` (under the runtime crate) for the current work.

## Objective and scope

Reconcile the implementation review (originally `src/rust/rt_runtime/docs-changes-loop-layer/`, now `src/rust/rt_runtime/docs-previous/implementation-review/`) into three destinations:

1. Correct outdated or incomplete explanations in `src/rust/rt_runtime/docs-previous/`.
2. Apply narrow corrections to the completed book chapters 1–4, preserving the approved chapter 7.
3. Consolidate the remaining relevant requirements into `DOCS_REWRITE_PLAN.md` for the unfinished chapters and reference material.

This is direct documentation work: read the notes and relevant public source, edit prose, and review the results manually. Do not build, extend or use programmed documentation accounting/validation flows. Run an already finished example only if its code changes during this work.

The user has explicitly authorized correcting `docs-previous/`, superseding the earlier archive-untouched rule for these targeted edits. The prior instruction to work on the current branch without staging or committing remains in force.

Out of scope: finishing the remaining book chapters, creating new examples, changing runtime behavior, rewriting loop-engine guides or repository architecture/evidence documents, adding validation tooling, CI changes, branches/worktrees, commits, pushes and PRs. Keep the handoff folder as the original review input; do not delete or rewrite its historical notes or JSON evidence.

## Immutable acceptance criteria

Goals and acceptance criteria require explicit user approval to change.

- The three work groups above are complete. Applicable current contracts are reconciled rather than copied verbatim from chronological notes.
- All eleven previous guides have been considered. Only passages needing correction are edited; valid examples, accurate contracts and historical evidence retain their meaning.
- Chapters 1–4 retain their approved structure, tone and working examples. Chapter 7 receives no semantic rewrite unless current source establishes a specific correctness issue.
- The five-part, nineteen-chapter order and existing paths remain unchanged. Unfinished chapters are not filled with handoff instructions or implementation logs.
- `DOCS_REWRITE_PLAN.md` contains concise, current requirements for the remaining chapters/reference pages and acknowledges the user's approval of Part I.
- The rewrite plan no longer requires archive immutability or programmed documentation bookkeeping/validation. Historical records of already performed checks remain historical, not future obligations.
- Generic runtime capabilities remain distinct from loop-library integration. Supported prepared-template operations are not described as unrestricted processor/shape replacement or general rollback.
- No new scripts, manifests, hash records, coverage registries, automated link/snippet checks, CI flows or machine-readable documentation ledgers are introduced or extended. Existing such files are left alone; this merge does not depend on running them.
- If an existing example's code changes, the changed example runs successfully in native debug and release, and its displayed excerpts agree with the source. If no code changes are needed, no builds or example runs are required.
- Unrelated work, the active branch and Git index are left unchanged by this task. All edits remain uncommitted.

## Design rules and constraints

- Use current public code to resolve API/behavior questions. Use tests as readable references when useful; this task does not require executing the test suites.
- Treat the handoff as chronological change instructions, not replacement chapters or a set of equally current claims. Later implementation notes supersede earlier pending/unsupported statements.
- Retain dates and scope of actual evidence. Do not turn the handoff's native tests, Miri results or generic-host checks into claims of completed book examples, independent loop hosts, benchmarks or final acceptance.
- Preserve the approved style: direct openings, concise technical prose, short examples, no objectives/prerequisites/recaps or obvious “why not” discussion in teaching chapters.
- Put advanced detail in its intended later chapter/reference section. Earlier chapters need only the necessary correction and a link.
- Keep required render work distinct from optional/background allowances, semantic identities from saturating diagnostics, and runtime/resource revisions from loop-domain revisions.
- Explain runtime resource/capacity accounting where technically necessary; the prohibition is on constructing documentation bookkeeping systems, not on documenting library behavior.
- Do not expand this into the handoff's collateral loop-guide/architecture rewrite. Record those as separate follow-ups in ordinary prose, without implying they were completed.
- Do not remove existing validation tools or rewrite historical logs. Remove their role as mandatory future documentation workflow from the rewrite plan; use manual review instead.
- Follow repository writing/editing instructions. No runtime source, test, workflow or dependency changes are needed. If a claimed behavior cannot be established, stop and ask rather than patching runtime code or inventing a contract.

## Implementation stages

Execute in order. Stage 1 establishes the current interpretation used by both the archive corrections and future chapter requirements. Stage 5 reviews all three work groups together.

### Stage 1 — Reconcile the handoff's current meaning

- [x] Read the handoff index, cross-cutting notes, nineteen chapter notes and relevant loop/repository follow-ups. Re-read destination passages before changing them.
- [x] Resolve superseded claims: loop live routing, stopped-owner configuration and paired running-program/graph publication now have supported implementations; independent hosts and other outstanding verification remain separately scoped.
- [x] Confirm the generic recovery correction in current transport code: eligible callback-snapshotted recovery can park held normal work, preserving original cancellation observation, FIFO resumption and validator progress within finite budgets.
- [x] Check any other disputed public API facts directly in the relevant source. Do not generate a baseline report, coverage matrix, hash inventory or change-accounting script.
- [x] Verification: manually explain the current generic-versus-loop distinction and identify obsolete claims before editing. Record only unresolved questions needed for the merge in this plan.

### Stage 2 — Correct the previous guides

- [x] In `integration.md`, `planning.md` and `performance.md`, replace obsolete recovery-blocked-behind-held-normal wording. Keep finite admission/work/publication limits, late-cancellation semantics and lack of unconditional normal-lane fairness explicit.
- [x] Reconcile `graph-activation.md` with those corrections. Retain ordinary Commit versus Ready/permit activation, fallible preflight before publication, independent owning retirement, actual vector-capacity sizing and pending-MIDI conflicts. Correct stale loop-integration status without attributing loop transactions to the generic runtime.
- [x] In `resources.md` and `capture-groups.md`, clarify raw-ID versus scoped-handle obligations, exact lease ownership, required versus background access, physical resource revisions, immutable prefixes, first-interval reservations, final-lease promises and cleanup/extraction where the existing text is incomplete or misleading.
- [x] In `midi-indexing.md` and `midi-state.md`, preserve correct checkpoint/state contracts; clarify strict pre-target state, event/byte positioning, original equal-time packets, projection limitations and separate loop-renderer attribution where needed.
- [x] Review `scheduling-midi.md` for current timing/result/recovery behavior, including same-frame graph/injection conflicts and the distinction between immediate parameters and deferred typed outcomes. Prefer concise cross-links over duplicating the graph guide.
- [x] Preserve the valid live-Commit workflow in `getting-started.md`; do not replace it merely because the new book uses startup graph installation. Review its processor/fault/teardown wording for necessary qualifications. Preserve the approved `changing-parameters.md` unless a specific factual correction is established.
- [x] Keep historical verification paragraphs explicitly scoped. Correct current-status prose rather than relabeling old counts as fresh results. Check edited references manually.
- [x] Verification: read each edited guide in context and compare shared recovery, activation, resource, MIDI and shutdown statements across guides. Confirm there are no remaining contradictory current claims in the passages covered by the handoff.

### Stage 3 — Apply narrow corrections to completed chapters

- [x] Chapter 1: qualify the signal DAG, add a short planner/timing-authority bridge and appropriate forward links; retain the overview and diagrams.
- [x] Chapter 2: retain the startup constructor, example and assertions. Change only a specific factual or reference issue found on rereading; no expansion into advanced activation machinery.
- [x] Chapter 3: add concise links/explanations for borrowed decisions/observations, interval-relative MIDI, required media access and processor staging versus planner capture authority. Include typed handlers in applicable processing restrictions and avoid treating an arbitrary resource refusal as MIDI-overflow reporting.
- [x] Chapter 4: distinguish the enclosing callback from internal intervals, note callback-end/zero-call semantics, and qualify continuation after `FailOnce` as example-specific policy. Link later cleanup discussion for domains faulting after publication; do not suggest universal resumability or rollback.
- [x] Chapter 7: preserve its approved semantics, pacing and source marker. Recovery preemption does not remove normal admission lead time or add cancellation opportunities.
- [x] Change an existing example only if a concrete correctness issue demands it; do not add advanced features to the small Part I programs. Update affected excerpts by reading them alongside the actual source.
- [x] If example code changes, run each changed example directly in debug and release in the prescribed build environment, for example `nix develop --command cargo run --locked -p rt_runtime --example first_graph` and the equivalent with `--release`. Record actual results briefly. Do not use the book checker/example runner or run unrelated workloads. **Not applicable: no example code changed.**
- [x] Verification: manually read chapters 1–4 and 7 in sequence, ensuring corrections remain narrow and keep advanced explanations behind links. Check excerpts and edited link targets directly. If code is unchanged, explicitly record that no example execution was necessary.

### Stage 4 — Consolidate future requirements in the rewrite plan

- [x] Update `DOCS_REWRITE_PLAN.md` to record the user's Part I approval, the corrected previous guides as current reference material, and this intentional exception to archive immutability. Keep past integrity/check results as dated history only.
- [x] Replace future script/checker/hash/inventory/CI-accounting requirements with direct authoring and manual prose/source/link/excerpt review. Keep actual execution of runnable examples where new or changed example code requires it. Do not add another tracking file or edit the existing scripts/JSON manifests.
- [x] Consolidate chapter 5–8 requirements: owning returns and separate result lifetimes; parked-normal recovery and cancellation; ordinary routing versus staged activation; MIDI packet order and reserved host recovery. Preserve chapter 7's no-semantic-change finding.
- [x] Consolidate chapter 9–12 requirements: upload/read/capture/export ownership; required/background access; byte versus event positioning; truthful immutable prefixes; checkpoint capacity and strict pre-target state versus restoration.
- [x] Consolidate chapter 13–16 requirements: planner decisions/bindings/observations and callback-wide allowances; immediate/pending typed outcomes and delivery-frame recovery; activation permits/retirement/MIDI conflicts; prepared capture-group preflight/staging/freeze/cleanup.
- [x] Consolidate chapter 17–19/reference requirements: storage versus callback work, retained vector capacities, independent flag/result pools, prepared ingress, and graceful versus abandoned/post-fault ownership cleanup. Keep worked examples small and resource lifetime claims explicit.
- [x] Separate generic book requirements from loop-specific integration and collateral repository follow-ups. Retain useful handoff links rather than copying long test recipes, milestone totals or duplicate requirements into every chapter batch.
- [x] Keep specific example expectations as small teaching demonstrations, not an obligation to reproduce entire integration-test matrices. Where integration tests provide extra assurance, reference their scope without claiming the tutorial implements those workloads.
- [x] Verification: manually compare the revised chapter requirements with all nineteen chapter notes and relevant cross-cutting notes. Each applicable topic is either incorporated, already correctly covered, or explicitly left as a separate out-of-scope follow-up. Use ordinary editorial judgment, not a generated completeness ledger.

### Stage 5 — Final end-to-end editorial validation and handoff

- [x] Read the corrected previous guides, completed chapters and revised future requirements as one documentation set. Resolve disagreements about recovery, timing, activation, ownership, storage and fault cleanup.
- [x] Confirm that current loop capabilities are accurately attributed and that remaining host/benchmark/final-verification limitations are not accidentally declared complete. Resolve chronological note conflicts rather than copying both versions.
- [x] Manually inspect changed links, API names, commands and Rust excerpts against their actual targets/source. Preserve numbering and chapter scope. No scripted documentation checks or full build/test/benchmark campaign is required.
- [x] If example code changed, confirm its debug/release executions succeeded and its excerpts match. If no example code changed, state that no code execution was required for this prose-only merge.
- [x] Review the ordinary file diffs for accidental scope expansion or unrelated edits. Confirm the handoff inputs, runtime code, scripts, JSON accounting files and CI configuration were not changed by this task.
- [x] Record a concise outcome in this plan: which guides/chapters were corrected, what future requirements were consolidated, any changed-example results, and any remaining questions or separate follow-ups. Leave all work uncommitted on the active branch.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Record each completed stage or meaningful milestone in this plan; do not commit. This follows the user's explicit no-commit/current-branch instruction instead of the plan template's default milestone-commit rule.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

## Progress

- Planning completed before execution. The impact analysis found narrow changes for completed chapters, broader future requirements, and superseded chronological claims.
- Stage 1: read all Markdown handoff notes and all eleven previous guides, plus completed chapters 1–4/7. Confirmed parking/resumption in `transport.rs`, sequence-preserving validation and callback/interval/recovery boundaries in `executor.rs`, graph permit/storage checks in `graph/activation.rs`, required media access and capture checks, and the separate loop configuration/program-graph implementation. Generic permits do not supply loop transactions or cleanup. The newest handoff checkpoint records consolidated Stage 6 Miri completion; older pending statements are historical. Independent loop hosts and final acceptance are not thereby complete. No unresolved contract questions, scripts, builds or example changes at this stage.
- Stage 2: corrected `integration.md`, `planning.md`, `performance.md`, `graph-activation.md`, `resources.md`, `capture-groups.md`, `midi-indexing.md`, `midi-state.md`, `scheduling-midi.md` and `getting-started.md`. Read each corrected guide in context. Shared contracts now agree on bounded parked-normal recovery, publication versus readiness, capacity/ownership, MIDI delivery/state and non-rollback cleanup. The valid live-Commit program and historical measurements/counts retain their scope. `changing-parameters.md` needed no correction and is unchanged.
- Stage 3: updated only chapter 1 (signal/planner/settlement boundaries), chapter 3 (borrowed authority, media, interval MIDI and handler obligations) and chapter 4 (callback envelope and example-specific fault continuation). Re-read chapters 1–4/7 and the first_graph/prepared_processor/callback_adapter/gain_graph sources directly; startup/parameter chapters and all source markers/snippets remain unchanged. Also read planned_gain to confirm its one-consumer, unguarded, immediate-result scope. **No example code changed; no example executions, builds or test runs were required or performed.**
- Stage 4: revised `DOCS_REWRITE_PLAN.md` with current chapter/reference requirements, Part I approval, separate loop/repository/rustdoc/host follow-ups and explicit current/historical status. Replaced future archive-integrity, scripted checker/inventory/hash and CI/full-suite gates with direct review and execution of new/changed examples. Retained earlier actual verification results as history; did not relabel them as checks of this merge. Compared the consolidated requirements directly with the chapter and cross-cutting notes; fuller regression matrices stay in source/tests rather than becoming tutorial infrastructure.
- Stage 5: completed the end-to-end manual editorial review, source/API/excerpt comparison and changed-link/path/anchor inspection. Confirmed the five parts/nineteen filenames remain unchanged and future chapters were not filled with handoff logs. Ordinary diff/index/branch inspection showed no runtime/example/workflow edits or staged changes; the branch remains `feat/loop-execution-layer`. Existing tooling, JSON files, evidence and handoff inputs were not edited by this task. Concurrent changes to `PLAN_LOOP_LAYER.md` and the new `PI_GOAL_X.md`, along with pre-existing unrelated drafts, were left alone. No programmed documentation validation or accounting was introduced or executed.
- Outcome: all three merge work groups are complete, with no unresolved merge questions. Remaining chapter authoring, loop-guide/architecture/rustdoc reconciliation, independent hosts and later implementation acceptance are separate work explicitly identified in the rewrite plan. All edits are uncommitted.
