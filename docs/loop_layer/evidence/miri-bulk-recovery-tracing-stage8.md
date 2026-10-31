# Bulk-MIDI interpreter recovery and tracing-inventory closure

## Collected exact recovery, not consolidated coverage

Job `20261028T145739-89dfd53bdb` **passed** from frozen implementation
`3d10c7a06f1e4ad01185d047e4a1c44abf525629`. It ran the unchanged complete test:

```sh
rustup run nightly-2026-05-04 cargo miri test --locked \
  --manifest-path tests/loop_engine_host/Cargo.toml \
  -p loop_engine_host_fixture --lib \
  browser::native_dense::tests::acknowledged_restart_keeps_media_and_owning_results_while_transport_is_unpolled \
  -- --exact
```

The original timeout and measured-cost implementation justified this one-command
unprofiled retry. All seven internal replay runs/partitions, complete samples,
MIDI, semantic bytes, receipts, owner checks, allocator/destructor guards and the
900-second command watchdog remain unchanged. This is **one command/one test
execution**, seed0, not seven independently selected interpreter tests. The sole
interpreter flag is `-Zmiri-seed=0`; Rust/rustdoc warnings remain denied.

- Start: `2026-10-28T14:57:39.512395Z`.
- Terminal success: `2026-10-28T15:10:30.291589Z`, exit0.
- Supervisor monotonic wall time: **770.7792120630038s**.
- Selected-command monotonic wall time: **768.9439976660069s**.
- Raw summary: one passed, zero failed/ignored,58 filtered out. The harness's
  simulated time is not the wall-time measurement above.
- Pinned Miri: `0.1.0 (ad3a598ca4 2026-05-03)`, nightly-2026-05-04.

Supported `collect` succeeded. Post-terminal verification matched all876 frozen
source entries, source archive and copied supervisor, the declared exact inventory,
report/log hashes and dead recorded supervisor/runner process identities. `.pi/`
remains excluded. The original metadata/source/supervisor identities are retained
in [MIRI_JOBS.md](../MIRI_JOBS.md); the evidence archive includes the collected
record, raw job/process/state/report/log files and independent verification.

| Artifact | SHA256 |
| --- | --- |
| Interpreter report | `23cf65e9d6bba1f59db33fe949b7a8c8a1d41fd7daa70877928ed33c39d8d73a` |
| Raw passing log | `49fe557d99a10d0ef95536ac1dfac2443534ba7186cd541b77faa0c8af2e5509` |
| Collected record | `8fe42d40d987d3209074a74554bc81d7fa404ade6fa63006b47604e91e44200b` |

This resolves that exact command's failure at this snapshot. Earlier timeouts and
interrupted diagnostics remain failed; differing/censored runs are not a paired
speedup measurement or CI margin. **Complete current/final146-command/299-execution
coverage remains required**, including the newly registered bulk-buffer model.
No remote native/worklet/Miri acceptance, reviewer approval or final delivery is
claimed. Stage7 remains7/8.

## Tracing inventory row verified

The existing implementation-stage inventory changes already added37 portable
source entries relative to the recorded `cc6ccb4a` baseline. A current exact file
comparison finds all54 `.rs` source files in the two libraries, including four
private test modules, represented in `docs/tracing_coverage.csv`. No inventory
edit is necessary merely to repeat those earlier additions.

Fresh commands, run inside the repository Nix environment:

```sh
python3 scripts/check_tracing_coverage.py --require-closed
python3 scripts/check_rt_runtime_boundary.py
python3 scripts/check_loop_engine_boundary.py
python3 scripts/check_shoop_test_usage.py
```

All pass. The closed inventory contains207 modules:63 excluded,33 directly
instrumented and111 indirectly instrumented. Every portable entry has its explicit
rationale. Production/build dependency checks pass for Linux, Windows, macOS and
Wasm targets; these are dependency-graph checks, not remote platform builds.

Application-tracing exclusion does **not** exempt callback code from validation.
The runtime exposes render/maintenance/fault diagnostics; the resolver exposes
semantic events, and the prepared observer/independent host encodes complete
semantic journals outside application tracing. The normal `shoop_test` macro
wraps test execution in native/Wasm tracing boundaries; no `no_trace` override
exists in the runtime/loop/fixture source and test trees reviewed here. Existing
heap/destructor guards, actual worklet canaries and interpreter obligations are
unchanged. No application tracing dependency or callback span was introduced.
The source/inventory/checker inputs match implementation3d10c7a0 through the
bookkeeping HEAD2db979ad recorded by the proof.

This verifies the Stage8 tracing-inventory row: **Stage8 is4/8**. It does not close
the separate exhaustive callback audit, broader models, consolidated Miri or
source-matched remote CI row. The archive's partial transaction review records
selected graph/topology/media/role owner moves, cancellation and callback-start
quota keys, with its remaining reachability/failure work explicitly open.

## Evidence

- Archive: `miri-bulk-recovery-tracing-stage8-logs.tar.gz`,15 files,94,804 bytes.
- SHA256: `ee2aad75dbaf0c5382acb079f0a2d4a3c984ea7a50a4275d77dda4eb63a85c7e`.
- Local scope: `target/loop-layer-bulk-midi-recovery-stage8/` and
  `target/loop-layer-callback-audit-stage8/`.

The preceding native/portable bulk-MIDI archive predates interpreter launch. This
new archive, rather than that earlier archive, preserves the terminal collection.
There are no production/test/dependency/runner changes in this milestone.
