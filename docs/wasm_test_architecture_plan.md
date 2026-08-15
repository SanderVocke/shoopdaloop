# WebAssembly Rust test architecture plan

## Status and execution contract

This document is an implementation plan. It changes only test infrastructure, test organization, CI, and test documentation. The production refactoring listed under **Prerequisite assumptions** is required before this plan starts and is not part of this plan.

During implementation:

- Keep this plan updated and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not change without explicit user approval.

### Readiness decision

**Ready to implement, starting at Stage 0.** The completed worklet-backend refactoring provides every required production seam. Implementation must be based on code containing PR #749 and the completed `docs/worklet_backend_refactoring_plan.md`; if that PR has not merged yet, branch from its head rather than from an older `master`.

No downstream implementation item is checked in this document yet. Existing `raw_wasm_host_contract.mjs` and `worker_fixture_contract.js` are prerequisite evidence and useful characterization fixtures, not substitutes for the Rust/Wasm harness, runtime-selectable fixture, inventory, or JUnit work below.

The review found no production blocker. It did find test-infrastructure details that Stage 0–3 must address explicitly:

- the production dummy engine is Worker-hosted, not a deterministically driven physical AudioWorklet;
- `audio_worker.js` is now an ES module and can be reused from Node through a small `node:worker_threads` bootstrap shim rather than copied or reimplemented;
- the existing browser fixture hard-codes protocol/bootstrap values and is browser-global-specific, so the new fixture must source protocol constants from Rust and own runtime adapters;
- `wasm-bindgen-test` does not serve arbitrary static assets or emit JUnit, so asset serving and fail-closed result conversion are first-class orchestration responsibilities;
- Worker processing mode is selected at construction. Tests may pause/resume a running Worker, but must restart to change between explicit, cooperative, and realtime modes.

## Goals

- Execute the portable Rust test suite as `wasm32-unknown-unknown`, rather than merely compiling Wasm while executing selected tests as native Linux binaries.
- Use `wasm-pack` and `wasm-bindgen-test` as the standard Wasm test harness.
- Make Node.js and a headless browser selectable execution environments for the same portable test inventory.
- Provide reusable single- and multi-remote-engine fixtures using the same command/event protocol, Worker-hosted dummy engine, and message-port boundary as production.
- Keep native `cargo nextest` coverage and maximize shared test source between native and Wasm execution.
- Reduce packaged-application browser smoke testing to a small, quick set of checks that cannot be represented faithfully in the regular Wasm suites.
- Produce actionable CI output and JUnit XML for the Wasm suites.

The earlier term “dummy worklet fixture” means a remote engine fixture at the production protocol boundary. Deterministic and free-running fixtures use the production Worker dummy. Genuine AudioWorklet callback timing remains packaged-browser evidence and is never inferred from Worker scheduling.

## Scope

Included:

- Test attributes and test-support crates.
- Worker-hosted dummy-engine fixtures and runtime adapters for Node.js and browsers.
- Test asset generation and orchestration.
- Test classification metadata and reproducible inventory generation.
- Migration and classification of existing Rust tests.
- JUnit conversion, raw logs, inventory reporting, CI changes, and test documentation.
- Reduction of the packaged-application smoke suite after replacement coverage exists.

Excluded:

- Production driver, backend, protocol, application, or UI refactoring.
- Replacing native JACK, CPAL, midir, Carla, native storage, or Tracy integration tests.
- Claiming that Node.js or browser Worker scheduling is equivalent to AudioWorklet realtime scheduling.
- Replacing the final packaged-application checks with mocks.
- Adding a custom nextest browser or Node target runner.
- Forking `audio_worker.js`, `raw_wasm_host.js`, or production command/event envelopes for tests.

If a required production seam is missing, implementation stops and records the missing prerequisite instead of adding test-only branches to production behavior.

## Prerequisite assumptions

This plan assumes the following production refactoring has landed:

1. The physical audio driver is separated from remote-engine communication. The application observes one backend/domain interface and does not own `AudioContext`, `AudioWorkletNode`, Worker, or `MessagePort` details.
2. Remote commands and events use one bounded, versioned transport contract. The browser AudioWorklet and Worker dummy receive and publish the same production envelopes and enforce the same sequence, capacity, replay, shutdown, and error rules.
3. The dummy engine runs inside an independent Worker realm in:
   - **Explicit mode**, where fixture control requests exact configured quanta.
   - **Cooperative mode**, where bounded batches run with mandatory event-loop yields.
   - **Realtime mode**, where sample rate and quantum pace bounded catch-up.
4. Driver selection, clocking, and physical audio are hidden below the backend boundary. Application and engine behavior tests do not need a physical device.
5. Each remote engine instance has isolated engine and driver state, a distinct production message port, observable readiness and shutdown, and bounded diagnostics.
6. The portable application core is exposed from library targets. Tests do not have to start the packaged executable or create a real window.
7. The dummy engine is built as the standalone, import-free `shoop_audio_worklet.wasm` artifact and loaded through `raw_wasm_host.js` without duplicating protocol logic.
8. Browser-specific and native driver adapters remain separable from portable behavior.
9. Tests reset mutable state by constructing fresh application/client/Worker/port/host state and completing acknowledged teardown; no global production reset command is required.

