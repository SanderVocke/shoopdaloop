# Miri jobs and outstanding verification

Miri runs asynchronously under the [plan's execution contract](../../PLAN_LOOP_LAYER.md#background-miri-execution-user-approved-scheduling-change).
Pending verification does not prevent implementation commits or further development;
it does prevent claiming an applicable stage fully verified or making final
delivery without the required passing evidence.

Default cadence is **one consolidated run per applicable numbered-stage
milestone**, not per smaller step or commit. Accumulate new/affected cases and
necessary regressions until that stage's implementation is ready. Use focused
extra runs only for actual Miri failures, concrete safety regressions or relevant
post-snapshot changes requiring refreshed evidence, with a recorded reason.
Do not repeatedly launch full/package suites during ordinary iteration. Final
coverage remains required; a complete stage run can also satisfy it only with a
verified match of the full required inventory and relevant inputs.

## Job record requirements

Each new job records:

- Job ID and state: queued, running, passed, failed, interrupted, cancelled or
  superseded. A missing supervisor/result is not success.
- Immutable source snapshot/commit, relevant dependency and runner/configuration
  hashes, pinned toolchain, and reproduction command.
- Package/target/filter/seed inventory and expected execution counts.
- Supervisor/process identity, isolated build/report paths, start/end time,
  monotonic wall duration, exit status and per-case logs.
- Verified artifact hashes, source applicability, any superseding job, and the
  milestone/final acceptance obligations it closes or leaves open.

Inspect jobs at natural development checkpoints, not in a blocking tool call or
a tight polling loop. Only promote a result to passing evidence after validating
the terminal result, complete expected inventory and source/artifact identities.
Changes made while a job runs belong to a later snapshot. Accumulate any needed
reruns for the next applicable stage/final verification barrier rather than
launching a new job after every edit.

## Outstanding work at the scheduling change

| Job / evidence | State | Verified scope | Outstanding obligation |
| --- | --- | --- | --- |
| `facade-topology-stage6`, interrupted foreground invocation | Interrupted; orphan process group terminated | 171 completed selected executions: 119 runtime and 52 loop, including all six new `runtime_topology` cases | Seven selected loop executions were not completed/reported; no full 178-execution pass or whole-invocation wall duration exists. No immediate repeat/resume is scheduled solely for this smaller step; include required coverage in the consolidated Stage 6 run |
| Background supervisor/snapshot support | Not implemented yet | Plan approved; no claim of a running background job | Implement/test durable launch, isolation, status and collection before relying on unattended runs |
| Final implementation Miri selection | Pending | Historical/milestone runs retain their original scope | Run and collect the complete required final selection against the final implementation snapshot; reconcile later changes and final-head CI |

The interrupted invocation used the working-tree candidate based on `9bab9ddf`,
not a committed final implementation. Its 104 Rust source hashes are recorded in
`evidence/facade-topology-stage6.json` and were rechecked during interruption
cleanup. The broad 25-gate pass is separate from Miri completion.

Local interruption archive:
`target/loop-layer-facade-topology-stage6/miri-interrupted/partial.json`, with
completed case logs, driver output, runner copy, source hashes and process-cleanup
record. The interrupted dense case's in-memory output was not recovered. The
remaining registered executions are:

1. `runtime`: `scenarios::dense_one_sample_media_keeps_all_128_boundaries_with_unread_external_results`.
2. `runtime`: `scenarios::facade_role_validation_invalid_operations_and_imported_revision_exhaustion_are_checked`.
3. `runtime`: `scenarios::integrated_restart_smoothing_and_gain_follow_decisions_not_host_cuts`.
4. `topology_views`: all three cases.
5. `interloop`: its single case.

Under the reduced cadence, defer routine completion/repetition of this smaller-step
selection to the consolidated Stage 6 run, expanding it for subsequent Stage 6
changes. The list above records missing historical evidence, not a requirement to
launch an immediate additional job. If a focused continuation is justified,
freeze a reproducible source snapshot first and confirm it matches the recorded inputs. A resumed batch must be reported separately from
the interrupted batch, not as a fresh uninterrupted 178-execution invocation.
If relevant inputs have changed, queue the affected cases against the new snapshot
instead. Keep all original completed and interrupted evidence.
