# Chapter 4 review: callback envelope, half-open boundaries and fault policy

Destination: `docs/part-1-process-audio/04-connecting-an-audio-callback.md`.
The complete chapter, `callback_adapter.rs`, current executor/clock/planner paths
and archived integration/planning text were read. Hashes/checks are in
`../part-one-baseline.json`. Keep the existing small guarded adapter: its invalid,
zero, prefix-clearing, non-rollback and continued `NoPlanner` calls all pass in
native debug and release.

## Apply these clarifications without growing a device tutorial

1. Distinguish the host callback from its internal intervals. Nonempty callback
   validation/clock advance/queue snapshot and allowance reset happen once. Due
   operations precede `plan`, then consumers render the selected interval and
   `finish` consumes observations. Repeat internally without refreshing ingress,
   reclamation, validation, recovery, transfer or telemetry allowances. Link
   chapters13/17 for the full lifecycle and accounting.
2. State the half-open cut at its first callback explanation: an action at the
   callback end affects the next nonempty callback's first sample/event, not the
   previous block. `finish` does not settle that next boundary. A zero-length call
   cannot be used as a flush/activation trick. Preserve the existing correct
   zero-call exemption from validation and clock/control progress.
3. `PlanContext::callback_start` remains the original host start while `frame`
   changes across intervals. It is a callback identity for private allowances,
   not a second musical clock. A planner must return positive progress within the
   external ceiling. There is no arbitrary subrange quota or host-imposed fixed
   musical quantum; finite same-frame settlement belongs to the domain planner.
   Keep these details behind the chapter13 link if they interrupt the adapter flow.
4. Qualify continued rendering after the example's deliberate error. `FailOnce`
   with `NoPlanner` is recoverable by that example's policy, **not a universal
   promise that every processor/planner can resume after a fault**. A domain can
   already have published a graph/program/media cut before rendering fails. Its
   planner may retain an unfinished committed interval; then stop callbacks and
   use its explicit off-render extraction/quiescence cleanup. Never fake `finish`,
   replay the failed frame or roll back published ownership. The loop facade's
   permanent quiescence is domain policy, not a new generic runtime method.
5. Reading diagnostics between callbacks is correctly restricted to the render
   owner. Cross-thread publication must use bounded copies; telemetry refusal
   cannot gate music. Keep reliable owning terminal results separate from lossy
   diagnostics and leave result recycling to chapters5/14/19.
6. Retain the browser boundary warning. Rust guards do not cover MessagePort,
   JavaScript allocations or output delivery. The generic host fixture is not
   evidence for the still-required independent loop AudioWorklet.

## Checks and archive corrections

The example already checks exact counts/prefix behavior, invalid frame/shape before
clock progress, zeroFrame999 as a no-op, fault after advancement to4 and successful
continuation at4..6. Do not replace those assertions with a success-only run.
`initial_graph`, planner execution and facade fault/cleanup tests cover the additional
boundaries; the current Stage6 paired/configuration lifecycle tests prove publication
is not rolled back by a later processor error. No new callback adapter is needed
solely for these prose refinements.

Do not copy the archived integration/planning claim that held normal validation
blocks recovery. The transport parks held normal work for eligible snapshotted
recovery, resumes without another dequeue/cancellation check, and preserves the
normal validator. Recovery still has finite independent credit and bounded work;
this is not unlimited fairness. Link the existing chapter5/19 change notes.
