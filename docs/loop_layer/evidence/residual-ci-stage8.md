# Residual interpreter diagnosis and native-measurement CI

## Scope and result

Base/source: `0b853b4a68cdd73092ad62a6ebf22a2e2a931f84`, on
`feat/loop-execution-layer`. This milestone changes workflow wiring, one Python
configuration check and the affected performance guide. **No production Rust,
manifest, lock, dependency, interpreter runner/inventory or musical workload
changes.** Existing unrelated `.pi/`, public-book and planning edits are excluded.

- The failure-driven residual diagnostic is **failed**, not passing evidence.
- Linux release CI now invokes the existing complete native benchmark and uploads
  all reports/raw records/logs from a fresh run/attempt-scoped temporary directory.
- Seventy-two helper tests, actionlint and the exact local benchmark payload pass.
- Stage7 remains **7/8**, Stage8 **3/8**. Broader models, exhaustive callback audit,
  complete current/final Miri, remote artifacts and Stages9–10 remain open.

## Residual diagnostic: unchanged full test, intentionally interrupted failure

The measured-cost candidate's actual900-second unprofiled timeout justified
profiling the same complete command again. Relevant interpreted runtime, loop,
fixture/support sources, manifests/locks, Cargo configuration and runner scripts
are unchanged between `802833f4` and `0b853b4a`; launch verification records the
comparison. This is not a duplicate routine passing campaign.

```sh
nix develop --command python3 scripts/run_rt_runtime_miri_jobs.py start \
  --stage stage-7-residual-diagnosis --commit 0b853b4a \
  --package loop_engine_host_fixture --shard 10/63 --measureme
```

The sole selected test is
`browser::native_dense::tests::acknowledged_restart_keeps_media_and_owning_results_while_transport_is_unpolled`,
with `-- --exact`, seed0, all seven replay runs/partitions and every existing
assertion/guard. The only interpreter flags are seed0 and the declared absolute
measureme path. Strict Rust/rustdoc flags and the900-second command watchdog remain.
Pinned nightly is2026-05-04; Miri0.1.0 (`ad3a598ca4`).

| Field | Recorded result |
| --- | --- |
| Job | `20261028T131907-e502c4cd23` |
| Frozen source entries | 872; `.pi/` excluded |
| Started | 2026-10-28T13:19:07.543223Z |
| Ended | 2026-10-28T13:30:18.412422Z |
| Monotonic wall seconds | 670.8692323739524 |
| Terminal result | failed/exit1, zero completed test executions |
| Passing collection | refuses `Cannot collect passing evidence: failed` |
| Job metadata SHA256 | `7f0d08305c0c5438842d8f2e6172874c4fa3d8a7c3ae5221d2ce5ab22ab09261` |
| Source archive SHA256 | `a90474f0ed9a1313b78c77bf4244de67d486aef6d5f345406215ec58ac3afe20` |
| Copied supervisor SHA256 | `75dfb8fdc9b45957a2bfd01e679216e0f5ce28be2dffafdf8aba5b938a36fa1d` |

At the natural post-workflow-edit checkpoint, source/supervisor verification and
boot/start-tick identity, ancestry, exact command, seed and profiler-path checks
preceded SIGINT to interpreterPID3939463. That finalized measureme's string index
before hard termination. The recorded ancestry ends at runner3939334; supervisor
is3939332. The interpreter identity was
`4c34f2f5-a686-4c4c-b7c1-72e3af69de01:34424093`. Recorded supervisor/runner
identities are dead, source/supervisor still match, and no active job remains from
this attempt. Interruption deliberately preserves a **failed diagnostic**, never
a shortened passing workload.

The finalized raw profile is2,389,684,069 bytes, SHA256
`d4a413d92e027d3d7b9172d98f4d2fb24721086ca5465a7fb38867901f5bfafc`.
It remains at the job's `reports/measureme/loop_engine_host_fixture-3939463.mm_profdata`.
A separately hashed copy outside verified reports was decoded with the existing
pinned measureme analyzer v11.0.1/Git`9aeaf4d0cfaafb13379f0d8f3c3b5e8c13071afc`.
Only the analyzer's historical third-party build needed its documented warning
allowance; project/interpreter flags remain strict. Raw profiles are not duplicated
into Git; the decoded JSON and analysis/failure/provenance logs are archived.

### Completed function-event costs, not a completed test

The profile contains666.539662436 seconds of completed events and14,839 labels.
The following **inclusive times overlap**. Calls are completed function-event
counts, not test executions. Four returned `run_stalls` function events do not
complete the seven-replay harness test. Concurrent development/helper checks and
instrumentation also preclude controlled timing claims.

