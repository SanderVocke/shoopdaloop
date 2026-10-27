# Background stage-level Miri jobs

Use one consolidated background run per applicable numbered-stage milestone, not
one per smaller edit/commit. Continue development while it runs. The final required
selection and source-matched result verification remain mandatory; see
[MIRI_JOBS.md](MIRI_JOBS.md) and the [plan](../../PLAN_LOOP_LAYER.md).

## Launch, inspect, collect

Commit the intended test inputs first. The launcher requires an **explicit
committed snapshot**; it never incorporates uncommitted or untracked files.
Include new test/support files in that commit. The selected commit must contain
the runner's `--list` and `--report-dir` support.

```sh
# At an applicable stage milestone, not after each smaller implementation step:
nix develop --command python3 scripts/run_rt_runtime_miri_jobs.py start \
  --stage stage-6 --commit HEAD

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
`--package rt_runtime|shoop_loop_engine` is available for a justified affected
selection, not a substitute for complete required stage/final coverage.

This local background wrapper currently requires **Linux `/proc` process
identities and POSIX process groups**. The existing foreground runner remains
portable for CI/other supported environments. No new test workload reductions,
watchdog increases or acceptance exemptions are introduced.

## Immutable source and isolation

The launcher resolves the requested commit once, exports its Git archive, records
all source-file hashes/symlink targets/executable bits, and removes snapshot write
permissions. Later working-tree edits or commits do not affect it. `.pi/` is not
exported; tracked `.pi/` or unmaterialized submodules are refused rather than
silently incorporated/omitted. Archive extraction rejects unsafe paths/links.

The supervisor itself is copied and hashed at launch. Nix uses an explicit
`path:` reference to the exported snapshot's configuration/lockfile; Git discovery
is capped at the job directory so a snapshot under `target/` cannot accidentally
resolve the mutable parent checkout. The runner uses its pinned Miri toolchain.
Each job has isolated Cargo build and report directories outside the source
snapshot. Registry/toolchain caches may be shared. Source and supervisor integrity
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

## Tests and current use

```sh
nix develop --command python3 -m unittest \
  scripts.tests.test_rt_runtime_miri scripts.tests.test_rt_runtime_miri_jobs
```

Fourteen tests cover the selected inventory, durable file output, isolated external
reports/progress, existing timeout/exit-race cleanup, immutable committed snapshots
while development changes, collection, missing results, count/log/source tampering,
missing supervisor identity, cancellation with descendant cleanup, recovery of a
known runner after supervisor SIGKILL, single-job admission, and the actual runner's
SIGTERM cleanup of a synthetic case and its descendant process group. Supervisor tests use explicitly synthetic runners and a Nix shim;
they do **not** execute Miri or stand in for Rust/Miri evidence.

The first cancellation fixture incorrectly waited reentrantly inside its signal
handler, causing a cleanup timeout; it now raises out of the wait before reaping.
The retained failure is `target/loop-layer-miri-background/python-first.log`.
The final fourteen-test pass is in `checks.log`. No additional routine Miri run was
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
[MIRI_JOBS.md](MIRI_JOBS.md). Launch and partial progress are not a passing result;
terminal collection and input/inventory/hash verification were required. Those checks
have now passed for the complete81-command/228-execution run. See the job ledger and
`evidence/miri-stage6{,-collected,-provenance}.json`; the raw report/log bundle is
`evidence/miri-stage6-logs.tar.gz`. This closes Stage6's interpreter gate, not later
host/final acceptance, and the user requires a goal pause before Stage7.
