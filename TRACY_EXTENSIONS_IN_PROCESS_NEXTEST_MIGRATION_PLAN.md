# Tracy Extensions In-Process Capture and Nextest Migration Plan

## Status and execution contract

- Status: implementation complete locally; CI canary and full matrix verification are pending.
- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised only for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
- If a required upstream contract cannot be made compatible with ShoopDaLoop, stop with the evidence gathered, attempted paths, blocker, and specific decision or input needed.

## Goals and scope

1. Move the ShoopDaLoop Tracy skill from the archived `SanderVocke/tracy-query` v0.1.0 release to the `SanderVocke/tracy-extensions` v0.4.0 `tracy-query` binaries and matching skill documentation.
2. Replace the native tracing CLI's external `tracy-capture` child-process controller with the patched `tracy-client-sys` 0.28.0 embedded-capture backend from `tracy-extensions` v0.4.0, consolidating tracing into one `--tracing` option that always captures in process.
3. Migrate Rust CI test execution from `cargo test` to pinned cargo-nextest, retain useful resource isolation, and run with a measured medium-aggressive concurrency profile.
4. Integrate `tracy-nextest-capture` so eligible failing tests publish finalized failure-only `.tracy` files, while passing attempts discard captures.
5. Prove the integration locally and in GitHub Actions with a controlled intentional failure, validate its trace, and upload failure traces as CI artifacts before making nextest the final Rust CI runner.

In scope: workspace Cargo configuration and lockfile, native tracing capture code and CLI behavior, eligible synchronous Rust tests, `.config/nextest.toml`, Rust-related steps in `.github/workflows/build_and_test.yml`, the Tracy skill, and affected developer/user documentation. Browser workflow scripts, non-Rust checks, benchmarks, packaging logic unrelated to the Tracy linkage, and changes to the upstream `tracy-extensions` repository are out of scope.

## Immutable acceptance criteria

- [ ] `.agents/skills/tracy/SKILL.md` downloads `tracy-query` from `SanderVocke/tracy-extensions` release `v0.4.0`, selects the correct existing platform asset, obtains the matching `tracy-query/SKILL.md` from the v0.4.0 source tag, and contains no operational dependency on the archived repository or v0.1.0 assets.
- [ ] Cargo resolves exactly one patched `tracy-client-sys` 0.28.0 from the pinned `tracy-extensions` v0.4.0 source for native capture/test builds, with unmodified exact `tracy-client` 0.18.4 and `tracing-tracy` 0.11.4 using compatible manual-lifetime features; `cargo tree -i tracy-client-sys` proves the resolution.
- [ ] `shoopdaloop` exposes one tracing mode: `--tracing` always performs embedded capture, `--tracing-capture` is removed, and `--tracing-engine-detail` requires `--tracing`.
- [ ] `shoopdaloop --tracing` requires no `tracy-capture` executable, `TRACY_CAPTURE_TOOL`, wrapper, signal handling, TCP connection, or helper process. It configures embedded capture before Tracy starts and saves a non-empty, valid capture under `./traces` on orderly shutdown.
- [ ] Application capture uses the upstream one-lifecycle contract: one configure/start/finalize sequence per process, an output path that does not already exist, joined/quiescent instrumentation producers and dropped span guards before finalization, atomic publication, explicit errors, and no `.partial` file represented as a successful trace.
- [ ] TCP/live-profiler support is removed as explicitly approved: the linked Tracy client uses only the embedded transport, and documentation does not suggest that a Tracy GUI or external capture server can attach live.
- [ ] A pinned cargo-nextest version compatible with the upstream integration (initial pin: 0.9.116) runs every CI Rust test command that cargo-nextest supports; unsupported harness cases remain explicit and documented rather than silently omitted.
- [ ] The CI nextest profile has `fail-fast = false`, starts with four test threads as the medium-aggressive setting, and preserves or strengthens serialization for Carla worker/deadline-sensitive and other measured high-resource tests. Concurrency is reduced only in response to recorded local/CI evidence.
- [ ] Under `TRACY_NEXTEST_CAPTURE=failure`, every opted-in passing attempt leaves no final `.tracy`, and every opted-in unwind panic or `Result::Err` publishes one uniquely named, non-empty finalized trace while preserving the original test result. Unsupported abort, signal, timeout, OOM, panic-abort, `#[should_panic]`, and async/custom-harness cases are documented as not capturable by this integration.
- [ ] A checked-in, ignored/opt-in ShoopDaLoop failure smoke fixture exercises real repository code/instrumentation. Local and opt-in CI runs intentionally fail it, observe the expected nonzero test result, verify the produced trace with v0.4.0 `tracy-query check`, `range`, `info`, and a semantic marker query, and find no `.partial` file.
- [ ] GitHub Actions uploads finalized nextest failure traces with `if: always()` and matrix-unique artifact names. A recorded CI smoke run proves an intentional failure trace is present in the uploaded artifact; the normal required CI path remains green and never runs the intentional failure by default.
- [ ] The final workspace test gates pass under nextest with the existing required feature/backend environment, formatting and `RUSTFLAGS="-D warnings"` build gates pass, tracing coverage remains closed, and affected tracing/testing documentation no longer instructs users or CI to use external `tracy-capture` or serial `cargo test` where superseded.

