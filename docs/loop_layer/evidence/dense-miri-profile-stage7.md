# Dense interpreter timeout: preserved retry and diagnostic support

Base: `dbe6d6236d7e7a8266d9dc75af5346490cfdf7fe`; profiling-support changes were
uncommitted during tooling verification. Production/fixture Rust, Cargo manifests
and lockfiles are unchanged in this milestone.

## Failed exact-test retry

Job `20261028T094652-f2718d3398` attempted exactly one seed0/full native-workload
execution of
`browser::native_dense::tests::acknowledged_restart_keeps_media_and_owning_results_while_transport_is_unpolled`.
It failed at2026-10-28T10:01:54.404514Z after901.8694647719967s wall time, exceeding
the unchanged900s command watchdog. **Zero executions completed**; its log ends
inside the test without a passing summary. All862 frozen source entries/copied
supervisor remain verified, recorded process identities are no longer live, and
collection refuses `Cannot collect passing evidence: failed`. The earlier full
Stage7 job's nine passes remain separate historical partial coverage.

The off-render membership index was not sufficient to meet the watchdog. Two
censored timeouts are not a measurement of its speedup/slowdown. No further
production optimization or workload reduction is inferred from this investigation.

## One-command diagnostic profiling

The actual repeated failure justifies the smallest diagnostic selection: the same
full exact test, host batch10/63, seed0, pinned nightly-2026-05-04. `--measureme`
adds only `-Zmiri-measureme=<isolated-report-directory>` to the existing seed flag.
All assertions, seven replay runs/partitions, frames, MIDI/semantic bytes, owning
results, guards and900s watchdog stay intact. The option refuses selections with
more than one whole command. Inherited arbitrary MIRIFLAGS are still removed.

The supervisor forwards/discovers this mode in the frozen source; current/completed
records persist exact interpreter flags. Collection checks mode/flags and requires
nonempty hashed profile artifacts. The CI complete-set verifier refuses diagnostic
reports and changed interpreter flags. A timed-out profile is diagnostic data only,
not a pass; trace events completed before interruption cannot establish full test
completion or native callback performance.

Sixty Python tests pass (32 Miri tooling/synthetic plus28 loop parser/orchestration).
Boundary/test-usage/closed-tracing policies pass. Explicit comparison with the prior
runner confirms all144 commands/291 executions, cases/filters/counts/seeds, pinned
toolchain and manifest scopes are unchanged, including the exact diagnostic case.
Native Rust gates from `dbe6d623` remain their recorded results, not fresh reruns.
No real profiled interpreter was executed during these helper tests.

Analysis tool: `summarize v11.0.1`, measureme Git commit
`9aeaf4d0cfaafb13379f0d8f3c3b5e8c13071afc`, installed under
`$HOME/.cache/loop-layer-measureme-9aeaf4d0/`. Initial installation failed on the
read-only default Cargo home; a writable private cache fixed storage. The pinned
third-party tool then failed because its obsolete unsafe CPUID blocks trigger
current rustc's denied warnings. Only this external analyzer was built with
`RUSTFLAGS="-A unused-unsafe"`; project and interpreter warning-denied builds,
safety checks, guards and watchdog are unchanged. Its unsupported `--version`
probe is retained; `cargo install --list` records the actual package/revision.
Usage: `summarize summarize FILE.mm_profdata` or add `--json`.

Raw archive: [`dense-miri-profile-stage7-logs.tar.gz`](dense-miri-profile-stage7-logs.tar.gz),
56,797 bytes, SHA256
`ed3b637513fdf25a03526dd34006ade2de7438f3ca6a70e5c6e1f1f13bea79b7`.
Root `target/loop-layer-miri-profile-stage7/` contains failed-retry state/logs,
verified refusal, helper/policy results, inventory comparison, implementation patch
and failed/successful analyzer setup logs. Later diagnostic launch/outcome belongs
in [MIRI_JOBS.md](../MIRI_JOBS.md), not in the frozen native-only evidence above.
Stage7 remains7/8; complete interpreter recovery, CI artifacts, Stage8–10 work and
final-head CI/review/independent completion audit remain open.
