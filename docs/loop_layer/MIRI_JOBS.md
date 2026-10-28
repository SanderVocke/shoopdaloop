# Miri jobs and outstanding verification

This is an implementation job/evidence history, not a live-process monitor or a
requirement to run Miri for prose acceptance. All source-match statements below
refer to their recorded snapshots/checkpoints, not the current dirty checkout.
The [documentation plan](../../DOCS_REWRITE_PLAN.md) requires no new interpreter
campaign; [current public guides](../../src/rust/shoop_loop_engine/README.md)
distinguish examples from this historical gate. Later implementation acceptance
obligations remain unchanged.

## Current Stage 7 verification debt

### Consolidated job failed; recovery required

Job **`20261028T083225-374aa055bf`** launched at2026-10-28T08:32:25.969265Z
from immutable commit **`8242de59fbb20ff17eb9724fe154c36aedb257e6`**:

```sh
nix develop --command python3 scripts/run_rt_runtime_miri_jobs.py start \
  --stage stage-7 --commit 8242de59
```

Expected scope:144 commands/291 executions (128 runtime+100 loop+63 host), pinned
nightly-2026-05-04, unchanged900s per-command watchdog, full host workloads and
guards. Job root: `target/loop-layer-miri-jobs/20261028T083225-374aa055bf/`.
Post-launch verification found the supervisor running, matched all856 source
entries and the copied supervisor, confirmed both workspace manifests/lockfiles,
native support/oracles and dev allocator inputs, and found no `.pi/` export.
The exact package-to-manifest mapping and warning-denied flags are recorded.
At the CI-scheduling inspection checkpoint,9 commands/9 executions had completed;
`09-lib-seed0.log` was running the complete dense acknowledged-restart/stall case.
This partial progress does not replace terminal collection or cover all291.

The job subsequently **failed** at2026-10-28T08:50:28.447343Z after
1082.4781163089792s wall time. The dense restart/stall command exceeded the unchanged
900s watchdog; its partial log ends inside the test, without a passing summary.
Nine preceding host commands/executions passed; the other135 commands are not
completed coverage. The supervisor/runner identities are no longer live, source/
supervisor hashes still match, and `collect` explicitly refuses the failed job.
Raw state, progress, completed/partial logs and collection refusal are preserved
with [CI-host evidence](evidence/ci-hosts-stage7.md). No watchdog, workload or guard
was weakened. The focused recovery launch below follows this failed checkpoint;
complete Stage7 recovery remains open.

- `job.json` SHA256: `bb53dd4414568c9ff7677ef5390843552853cff5e0c9c247280a7ffd324f4053`.
- Source archive SHA256: `0cc88b39499bc8db876858a4817faa6308dbc1dda952c5a76a4f6409cc724eed`.
- Copied supervisor SHA256: `5e9ac63c03b06687db2adcfac79d9c476c736f07cca4d06de3bb13164d23b8e2`.

Launch/input verification and nine partial passes are **not a passing complete
interpreter result**. Preserve this failed job, investigate the expensive command,
and justify the smallest adequate affected recovery selection. Continue useful
implementation work without routinely repeating the full suite. Stage7 remains7/8;
interpreter recovery, remote CI artifacts and later delivery obligations stay open.

### Dense recovery candidate (native verified; interpreter pending)

The supplementary dense event checks now use an off-render BTreeSet membership
index after the unchanged complete semantic-byte comparison. All assertions,
seven replay runs/partitions, frames/packets/receipts, seed0, guards and900s watchdog
remain unchanged. This removes repeated linear query work, not musical execution;
no interpreter speedup or recovery is claimed yet. Native57 Python/59 fixture
checks per profile, both complete native report comparisons and2141 workspace
passes/four skips validate the candidate. See
[evidence/dense-miri-recovery-stage7.md](evidence/dense-miri-recovery-stage7.md).

The supervisor now forwards/verifies a declared selection. Host batch10/63 names
exactly the failed test, so one committed focused job is justified by the actual
900s timeout. Its eventual pass would prove only that case; all remaining stage/
final coverage is still required.