## Design rules and constraints

- Pin the integration to the v0.4.0 tag/revision; do not consume a moving branch. Keep the Tracy 0.13.1/protocol-76 compatibility boundary explicit.
- Prefer Cargo's git package and `[patch.crates-io]` support for the nested upstream crates; do not copy untracked upstream C++/Rust sources into this repository unless reproducible Cargo consumption proves impossible and the plan is revised with evidence.
- Configure embedded capture before any `tracy_client::Client::start()` call. In the current application this means capture path/configuration must precede `shoop_common::init()`, because logging initialization constructs the Tracy layers/client.
- Use only the v0.4.0 embedded transport. TCP/live-profiler support and the separate `--tracing-capture` mode are intentionally removed; do not retain dead runtime selection code or claim that an external GUI can attach.
- Finalize only after eframe/application/engine workers and active tracing guards are quiescent. Do not finalize from a panic hook, signal handler, forced timeout, or `Drop` path that can race producers.
- Preserve collision-safe numbered application filenames where practical. Retain a simple manifest only if it accurately reflects embedded finalization; remove external-process-only fields and `tracy-capture.log` expectations.
- Use the upstream `tracy-nextest-capture` attribute/runtime rather than recreating its catch-unwind, save/discard, attempt-identity, and atomic-publication policy. A small Shoop-specific test wrapper is allowed to scope `tracing-tracy`, enable Shoop's tracing gates, and drop them before the upstream finalizer.
- Opt tests in deliberately. Exclude or separately handle the four current `#[should_panic]` tests, no-allocation tests, tests that manually own a Tracy client, and any test that cannot prove producer quiescence. Maintain an auditable exclusion list.
- Nextest traces are diagnostic artifacts and may contain source/code metadata. Upload them only to the existing CI artifact boundary, with normal repository/action permissions and bounded retention.
- Do not treat missing traces from aborts, forced termination, or timeout kills as an integration success. Upload only final `*.tracy`, never `*.partial`.

## Staged implementation

### Stage 1 — Pin upstream contracts and establish the embedded-only CLI baseline

- [x] Record the v0.4.0 tag commit, component versions, release asset names, Cargo package locations, required features, ABI version, nextest environment variables, and lifecycle/failure limitations in this plan's progress notes.
- [x] Add a disposable Cargo metadata/tree probe (not committed) proving that tag-pinned git dependencies resolve the nested `tracy-nextest-capture` and patched `tracy-client-sys` packages without duplicate sys crates.
- [x] Build a minimal embedded probe, or inspect/link-test the upstream socket selection, to confirm that `embedded-capture` replaces network sockets for the whole linked client and successfully saves a trace.
- [x] Record the approved CLI contract in implementation notes: retain `--tracing` as the sole tracing mode, make it always capture in process, remove `--tracing-capture`, and make `--tracing-engine-detail` require `--tracing`.

Progress evidence: v0.4.0 resolves to commit `8fe922290c1fedfb35779713ff91a8306ebb50a5`; ABI v2, Tracy 0.13.1/protocol 76, exact Rust package versions, six query assets, one-lifecycle/fatal-exit limits, and nextest identity variables were reviewed from the tagged source. Cargo metadata/tree resolved the nested git packages and one sys crate. The embedded smoke published and queried a valid capture; the approved CLI is embedded-only.

Verification: `cargo metadata` and `cargo tree` evidence names the pinned source and one sys package; the probe demonstrates embedded save and absence of a live socket path; CLI parsing has one unambiguous tracing mode.

### Stage 2 — Adopt the patched sys crate and implement application embedded capture

