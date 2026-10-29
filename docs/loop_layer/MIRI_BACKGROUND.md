# Background stage-level Miri jobs

This is the implementation runner's operational reference, not documentation
acceptance or an instruction to commit the current checkout.
[VALIDATION.md](../../VALIDATION.md#miri-evidence) is the single current result/debt
index. Launch only at an applicable authorized implementation gate; prose
maintenance alone needs no job. A paused goal remains paused.

Use one consolidated background run per applicable numbered-stage milestone, not
one per smaller edit/commit. Continue authorized development while it runs. The
final required selection and source-matched result verification remain mandatory;
see the [plan](../../PLAN.md#miri-execution-contract).

## Launch, inspect, collect

Commit the intended test inputs first. The launcher requires an **explicit
committed snapshot**; it never incorporates uncommitted or untracked files.
Include new test/support files in that commit. The selected commit must contain
the runner's `--list` and `--report-dir` support.

```sh
# At an applicable stage milestone, not after each smaller implementation step:
nix develop --command python3 scripts/run_rt_runtime_miri_jobs.py start \
  --stage stage-7 --commit HEAD

# Use the job directory printed by start; inspect at natural work checkpoints:
python3 scripts/run_rt_runtime_miri_jobs.py status /absolute/path/to/job
python3 scripts/run_rt_runtime_miri_jobs.py collect /absolute/path/to/job

# Explicit cancellation; inspect terminal state afterward:
python3 scripts/run_rt_runtime_miri_jobs.py cancel /absolute/path/to/job
```

Launch returns after creating the snapshot and detached supervisor, not after
Miri finishes. The supervisor survives the launcher's ordinary exit and streams
output into files rather than into an interactive tool's pipe. One active job is
allowed per jobs directory; an atomic launch lock prevents concurrent starts.
`--package rt_runtime|shoop_loops|loops_host_fixture` is available for
a justified affected selection, not a substitute for complete required stage/final
coverage. Inspect the standalone fixture without interpreting tests:

```sh
python3 scripts/run_rt_runtime_miri.py --list --package loops_host_fixture
```

Discover inventory from the **intended frozen source**, rather than copying old
counts or shard positions from a historical example:

```sh
python3 scripts/run_rt_runtime_miri.py --list
```

The current recorded inventory/results live in [VALIDATION.md](../../VALIDATION.md#miri-evidence).
Retain every required exact filter, minimum count, seed, workload, guard and relative
order. New coverage is additive, not a replacement for old cases. A list/discovery
command is not interpreter execution or passing evidence.

This local background wrapper currently requires **Linux `/proc` process
identities and POSIX process groups**. The existing foreground runner remains
portable for CI/other supported environments. No new test workload reductions,
watchdog increases or acceptance exemptions are introduced.

## CI batches

CI uses16 disjoint round-robin batches rather than putting the entire stage
selection into one40-minute job. Each batch keeps complete commands, exact filters,
counts, seeds and the900s per-command watchdog. Batch sizes depend on the frozen
inventory; use `--list` to inspect them. Neither command counts nor historical local
wall times guarantee a margin on CI hardware.
An individual command exceeding900s still fails and needs investigation.

```sh
python3 scripts/run_rt_runtime_miri.py --shard 1/16 --list
python3 scripts/run_rt_runtime_miri.py --shard 1/16
# After downloading all batch artifacts into separate subdirectories:
python3 scripts/run_rt_runtime_miri.py --verify-batches target/miri-ci-batches
```

Reports record the source commit, full-selection fingerprint, batch identity,
selected manifest scope and run attempt. Collection requires every batch exactly
once, the current source/full selection, consistent pinned toolchain/version, exact
case/filter/count/seed records and every passing raw log. Missing/duplicate/mixed
batches, missing tests, scope substitution and failed summaries cannot pass.
`verified.json` contains aggregate counts and hashes only after complete validation.
These are **separate CI batches**, not one uninterrupted invocation. Local stage
jobs still use the supervised immutable-snapshot workflow; batch support does not
authorize routine extra interpreter runs.

## Focused failure recovery

A concrete interpreter failure may justify a declared smaller selection. The
supervisor forwards `--shard INDEX/TOTAL` to both inventory discovery and execution
and validates batch/full-selection identity and source commit during collection.
This does not shrink an individual test or waive remaining coverage. First list
that frozen source/package and choose a shard containing exactly the affected
whole command. Old `10/63` dense-host or `151/151` sync-model shard identities belong
to their recorded inventories, not an unqualified current `HEAD`.

For example, set `COMMIT`, `PACKAGE` and `SHARD` to the verified source/selection:

```sh
python3 scripts/run_rt_runtime_miri_jobs.py start --stage failure-recovery \
  --commit "$COMMIT" --package "$PACKAGE" --shard "$SHARD"
```

Record the exact test, failure rationale and pending remainder. A focused recovery
pass is not a complete stage pass.

For a concrete unresolved timeout, `--measureme` adds only the pinned interpreter's
function-profiler flag. It requires exactly one selected whole command and keeps
its seed, assertions, guards and900s watchdog. The option is forwarded by the
supervisor and recorded with exact interpreter flags in current/completed records:

```sh
python3 scripts/run_rt_runtime_miri_jobs.py start --stage failure-diagnosis \
  --commit "$COMMIT" --package "$PACKAGE" --shard "$SHARD" --measureme
```

Profiles go under the isolated job's `reports/measureme/`. Collection checks mode,
flags and profile hashes; missing/modified profiles cannot become passing evidence.
The CI complete-set verifier rejects diagnostic-profile reports. Profiling overhead
can itself time out; preserve partial profiles/logs and never call that a pass or
native callback performance evidence. Analyze a retained `.mm_profdata` file with
measureme's `summarize summarize FILE.mm_profdata` (or `--json`). Collect successful
jobs first and analyze a copy outside the reports directory: JSON analysis writes
beside its input and must not change the verified artifact set. An incomplete
profile describes completed trace events, not successful completion of the test.

## Immutable source and isolation

The launcher resolves the requested commit once, exports its Git archive, records
all source-file hashes/symlink targets/executable bits, and removes snapshot write
permissions. Later working-tree edits or commits do not affect it. The archive
explicitly selects committed top-level paths except `.pi/`, including when that
private state is tracked. `excluded_paths` records that exclusion; no `.pi/` data
enters the archive or source manifest. Submodules still require an explicit
dependency snapshot and are refused. Archive extraction rejects unsafe paths/links.

The standalone fixture's manifest, lockfile, production dependencies, Rust support
and independent oracles are ordinary committed inputs. Inventory and result reports
record the selected package-to-manifest mapping. Collection rejects mismatched
manifest scope as well as mismatched cases, counts or seeds.

The supervisor itself is copied and hashed at launch. Nix uses an explicit
`path:` reference to the exported snapshot's configuration/lockfile; Git discovery
is capped at the job directory so a snapshot under `target/` cannot accidentally
resolve the mutable parent checkout. The runner uses its pinned Miri toolchain.
Each job has isolated Cargo build and report directories outside the source
snapshot, with warning-denied Rust/rustdoc flags recorded in its environment.
Registry/toolchain caches may be shared. Source and supervisor integrity
are rechecked before execution and collection. This protects against accidental
input changes, not a malicious operator who can rewrite every artifact/hash.

## Artifacts and verification

Default job root: `target/loop-layer-miri-jobs/<job-id>/`.

- `job.json`: stage, resolved commit, source manifest/archive hash, supervisor hash,
  exact package/target/filter/seed/count inventory, command and selected environment.
- `source/`, `supervisor.py`: retained inputs. Uncommitted development files are
  deliberately absent; keep this distinction in evidence.
- `process.json`, `child.json`: PID plus Linux boot identity/process start ticks,
  guarding against accidentally signalling a reused PID or a process after reboot.
- `supervisor.log`, `runner.log`, `reports/*.log`: durable output. Per-case output
  streams while a case runs, including partial output before interruption.
- `reports/progress.json`: atomic inventory/current-case/completed-case progress.
- `reports/result.json`: runner success only after all selected cases pass.
- `state.json`: atomic supervisor terminal state, exit code, monotonic wall time,
  verified execution count and artifact hashes.
- `collected.json`: explicitly checked passing evidence for the recorded snapshot.

Collection requires terminal supervisor success **and** exit zero, complete exact
inventory/counts/seeds, passing per-case log summaries, pinned toolchain/version
reporting, unchanged source/supervisor and unchanged result/log hashes. Exit zero
without a result, a partial selection, missing log, changed count, stale source or
modified artifact cannot become passing evidence. A missing supervisor without a
terminal result is reported as orphaned while its recorded runner is live, or
interrupted otherwise, not guessed to have succeeded. A known orphan blocks
another launch in that jobs directory and can be signalled with `cancel`.

A cancellation signals the known supervisor identity. It interrupts its runner;
the runner cleans up the current per-case process group and preserves output.
The per-command watchdog remains 900 seconds. The wrapper escalates termination
of an unresponsive runner after a bounded cleanup wait. Catastrophic external
process kills/machine loss still require inspecting the recorded process identities
and artifacts; no success is inferred from a disappeared process. Do not delete a
job's source/reports while it is running. Preserve unsuccessful and superseded
records rather than overwriting them with later successes.

Monotonic per-case durations are included in runner records, and whole-job wall
time in supervisor state. Neither is Miri's simulated Rust test-harness clock.
Record contention/cache conditions; timings are not deadline guarantees.

## Helper tests and historical setup evidence

```sh
nix develop --command python3 -m unittest \
  scripts.tests.test_rt_runtime_miri scripts.tests.test_rt_runtime_miri_jobs \
  scripts.tests.test_rt_runtime_miri_batches
```

At the original tooling milestone, thirty-two tests covered the selected inventory, exact standalone manifest dispatch,
seed/report metadata, zero-test refusal, tracked-private-state exclusion without
losing standalone source/lockfiles, manifest-scope tampering, complete disjoint CI partition/collection and CLI paths,
durable file output, isolated external reports/progress, existing timeout/exit-race cleanup, immutable committed snapshots
while development changes, collection, missing results, count/log/source tampering,
missing supervisor identity, cancellation with descendant cleanup, recovery of a
known runner after supervisor SIGKILL, single-job admission, and the actual runner's
SIGTERM cleanup of a synthetic case and its descendant process group. Supervisor tests use explicitly synthetic runners and a Nix shim;
they do **not** execute Miri or stand in for Rust/Miri evidence.

The first cancellation fixture incorrectly waited reentrantly inside its signal
handler, causing a cleanup timeout; it now raises out of the wait before reaping.
The retained failure is `target/loop-layer-miri-background/python-first.log`.
The initial fourteen-test pass is in `checks.log`. No additional routine Miri run was
started for this tooling step. Its first real interpreter use belongs to the next
applicable consolidated stage run. Source/log hashes and the unchanged178-case
inventory are recorded in [evidence/miri-background.json](evidence/miri-background.json).
A subsequent real-repository smoke exported commit `aa41a9b3` (604 files), queried
its178-execution inventory and ran locked Cargo metadata in the snapshot's actual
Nix environment. It exited zero with all source hashes unchanged; this verifies
snapshot/environment setup, not Miri execution.

The first actual consolidated interpreter job was subsequently launched for Stage6
from committed readiness snapshot `faec78df`, after the nested lifecycle gates and
coverage audit. It uses81 commands/228 executions and the unchanged900-second
per-command watchdog. Job identity/progress are recorded in
the [historical ledger](../history/pre-consolidation-fffc1faa/docs/loop_layer/MIRI_JOBS.md#collected-stage-6-pass). Launch and partial progress are not a passing result;
terminal collection and input/inventory/hash verification were required. Those checks
have now passed for the complete81-command/228-execution run. See the job ledger and
`evidence/miri-stage6{,-collected,-provenance}.json`; the raw report/log bundle is
`evidence/miri-stage6-logs.tar.gz`. This closes Stage6's interpreter gate, not later
host/final acceptance. The user-required pause before Stage7 applied to that
implementation checkpoint. Later complete results and remaining failures belong
in [VALIDATION.md](../../VALIDATION.md#miri-evidence), not a second progress ledger
here. These historical tooling details authorize no implementation or interpreter
run. The [original operational reference](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/docs/loop_layer/MIRI_BACKGROUND.md)
retains the earlier inventory-specific recipes.