Focused job **`20261028T094652-f2718d3398`** launched from committed candidate
**`dbe6d6236d7e7a8266d9dc75af5346490cfdf7fe`** at
2026-10-28T09:46:52.535072Z, supervisorPID3709585:

```sh
nix develop --command python3 scripts/run_rt_runtime_miri_jobs.py start \
  --stage stage-7-recovery-dense --commit dbe6d623 \
  --package loop_engine_host_fixture --shard 10/63
```

Input verification matched all862 frozen source entries and the copied supervisor,
excluded `.pi/`, confirmed the explicit source-commit environment and exactly one
seed0 execution of the failed full test. The package-wide63-command inventory
fingerprint is `6a20e86fab488dc30b6e0d6f4da0a07bf48a6ff12d97f7c7b80e2a8b24ed3b8d`.
At the09:53Z natural checkpoint the supervisor was still running. It subsequently
**failed** at2026-10-28T10:01:54.404514Z after901.8694647719967s: the same full test
again exceeded the unchanged900s watchdog, with **zero completed executions**.
The partial log has no passing summary. All862 source entries and the copied
supervisor still match; known supervisor/runner identities are no longer live;
collection refuses `Cannot collect passing evidence: failed`. The checker index
was not sufficient to meet the watchdog; censored timeouts do not measure a speedup
or slowdown. Both failed jobs and their different scopes remain preserved.
Stage7's complete291-execution selection and final source/CI/review obligations
remain open.

- Root: `target/loop-layer-miri-jobs/20261028T094652-f2718d3398/`.
- `job.json` SHA256: `e2fe8e309d4be009016606acab2b983b3d0b831e0598e4f76d676378f4371e50`.
- Source archive SHA256: `cd36c73d87b7fd1640b3f2a9527b10c9da4e0ccc398469293e934e28bd8b3343`.
- Copied supervisor SHA256: `c2cd7c0db36774db2a086e0721745ccca750b786b02870756abdbfb304fcd20d`.
- Local launch/input report: `target/loop-layer-miri-recovery-stage7/launch-verification.json`.

### Exact-test diagnostic profiling (interrupted; not a pass)

The second concrete timeout justifies profiling the same one-command/full-test
selection before further optimization. `--measureme` adds only Miri's function
profiler, requires one selected command, records exact flags and profile mode, and
keeps seed0/full workloads/guards/900s watchdog. Collection checks profile artifacts;
CI complete-set verification refuses diagnostic-profile reports. Thirty-two Miri
helper/synthetic tests (sixty with loop parser/orchestration tests) pass. Native
Rust/Wasm/dependencies are unchanged from `dbe6d623`; no fresh interpreter pass or
native performance result is inferred. See
[evidence/dense-miri-profile-stage7.md](evidence/dense-miri-profile-stage7.md).

The analysis tool is `summarize v11.0.1` from measureme commit
`9aeaf4d0cfaafb13379f0d8f3c3b5e8c13071afc`, installed in a writable private cache.
Initial attempts hit read-only Cargo storage and obsolete third-party `unsafe`
blocks under current rustc's denied warnings. The analyzer alone was built with
`RUSTFLAGS=-A unused-unsafe`; project/Miri warning-denied builds, safety checks and
guards are not changed. Its unsupported `--version` probe is retained; Cargo's
installed-package listing records the version/revision.

Diagnostic job **`20261028T102117-ca19c81152`** launched from committed tooling
snapshot **`3574d8fc3abe3abcc7ef933d3200f3876357eca3`** at
2026-10-28T10:21:18.263541Z, supervisorPID3725186:

```sh
nix develop --command python3 scripts/run_rt_runtime_miri_jobs.py start \
  --stage stage-7-dense-diagnosis --commit 3574d8fc \
  --package loop_engine_host_fixture --shard 10/63 --measureme
```