### Prerequisite audit after production refactoring

The current code satisfies those assumptions:

- `shoop_worklet_client` is the platform-neutral backend/client. It owns bounded sequencing, generations, deterministic replay, readiness, typed outcomes, transfer assembly, and quiescence. Browser drivers receive only `RemoteBackendControl` and implement `MessageEndpoint`.
- `BrowserAudioDriver` and `BrowserWorkerDriver` are separate physical and dummy lifecycle owners. Browser presentation and host MIDI are separate from transport internals.
- `shoop_audio_worklet.wasm` is the standalone import-free engine artifact. `raw_wasm_host.js` owns the shared ABI, memory views, commands, processing, diagnostics, and destruction for both `audio_worklet.js` and `audio_worker.js`.
- `audio_worker.js` owns isolated host and scheduler state and supports explicit, bounded cooperative, and realtime modes. Fixture audio, explicit processing, pause/resume, fixture diagnostics, and fixture shutdown use a separately transferred fixture-only `MessagePort`; the production application port accepts only protocol JSON.
- `audio_worker.js` and `raw_wasm_host.js` are ES modules. A Node adapter should dynamically import the exact production Worker module after installing a minimal `self`/parent-port bootstrap around `node:worker_threads`; Node's Worker bootstrap is test infrastructure, while scheduler and host behavior remain production code.
- A browser Wasm test cannot assume that `wasm-bindgen-test-runner` serves repository assets. The browser runtime adapter must fetch the staged production scripts and Wasm from the orchestrator's CORS-enabled asset server, create module Blob URLs, rewrite only the raw-host import URL in memory, and revoke every URL during teardown.
- Rust fixture code can pass `shoop_audio_protocol::PROTOCOL_VERSION` and `COMMAND_MAX_BYTES` into Worker bootstrap. The new fixture must not copy the hard-coded values currently present in `worker_fixture_contract.js`.
- Isolation/reset is achieved by constructing fresh clients, message channels, Wasm hosts, scheduler state, and bridge state, then performing acknowledged shutdown and destruction. Generation guards reject late callbacks.
- Physical callbacks, permission policy, actual `AudioContext`/`AudioWorkletNode` wiring, and secondary-browser behavior remain packaged-smoke evidence.

Stage 0 must re-run artifact, dependency, teardown, and multi-instance contracts and prove the Node bootstrap and browser asset-loading strategy before broad migration. These are verification tasks, not requests for another production seam.

## Current baseline

The current workspace has 15 Rust packages. A source scan at this review found 1,419 `#[test]` or Tracy-capture test attributes. That is not an authoritative runnable inventory because target `cfg`, generated tests, integration binaries, ignored tests, and features affect discovery; Stage 0 must generate exact native and target-specific IDs.

The web CI job currently executes `cargo nextest` as a **native Linux process** for five selected packages:

- `shoop_audio_protocol`
- `shoop_audio_worklet`
- `shoop_worklet_client`
- `shoop_egui`
- `shoopdaloop`

Those package trees contain 232 test attributes in the same source scan. The job also checks/builds the actual `wasm32-unknown-unknown` application and worklet, verifies dependency isolation and the import-free artifact, and runs `raw_wasm_host_contract.mjs`, but it does not execute a Rust Wasm test binary.

The packaged browser matrix is intentionally broad today. Each web debug run has 13 Chrome invocations (seven hosted and six self-contained). Each web release run has those 13, five extended Chrome invocations, and one Firefox invocation. On the final PR #749 ready-for-review run, representative step times were:

| Web cell | Build | Host-native selected tests | Packaged browser workflows | Total job |
| --- | ---: | ---: | ---: | ---: |
| debug | 74 s | 54 s | 490 s | 11 m 44 s |
| release | 120 s | 100 s | 406 s | 12 m 03 s |

These are a review snapshot, not the Stage 0 benchmark. Stage 0 records exact commands, test IDs, per-step durations, and repeated uncontended timing because caches and hosted-runner load vary.

## Target test architecture

### Test layers

| Layer | Harness | Purpose |
| --- | --- | --- |
| Shared Rust behavior | Native nextest and Wasm-pack | Protocol, engine, backend, application, scripting, session, settings values, UI models, and deterministic dummy-engine behavior from shared source |
| Wasm runtime contracts | Wasm-pack in Node.js and Chromium | Wasm bindings, Worker lifecycle, real MessagePorts, production Worker module loading, and multi-engine fixtures |
| Native platform integration | Native nextest | JACK, CPAL, midir, Carla, subprocess/shared-memory behavior, native atomic storage, and native Tracy capture |
| Browser adapter contracts | Wasm-pack in Chromium | Browser APIs that need a browser but not the packaged application or physical AudioWorklet |
| Packaged-application smoke | Existing production artifact automation | Production bootstrap, artifact wiring, user gesture/permission policy, real AudioContext/AudioWorklet scheduling, and packaging modes |