- [x] Update root `Cargo.toml` with exact higher-level Tracy versions/features, a direct embedded-capable sys dependency, and the tag-pinned `[patch.crates-io]`; update `Cargo.lock` and audit duplicate/default feature activation.
- [x] Rewrite `src/rust/shoop_common/src/tracing_capture.rs` around ABI-v2 configure/state/error/statistics/finalize calls. Remove executable lookup, child processes, logs, signal/wrapper code, TCP connection polling, and external-capture-only errors/tests.
- [x] Preserve safe output-directory creation, confined/sanitized collision-free application naming, non-overwrite behavior, useful finalization diagnostics/statistics, and an accurate embedded manifest if retained.
- [x] Simplify `NativeCli` and `NativeTracing` in `src/rust/shoopdaloop/src/main.rs`: remove `--tracing-capture` and the tracing-mode argument group; make `--tracing-engine-detail` require `--tracing`; and make `--tracing` enable both instrumentation and capture.
- [x] Reorder startup so it sets gates and configures embedded output before logging starts, initializes the Tracy client/layers, waits for capturing, runs the app, quiesces producers/guards, and finalizes exactly once.
- [x] Make startup, normal shutdown, GUI failure, and finalizer failure paths explicit; prevent `Drop` from attempting an unsafe second finalization.
- [x] Update CLI parser/unit tests to reject the removed option and add a process-isolated native capture integration test for successful publication, existing-path rejection, and clear diagnostics.

Progress evidence: the process-isolated library smoke published a 621-byte trace; `tracy-query` 0.4.0 `check`, `range`, `info`, message query, and zone query succeeded. The hidden application smoke path published a 706-byte `0001-application.tracy` with the application marker and no partial.

Verification: warning-denying builds on supported native targets; focused nextest tests; no `tracy-capture` process or network listener; one non-empty capture, no partial, expected manifest state if retained; v0.4.0 `tracy-query check`, `range`, `info`, and a query for a known application marker/zone all succeed.

### Stage 3 — Update Tracy capture/query documentation

- [x] Change `.agents/skills/tracy/SKILL.md` release URLs, tag, asset/skill download commands, compatibility text, embedded application capture procedure, expected files, and validation checklist.
- [x] Update `src/rust/shoopdaloop/README.md` and `docs/source/developers.tracing.rst` to document `--tracing` as capture-only, remove `--tracing-capture`, `TRACY_CAPTURE_TOOL`, live-profiler instructions, and external-capture setup, and explain orderly in-process finalization and unsupported fatal exits.
- [x] Search the repository for stale v0.1.0, archived-repository, `tracy-capture`, wrapper/log, and obsolete capture-manifest instructions; update all user-facing matches within scope.

Verification: execute the documented v0.4.0 download/version/check commands on the generated application capture; repository search finds no stale operational instructions except intentional historical text.

### Stage 4 — Add Shoop-aware nextest failure capture and prove it locally

- [x] Add tag-pinned `tracy-nextest-capture` as a workspace/dev dependency and, if needed, a narrowly scoped test-support runtime/macro that composes the upstream attribute with a scoped `tracing-tracy` subscriber and Shoop tracing gates.
- [x] Ensure the Shoop wrapper initializes only after the upstream capture starts, returns the original `()`/`Result` value, joins test-owned producers, and drops dispatchers/spans/gates before upstream save/discard finalization.
- [x] Inventory test functions by supported signature/harness and record explicit exclusions (`#[should_panic]`, no-allocation/manual-client tests, unsupported harnesses, and unquiesced workers).
- [x] Add an ignored, exact-filterable failure smoke fixture that enters repository tracing code, emits a stable Shoop marker, and intentionally unwind-panics (plus a passing control using the same instrumentation).
- [x] Run the passing control with failure-only policy and prove the fresh output directory stays empty.
- [x] Run the intentional failure locally under nextest, assert the command fails for the intended panic, and validate its sole final trace with v0.4.0 `tracy-query`; reject any partial and record commands/results in this plan.
- [x] Confirm ordinary `cargo test`/listing and incomplete nextest identity are inert, as required by the upstream integration.

Progress evidence: 1,298 supported tests use the upstream attribute. Explicit exclusions are four `#[should_panic]` tests, 32 no-allocation/manual-client or embedded-lifecycle tests, and the ignored canary. Failure policy left the pass directory empty. The intentional failure exited 100, published exactly one trace with no partial, and `check`, `range`, `info`, Shoop message query, and Shoop zone query succeeded.

Verification: pass-discard and failure-save evidence, semantic query output containing the Shoop and nextest attempt markers, preserved panic/result status, unique filename, and no finalizer diagnostic or partial file.

### Stage 5 — Migrate eligible tests and tune medium-aggressive nextest concurrency