Launch verification matched all864 frozen source entries and the copied supervisor,
excluded `.pi/`, confirmed warning-denied Rust/rustdoc flags and the explicit source
commit, and matched the exact one-test inventory/seed0 plus these sole interpreter
flags: `-Zmiri-seed=0 -Zmiri-measureme=<job>/reports/measureme`.
The launch-verification checkpoint was **running**, current `00-lib-seed0.log`.
This is a justified diagnostic invocation, not a second routine full campaign or
a passing interpreter result. Preserve any partial profiles on timeout; analyze
a copy outside the reports directory. Continue useful work and check at natural
checkpoints rather than waiting/polling. Complete Stage7/final coverage stays open.

- Root: `target/loop-layer-miri-jobs/20261028T102117-ca19c81152/`.
- `job.json` SHA256: `ba3619d4c0c6420d10e1d267e96bada5ad512a79296c7be40b1a6cdbd2edaecd`.
- Source archive SHA256: `d26da23dd767ba4e3f5dc74424b5bdcd246c049c6552c7a9d413a8a43e8057c6`.
- Copied supervisor SHA256: `75dfb8fdc9b45957a2bfd01e679216e0f5ce28be2dffafdf8aba5b938a36fa1d`.
- Local launch/input report: `target/loop-layer-miri-profile-stage7/launch-verification.json`.

The diagnostic was deliberately interrupted with SIGINT to the identity-verified
interpreter child so Miri could finalize its profile before the unchanged hard
watchdog. An earlier identity/path assertion refused to signal; the corrected
check matched the actual isolated build path before signalling. The supervisor
recorded **failed**, exit1, at2026-10-28T10:34:59.550132Z after821.2866128640017s,
with **zero completed executions**. All864 source entries/supervisor still match;
collection refuses `Cannot collect passing evidence: failed`. This interruption
is diagnostic data, never a shortened passing workload or a timeout-recovery pass.

The live profile initially lacked its string index and could not be decoded.
After finalization, a copy outside reports decoded successfully with the pinned
analyzer:816.845415594s of recorded completed function events and14,530 labels.
MIDI restore consumed231.277s inclusive, cleanup72.915s across its two instantiations;
there were1,576,960 note lookups. Journal/MIDI-buffer preparation consumed98.057s/
120.302s inclusive; ordered MIDI insertion consumed108.454s inclusive, including
66.720s in partition-point lookup. These nested times must not be summed as
independent costs or mistaken for completed execution/native callback timings.
The3,232,339,355-byte raw profile remains in the job; SHA256
`c1e1a5fdf46b5a8232e8d5ba2aa63cc96cf1214cbcb64e82a8d21e8caa316268`.
See [measured-cost candidate evidence](evidence/midi-sparse-stage8.md) for analysis,
raw failed logs and subsequent native/Wasm verification, not an interpreter pass.

### Measured-cost candidate and current registered selection

Fixed channel bit indexes now expose ascending non-default-note/known-control
iteration; cleanup/restoration retains every required packet and its order.
Bulk pre-touch still writes all prepared bytes/slots, and ordered insertion has an
append fast path with unchanged stable middle insertion/refusal. Two new guarded
models compare indexes with dense getters and insertion with a scalar stable sort.
Existing full musical workloads, seven replay runs, guards/seeds and900s watchdog
are unchanged. Native190runtime/153loop/59fixture tests per profile,186runtime/
153loop tests in each Node/Chromium, all loop bindings/actual contexts/six failures
per profile, complete native report equality and2143 workspace passes/four skips
validate this candidate. No interpreter speedup/recovery is claimed yet.

The current selection is **145 commands/298 executions =135runtime+100loop+63host**.
All prior filters/seeds and their relative order remain. `midi_state` grows from
six to seven tests; the full `rt_runtime` `midi` target adds six executions (five
existing tests plus the new insertion model). No prior coverage is removed.
Host batch10/63 and its fingerprint remain unchanged. The repeated concrete dense
timeout plus measured-cost implementation changes justify one unprofiled retry
of that same complete exact test/seed0 as the smallest adequate recovery check.
A successful retry would cover only that case; consolidated current/final coverage,
remote CI artifacts, remaining Stage8 work and final review remain mandatory.