### Planned test infrastructure

Use explicit, narrow components rather than adding test behavior to production crates:

- `shoop_test_macros`: the shared cross-target test attribute proc macro.
- `shoop_wasm_test_support`: runtime detection, asynchronous waits, asset metadata, JS adapter bindings, Worker/port lifecycle helpers, and bounded fixture observations. It may depend on protocol/domain data but must not force browser or native-driver dependencies into portable packages.
- `shoop_wasm_runtime_tests`: cross-package Worker/MessagePort scenarios that would otherwise create dev-dependency cycles, including full remote-client and multi-engine contracts.
- `scripts/run_wasm_tests.py`: package discovery, tool validation, one-time asset build/server ownership, wasm-pack invocation, timeout handling, raw logs, JUnit, and aggregate status.
- `scripts/wasm_test_inventory.py` and `tests/wasm_test_classification.toml`: canonical test accounting and explicit exclusion reasons.
- `docs/wasm_test_baseline.md` and `docs/wasm_smoke_migration.md`: reproducible baseline and assertion-by-assertion replacement records.

Generated files live below `target/wasm-tests/<profile>/`; test assets and reports are never written into or committed from `src/rust/shoopdaloop/generated`.

### Shared test attribute and runtime configuration

Add a test-support attribute used by portable test functions:

- Native expansion retains `tracy_nextest_capture::tracy_capture_test` behavior.
- Wasm expansion uses `#[wasm_bindgen_test]`.
- Synchronous, asynchronous, ignored, expected-panic, and intentional failure-canary forms have explicit macro tests.
- Portable async tests use one body; the native expansion drives the future with the approved test executor while the Wasm expansion leaves it async for `wasm-bindgen-test`.
- Tracy and its native dependencies are absent from the Wasm dependency graph through target-specific dependency gating.

`wasm_bindgen_test_configure!` may occur at most once per test binary and cannot be hidden safely in every test attribute. Each Wasm-capable unit or integration test binary therefore includes one shared configuration stanza. Node is the default. The `wasm-test-browser` feature emits `wasm_bindgen_test_configure!(run_in_browser)` exactly once; the orchestrator rejects incompatible runtime features. A pilot must prove this arrangement for both unit and integration test binaries before migration expands.

Use one test body for native and Wasm assertions. Platform-specific setup may be selected behind small fixture adapters, but assertions and scenarios must not be copied into parallel native and Wasm files.

### Toolchain and runtime pins

The initial compatible set is:

- the locked production `wasm-bindgen = 0.2.127` resolution;
- `wasm-bindgen-test = 0.3.77` and matching `wasm-bindgen-cli = 0.2.127`;
- `wasm-pack = 0.15.0`;
- Node.js 22.x for `node:worker_threads` and module Workers;
- the repository Rust toolchain and `wasm32-unknown-unknown` target;
- a matched Chromium/ChromeDriver pair installed by CI.

Pin exact patch versions in CI and lockfiles. The orchestrator prints and validates all versions before discovery; unsupported or mismatched tools fail before tests start. Version upgrades are ordinary reviewed maintenance and must re-run pilot, failure-canary, output-parser, and Worker-transfer contracts.

### Runtime selection, assets, and orchestration

Provide one repository command with an explicit runtime:

```sh
python3 scripts/run_wasm_tests.py --runtime node --profile ci
python3 scripts/run_wasm_tests.py --runtime chrome --profile ci
```

`--profile dev` uses debug test and worklet artifacts. `--profile ci` uses release Wasm/worklet artifacts and CI timeouts. Filtering is optional and must be reflected in report metadata; the unfiltered commands above are canonical gates.

The orchestrator must:

1. Validate pinned Rust, wasm-pack, wasm-bindgen, Node, and browser tools.
2. Discover the stable ordered list of Wasm-capable packages from explicit Cargo package metadata; do not maintain a second hard-coded package list in the workflow.
3. Build `shoop_audio_worklet.wasm` once for the selected profile, reusing or extending `build_worklet.py` with an explicit output directory.
4. Stage the exact production `raw_wasm_host.js` and `audio_worker.js`, the Node bootstrap, the worklet Wasm, and a hash/path/profile manifest under `target/wasm-tests/<profile>/assets`.
5. Start one bounded CORS-enabled loopback asset server for Chromium runs. The browser adapter fetches assets, creates per-fixture Blob module URLs, structured-clones a cached compiled `WebAssembly.Module` while transferring the ports, and revokes every URL after Worker readiness/teardown; Node uses file URLs and `node:worker_threads`.
6. Invoke `wasm-pack test` once per package with Node or headless Chrome selection and the correct package feature. Browser-only additions are separate report groups, not silent changes to shared inventory.
7. Pass asset location to test binaries through a compile-time test-only environment value. Protocol version and capacities come from the Rust protocol crate at fixture bootstrap, not the asset manifest or copied literals.
8. Preserve every command's exact exit status and raw stdout/stderr.
9. Run list/accounting and test execution in a fail-closed way, emit per-package/runtime JUnit XML, and write an aggregate machine-readable summary.
10. Enforce per-package, Worker-readiness, testcase, browser-startup, and global deadlines; terminate child processes and the asset server on every exit path.
11. Fail if discovery is empty, expected test counts differ, output is malformed, a browser/Worker exits, reports are missing, or cleanup is incomplete.

