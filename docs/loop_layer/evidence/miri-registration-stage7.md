# Stage 7 standalone Miri registration and snapshot isolation

## Source and scope

Source base: `5d864a75140226229aa0becbb705679cf092fc8a`, with the registration,
supervisor, helper-test and affected documentation changes still uncommitted when
these checks ran. Production Rust, fixture Rust, dependencies and lockfiles are
unchanged from that base. This is native dispatch/infrastructure evidence, **not
an interpreter pass, fresh worklet execution, benchmark or final-head CI**.

The pinned `nightly-2026-05-04` runner now names every one of the standalone
fixture's59 native library tests with `-- --exact` and one expected passing test.
Primitive-wire and graph-actor cases additionally run at seeds1/2:63 host
executions. All35 previously accumulated graph/export/staging/dense/stall/media/
planner cases and the24 ingress/observer/codec/allocator/foundation cases are
included. No host workload, seed, allocator/destructor guard or900s watchdog was
reduced. Host cases run first to expose new-path failures before older regressions.

The preceding81-command/228-execution selection compares equal in package, target,
filter, expected count, seed and relative order. The full default is144 commands/
291 executions:128 runtime,100 loop and63 host. Structured inventory/result reports
record the standalone manifest separately from the root workspace manifest;
collection rejects manifest-scope substitution.

The exporter now selects committed top-level paths explicitly except `.pi/`, even
when private goal state is tracked. That state does not enter the archive or source
manifest. Uncommitted/untracked files remain excluded, submodules still require an
explicit dependency snapshot, and existing immutable-source/process-group/hash
checks remain active. Warning-denied Rust/rustdoc flags are recorded in job inputs.

## Observed checks

- `python3 -m unittest scripts.tests.test_rt_runtime_miri scripts.tests.test_rt_runtime_miri_jobs`:
  **19 passed** (final1.377s). New tests cover standalone dispatch/seed/manifest
  reports, zero-test refusal, tracked-private-state exclusion while retaining the
  standalone manifest/lockfile/source, and manifest-scope tampering. Synthetic
  supervisor fixtures do not execute or replace Miri.
- Native discovery: **59 unique tests**, exactly equal to the registered seed0 set.
- Exact native mirrors: **63 commands/63 passes**, including the four seeded
  dispatch copies. Each requires exactly one passing summary. Native execution
  ignores `MIRIFLAGS`; this verifies dispatch, not interpreter concurrency seeds.
- Default/host-only `--list` and launcher `start --help` commands succeed.
- Warning-denied locked workspace build, root/fixture format checks, runtime/loop
  boundaries, test-usage and closed tracing inventory pass.
- Pinned nextest0.9.116 complete workspace suite: **2141 passed/four existing
  skips**,33.386s warm. This is not cold-cache, performance or CI-margin evidence.
- Tools: rustc1.97.1 (`8bab26f4f`), cargo1.97.1 (`c980f4866`), pinned Miri0.1.0
  (`ad3a598ca4`,2026-05-03). The version query alone interprets no test.

All Cargo/Python checks ran inside `nix develop --command ...`, with warning-denied
Rust/rustdoc flags for the native gates. The archive contains the exact native
mirror driver, structured command/results, complete per-case logs, inventories,
helper/workspace logs, versions, CLI help and the four-script implementation patch.
Extract it at the repository root; its `native-driver.py` reproduces discovery and
native dispatch from the corresponding source tree.

## Raw evidence and pending gate

Archive: [`miri-registration-stage7-logs.tar.gz`](miri-registration-stage7-logs.tar.gz)
(81,187 bytes), SHA256
`718bdca910c24cd5b5a68e0022a6610b7aa9357f30bb8a152e8c7a3056b8de1d`.
Root: `target/loop-layer-miri-registration-stage7/`.

Registration does not close interpreter coverage. Commit these intended inputs,
then launch one full supervised Stage7 snapshot with the existing900s per-command
watchdog; record actual job state in [MIRI_JOBS.md](../MIRI_JOBS.md), continue useful
work, and require terminal collection/full inventory/source/artifact verification.
Stage7 remains7/8. CI artifact wiring, Stages8–10, final source-matched gates and
unmerged PR/green required CI/review remain open.