Focused unprofiled job **`20261028T113106-86abdbc4e8`** launched from committed
candidate **`802833f492df0b7e8f5a8f1c955c16ea102c2cdd`** at
2026-10-28T11:31:07.191913Z, supervisorPID3899660:

```sh
nix develop --command python3 scripts/run_rt_runtime_miri_jobs.py start \
  --stage stage-7-recovery-measured --commit 802833f4 \
  --package loop_engine_host_fixture --shard 10/63
```

Launch verification matched866 frozen source entries/the copied supervisor,
excluded `.pi/`, confirmed strict Rust/rustdoc/source-commit environment and the
unchanged exact full-test selection/seed0. Its sole interpreter flag is
`-Zmiri-seed=0`; there is no profiler or safety-flag override. The launch-verification
checkpoint was **running**, not passing. No second routine job should run while
this one is active. Continue useful development; inspect at natural checkpoints
and require terminal collection before any passing claim.

- Root: `target/loop-layer-miri-jobs/20261028T113106-86abdbc4e8/`.
- `job.json` SHA256: `aa5e8a8811a161c2fc4a3217bcc01da0b682dc12df562e85f36bd71e63662f69`.
- Source archive SHA256: `74ba9648fccc81ff345fb9a4b00c1e9b6b39c2a4864ea65882d32f763c5d2d4b`.
- Copied supervisor SHA256: `75dfb8fdc9b45957a2bfd01e679216e0f5ce28be2dffafdf8aba5b938a36fa1d`.
- Local proof: `target/loop-layer-midi-sparse-stage8/recovery-launch-verification.json`.
  Its first reporting attempt used the wrong archive-key name after source/selection
  checks; the corrected report records the actual key. No passing result was written.
  The preceding candidate archive predates launch and is not interpreter evidence.

### Historical Stage7 registered selection

Stage7 host implementation is ready for its consolidated interpreter gate; no
complete passing Stage7 interpreter result exists yet. The pinned runner now registers all59 native
standalone-host library tests at seed0, plus primitive-wire and graph-actor tests
at seeds1/2:63 host executions. This includes the35 cases detailed below and all24
remaining ingress/return, observer, codec, allocator and foundation cases. Exact
filters each require one passing test, use the standalone manifest/lockfile and
retain full workloads/guards. Native discovery and63 exact native dispatches pass;
these are not interpreter results. Nineteen synthetic/tooling Python tests pass.