Because `wasm-pack` operates per package and browser startup occurs per Cargo test binary rather than per test function, retain a modest number of test binaries. Consolidate only when measured startup dominates; do not duplicate or flatten logical modules solely to reduce process count.

### Multi-Worker remote-engine fixture

Provide a reusable interface equivalent to:

```rust
let fixture = MultiWorkerFixture::spawn(&[
    ProcessingMode::Explicit,
    ProcessingMode::Cooperative,
    ProcessingMode::Realtime,
]).await?;
fixture.worker(0).process_quantum(input, 2).await?;
fixture.worker(1).pause().await?;
fixture.worker(1).resume().await?;
fixture.worker(0).wait_for_revision(expected_revision).await?;
fixture.shutdown().await?;
```

Processing mode is immutable for one Worker instance, matching production. Restart an instance to change mode. Exact sample injection/output capture is available only in explicit mode and one request must match the configured quantum.

Required behavior:

- Spawn any practical number of isolated Worker dummy engines for one testcase.
- Give every instance its own production `MessageChannel`, optional fixture `MessageChannel`, remote-client generation, and engine host.
- Instantiate the exact production `audio_worker.js`, `raw_wasm_host.js`, and `shoop_audio_worklet.wasm`; runtime adapters may bootstrap globals and asset URLs but may not fork scheduler, host, or protocol behavior.
- Cache immutable script text and compiled Wasm modules per test binary while creating fresh ports, host state, scheduler state, and controls per testcase.
- Use `node:worker_threads` `Worker`, `MessageChannel`, and `MessagePort` in Node.js.
- Use browser `Worker`, `MessageChannel`, and `MessagePort` in Chromium.
- Default behavior tests to explicit processing. Use cooperative or realtime modes only for lifecycle, responsiveness, pacing, backpressure, restart, and concurrency evidence.
- Wait on typed readiness, revisions, acknowledgements, callback counters, diagnostics, or bounded deadlines rather than arbitrary sleeps.
- Expose bounded fixture input/output audio, MIDI/application protocol observations, diagnostics, and callback counters. Do not send production audio over the application port.
- Exercise production-port shutdown and fixture-port shutdown separately.
- Shut down and verify every Worker, production port, fixture port, timer, Blob URL, listener, pending command, and host. Teardown failure fails the testcase.
- Keep a testcase generation identifier so late messages from a failed or completed fixture cannot affect another testcase sharing a Node process or browser page.

The Node port implementation is contract-compatible transport evidence, not browser event-loop or AudioWorklet timing evidence. Chromium fixture runs provide browser MessagePort/Worker evidence. Packaged physical tests remain the only AudioWorklet callback authority.

### Test classification and expected overlap

Every logical Rust testcase belongs to exactly one category:

- `shared`: the same source body runs under native nextest and the portable Node/Chromium Wasm inventory.
- `native-driver`: requires a native audio/MIDI/plugin driver or subprocess/shared-memory integration.
- `native-platform`: requires native filesystem guarantees, OS integration, global allocator checks, environment/process control, or native Tracy facilities.
- `wasm-runtime`: tests Worker, MessagePort, Wasm binding, or dummy-engine runtime behavior.
- `browser-adapter`: requires a browser API but not the packaged application.
- `packaged-smoke`: requires production artifact/bootstrap, browser policy, or genuine AudioWorklet behavior.

The canonical native inventory comes from `cargo nextest list` with the same features and backend policy as CI. Wasm inventories come from each pinned runner's list mode. `tests/wasm_test_classification.toml` records stable package/binary/test identifiers or narrowly reviewed patterns, category, and non-shared reason. The inventory tool rejects unmatched tests, stale classification entries, overlapping categories, duplicate IDs, unexplained Node/Chromium differences, and shared IDs missing from either runtime.

Source scanning supplements runner inventories so target-`cfg` tests cannot silently disappear before either runner lists them. Macro expansion and generated tests need explicit handling in the baseline report.

Do not set an overlap percentage before Stage 0 classification. The previous 90–95% estimate predates the current 1,419-attribute inventory and is not evidence. Maximize overlap package by package, but explicit accounting and defensible reasons—not a percentage target—are authoritative.

