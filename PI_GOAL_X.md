# Planning, execution, and auditing: write_plan.md versus pi-goal-x

## Conclusion

The overlap is real: `.agents/prompts/write_plan.md` defines a planning and execution discipline, while pi-goal-x also defines a planning and execution discipline with its own objective, task tree, completion requirements, and progress records. Using both without a clear division of responsibility creates two representations of the same work that the executor must keep synchronized.

The preferred architecture is:

> The Markdown plan defines the work. The extension observes and coordinates execution. The auditor decides whether the approved outcomes have actually been achieved.

Keep the existing planning guidelines and make the resulting Markdown plan authoritative. Add a narrow, plan-linked execution extension that references that document, displays truthful progress, coordinates continuation and interruption, and independently audits completion. Do not introduce a second independently editable plan.

pi-goal-x can approximate this workflow as an interim solution, but its underlying ownership model does not fit it naturally. A small custom extension is preferable to a large fork unless later investigation identifies a clean upstream integration path.

This document records an architectural review, not an implementation plan. No extension implementation, configuration change, goal creation, or goal focus change was performed during the review.

## Review scope

The review examined:

- `.agents/prompts/write_plan.md`.
- The installed `pi-goal-x` npm package, version **0.34.0**, including its implementation rather than only its README.
- Relevant Pi documentation for extensions, prompt templates, terminal UI, packages, and SDK sessions.

Implementation references below are relative to the installed package root:

```text
~/.pi/agent/npm/node_modules/pi-goal-x/
```

The installed implementation is the basis for behavioral findings. Some package documentation and comments do not exactly describe all current source behavior. Future versions should be rechecked before relying on these details.

## 1. What each system is

### write_plan.md: a workflow specification

The prompt asks the agent to investigate the request and relevant code, then produce an actionable implementation plan without implementing it yet.

It distinguishes three categories of content:

| Category | Rules |
| --- | --- |
| Goals, scope, and acceptance criteria | Must be explicit; goals and acceptance criteria cannot change without explicit user approval. |
| Design rules and important constraints | Must be documented; design rules may change for a documented, well-supported reason. |
| Implementation stages | Checkbox-based, ordered, dependency-aware, and verified per stage; implementation steps may change when new evidence warrants it. |

The execution contract requires:

- Keeping the plan updated and checking off completed work.
- Committing each completed stage or meaningful milestone.
- Verifying every stage.
- Performing final end-to-end validation.

Unless instructed otherwise, the plan must also include:

- Branch creation.
- Pushing changes.
- Creating a PR.
- Working until CI is green.
- Checking for automated review and repeatedly addressing feedback as appropriate until the reviewer approves.

The result is a human-readable engineering contract. The prompt itself does not provide automatic continuation, persistent session focus, a progress widget, mechanical change protection, independent verification, or interruption recovery. Execution depends on the agent following the document.

### pi-goal-x: a persistent agent runtime

pi-goal-x is substantially more than a completion auditor. It provides:

- Guided goal clarification and proposal confirmation.
- A persistent objective and success criteria.
- A hierarchical task plan.
- Goal-level and task-level verification contracts.
- Task progress and recorded evidence.
- Automatic continuation.
- Pause, resume, block, completion, and budget-limited states.
- Multiple open goals with session-local, human-owned focus.
- Compaction resynchronization and provider-error recovery.
- Token/time accounting and optional execution limits.
- An independent completion auditor.
- A persistent dashboard and expanded task view.
- An optional separate-agent Oracle for blocker advice.

This is a complete workflow, not merely an execution backend for an external Markdown plan.

## 2. Capability comparison