- [x] Extend `.config/nextest.toml` with `nextest-version.required = "0.9.116"` and a CI profile using `fail-fast = false`, `test-threads = 4`, no retries by default, concise success output, and complete failure output.
- [x] Keep the existing `carla-worker` group and full-thread reservations; add overrides only for tests shown by baseline behavior or CI-reproduction runs to contend on global audio/MIDI/GUI resources or deadlines.
- [x] Apply the Shoop/upstream capture attribute to eligible synchronous tests in reviewable package batches, running each package before proceeding. Keep exclusions plain and documented rather than weakening their semantics.
- [x] Translate package, feature, release, exact-name, ignored, and Web-host Rust test invocations to nextest filters/options without changing which tests are intended to run.
- [x] Compare serial baseline and four-thread runs, including the complete retained workspace command and resource-sensitive Carla suites. Use repeated runs and the techniques in `.agents/info/ci-repro.md` before changing concurrency.
- [x] Update `.agents/info/test.md`, `.agents/info/troubleshooting.md`, `INSTALL.md`, and `docs/source/developers.software.rst` with canonical nextest commands and the explicit fallback role, if any, of ordinary `cargo test`.

Progress evidence: the four-thread complete workspace ran 1,333 tests successfully in 10.796 seconds without capture. One shared-state dummy topology test failed on the first parallel run, passed alone, and now reserves all test threads. The complete failure-capture run then passed all 1,333 tests (1 ignored) in 159.808 seconds and published no success traces.

Verification: nextest's listed test set matches the intended cargo-test baseline; complete workspace and targeted suites pass repeatedly at four threads; exclusions are accounted for; wall time/resource observations and any override are recorded.

### Stage 6 — Add an opt-in CI intentional-failure canary and artifact proof

- [x] Install cargo-nextest 0.9.116 reproducibly in CI and include its version in diagnostics/cache considerations.
- [x] Add a non-default `workflow_dispatch` input or equivalent opt-in canary path on the Linux debug job that creates a fresh trace directory and runs only the ignored intentional-failure fixture.
- [x] Have the canary assert that nextest returned nonzero for the expected failure, then download/use the v0.4.0 Linux `tracy-query` asset to validate the final trace and stable semantic marker; fail the canary for no trace, multiple unexpected traces, a partial, or a query failure.
- [x] Add an `if: always()` upload step with a matrix/run-unique artifact name, `if-no-files-found: ignore`, bounded retention, and a path matching only finalized `.tracy` files.
- [ ] Push the canary milestone, trigger it, download the artifact, rerun `tracy-query check` locally, and record the workflow URL/run ID, artifact name, trace name, intentional test failure, and validation result in this plan.

Verification: GitHub Actions visibly executes an intentional Shoop test failure, subsequent validation and upload steps run, the downloaded artifact contains the valid queried trace, and the overall controlled canary reports success only because the expected failure was positively checked.

### Stage 7 — Final CI migration and end-to-end validation

- [x] Replace every supported Rust `cargo test` invocation in `.github/workflows/build_and_test.yml` with the equivalent pinned nextest invocation and CI profile, including native package sets, complete Linux workspace, real Carla targeted tests, Carla UI smoke, and browser-independent host tests.
- [x] Set `TRACY_NEXTEST_CAPTURE=failure` and a fresh absolute matrix-specific output directory for captured test steps; retain required `SHOOP_ALLOW_MISSING_BACKENDS`, `SHOOP_REQUIRE_CARLA_TESTS`, features, release mode, and warning flags.
- [x] Keep failure-trace upload after test steps with `if: always()` so an earlier test failure cannot skip artifact publication; do not upload empty/partial files.
- [x] Run the full local gates: `cargo fmt --all -- --check`, `RUSTFLAGS="-D warnings" cargo build --workspace`, `python3 scripts/check_tracing_coverage.py --require-closed`, and `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend --profile ci`.
- [ ] Run normal CI across Linux, Windows, macOS, debug/release, and web matrix entries; investigate failures at four threads before adding narrowly justified overrides. Re-run the opt-in intentional-failure canary after the final workflow shape.
- [ ] Audit final dependency trees, trace artifact names/retention, documentation searches, normal application startup without tracing, embedded application capture, pass-discard behavior, failing-test save behavior, and unsupported-failure documentation.
- [ ] Remove any temporary non-ignored failure or one-off workflow code; retain only the ignored opt-in smoke fixture/canary and the standard failure upload path.

Verification: all normal required CI jobs pass using nextest, the canary artifact proof still passes, `--tracing` produces a valid in-process application trace without external tools or live TCP support, passing tests publish no failure traces, an eligible failure publishes and uploads one valid trace, and all immutable acceptance criteria above are checked.