Migrate in evidence-sized waves:

1. protocol/value crates: `shoop_audio_protocol`, `shoop_plugin_protocol`, `shoop_app_api`, `shoop_settings`, and `shoop_session`;
2. portable runtime crates: `shoop_scripting`, `shoop_engine`, `shoop_backend`, `shoop_worklet_client`, and `shoop_app`;
3. UI/composition crates: `shoop_egui`, `shoop_audio_worklet`, and `shoopdaloop`;
4. classify `shoop_common`, `shoop_tracing`, and any remaining targets according to actual portable versus native responsibilities.

The Stage 1 pilot precedes these waves and may expose dependency or harness work that changes ordering.

### Packaged-application smoke boundary

Retain only checks that regular Wasm suites cannot represent faithfully:

1. A hosted production artifact boots, paints, accepts the required user gesture, creates a real `AudioContext` and `AudioWorkletNode`, advances real callbacks, and completes one application-to-worklet command and audio-route round trip.
2. The self-contained production artifact boots from its supported delivery mode, loads embedded application/host/worklet assets, and advances a real callback.
3. One secondary-browser hosted check confirms production bootstrap and real AudioWorklet compatibility where CI supports it.

Move protocol sequencing, saturation, restart replay, deterministic record/playback, session replacement, processor capability rejection, settings behavior, scripting, keyboard control, mocked permission behavior, Web MIDI model behavior, UI model behavior, and multi-Worker scenarios into shared, Wasm runtime, or browser-adapter suites before deleting smoke equivalents.

Keep browser-policy behavior that depends on production origin, user gesture, real permission APIs, autoplay, local-file delivery, or physical browser APIs in packaged smoke tests. `docs/wasm_smoke_migration.md` maps every current `browser_smoke.mjs`/Firefox assertion to replacement test evidence or one retained invocation.

The final smoke suite has at most three primary invocations and targets completion within five minutes on an uncontended CI runner, excluding artifact build and browser installation.

### Reporting

`wasm-pack` and `wasm-bindgen-test-runner` do not natively emit JUnit XML. The orchestration layer captures pinned terse output and emits JUnit with:

- stable testcase names matching inventory IDs;
- package, runtime, profile, tool-version, and filter properties;
- panic/failure output and retained raw-log references;
- elapsed suite time where the runner exposes it;
- explicit ignored/skipped classifications where representable;
- expected/listed/executed/passed/failed/ignored counts;
- a synthetic failed testcase for compilation failure, runner crash, timeout, malformed output, count mismatch, Worker/browser startup failure, or teardown failure.

Test the parser against checked-in successful, failed, ignored, expected-panic, malformed, truncated, timeout, browser-crash, and zero-test fixtures. Preserve raw logs and partial JUnit on every failure. A parser error never converts a failing or unknown command into success.

## Immutable acceptance criteria

- The canonical Node.js and Chromium commands execute actual `wasm32-unknown-unknown` test binaries through `wasm-pack`; neither command substitutes host-native test execution.
- The same portable test inventory can be selected for Node.js or Chromium, with any environment-specific additions reported separately.
- Portable tests use one source body under native nextest and Wasm unless an explicit classification documents why sharing is impossible.
- Multi-engine tests use real runtime message ports and isolated production Worker dummy engines, support explicit and free-running processing, and verify clean teardown.
- Wasm test dependency trees contain no native audio/MIDI drivers, Carla runtime, native Tracy client, or other forbidden native backend dependencies.
- Native nextest continues to run the complete native workspace and retains native Tracy failure capture.
- Every excluded or platform-specific test appears in the generated inventory with a reason; no test silently disappears under target configuration.
- Wasm CI publishes raw logs, JUnit XML, and the overlap inventory, and reports runner-level failure even when no individual testcase result is available.
- Packaged-application smoke coverage is reduced only after replacement evidence passes, contains no behavior already covered adequately by lower test layers, and meets the stated invocation and runtime bounds.
- Repository test documentation gives copyable local commands for native, Node.js Wasm, Chromium Wasm, and packaged smoke verification.
- No production behavior or protocol fork is introduced solely for tests.

## Implementation stages

Dependencies are sequential unless a stage explicitly says otherwise.

### Stage 0 — Confirm prerequisites and freeze the baseline

- [x] Verify every prerequisite assumption against code containing PR #749.
- [x] Prove a Node 22 module Worker can structured-clone `WebAssembly.Module` and transfer Node MessagePorts through the bootstrap shim while importing the exact production `audio_worker.js` and `raw_wasm_host.js`.
- [x] Prove a minimal wasm-bindgen Chromium test can fetch staged assets, create/revoke module Blob URLs, transfer the compiled module, and shut down one production Worker.
- [x] Stop and record a blocker for any missing transport, driver, lifecycle, asset-loading, reset, or library seam.
- [x] Add `docs/wasm_test_baseline.md` with exact native nextest IDs and current web-selected host-native IDs.
- [x] Record current CI duration by build, host-native tests, artifact/package contracts, each browser-smoke mode, and total job.
- [x] Provisionally classify every current test target and identify tests relying on threads, files, subprocesses, environment variables, global allocators, physical drivers, browser policy, or Tracy.
- [x] Record the current dependency trees, worklet artifact hash/imports, protocol version, and Worker teardown/multi-instance characterization results.