| Area | write_plan.md | pi-goal-x |
| --- | --- | --- |
| Investigate before planning | Explicitly required. | Targeted research is allowed during guided drafting. |
| Planning output | Human-readable implementation plan. | Goal objective plus structured task tree. |
| Goals and acceptance criteria | Explicit and protected by user-approval policy. | Success criteria in the objective; objective changes are routed through confirmed tweaking. |
| Separate design rules | Explicitly distinguished from acceptance criteria. | Generally prose inside objective or constraints. |
| Adapt implementation steps | Agent may revise based on evidence. | Structural task changes require confirmation. |
| Dependencies and ordering | Must be explicit. | Task tree and list ordering; no dependency graph in the task model. |
| Stage verification | Required for every stage. | Optional task contracts; contracted tasks require completion evidence. |
| Final end-to-end validation | Required. | Must be expressed in the objective or contract; no project-specific default. |
| Branch, commits, PR, CI, automated review | Included by default. | Only required when included in the goal or its requirements. |
| Progress updates | Markdown checkbox edits. | Task-progress tool calls. |
| Automatic continuation | None. | Built in. |
| Independent completion review | None. | Built in unless disabled or bypassed. |
| Cross-session persistence | Plan file. | Goal files, ledger, and session focus. |
| Always-visible status | None. | Dashboard while tracking a goal. |
| External authoritative plan | The document is the natural authority. | No first-class plan-file binding. |

### Revision authority is the central mismatch

The planning prompt freezes outcomes while allowing justified changes to methods:

- Goals and acceptance criteria need user approval to change.
- Design rules need documented supporting reasons to change.
- Implementation steps can adapt to new evidence.

pi-goal-x's active-goal policy describes the entire objective as immutable and directs the agent to ask the user to use `/goal-tweak`. Structural task replacement also requires confirmation.

Copying the entire implementation plan into the objective therefore places adaptable methods inside an ostensibly immutable goal. The two systems disagree about which changes the executor may make independently.

These protections also have different strengths. Prompt instructions guide agent behavior; they are not equivalent to filesystem-level enforcement against arbitrary edits through shell tools.

## 3. Where duplication originates

### 3.1 A second planning pass

The guided `/goal` flow asks the agent to establish:

- Objective.
- Success criteria.
- Boundaries.
- Constraints.
- Verification contract.
- Blocker policy.
- Tasks for decomposable work.

These substantially overlap with the existing planning prompt. The drafting prompt explicitly instructs the agent to include tasks in a proposal when the work naturally decomposes into trackable milestones. Feeding it an existing substantial plan naturally produces another structured plan.

Reference: `extensions/goal-draft.ts`, `goalDraftingPrompt`.

### 3.2 A second checklist

The Markdown document has checkboxes. pi-goal-x has separate task objects with IDs, status, evidence, and subtasks. There is no built-in synchronization with an arbitrary external plan file.

The executor can consequently end up having to:

1. Perform the work.
2. Update the plan document.
3. Update the goal task tree.
4. Reconcile differences between the two.
5. Explain discrepancies to the auditor.

This is genuine coordination overhead and creates opportunities for drift.

### 3.3 Repetition within goal storage

The serialized goal file includes:

- A JSON metadata object containing the objective and structured task state.
- A `# Goal Prompt` section repeating the objective.
- Generated progress and task sections.

This is deliberate machine-readable plus human-readable serialization. It becomes particularly conspicuous when the objective contains an entire external plan.

The representations are not freely interchangeable: the parser obtains task state from metadata, while specially extracting the objective from `# Goal Prompt`. Editing generated task checkboxes is not equivalent to updating the structured task state.

Reference: `extensions/storage/goal-files.ts`, `serializeGoalFile` and `parseGoalContentCached`.

### 3.4 Context repetition is a separate concern

The installed version mitigates an older issue where automatic continuation persisted the full goal prompt on every checkpoint. It now uses tiny continuation markers and request-time goal-state injection with bounded excerpts and retrieval tools.

Distinguish:

- Workflow duplication between the external plan and the goal: still present.
- Persisting the entire goal on every automatic continuation: specifically mitigated in this version.

The injected objective excerpt is capped at 1,500 characters, with instructions to retrieve omitted content. Copying a long plan into the objective does not make the full plan continuously visible to the executor.

Reference: `extensions/prompts/goal-prompts.ts`.

## 4. Independent completion auditing

### What is worth preserving