| Function/path | Calls | Inclusive seconds | Self seconds |
| --- | ---: | ---: | ---: |
| Engine process | 166 | 463.319142 | 0.006352 |
| Runtime route | 6,528 | 104.405218 | 0.870488 |
| MIDI merge window | 3,808 | 95.477682 | 4.402843 |
| MIDI packet parse | 209,644 | 78.209280 | 13.777029 |
| MIDI-buffer preparation | 44 | 74.401713 | 0.118576 |
| MIDI restoration | 528 | 66.904405 | 3.518201 |
| Ordered insertion | 104,556 | 62.153694 | 9.889077 |
| MIDI append | 69,704 | 52.872517 | 4.823953 |
| Indexed MIDI seek implementation | 532 | 47.399498 | 17.532859 |
| Event slice fill | 44 | 41.265476 | 5.663304 |
| Event vector initialization | 44 | 32.869808 | 7.370721 |
| MIDI event get | 279,916 | 22.585497 | 12.264126 |
| Checkpoint-before lookup | 532 | 13.229206 | 12.536884 |
| Journal preparation | 5 | 0.009473 | 0.000837 |

Large state-valued seek/result movement is also visible: two Result branching
labels each have532 events and about12.35s self time. Inspect those value paths
before proposing an API change. Immediate implementation candidates are safe bulk
Event initialization and already-validated whole-buffer MIDI merging, backed by
independent ordered/window/refusal/reset/reuse models. These are hypotheses, not
implemented improvements or measured speedups. Full pre-touch and complete packet
semantics remain obligations. Different interrupted prefixes cannot establish
before/after speedup or native callback performance.

No further routine unprofiled retry is launched in this milestone. The unchanged
current selection remains **145 commands/298 executions =135runtime+100loop+63host**.
A later exact-test pass alone would not satisfy consolidated current/final coverage.

## Release-measurement CI and local command rehearsal

`.github/workflows/build_and_test.yml` adds the Linux release payload:

```sh
python3 scripts/benchmark_loop_engine.py --iterations 1024 --trials 3 \
  --report-dir '${{ runner.temp }}/loop-engine-bench-${{ github.run_id }}-${{ github.run_attempt }}'
```

Both warning-denied environment flags are explicit. The matching artifact uses
that complete directory, `always()` and `if-no-files-found: error`. Run/attempt
scoping avoids restored/stale reports; the driver's existing fresh-directory,
source/binary-drift, raw-data, scalar/ownership and exact heap-witness checks remain.
The Linux debug helper gate now includes `scripts.tests.test_loop_benchmark`.
Existing application/runtime/native/browser/Miri gates are retained.

A new helper test checks the Linux release selection, three1024-callback trials,
temporary/attempt-scoped directory, always-upload/missing-artifact failure and
benchmark-helper registration. The twelve benchmark tests plus existing sixty
Miri/loop parser/orchestration tests pass: **72 total**,6.578s observed. Workflow
lint passes with pinned actionlint1.7.9 and shellcheck disabled as in the preceding
workflow validation. This is syntax/configuration validation, not a remote run.

The exact payload was rehearsed locally, after the interpreter exited, under Nix
with `RUSTFLAGS="-D warnings" RUSTDOCFLAGS="-D warnings"` and a fresh local report
path. All14 cases ×3 trials ×1024 measured callbacks = **43,008** raw records pass,
with128 checked warmups/case/trial, **12,288** repeated same-frame cascade starts,
**six specific heap canaries** and complete scalar/content/request/pool/owner
checks. Every one of **6,144** dense/burst deadline misses remains in the result;
timing does not gate success. All scoped native source hashes match committed
`0b853b4a`; wrapper metadata honestly records its dirty worktree. Reports retain
binary/toolchain/hardware/configuration and raw timing/work/storage/credit data.
This rehearsal does not relabel or replace the preceding benchmark's measurements.

The affected performance guide now explains how to locate CI run/attempt artifacts
and distinguish local validation from actual remote acceptance. There is no new
package-Wasm, actual-worklet, generic-runtime-host or application campaign here.
Remote matrix execution, complete source-matched Miri and final-head review remain
open. No branch was pushed or merged.

## Initial callback inspection is not an exhaustive audit

The archived scan has149 lexical matching lines, including off-render preparation
and cleanup. Initial source inspection covered the executor/facade/node,
planner activation and transport snapshot/reliable-return paths. Its technical
notes record inspected invariants and remaining ownership/reachability work.
Neither a keyword scan nor unchanged canaries establish every descendant path's
safety; the Stage8 callback-audit row stays unchecked.

## Artifacts

Local root: `target/loop-layer-residual-stage8/`. The archive retains launch and
terminal verification, metadata/raw failed logs/interruption proof, decoded
profile/analysis, callback scan/inspection, helper/lint logs, exact CI-payload
report and all raw CSVs/canary logs, source scope and the implementation patch.
It excludes the2.39GB raw/copied profiles, immutable source/build trees and
unrelated worktree changes.

Archive: `residual-ci-stage8-logs.tar.gz`, **830,434 bytes**,78 files.
SHA256: `886735cc79b89a9450e3786231b3191bbb54e374fb7d3ecc4b00843a0985dbd2`.
