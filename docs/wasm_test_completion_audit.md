# WebAssembly test architecture completion audit

This audit maps `wasm_test_architecture_plan.md` to repository, runner, CI, and
PR evidence. Generated evidence paths are intentionally below `target/` and are
reproduced by the checked-in commands; they are not source artifacts.

## Objective and boundaries

The required end state is: portable Rust tests execute as real
`wasm32-unknown-unknown` binaries in selectable Node and Chromium runtimes;
production Worker/MessagePort behavior has reusable exact-asset fixtures;
native nextest remains authoritative; every test is classified; reports fail
closed; packaged browser automation contains no more than three irreducible
AudioWorklet smokes; documentation is reproducible; and PR #751 is green on
current `master`.

The branch contains squash-merged PR #749 and is merged with current `master`
(PR #752). `git diff origin/master` changes test attributes, test-only target
gating, harness/orchestration/reporting, CI, classification, and documentation.
There is no branch diff in `audio_worker.js`, `audio_worklet.js`, or
`raw_wasm_host.js`; no production protocol envelope, scheduler, host bridge,
driver, backend, application behavior, or UI behavior fork was added.

## Goal and artifact checklist

| Plan goal or named deliverable | Concrete artifact/evidence |
| --- | --- |
| Execute portable Rust tests as Wasm | `shoop_test_macros::shoop_test`; 1,175 shared native IDs and 1,179 actual tests in each Wasm runtime |
| Standard wasm-pack/wasm-bindgen harness | `scripts/run_wasm_tests.py`; locked `wasm-bindgen 0.2.127`, `wasm-bindgen-test 0.3.77`; pinned wasm-pack 0.15.0 |
| Selectable Node and browser runtimes | `--runtime node|chrome`; identical checked inventory hash in `tests/wasm_test_classification.toml` |
| Reusable production-boundary engine fixture | `shoop_wasm_runtime_tests/js/worker_fixture.js`, `tests/production_worker.rs`, and `tests/wasm/node_worker_bootstrap.mjs` |
| Preserve native nextest and shared source | Native macro expansion retains Tracy capture; canonical complete native command passes 1,428 executed plus two skipped IDs |
| Minimize packaged browser smoke | `docs/wasm_smoke_migration.md`, `scripts/check_wasm_smoke_budget.py`, and exactly two Chromium plus one Firefox workflow invocation |
| Actionable reports and JUnit | `wasm_test_report.py`, parser fixtures, per-package logs/XML, aggregate summaries, and inventory-policy JSON uploaded with `if: always()` |
| Shared support crates | `shoop_test_macros`, `shoop_wasm_test_support`, and `shoop_wasm_runtime_tests` workspace members |
| Canonical accounting | `wasm_test_inventory.py`, `check_wasm_test_inventory_policy.py`, and `wasm_test_classification.toml` |
| Reproducible records | `wasm_test_baseline.md`, `wasm_smoke_migration.md`, this audit, and the cross-target section of `src/rust/shoopdaloop/README.md` |

## Orchestration requirements 1–11

1. `validate_tools` verifies wasm-pack, Node major, locked binding versions,
   Rust, target installation, ChromeDriver, and browser before discovery.
2. `discover_packages` reads explicit Cargo metadata and stable-sorts packages;
   the workflow has no duplicate package execution list.
3. `stage_assets` invokes the worklet build once per profile.
4. It copies the exact production Worker/host, bootstrap, and import-free Wasm
   into `target/wasm-tests/<profile>/assets` with SHA-256 manifest entries.
5. `asset_server` owns one bounded CORS loopback server; the browser fixture
   owns and revokes module Blob URLs. Node uses file URLs and worker_threads.
6. One wasm-pack invocation is made per opted-in package with explicit runtime,
   no-default-feature, browser-feature, user-feature, and filter metadata.
7. Asset locations are test-only environment values; fixture bootstrap obtains
   protocol version/capacity from the Rust protocol example output.
8. `invoke_package` retains command, exact return code, combined raw output,
   elapsed time, and report paths.
9. `wasm_test_report.py` validates listed/executed/result counts and writes
   per-package JUnit; summaries and inventory reports are machine-readable.
10. Package deadlines are bounded by `--package-timeout`; all package execution
    is bounded by `--global-timeout`; Worker/test/browser waits are bounded and
    subprocess timeout paths are terminated.
11. Empty, malformed, truncated, mismatched, failed, crashed, timed-out, and
    teardown-failed states return nonzero. Synthetic JUnit represents runner
    failures with no testcase result.

## Fixture behavior checklist

`MultiWorkerFixture` creates arbitrary fresh instance lists with immutable
explicit/cooperative/realtime mode; independent production and fixture channels,
generations, raw hosts, schedulers, timers, and diagnostics; exact production
Worker/host/Wasm assets; Node worker_threads and browser Worker adapters; cached
immutable modules with fresh mutable state; explicit quantum audio; bounded
MIDI/protocol/diagnostic observations; typed readiness/revision/callback waits;
separate production and fixture shutdown; restart and stale-generation guards;
and teardown accounting for Workers, ports, hosts, timers, listeners, commands,
and Blob URLs.

The four runtime contracts prove explicit processing and two-instance isolation;
all three free-running modes and pause/resume/restart; sequence rejection,
commands, MIDI, diagnostics, and production shutdown; and terminal-capacity
failure isolation from a surviving peer. The shared worklet-client suite adds
readiness, replay, transfer, saturation, quiescence, stale-generation, and
multi-client evidence. Node is treated only as transport/runtime evidence;
physical callback authority remains in packaged smoke.

## Classification and migration checklist

The canonical report at `target/wasm-tests/dev/inventory.json` accounts for:

- 1,430 native IDs: 1,175 shared, 136 native-platform, 119 native-driver;
- 1,433 source declarations, including four Wasm-runtime-only declarations;
- 1,179 Node and 1,179 Chromium tests with identical IDs;
- no pending, unclassified, stale, overlapping, duplicate, silently missing, or
  unexplained runtime-specific tests.

The migration covers protocol, app API, plugin protocol, settings values,
sessions/media, scripting, engine graph/loops/MIDI/storage, backend/scheduler,
remote client, application orchestration, egui models, raw worklet host, and
browser MIDI composition. Native exclusions name concrete thread, filesystem,
allocator/lock, Tracy, JACK, CPAL, midir, Carla, subprocess, and platform-UI
reasons. The source/runtime count and membership hashes make unreviewed drift a
CI failure.

## Packaged smoke checklist

`wasm_smoke_migration.md` maps every former mode/assertion group to named shared
or runtime tests. CI retains only:

1. hosted Chromium output-only production AudioWorklet startup and callbacks;
2. self-contained Chromium embedded-asset AudioWorklet startup and callbacks;
3. hosted Firefox startup and callbacks as the second implementation.

Run `31905533513` measured 3.3 s, 13.4 s, and 44.9 s respectively (about 62 s
combined, excluding build/browser installation). Each observes application
startup commands, a running driver, positive callback/frame progress in exact
128-frame quanta, bounded overflow state, packaging policy, and clean browser
teardown. Domain, Worker, settings, Web MIDI, lifecycle, stress, and UI model
assertions are no longer packaged-smoke responsibilities.

## Fail-closed evidence matrix

| Failure class | Evidence |
| --- | --- |
| testcase/panic | Opt-in `wasm-test-failure-canary` returns nonzero in native, Node, and Chromium; Wasm logs and JUnit retain the panic |
| expected panic/ignored | Default support suite passes with five listed tests and one ignored; parser validates expected panic and ignored counts |
| compile or runner/browser crash | `test_compile_or_browser_crash_gets_synthetic_failure` and an observed missing-linker compile failure produce nonzero synthetic JUnit |
| malformed/truncated/count mismatch/zero discovery | Dedicated `test_wasm_test_report.py` fixtures all pass and assert failing JUnit |
| package timeout | `test_timeout_without_summary_fails_closed` plus the orchestrator timeout path |
| global timeout | Actual `--global-timeout 0` run returns nonzero and writes raw log plus three synthetic failing JUnit cases |
| Worker/teardown failure | Runtime contract rejects terminal Worker failure without harming its peer; fixture cleanup assertions include every owned resource |
| inventory drift | Source and runtime count/SHA policy plus full `--require-closed` inventory |
| dependency contamination | `check_wasm_test_dependencies.py` rejects native drivers, Carla, libloading, Tracy client/capture, and platform dependencies for all 15 opted-in packages |

## Acceptance-criterion audit

| Immutable criterion | Evidence/status |
| --- | --- |
| Actual wasm32 binaries in Node and Chromium | PASS: canonical commands invoke wasm-pack and report 1,179 tests/runtime |
| Same portable selectable inventory | PASS: identical count and SHA; four additions explicitly `wasm-runtime` |
| One source body unless classified | PASS: 1,175 shared IDs and closed source/native/runtime audit |
| Real ports, isolated production Workers, explicit/free-running, teardown | PASS: four runtime contracts and fixture ownership checks |
| No forbidden Wasm dependencies | PASS: all package dependency gates |
| Complete native nextest and Tracy capture retained | PASS: 1,428 passed, two skipped; macro expands to Tracy capture |
| Every excluded/platform test reasoned | PASS: 255 explicit native exclusions; no pending/stale/overlap |
| Raw logs, JUnit, overlap inventory, runner failure | PASS: report artifacts and fail-closed matrix |
| Smoke reduced only after replacements, no lower-layer duplication | PASS: migration map, three-invocation guard, 62-second measured boundary |
| Copyable local commands | PASS: technical README covers native/Wasm package/filter/runtime/policy/canary/smoke commands and pins |
| No production fork for tests | PASS: boundary diff audit; exact production assets/protocol are reused |

## Final command evidence

From a clean tracked worktree on current `master`:

- `cargo fmt --all -- --check` and `git diff --check`: pass.
- Warning-denying native workspace/all-target check with
  `shoop_engine/app_backend`: pass.
- Complete warning-denying native nextest command: 1,428 passed, two skipped.
- Canonical Node debug command: 1,179 passed.
- Canonical Chromium debug command: 1,179 passed.
- Full inventory with `--require-closed`: native 1,430, source 1,433,
  Node/Chromium 1,179, categories 1,175/136/119.
- Debug/release application Wasm checks and worklet builds: pass; both worklet
  modules have zero imports.
- Worklet-client and all-package Wasm dependency gates: pass.
- Hosted and self-contained Chromium plus Firefox smokes: pass in CI.
- Parser fixtures, explicit native/Node/Chromium canaries, global timeout,
  smoke budget, tracing inventory, and source/runtime policy gates: pass.

The final PR rollup and exact current CI run are recorded after the head run is
green; no completion claim is made while that evidence is pending.
