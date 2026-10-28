# Stage 7 CI host wiring and failed Miri checkpoint

## Source and changes

Base: `622d506f1edf51bfd5475a3a25a65767579cda52`, with the workflow, Python runner/
test and affected documentation changes uncommitted during verification. No Rust,
manifest, lockfile or JavaScript changed in this milestone.

- All six Linux/Windows/macOS debug/release cells now test both libraries and the
  standalone fixture, run `gain_graph`, `planned_pair`, `pending_operations` and
  `coordinated_loop`, and retain complete native replay reports.
- `run_loop_engine_hosts.py --native-only --profile debug|release` runs17 native
  commands: primitive report, six complete direct/actor pairs, planner control,
  and primitive direct/wire/two-thread drivers. Rust checks independent oracles;
  Python additionally compares complete actor reports. Output is explicitly
  `native-replay` under separate `reports/native-<profile>/` paths. It refuses
  actual-worklet failure flags and never dispatches Wasm/Node/browser work.
- Both web profiles now run the complete loop bindings, actual offline/live hosts
  and six specific failure gates. Loop source/fixture changes trigger Chromium
  policy coverage; the loop crate is included in explicit Wasm dependency checks.
  Native and worklet artifact uploads require files. Existing runtime/application
  gates remain.
- Miri CI uses16 disjoint round-robin batches of whole commands, preserving the
  full144-command/291-execution inventory, counts, filters, seeds and900s watchdog.
  The complete-set verifier requires current source/full-selection fingerprint,
  manifest scope, consistent pinned toolchain/version, every case/count/seed and
  every passing raw log. Missing/duplicate/mixed batches, empty selections, wrong
  scopes, missing tests and failed logs cannot pass. Artifact hashes and per-batch
  attempts are retained after verification. The dependent job also requires the
  entire matrix to succeed.

These are separate CI batches, **not one uninterrupted invocation**. Nine commands
per batch do not guarantee a40-minute margin; individual900s failures still need
investigation. Remote CI and Windows/macOS execution are not proved locally. Loop
benchmark/release-measurement wiring remains later Stage8 work.

## Local observations

All Cargo/Python operations used `nix develop --command ...`.

| Gate | Observed result |
| --- | --- |
| Miri runner/supervisor/batch tests |28 passed; synthetic/tooling, not interpreter evidence |
| Loop report/failure parser and native orchestration tests |28 passed; total Python56 |
| Standalone Rust fixture |59 passed per native profile |
| Native-only runner |17 replay commands pass per profile, complete actor equality |
| Shared bindings |20 groups in each Node/Chromium runtime/profile |
| Actual Web Audio |16 successful contexts plus six exact intentional failures per profile |
| Public CI examples |All four commands pass in debug/release |
| Workflow syntax/expressions |Project-pinned nixpkgs `actionlint` passes; shellcheck disabled |
| Root gates |Warning-denied workspace build, root/fixture formatting, boundaries, test usage, explicit runtime/loop Wasm dependencies and closed tracing inventory pass |
| Pinned nextest0.9.116 workspace |2141 passed/four existing skips;33.417s warm |

Browser validation intentionally used `--no-build`: Rust/Wasm/JS source is unchanged
and existing verified native/Wasm artifacts were reused. The new native-only path
also performed its locked native build in each profile. Actual browser commands
explicitly selected Node22.22.2 and matching Chromium/driver147.0.7727.137; the
ordinary Nix-shell Node version in `versions.log` is22.23.2 and was not the selected
browser-run executable. Reports preserve binary hashes and fixed-memory/content/
receipt/ownership/journal checks. This is fresh browser execution with deliberate
binary reuse, not a new Wasm build. Warm suite duration is not performance,
cold-cache, deadline or CI-margin evidence.

The first YAML-module probe failed because PyYAML was unavailable. That attempt is
recorded; workflow validation instead used actionlint from the repository-pinned
nixpkgs input. Nix emitted ignored SQLite evaluation-cache contention warnings in
some parallel checks; the actual test/lint commands exited zero.

## Miri failure retained, not waived

Job `20261028T083225-374aa055bf`, immutable source
`8242de59fbb20ff17eb9724fe154c36aedb257e6`, failed at
2026-10-28T08:50:28.447343Z after1082.4781163089792s wall time. The first nine host
commands/executions passed. Command10,
`browser::native_dense::tests::acknowledged_restart_keeps_media_and_owning_results_while_transport_is_unpolled`,
exceeded its unchanged900s watchdog. Its log has no passing summary. No complete
291-execution result exists; the other135 commands are not completed coverage.

After failure, all856 source entries and copied supervisor still matched. Known
supervisor/runner identities were no longer live. The supported collector returned
`Cannot collect passing evidence: failed`. The archive's `miri-failed/` preserves
job/source manifest, terminal/process state, runner/supervisor output, full expected
inventory/progress, nine completed logs and the partial timeout log;
`miri-failure.json` records verification and refusal. The immutable source/build
remain in the original job directory. No workload, seed, guard or watchdog was
weakened, and no replacement interpreter run was launched for this CI change.
Investigate the costly case and justify the smallest adequate affected recovery.

## Raw archive and remaining obligations

[`ci-hosts-stage7-logs.tar.gz`](ci-hosts-stage7-logs.tar.gz):4,228,071 bytes,
SHA256 `af477472b7b09e8a3ff155be1d89da7db2d176e06bbfef887b160324b40d538f`.
Root: `target/loop-layer-ci-stage7/`. Includes native/actual host reports, gates,
raw failed Miri state/logs, versions, attempted/actual validation commands,
implementation patch and the new helper-test sources.

Stage7 remains7/8. Miri recovery/complete coverage, real CI artifacts, Stage8
models/audit/benchmark, final source-matched verification, independent completion
audit and unmerged PR/final-head green required CI/review remain open. Workflow
configuration, local passes and partial Miri results do not satisfy those gates.