When the executor requests completion, pi-goal-x starts a separate in-memory Pi agent session. The auditor receives:

- The goal objective.
- The goal verification contract, when enabled and present.
- A task-state summary.
- Limited recent parent context.
- An explicitly untrusted executor completion claim.

The auditor can inspect the workspace and run verification commands. Its instructions emphasize that:

- Executor claims are not evidence.
- Build success and task counts alone do not prove completion.
- Missing or weakly verified requirements warrant rejection.
- Actual user-facing outcomes matter.

Approval requires a final `<approved/>` marker. Rejection, missing markers, errors, and aborts do not constitute approval. Rejection feedback returns to the executor for rework. Explicit user-controlled disabling and bypass paths also exist.

This is a useful completion gate and a good model for the proposed custom extension.

References: `extensions/goal-auditor.ts` and `extensions/goal-completion.ts`.

### Limitations and caveats

#### Independent context does not necessarily mean a different model

The auditor runs in a separate session but defaults to the executor's model unless configured otherwise. Independence is primarily contextual, not guaranteed model diversity.

#### Read-only behavior is instructed, not sandbox-enforced

The auditor has `bash` and is told never to mutate files. This is not an operating-system-level read-only environment. Verification commands themselves can also have side effects.

#### Project guidelines are excluded by default

The default isolated resource loader supplies no project agent files, skills, or extensions. An opt-in `auditorProjectResources` setting enables broader project-resource loading.

For a custom implementation, prefer deliberately passing selected project rules rather than loading all extensions into the auditing session.

#### The main task block is not a complete task evidence package

The current auditor task renderer includes task titles and statuses, but not each task's complete verification contract and evidence. Some evidence arrives through recent ledger context, but that window is not an exhaustive task-by-task record.

A custom implementation should pass every acceptance criterion and its associated evidence explicitly.

#### Planning conventions are not automatically audited

Branch creation, milestone commits, PR creation, CI, and automated review must appear in the supplied requirements or explicitly referenced plan. The auditor does not inherently know the project's planning conventions.

#### Completion auditing is not continuous stage auditing

The normal audit happens when completion is requested. It should not be confused with independent verification of every task transition.

## 5. Progress reporting: capability versus meaning

pi-goal-x already has a persistent dashboard showing goal status, task progress, current task, subtasks, time, tokens, and audit activity. An expanded view exposes more task details, evidence, and recent activity.

The presentation is useful, but activity, reported completion, and verified completion must be distinguished.

### 5.1 Task progress depends on executor bookkeeping

The executor must call `update_goal_task`. The extension does not establish that a milestone is actually complete from code changes or test results.

Contracted tasks require nonempty completion evidence, but providing an evidence string is not an independent stage audit.

### 5.2 Skipped tasks count toward done

The dashboard percentage is based on top-level tasks that are complete **or skipped**. Subtasks have separate progress indicators.

A 100% task display therefore means the top-level task list has been resolved, not that every originally requested activity was performed and independently accepted.

The tool/prompt task summaries can also count all tree nodes, whereas the main dashboard progress uses top-level milestones. These numbers describe different units.

Reference: `extensions/widgets/goal-dashboard-model.ts`, `deriveTopLevelTaskProgress`.

### 5.3 Current task may be inferred

If no valid current task was explicitly started, the dashboard falls back to the first pending task. The expanded view discloses that inference. It is a useful default, not proof of the executor's actual current activity.

Reference: `extensions/widgets/goal-dashboard-model.ts`, `deriveCurrentTask`.

### 5.4 Audit percentages and check stages are heuristic

The audit progress estimate advances based on tool activity:

| Event or tool | Display estimate |
| --- | --- |
| Read, list, or find | 25% |
| Grep | 50% |
| Bash | 75% |
| Report production | 90% |
| Finished | 100% |

The estimate is kept from regressing as these events occur. The dashboard maps percentage bands onto apparent check-stage advancement. These are not actual independent verdicts for individual requirements or check stages.

Prefer truthful activity reporting:

```text
Auditing · running verification command · elapsed 2m14s · verdict pending
```

