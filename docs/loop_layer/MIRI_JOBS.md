# Miri jobs and outstanding verification

This is an implementation job/evidence history, not a live-process monitor or a
requirement to run Miri for prose acceptance. All source-match statements below
refer to their recorded snapshots/checkpoints, not the current dirty checkout.
The [documentation plan](../../DOCS_REWRITE_PLAN.md) requires no new interpreter
campaign; [current public guides](../../src/rust/shoop_loop_engine/README.md)
distinguish examples from this historical gate. Later implementation acceptance
obligations remain unchanged.

## Current Stage 7 verification debt

Stage7 implementation continues; there is no active interpreter job. Native,
public-Wasm binding and actual worklet passes do not extend the collected Stage6
inventory. Accumulate the fixture's prepared ingress/return, semantic-observer and
graph-payload ownership cases for one applicable consolidated Stage7 snapshot/run.
The new standalone `loop_engine_host_fixture` graph receiver contributes seven
native cases under `graph_transfer::tests`, including unchanged128-round lifecycle
workloads, actual route activation, stalled owning returns and disconnect cleanup.
The native graph command/return milestone adds another eight cases: six under
`graph_wire::tests` and two under `graph_bridge::tests`, including128 wire candidate
rounds, mixed four-normal/two-graph-credit saturation, a local owning Full refusal
and intact unexpected-completion quarantine. All15 graph-focused native cases are
**not yet registered in the pinned interpreter runner or interpreted**.
Node/Chromium run shared payload bindings, not the native graph command cases. Keep the
existing900s per-command watchdog, guards and workloads when adding the standalone
manifest/lockfile and exact filters to the supported selection. Final delivery
still requires the complete applicable source-matched selection; no routine
per-commit Miri invocation was launched for this payload foundation.

## Collected Stage 6 pass

Job `20261027T025325-e065d48941` completed successfully and was collected with the
supported `collect` command. **81 commands/228 executions passed:128 runtime+100
loop**, pinned nightly-2026-05-04 / miri0.1.0(ad3a598ca4), unchanged900s watchdog.
Implementation snapshot: `faec78df0a990d28b4fefb2f75e43ea0ace7f1ea`. Ended
2026-10-27T04:55:23.943226Z, total7318.292420581012s. Full snapshot/archive/supervisor,
ordered inventory/seeds/toolchain and82 artifact hashes were verified. Current
123 scoped Rust/10 configuration-runner inputs and supervisor/helper hashes match.

Committed evidence: `evidence/miri-stage6.json`, `miri-stage6-collected.json`,
`miri-stage6-provenance.json`, and `miri-stage6-logs.tar.gz` containing the raw
81 logs and result report. Report SHA256:
`d4c8fa7b5e0caf610783a56d178ae4343288b413f63e2841184847ae15df3ae0`.
The dense128-boundary case passed in877.7475190730183s; no watchdog/workload/guard
change or second routine invocation was needed. Stage6 is now complete. Per the
then-current user instruction, the implementation goal paused before Stage7;
this does not pause the documentation goal or waive later implementation gates.
The earlier interrupted171/178 invocation is still partial and is not relabelled.

## Historical Stage6 launch and while-running checkpoints

Launched `20261027T025325-e065d48941` at2026-10-27T02:53:25Z from immutable commit
`faec78df0a990d28b4fefb2f75e43ea0ace7f1ea` with the full selection:
**81 commands /228 executions (128 runtime+100 loop)**, pinned
`nightly-2026-05-04`, unchanged900-second per-command watchdog. Launch command:

```sh
nix develop --command python3 scripts/run_rt_runtime_miri_jobs.py start \
  --stage stage-6 --commit faec78df
```

Job directory: `target/loop-layer-miri-jobs/20261027T025325-e065d48941/`.
Its immutable `job.json` SHA256 is
`baf376abb633bc363f7e94712cbef3af9108d3675ad1026b8061db28dd06d807`.
The manifest includes source/archive/supervisor hashes, exact filters/seeds/counts,
command/environment and isolated paths; live process identity is separately recorded.
The first natural documentation-review checkpoint verified the supervisor running;
progress subsequently reported8 completed commands/49 executions and `midi_state`
in progress. At the Part I handoff commit checkpoint it had22 completed commands/
111 executions, with `capture_group_model` in progress and the supervisor still
running. **This is partial progress, not a collected pass or Stage6 completion.**
Do not start another routine job. Continue documentation work, inspect at natural
checkpoints, and collect/verify the terminal result before updating acceptance.
The working-tree documentation/example review does not mutate this snapshot.
The subsequent resource/MIDI/capture handoff checkpoint verified the supervisor
still running:37 commands/162 executions completed, with the configuration
input-projection preserve/reset case in progress. Selected native documentation
checks ran in a different Cargo output directory; no additional Miri was launched.
This later progress is also partial and does not replace terminal collection.
The capacity/ingress/shutdown review checkpoint verified53 commands/178 of228
executions completed, with the paired cancellation/released-generation case running.
Do not confuse this new-job partial count with the historical interrupted178-selection.
Full native runtime tests and bounded documentation smokes/canaries ran in separate
output while the immutable interpreter job continued; no second job was started.
The control/planning review checkpoint verified67 completed commands/207 of228
executions, with the loop MIDI target (`67-midi-seed0.log`) in progress. Four native
example executions and the drafting checker ran separately. All123 scoped Rust and
10 configuration/runner input hashes remain unchanged; this review adds no new
interpreter invocation or terminal-pass claim. Stage6 remains incomplete.
The loop-guide/facade-contract review checkpoint verified70 completed commands/
215 of228 executions; `70-runtime-seed0.log` was running the stopped-input/half-open-
prefix export case. Fresh native example/rustdoc checks used separate output, and
all scoped Rust/configuration/archive hashes remained unchanged. Still no terminal
collection or new interpreter invocation; the existing job alone continues.
The repository/architecture review checkpoint verified75 completed commands/220 of228
executions, with the dense128-boundary runtime case in `75-runtime-seed0.log` running
under the unchanged900-second watchdog. Both-crate rustdoc and a drafting book check
ran separately; source/configuration hashes remain unchanged. This is still partial
progress and does not authorize Stage6 completion or another routine Miri invocation.