Verification:

- [x] The reproducible baseline accounts for every discovered native test and every source-level test not discovered under the native configuration.
- [x] The Node and Chromium feasibility probes use production Worker/host sources and leave no Worker, port, timer, host, listener, or Blob URL alive.
- [x] No implementation stage begins while a prerequisite remains unresolved.

Evidence: `docs/wasm_test_baseline.md` records the 1,420-test production native baseline, all 232 previously selected host-native web tests, source/native count differences, package waves, protocol constants, dependency boundaries, artifact hashes/imports, and measured CI steps. The canonical inventory now reproduces all native and source declarations. Checked-in Node and wasm-bindgen runtime probes load the exact staged production Worker, raw host, and import-free engine artifact, run two isolated explicit engines through real ports, and complete acknowledged teardown. The Chromium adapter fetches the staged assets through the bounded CORS server, uses revocable module Blob URLs, and passes under headless Chrome. No production prerequisite is missing.

### Stage 1 — Establish the cross-target test harness

- [x] Add `shoop_test_macros` and `shoop_wasm_test_support` with target-specific dependency gating.
- [x] Pin `wasm-bindgen-test 0.3.77` and matching Wasm tools without changing the locked production binding version.
- [x] Implement the shared test attribute while preserving native Tracy capture.
- [x] Implement explicit synchronous, async, ignored, expected-panic, and opt-in failure-canary forms.
- [x] Implement once-per-test-binary Node/browser configuration and reject conflicting features.
- [x] Move Tracy test dependencies behind non-Wasm target conditions.
- [x] Port `shoop_audio_protocol` plus test-support self-tests as the pilot; do not begin broad package migration yet.
- [x] Add Wasm dependency-tree checks that reject native backend, audio/MIDI driver, Carla, and Tracy dependencies.

Verification:

- [x] Pilot test bodies run under native nextest, Node.js Wasm, and Chromium Wasm from the same source.
- [x] Unit and integration test binaries select the intended runtime exactly once.
- [x] Intentional failures, expected panics, ignored tests, and async completion are reported correctly in all three environments.
- [x] Wasm dependency isolation checks pass.

Evidence: `shoop_test_macros::shoop_test` expands synchronous and asynchronous bodies to Tracy-captured native tests and `wasm_bindgen_test` Wasm tests, including native panic checking compatible with Tracy's harness. `shoop_wasm_test_support` contains unit and integration pilots for synchronous, asynchronous, ignored, expected-panic, and opt-in failure forms. The six production-envelope protocol tests now use the shared body. Warning-denying native, Node Wasm, and headless Chromium runs pass; opt-in native, Node, and Chromium canaries fail observably; `check_wasm_test_dependencies.py` rejects native driver and Tracy dependencies from both pilot Wasm trees.

### Stage 2 — Build orchestration, asset staging, inventory, and reporting

- [x] Add `scripts/run_wasm_tests.py` with pinned tool validation and explicit node/chrome runtime selection.
- [x] Add Cargo-metadata-driven package discovery and stable ordering.
- [x] Build/cache the worklet artifact once per profile outside the source tree.
- [x] Stage and hash exact production Worker/host assets plus the Node bootstrap.
- [x] Implement bounded CORS asset-server ownership and browser Blob-module loading/cleanup.
- [x] Add raw-log capture, fail-closed result accounting, JUnit generation, and aggregate summaries.
- [x] Add parser tests for successful, failed, panicking, ignored, malformed, truncated, timeout, zero-test, and runner/browser-crash output.
- [x] Add the canonical inventory tool and classification manifest with explicit exclusion reasons.
- [x] Make filters, package subsets, runtime-only additions, and profile visible in all output metadata.

Verification:

- [x] Canonical Node.js and Chromium commands run the pilot package independently through actual Wasm binaries.
- [x] Every fixture failure mode produces the correct nonzero status, raw log, partial/final JUnit, and synthetic runner testcase where needed.
- [x] Repeated package runs reuse one staged worklet/Worker asset set and never rebuild Wasm or production JavaScript per testcase; required per-fixture Blob URLs are revoked.
- [x] Asset server, subprocesses, temporary files, and browser sessions are cleaned on success, failure, timeout, and interruption.