Do not imply that a known fraction of requirements has passed merely because particular tools were used.

References: `extensions/goal-auditor.ts`, `estimateAuditProgress`; `extensions/widgets/auditor-dashboard-model.ts`, `deriveAuditorDashboardModel`.

### 5.5 Stall detection is not an independent watchdog

The optional stall timeout is disabled by default. Its check runs during context construction, rather than in an independent monitoring loop. It does not guarantee a fresh status update when execution is genuinely stuck.

Reference: `extensions/goal-state.ts`, `checkStall`.

### 5.6 Completed task state is unusually rigid

The task-progress API can reopen skipped tasks, but not completed tasks. This is awkward when later integration testing or audit feedback invalidates a previously completed milestone.

A custom executor should support explicit reopening with a reason and invalidate affected verification state rather than treating completion bookkeeping as irreversible.

Reference: `extensions/goal-task-tools.ts`.

## 6. Interim approaches using pi-goal-x unchanged

### Option A: reference the plan and disable goal tasks

Use direct creation rather than guided drafting, with a short objective such as:

> Execute `FEATURE_PLAN.md` according to its execution contract. Preserve its goals and acceptance criteria. Completion requires its final validation and delivery obligations to be satisfied. The auditor must read that document.

Disable pi-goal-x task tracking.

Advantages:

- Avoids the second planning conversation.
- Avoids a duplicate task tree.
- Retains automatic continuation and final auditing.

Limitations:

- Status, elapsed time, and tokens remain visible, but stage-completion progress is absent.
- The plan reference is prose, not a tracked binding.
- Approved acceptance criteria are not pinned against accidental weakening.
- The auditor must be instructed to read the plan.

This is a reasonable temporary workaround, not a clean integration.

### Option B: retain only coarse goal milestones

Keep detailed checkboxes exclusively in the plan. Track only a few coarse milestones in pi-goal-x, such as implementation, integration verification, final validation, and PR/CI/review.

Advantages:

- Provides useful dashboard progress.
- Duplicates less content than importing the entire checklist.

Limitations:

- Two progress records still require synchronization.
- Structural task revisions still follow pi-goal-x's confirmation policy.
- Stage state can drift from the document.

`/goal-direct` does not automatically create a tracked task tree. The guided proposal path can derive tasks from checklist or numbered lines, but that derivation is a simple flattened extraction, not a semantic plan importer. It does not preserve the full meaning of stages, nesting, dependencies, and verification sections.

References: `extensions/goal-commands.ts`, `handleDirectGoalSet`; `extensions/goal-drafting.ts`; `extensions/goal-task-derive.ts`.

Neither option justifies copying the full plan into the objective merely to obtain progress reporting.

## 7. Preferred architecture: a plan-linked executor

### 7.1 One authoritative plan

Bind execution to a plan path instead of translating the plan into a separate goal objective and independently editable task tree.

The plan owns:

- Goals and acceptance criteria.
- Design rules and constraints.
- Ordered stages and dependencies.
- Checkboxes.
- Verification requirements.
- Execution and delivery obligations.

The extension owns runtime metadata:

- Focused plan identity and path.
- Current stage or item.
- Running, paused, waiting, blocked, auditing, or completed state.
- Latest activity and timestamps.
- Evidence references and audit results.

Any parsed task representation is a derived view, not a second plan authority. Prefer storing references to stable plan item identities rather than duplicating their prose. Supporting that may require a small, explicit plan-format convention; the current planning prompt does not define machine-stable item IDs or a parsing schema.

### 7.2 Preserve outcome immutability and method flexibility

Apply the existing revision rules faithfully:

- Goal or acceptance changes require explicit user approval.
- Design-rule changes require documented supporting reasons.
- Implementation changes may follow new evidence.

A fingerprint of the approved goals and acceptance criteria can detect changes. A hash does not prove authorization: accepting a changed baseline must be an explicit user action.

There is no need to freeze every implementation detail or maintain a second live copy of the whole plan. Historical audit snapshots, if retained, must be clearly identified as evidence, not alternate authorities.