Latest nested-lifecycle/readiness milestone: **228 selected =128 runtime+100 loop**.
The added guarded interloop case passes native/release/Node/Chromium; all153 loop
cases and2141 workspace cases/four skips pass. All28 topology and both interloop
cases now use individual exact filters. Native discovery matches every name and
all30 filters execute exactly one native test each. Workloads, seed0, guards and
the900-second per-command watchdog are unchanged. Historical interloop duration
468.43s and grouped suite growth motivate finer supervision; no new timeout or
interpreter pass was claimed at readiness. The subsequently collected job above now
verifies the full inventory. [Stage6 completion audit](STAGE6_READINESS.md) maps
requirements and records terminal collection/source checks. Later acceptance remains
open in that implementation plan; its historical pause is not an instruction
for the current documentation authoring workflow.

Previous paired program/graph milestone: **227 selected =128 runtime+99 loop**.
Four new guarded facade cases are registered, not interpreted. The existing private
probe case also checks selection-query phases. Native/release/Node/Chromium pass152
loop tests; fourteen supervisor/runner helper tests pass. Full Stage6 still needs
its consolidated supervised immutable-snapshot interpreter run.

Latest configuration milestone: **223 selected =128 runtime+95 loop**, including
seven new facade configuration cases and one private role/source exhaustion case.
This is registration only, not interpreter evidence. Native debug/release and
Node/Chromium passed148 loop cases; fourteen runner-helper tests passed. Preserve
the historical interrupted171/178 result as partial. Consolidated Stage6 execution
still awaits a final immutable committed snapshot and supervised background run.

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
Before consolidation, the smaller program-probe, owning-transaction,
facade-adapter and generic graph-permit steps had no separate background run.
At the media-owner prerequisite the inventory was215=128runtime+87loop; the later
configuration, paired-program/graph and nested cases brought it to228. These were
registration-only checkpoints then, subsequently covered by the collected Stage 6
run above, not retroactive passes of the individual earlier invocations.

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

## Historical outstanding work at the scheduling change

| Job / evidence | State | Verified scope | Outstanding obligation |
| --- | --- | --- | --- |
| `facade-topology-stage6`, interrupted foreground invocation | Interrupted; orphan process group terminated | 171 completed selected executions: 119 runtime and 52 loop, including all six new `runtime_topology` cases | Seven selected loop executions were not completed/reported; no full 178-execution pass or whole-invocation wall duration exists. No immediate repeat/resume is scheduled solely for this smaller step; include required coverage in the consolidated Stage 6 run |
| Background supervisor/snapshot support | Implemented; 14 Python tests pass | Immutable committed exports, detached Linux supervisor, streamed output, exact inventory/hash collection and cancellation/descendant cleanup tested with synthetic fixtures | First actual interpreter use remains the consolidated applicable stage run; no real Miri pass is claimed by these infrastructure tests |
| Unpublished program-boundary probe prerequisite | Deferred to consolidated Stage6 run | Four new executions registered: one private library case and three `program_boundaries` cases; native/release/Node/Chromium/workspace gates pass | Include these and affected resolver regressions in the stage snapshot. Current selected inventory is182=119runtime+63loop, not a completed run |
| Standalone owning single-program transaction | Deferred to consolidated Stage6 run | Nine `program_changes` executions registered; native/release/Node/Chromium/workspace gates pass | Include the new owner/activation cases and affected resolver regressions in the stage snapshot. Current inventory is191=119runtime+72loop; no fresh interpreter pass is claimed |
| Fixed-graph facade owning program activation and permanent quiescence | Deferred to consolidated Stage6 run | Five new `runtime_topology` cases and one new `program_changes` case registered; native/release/Node/Chromium/workspace gates pass | Include all new and affected resolver/facade lifecycle cases. Current inventory is197=119runtime+78loop; no interpreter run for this slice |
| Media-ledger owner-generation publication | Deferred to consolidated Stage6 run | Three guarded media cases including512-edit independent reference model and logical generation exhaustion; loop140 native/Node/Chromium, workspace2128/four skips,19 scoped gates and8 positive/23 negative proofs pass | Include new and affected media/facade regression cases. Inventory215=128runtime+87loop; no interpreter invocation. Facade role-generation integration remains pending |
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
