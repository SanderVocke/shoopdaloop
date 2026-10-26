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

Use [`scripts/run_rt_runtime_miri_jobs.py`](../../scripts/run_rt_runtime_miri_jobs.py)
for launch/status/collection; [MIRI_BACKGROUND.md](MIRI_BACKGROUND.md) documents the
commands, committed-snapshot requirement, Linux-only wrapper and failure handling.
No real background interpreter job has been launched for the subsequent smaller
program-probe, owning-transaction, facade-adapter or generic graph-permit steps.
The current required inventory is212=128runtime+84loop. The six new live-graph facade cases are registered, not interpreted. The interrupted partial evidence remains unchanged.

## Job record requirements

Each new job records:

- Job ID and state: queued, running, passed, failed, orphaned, interrupted, cancelled or
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
| Background supervisor/snapshot support | Implemented; 14 Python tests pass | Immutable committed exports, detached Linux supervisor, streamed output, exact inventory/hash collection and cancellation/descendant cleanup tested with synthetic fixtures | First actual interpreter use remains the consolidated applicable stage run; no real Miri pass is claimed by these infrastructure tests |
| Unpublished program-boundary probe prerequisite | Deferred to consolidated Stage6 run | Four new executions registered: one private library case and three `program_boundaries` cases; native/release/Node/Chromium/workspace gates pass | Include these and affected resolver regressions in the stage snapshot. Current selected inventory is182=119runtime+63loop, not a completed run |
| Standalone owning single-program transaction | Deferred to consolidated Stage6 run | Nine `program_changes` executions registered; native/release/Node/Chromium/workspace gates pass | Include the new owner/activation cases and affected resolver regressions in the stage snapshot. Current inventory is191=119runtime+72loop; no fresh interpreter pass is claimed |
| Fixed-graph facade owning program activation and permanent quiescence | Deferred to consolidated Stage6 run | Five new `runtime_topology` cases and one new `program_changes` case registered; native/release/Node/Chromium/workspace gates pass | Include all new and affected resolver/facade lifecycle cases. Current inventory is197=119runtime+78loop; no interpreter run for this slice |
| Fixed-role loop facade live graph and paired topology activation | Deferred to consolidated Stage6 run | Six additional guarded runtime_topology cases; loop137 native/Node/Chromium, workspace2125/four existing skips,19 scoped gates and7 positive/18 negative proofs pass | Include new cases and affected facade regressions. Inventory212=128runtime+84loop; no new interpreter invocation. General role/media transactions and Stage6 completion remain open |
| Generic staged-graph activation capability | Deferred to consolidated Stage6 run | Seven guarded `graph_activation` cases, one guarded held-request recovery transport case and one exact private scope/generation/epoch case registered; both packages' native/Node/Chromium, workspace, generic hosts/canaries and lifetime proofs pass | Include new cases and affected executor/planner/graph/transport regressions. Inventory at that step was206=128runtime+78loop. Exact-filter Cargo/harness argument separation is checked by helper tests and a native mirror, not a Miri invocation |
| Final implementation Miri selection | Pending | Historical/milestone runs retain their original scope | Run and collect the complete required final selection against the final implementation snapshot; reconcile later changes and final-head CI |

The interrupted invocation used the working-tree candidate based on `9bab9ddf`,
subsequently committed as `d89f96a7`; it was not a committed final implementation
when tested. Its 104 Rust source hashes are recorded in
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