Evidence: `run_wasm_tests.py` validates pinned tools, discovers package metadata, stages one profile-specific hashed asset set outside the source tree, owns a CORS loopback server, applies package/runtime filters, enforces deadlines, and emits raw logs, per-package JUnit, and aggregate JSON. `wasm_test_report.py` has fail-closed fixtures for pass, failure, panic, ignore, malformed/truncated/count-mismatched/zero output, timeout, and browser crash; an actual ignored canary produces nonzero JUnit failure. `wasm_test_inventory.py` combines native, source, Node, and Chromium IDs with explicit non-overlapping classification rules. Canonical Node and Chromium pilot commands pass, and the production Worker browser probe proves Blob URL cleanup.

### Stage 3 — Implement the multi-Worker remote-engine fixture

- [x] Add `shoop_wasm_runtime_tests` and the common fixture API without creating package dependency cycles.
- [x] Implement readiness, generations, production/fixture port ownership, bounded waits, and teardown accounting.
- [x] Implement the Node `worker_threads` bootstrap around exact production Worker/host modules.
- [x] Implement browser Worker/MessageChannel spawning through staged production assets and Blob module URLs.
- [x] Implement explicit process-quantum input/output and cooperative/realtime pause/resume/diagnostic controls without dynamic mode switching.
- [x] Expose bounded audio, MIDI/protocol event, diagnostic, revision, and callback observations.
- [x] Add single-Worker, multi-Worker, ordering, saturation, stale-generation, restart, cooperative, realtime, production shutdown, fixture shutdown, and teardown-failure contracts.
- [x] Verify that simultaneous Workers have independent engine state, IDs, sequences, ports, timers, failures, and cleanup.

Verification:

- [x] The same applicable multi-Worker scenarios pass in Node.js and Chromium; runtime-specific additions are separately reported.
- [x] Explicit-mode tests are deterministic across repeated and parallel runs.
- [x] Free-running tests use bounded observable conditions and leave no Workers, ports, timers, hosts, listeners, Blob URLs, or pending commands alive.
- [x] Production application envelopes remain byte-compatible and fixture commands never enter the production port.

Evidence: `shoop_wasm_runtime_tests/js/worker_fixture.js` exports `MultiWorkerFixture` and runtime-neutral contracts while importing the exact staged production Worker, raw host, and engine Wasm. Node uses a minimal `worker_threads` global shim; Chromium fetches CORS assets and owns revocable module Blob URLs. Four Rust Wasm tests cover two-instance explicit audio/isolation and leak detection; all three immutable processing modes with bounded progress, pause/resume, restart, and diagnostics; out-of-order sequence rejection, track creation, MIDI injection/observation, production shutdown; and command-capacity terminal failure isolated from a surviving peer. The migrated remote-client suite adds stale-generation, saturation, replay, readiness, transfer, and dual-client contracts. All applicable tests pass identically in Node and Chromium with real message channels, while the byte-stable protocol test verifies unchanged production envelopes.

### Stage 4 — Migrate portable Rust coverage

- [x] Migrate protocol, application API, settings-value, session, scripting, and pure UI/model tests in the recorded package waves.
- [x] Migrate engine and backend tests that can use dummy ports, explicit process quanta, or Worker fixtures.
- [x] Migrate worklet command, audio, MIDI, capacity, transfer, readiness, and lifecycle tests where cross-boundary evidence adds value.
- [x] Migrate application orchestration and multi-backend scenarios without starting the packaged application.
- [x] Keep pure unit tests pure; do not force a Worker fixture where no boundary behavior is involved.
- [x] Classify and retain unavoidable native, Wasm-runtime-only, browser-adapter, and packaged tests with concrete reasons.
- [x] Compare Node.js and Chromium inventories after every package and resolve unexplained differences before continuing.
- [x] Update baseline, classification, overlap, and duration reports after each package wave.

Verification after each package wave:

- [x] Native nextest remains green with Tracy capture enabled.
- [x] Node.js and Chromium Wasm suites pass for migrated shared packages.
- [x] The inventory accounts for every test and reports shared/runtime-specific/excluded totals without stale entries.
- [x] Warning-denying builds and Wasm dependency isolation remain green.

Evidence: 1,175 logical tests now share one source body under native Tracy-captured nextest, Node Wasm, and Chromium Wasm. Four additional production Worker/MessagePort tests run in both Wasm runtimes. Package waves cover protocol, app API, plugin protocol, settings values, sessions/media, scripting, engine graph/loops/MIDI/storage, backend/scheduler, remote client, application cooperative orchestration, egui models, raw worklet host, and browser MIDI composition, including the five portable composite-editor tests from current `master`. Canonical runs pass 1,179 tests per Wasm runtime. `wasm_test_classification.toml --require-closed` accounts for all 1,430 native IDs as 1,175 shared, 136 native-platform, and 119 native-driver tests with no pending, stale, overlapping, or unexplained runtime entries. Native package tests retain Tracy attributes through the shared macro; the complete native gate is re-run in Stages 6–7.

### Stage 5 — Minimize packaged-application smoke coverage