The full selection is144 commands/291 executions:128 runtime+100 loop+63 host.
Host cases run first; all prior81 commands/228 executions retain their filters,
counts, seeds and relative order. The new `Controller::resource` path and host
ownership/planner/observer support are included with their runtime/loop regressions.
The immutable committed Stage7 job above attempted this selection with the
unchanged900s per-command watchdog but failed after nine executions. Until verified
recovery/complete terminal collection, interpreter/CI
artifacts and the last Stage7 row remain open. See
[evidence/miri-registration-stage7.md](evidence/miri-registration-stage7.md).
Native, public-Wasm binding and actual worklet passes do not extend the collected
Stage6 inventory.
The new standalone `loop_engine_host_fixture` graph receiver contributes seven
native cases under `graph_transfer::tests`, including unchanged128-round lifecycle
workloads, actual route activation, stalled owning returns and disconnect cleanup.
The native graph command/return milestone adds another eight cases: six under
`graph_wire::tests` and two under `graph_bridge::tests`, including128 wire candidate
rounds, mixed four-normal/two-graph-credit saturation, a local owning Full refusal
and intact unexpected-completion quarantine. Public graph command bindings add two
native `browser::tests` cases: `graph_binding_buffers_cover_route_cut_and_reliable_owning_release`
(three partitions/full media/receipts/semantic/recovery) and
`graph_binding_fault_disposal_abandons_credits_without_fabricating_returns` (both
actual FX fault modes). Actual graph-worklet integration adds
`browser::native_graph::tests::acknowledged_graph_worklet_script_has_complete_partition_invariant_native_reference`,
covering whole/single/irregular/zero partitions and complete native payload/command/
result/media/semantic/resource evidence for the audible checkpoint script.
The native actor extension adds
`browser::native_graph::tests::bounded_two_thread_graph_transport_matches_complete_native_reference`:
capacity-one copied-byte transport, renderer-local owners, malformed retries,
whole/single/irregular/zero partitions and complete direct-report equality.
Immutable export integration adds four more native cases: two `browser::native_export::tests`
(complete direct/threaded two-revision snapshots/partitions and cancellation/EOF/stalls/
128 export reuses) and two `export_wire::tests` (canonical/full-width codecs,
truncations/projection validation and transactional credits/nonwrapping generations).
Resource/program staging adds four more native cases: two
`browser::native_staging::tests` (complete direct/threaded resource revisions,
raw-MIDI source refusal, nested play/iteration-seek/stop and full independent
semantic/media tables; owning chunk cancellation and128 program reuses in both
actor modes), plus two `staging_wire::tests` (full-width codecs/all truncations/
padding and transactional/nonwrapping client identities). The new
`Controller::resource` forwarding path and prepared topology/source settings are
also relevant source inputs for the consolidated selection.
Dense host coverage adds two cases: `scenario::dense::tests::dense_capture_cascades_and_clocked_composites_keep_music_with_unread_results`
(direct facade, unchanged131-frame full media/cleanup workload) and
`browser::native_dense::tests::dense_wire_and_capacity_one_actors_preserve_music_with_all_results_stalled`
(whole/single/irregular/zero partitions in direct and capacity-one actor modes,
complete independent media/semantic/receipt oracle). Larger fixed MIDI/observer
storage is prepared off-render; there is no reduced interpreter workload.
Transferred-result buffer exhaustion adds
`browser::native_dense::tests::acknowledged_restart_keeps_media_and_owning_results_while_transport_is_unpolled`:
the complete136-frame restart script, seven requests/fourteen replies and independent
media/semantic tables across whole/single/irregular/zero partitions and both actor
modes. Actual MessagePort buffer holding is JavaScript/browser evidence, not a
native interpreter feature. The affected dense native support/oracle must be part
of the consolidated source snapshot.
Prepared media adds three applicable native cases:
`scenario::media_case::tests::replacement_countdown_and_capture_exhaustion_keep_exact_prefix_and_live_audio`,
`browser::native_media::tests::replacement_countdown_prefix_exports_and_bounded_actors_match_complete_oracles`,
and `staging_wire::tests::countdown_results_keep_delivery_distinct_from_terminal_frame_and_preserve_credit_on_corruption`.
They retain complete indexed replacement/MIDI restoration, delayed delivery/terminal
frames, truthful common-prefix exhaustion/export/initial state and direct/capacity-one
partition workloads. The existing canonical staging test now covers all27 command
kinds. Include the new constructor/catalog/cleanup paths and source/oracle support
in the consolidated snapshot; browser output is not interpreter evidence.
Planner lifecycle canaries add two applicable cases:
`planner_canary::tests::resolver_planner_control_is_partition_invariant_with_complete_boundary_tables`
and `planner_canary::tests::planner_errors_and_heap_faults_have_exact_lifecycle_origins_and_are_not_successful_cleanup`.
The first retains full128-frame whole/single/irregular/zero workloads and independent
512-record semantic tables; the second retains actual planner error and deliberate
allocator instrumentation, permanent faults and off-render abandonment. Include the
new module's real Resolver adapter/decision consumer, destructor guards and support
in the snapshot. The existing dev-only allocator remains enabled; no workload or
900s watchdog reduction and no immediate per-commit interpreter run is introduced.
All19 graph-focused, four export-focused, four staging-focused, three dense/stall,
three media and two planner cases (**35**) are now **registered, without a collected
passing interpreter result**.
Node/Chromium now execute shared
public payload and command bindings, including128 real candidate return cycles;
they do not execute these native test functions or extend Miri coverage. Keep the
existing900s per-command watchdog, guards and workloads in the registered
standalone manifest/lockfile and exact-filter selection. Final delivery
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
