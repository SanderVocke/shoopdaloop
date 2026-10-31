# Consolidated Stage 7 interpreter recovery collected

**Passed and collected**, not merely launched: job `20261028T153110-784d2380c0`
proves immutable `3bf1dbca7c93f486be50cd7359555172dad723f3` with the complete
**146-command/299-execution** selection:136runtime+100loop+63host. The standalone
host runs all59 seed0 tests plus seeds1/2 for primitive-wire and graph actors.
Every previously selected runtime/loop case, count, seed, assertion and workload
is retained, including the recovered dense seven-replay command.

| Field | Verified result |
|---|---|
| Started | 2026-10-28T15:31:10.449476Z |
| Ended | 2026-10-28T18:25:48.965005Z |
| Supervisor monotonic wall time | **10478.51554903097s** |
| Selection | 146 commands/299 executions, all expected counts passed |
| Toolchain | nightly-2026-05-04; Miri0.1.0, commit`ad3a598ca4` |
| Interpreter flags | Only each declared `-Zmiri-seed=0`, `1` or `2`; no profiling/safety override |
| Rust/rustdoc flags | `-D warnings` |
| Per-command watchdog | Unchanged900 seconds |
| Source | 878 immutable entries, no`.pi/`, isolated source/build and copied supervisor |
| Processes | Recorded supervisor32802 and runner32804 identities are dead |

Supported commands:

```sh
nix develop --command python3 scripts/run_rt_runtime_miri_jobs.py status \
  /home/sander/dev/shoopdaloop/target/loop-layer-miri-jobs/20261028T153110-784d2380c0
nix develop --command python3 scripts/run_rt_runtime_miri_jobs.py collect \
  /home/sander/dev/shoopdaloop/target/loop-layer-miri-jobs/20261028T153110-784d2380c0
```

Collection succeeded. The archived one-off `commands/verify.py` additionally:

- Rechecks all878 source-manifest entries and the copied supervisor.
- Re-exports the exact committed literal-path archive, excluding`.pi/`, and matches
  its recorded archive digest. The development checkout cannot alter the input.
- Loads the frozen runner and compares its complete JSON-normalized inventory;
  verifies all146 package/manifest/target/filter/count/seed/flag records and all147
  report/raw-log artifact hashes against the supervisor terminal result.
- Checks strict environment, source commit, package/shard/profile scope and pinned
  interpreter version; rejects missing test summaries or failed counts.
- Confirms neither recorded process identity remains alive, and the supported
  collection contains the same immutable job and terminal result.

| Artifact | SHA256 |
|---|---|
| `job.json` | `2ef0873da407abdeb054ecdd5e8c595d3f6fa36ab1fb10af3cd7400ea83bfc2b` |
| Re-exported source archive | `20d8a871153e2f37c871360738b5125344d00fc7402fe3ffa49415c7b369780c` |
| Copied supervisor | `75dfb8fdc9b45957a2bfd01e679216e0f5ce28be2dffafdf8aba5b938a36fa1d` |
| `reports/result.json` | `dc35c38bbe0d2e693670b68b02cf684982627453751076189569c42640b16d7d` |
| `collected.json` | `7af251c86bcd7b3bbb3c5586fa57c20533164bb74621628f50b1bc8eda8c17ca` |

## Scope and remaining work

This completes the frozen consolidated Stage7 interpreter recovery. All earlier
failed/interrupted campaigns remain failed; a later complete pass does not erase
those records. Wall time is the measured supervisor clock, not Miri's simulated
harness time, a native callback benchmark or a paired speedup/CI-margin claim.

It does **not** include the later descriptor-bank case or the four
[timeline/delivery/capture model cases](adversarial-models-stage8.md). Their current
selection is151 commands/304 executions. That applicable Stage8/final selection
still requires its own complete source-matched result; native exact dispatch,
package-Wasm tests and this old snapshot cannot substitute.

Stage7 remains7/8 because checked source-matched remote native/worklet/Miri
artifacts are not yet accepted. Broader model verification closes a separate
Stage8 row, not its interpreter or remote-CI rows. Final-head CI, review,
independent final audit and Stages9–10 remain mandatory. No push, merge or final
acceptance is claimed.

## Raw evidence

[Complete collection/proof archive](miri-consolidated-stage7-logs.tar.gz):
**170103 bytes/159 files**, SHA256
`e9046e4ff70dbf9279a5e4a65e92a81940e71fc1add42bc0633aa0004ed187c7`.
All archive members were compared byte for byte with local sources. It contains
complete raw case logs/report/progress, immutable metadata/manifest, process
identities, copied supervisor, collection and terminal verification command/proof.
The source itself remains reproducible from the committed snapshot and verified
archive digest; no unrelated local state is packaged as an input.