- [x] Build `docs/wasm_smoke_migration.md` mapping every current Chrome/Firefox invocation and assertion to a target test layer.
- [x] Move each reducible assertion and record passing replacement IDs before removing it from smoke automation.
- [x] Collapse hosted Chrome coverage to one production-bootstrap/user-gesture/real-AudioWorklet round trip.
- [x] Collapse self-contained coverage to one embedded-asset/real-callback check.
- [x] Retain one minimal secondary-browser production AudioWorklet check where supported.
- [x] Remove redundant mode combinations, stress loops, mocked-policy/model checks, and multi-Worker checks only after replacements pass.
- [x] Measure invocation count and repeated uncontended runtime.

Verification:

- [x] Every removed assertion has a passing replacement test reference in the migration record.
- [x] The retained smoke suite covers only the documented irreducible production boundary.
- [x] It uses at most three primary invocations and completes within five minutes excluding build and browser installation.

Evidence: `docs/wasm_smoke_migration.md` maps the 31 former Chromium launches and one Firefox launch by assertion group to named shared/Worker tests. `check_wasm_smoke_budget.py` enforces exactly hosted Chromium output-only, self-contained Chromium output-only, and hosted Firefox output-only invocations. PR #751 run `31905533513` passed all three against packaged artifacts in approximately 3.3, 13.4, and 44.9 seconds respectively; their combined 62-second runner time is below the five-minute boundary. The retained checks assert only production packaging, user gesture, application/worklet startup and routing, real 128-frame callback progress, second-browser compatibility, and process teardown.

### Stage 6 — Integrate CI and documentation

- [x] Replace the current host-native five-package web nextest step only after equivalent native and Wasm inventory evidence exists.
- [x] Run the full portable Node.js suite in the fast web path.
- [x] Run the same shared inventory under Chromium in one authoritative web path, with browser-only additions reported separately.
- [x] Retain debug/release Wasm package builds, actual-artifact contracts, artifact verification, and dependency-isolation checks.
- [x] Upload raw Wasm logs, JUnit XML, aggregate summaries, classification, overlap inventory, and timing reports on success and failure.
- [x] Keep the complete native nextest gate and Tracy failure capture unchanged except for shared test-attribute plumbing.
- [x] Run the minimized three-invocation packaged smoke boundary independently from regular Wasm suites.
- [x] Document prerequisites, exact tool pins, runtime selection, filtering, package selection, asset diagnostics, fixture diagnostics, timeouts, and failure reproduction.

Verification:

- [x] CI demonstrates native, Node.js Wasm, Chromium Wasm, and packaged-smoke gates independently.
- [x] A failure in any Wasm testcase, Worker, browser runner, result parser, inventory comparison, asset server, or cleanup check fails the owning job and appears in JUnit.
- [x] Local copyable commands reproduce every CI gate from a clean checkout.

Evidence: the web debug cell installs Node 22.22.2, wasm-pack 0.15.0, and matched Chrome/ChromeDriver 147.0.7727.137, runs the required 1,179-test Node suite, policy-triggered identical Chromium suite, source/runtime hash gate, parser fixtures, smoke budget, and all 15 dependency trees. Scheduled/manual release cells run optimized Node and Chromium. Existing debug/release application/worklet builds, package contracts, import checks, raw-host checks, and artifact verification remain. `if: always()` uploads per-package raw logs/JUnit, aggregate summaries, inventory-policy timing/category reports, and checked-in classification. Run `31905533513` passed both web cells and every independent Wasm/smoke step; parser fixtures cover testcase, panic, malformed/truncated/count/zero, timeout, browser-crash, and synthetic runner failures. `src/rust/shoopdaloop/README.md` and `docs/wasm_test_baseline.md` provide pinned copyable full/package/filter/policy/smoke commands and diagnostics.

### Stage 7 — Final end-to-end validation

- [ ] Run formatting checks.
- [ ] Run warning-denying native workspace builds.
- [ ] Run the complete native nextest suite with required features, backend policy, and Tracy failure capture.
- [ ] Run the complete canonical Node.js Wasm suite.
- [ ] Run the complete canonical Chromium Wasm suite and browser-only additions.
- [ ] Build debug/release application and worklet Wasm artifacts and inspect dependency isolation/imports.
- [ ] Run the minimized hosted, self-contained, and secondary-browser smoke checks.
- [ ] Verify JUnit, raw logs, aggregate summaries, classification, inventory, overlap, and runtime measurements from a clean checkout.
- [ ] Verify intentional testcase failure, Worker crash, browser crash, malformed output, timeout, zero-discovery, count mismatch, and teardown failure all fail closed.
- [ ] Confirm documentation commands and tool-version pins on a clean development environment.
- [ ] Audit that no production protocol, scheduler, host bridge, or application behavior fork was added for tests.

Final evidence must map every acceptance criterion to concrete commands, reports, test IDs, artifacts, and CI jobs. It must show that all remaining platform-specific tests are explicitly classified and justified and that no removed packaged-smoke assertion lacks passing replacement evidence.
