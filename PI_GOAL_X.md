# Planning/execution tooling review — archived

The maintained conclusion is in
[ARCHITECTURE.md — planning and execution tools](ARCHITECTURE.md#planning-and-execution-tools):
one authoritative Markdown plan, native lifecycle/activity and an independent
acceptance auditor. A replacement extension remains proposed, not implemented.
Current native-record interpretation is in [PLAN.md](PLAN.md#native-execution-records).

Full installed-version analysis, caveats and alternatives:
[offline archive](docs/history/README.md) ·
[immutable source](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/PI_GOAL_X.md).
No native goal focus, lifecycle or storage is changed by this migration.

<a id="planning-execution-and-auditing-write_planmd-versus-pi-goal-x"></a>
<a id="conclusion"></a>
<a id="review-scope"></a>
<a id="1-what-each-system-is"></a>
<a id="write_planmd-a-workflow-specification"></a>
<a id="pi-goal-x-a-persistent-agent-runtime"></a>
<a id="2-capability-comparison"></a>
<a id="revision-authority-is-the-central-mismatch"></a>
<a id="3-where-duplication-originates"></a>
<a id="31-a-second-planning-pass"></a>
<a id="32-a-second-checklist"></a>
<a id="33-repetition-within-goal-storage"></a>
<a id="34-context-repetition-is-a-separate-concern"></a>
<a id="4-independent-completion-auditing"></a>
<a id="what-is-worth-preserving"></a>
<a id="limitations-and-caveats"></a>
<a id="independent-context-does-not-necessarily-mean-a-different-model"></a>
<a id="read-only-behavior-is-instructed-not-sandbox-enforced"></a>
<a id="project-guidelines-are-excluded-by-default"></a>
<a id="the-main-task-block-is-not-a-complete-task-evidence-package"></a>
<a id="planning-conventions-are-not-automatically-audited"></a>
<a id="completion-auditing-is-not-continuous-stage-auditing"></a>
<a id="5-progress-reporting-capability-versus-meaning"></a>
<a id="51-task-progress-depends-on-executor-bookkeeping"></a>
<a id="52-skipped-tasks-count-toward-done"></a>
<a id="53-current-task-may-be-inferred"></a>
<a id="54-audit-percentages-and-check-stages-are-heuristic"></a>
<a id="55-stall-detection-is-not-an-independent-watchdog"></a>
<a id="56-completed-task-state-is-unusually-rigid"></a>
<a id="6-interim-approaches-using-pi-goal-x-unchanged"></a>
<a id="option-a-reference-the-plan-and-disable-goal-tasks"></a>
<a id="option-b-retain-only-coarse-goal-milestones"></a>
<a id="7-preferred-architecture-a-plan-linked-executor"></a>
<a id="71-one-authoritative-plan"></a>
<a id="72-preserve-outcome-immutability-and-method-flexibility"></a>
<a id="73-separate-progress-activity-and-verification"></a>
<a id="74-independent-auditing-against-approved-outcomes"></a>
<a id="75-narrow-runtime-scope"></a>
<a id="8-recommendation"></a>
<a id="short-term"></a>
<a id="long-term"></a>

[Continue in the unified architecture](ARCHITECTURE.md#planning-and-execution-tools).