### 7.3 Separate progress, activity, and verification

An illustrative persistent display:

```text
FEATURE_PLAN.md · Running
Stages: 3/7 complete · Items: 18/42 checked · 1 skipped
Current: Stage 4 — Integration verification
Activity: pytest running · 2m14s elapsed
Last milestone: Stage 3 verified and committed · 11m ago
Audit: Not yet requested
```

Required semantics:

- Display checked and skipped work separately.
- Distinguish current activity from completion.
- Do not present stage counts as estimates of remaining time or effort.
- Show elapsed time for long-running tools.
- Distinguish waiting for CI or external input from active execution.
- Describe stale telemetry as no recent activity, not as continuing progress.
- When no plan is selected, show that tracking is inactive rather than inventing a percentage or selecting a plan autonomously.

Activity should update automatically from Pi events. Semantic milestone completion still needs an explicit executor record: a command finishing does not establish that a stage is done.

The persistent UI must not depend on the model remembering to narrate progress. Non-TUI operation should retain useful status through appropriate text or structured outputs rather than requiring a terminal widget.

### 7.4 Independent auditing against approved outcomes

Give the auditor:

- The current plan.
- The approved acceptance baseline.
- Relevant project execution rules.
- Evidence references for every criterion.
- The executor's completion claim, explicitly untrusted.
- The workspace revision and relevant working-tree state being audited.
- PR, CI, and review state when required by the plan.

Report each acceptance criterion as:

- Satisfied.
- Unsatisfied.
- Unverifiable.

Then produce an overall verdict. Audit failure must never be repaired by weakening acceptance criteria.

Tie approval to the inspected revision and relevant state. Later changes must not silently reuse stale approval. A completed milestone should be reopenable with an explanation when new evidence invalidates it.

For external systems, report waiting and environment failures honestly. The extension must not manufacture review approval or treat unrelated CI success as validation of the current revision.

### 7.5 Narrow runtime scope

Retain:

- Explicit human-owned focus.
- Pause/resume and safe cancellation.
- Optional automatic continuation.
- Durable progress.
- Compaction resynchronization.
- Independent completion audit.
- Persistent, truthful status.

Initially omit:

- Another goal-drafting conversation.
- A second editable task hierarchy.
- Sisyphus as a separate user concept.
- Blocker Oracle.
- Synthetic audit percentages.
- Token-budget machinery unless specifically needed.

Sisyphus in the reviewed implementation is primarily a prompt/criteria style, not a separate step-counter state machine. Ordered execution can instead follow the plan's explicit ordering and dependencies.

Lifecycle safety is still substantial work. pi-goal-x addresses interruption, persistence, focus changes, stale continuations, concurrent writers, accounting, and recovery. A smaller extension should learn from those mechanisms rather than assume a continuation loop and a widget are sufficient.

## 8. Recommendation

### Short term

Use a small plan-referencing goal. Either disable task tracking or retain only coarse milestones, accepting the remaining synchronization cost. Avoid another guided planning pass for an already-approved plan.

### Long term

Build a small plan-linked extension instead of maintaining a large pi-goal-x fork. Borrow the independent-auditor approach and lifecycle safety lessons, but do not inherit a second planning authority.

Keep `.agents/prompts/write_plan.md` as the source of planning policy. Extend it only where a deliberately chosen machine-readable convention is needed for reliable item identity, progress derivation, and protected acceptance sections. Do not replace its useful distinction between immutable outcomes and adaptable implementation methods.

The intended division of responsibility is:

| Component | Responsibility |
| --- | --- |
| Planning guidelines | Define how a good plan is investigated and written. |
| Markdown plan | Authoritative outcomes, constraints, implementation stages, and checklist. |
| Execution extension | Track focus and activity, coordinate execution, persist runtime metadata, and display progress. |
| Independent auditor | Check real evidence against approved outcomes and required delivery obligations. |
| User | Approve outcome changes, select focus, and control exceptional bypasses. |

This retains the strongest parts of both approaches without asking the executor to keep two plans consistent.
