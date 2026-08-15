# WebAssembly Rust test architecture plan

## Status and execution contract

This document is an implementation plan. It changes only test infrastructure, test organization, CI, and test documentation. The production refactoring listed under **Prerequisite assumptions** is required before this plan starts and is not part of this plan.

During implementation:

- Keep this plan updated and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not change without explicit user approval.

## Goals

- Execute the portable Rust test suite as `wasm32-unknown-unknown`, rather than merely compiling Wasm while executing selected tests as native Linux binaries.
- Use `wasm-pack` and `wasm-bindgen-test` as the standard Wasm test harness.
- Make Node.js and a headless browser selectable execution environments for the same portable test inventory.
- Provide reusable single- and multi-worklet fixtures using the same command/event protocol and message-port boundary as production.
- Keep native `cargo nextest` coverage and maximize shared test source between native and Wasm execution.
- Reduce packaged-application browser smoke testing to a small, quick set of checks that cannot be represented faithfully in the regular Wasm suites.
- Produce actionable CI output and JUnit XML for the Wasm suites.

## Scope

Included:

- Test attributes and test-support crates.
- Dummy-worklet fixtures and runtime adapters for Node.js and browsers.
- Test asset generation and orchestration.
- Migration and classification of existing Rust tests.
- JUnit conversion, inventory reporting, CI changes, and test documentation.
- Reduction of the packaged-application smoke suite after replacement coverage exists.

Excluded:

- Production driver, backend, protocol, application, or UI refactoring.
- Replacing native JACK, CPAL, midir, Carla, native storage, or Tracy integration tests.
- Claiming that Node.js Worker scheduling is equivalent to AudioWorklet realtime scheduling.
- Replacing the final packaged-application checks with mocks.
- Adding a custom nextest browser or Node target runner.

If a required production seam is missing, implementation stops and records the missing prerequisite instead of adding test-only branches to production behavior.

## Prerequisite assumptions

This plan assumes the following production refactoring has already landed:

1. The audio driver is separated from cross-worklet communication. The application observes one backend/control interface and does not own `AudioContext`, `AudioWorkletNode`, Worker, or `MessagePort` details.
2. Cross-worklet commands and events use one bounded, versioned transport contract. The browser AudioWorklet and dummy worklets receive and publish the same envelopes and enforce the same sequence, capacity, replay, shutdown, and error rules.
3. A dummy audio driver can run inside an independent worklet or Worker realm. It can run in either:
   - **Explicit mode**, where the test requests exact process quanta.
   - **Free-running mode**, where callbacks continue until paused or stopped.
4. Driver selection, clocking, and physical audio are hidden behind injected capabilities. Application and engine behavior tests do not need a physical device.
5. Each worklet instance has isolated engine and driver state, a distinct message port, observable readiness and shutdown, and bounded diagnostics.
6. The portable application core is exposed from library targets. Tests do not have to start the packaged executable or create a real window.
7. The dummy-worklet implementation can be built as a standalone Wasm module or loaded through a stable test-only artifact interface without duplicating production protocol logic.
8. Browser-specific adapters and native driver adapters remain thin enough to test separately from portable behavior.
9. Tests can reset all mutable application, worklet, timer, storage, and subscription state without restarting the browser process.

### Prerequisite audit after production refactoring

The production refactoring now provides the required seams, with these evidence-based implementation details for this downstream project:

- `shoop_worklet_client` is the platform-neutral backend/client and owns bounded sequencing, generations, replay, readiness, typed outcomes, and quiescence. Browser drivers receive only `RemoteBackendControl` and a `MessageEndpoint` implementation.
- `shoop_audio_worklet.wasm` is the standalone, import-free engine artifact. `raw_wasm_host.js` is the adapter-neutral ABI bridge used by both the physical AudioWorklet and browser Worker and has an actual-artifact Node contract test.
- The Worker has **explicit**, bounded cooperative **free-running**, and realtime-paced modes. Explicit processing, staged fixture audio, mode control, and fixture diagnostics require a separately transferred fixture-only `MessagePort`; the production application port accepts only unchanged protocol JSON.
- Browser Worker and AudioWorklet adapters share the Wasm bridge but not scheduler code. A Node fixture should reuse the raw bridge and production envelopes with `node:worker_threads`; it should not attempt to execute the browser-specific `importScripts`, timer, or `MessagePort` adapter unchanged.
- Isolation/reset is achieved by constructing fresh client, ports, Wasm host, scheduler, and bridge state and then performing acknowledged shutdown/destruction. No global reset command or shared singleton is required.
- Physical browser callbacks remain packaged-smoke evidence. Node scheduling is still not treated as AudioWorklet timing evidence.

These details satisfy the prerequisite boundary without adding another production seam. Stage 0 of this plan must re-run the artifact, dependency, teardown, and multi-instance contracts rather than assuming that a browser Worker implementation is directly executable under Node.

## Current baseline

The web CI matrix currently runs `cargo nextest` without a Wasm target for four selected packages, so those tests execute as native Linux binaries. The selected source inventory is roughly 200 tests, about 15% of the approximately 1,387 declared Rust tests. The actual Wasm target is checked and built but runs no Rust test harness.

The packaged-application browser suite currently carries both browser-integration coverage and behavior that can move to deterministic Rust/Wasm tests. This makes it broader and slower than the final smoke boundary should be.

Record exact native and selected-package inventories at implementation time because test counts will continue to change.

## Target test architecture

### Test layers

| Layer | Harness | Purpose |
| --- | --- | --- |
| Shared Rust behavior | Native nextest and Wasm-pack | Protocol, engine, backend, application, scripting, session, settings values, UI models, and dummy-driver behavior from the same test source |
| Wasm runtime contracts | Wasm-pack in Node.js or Chromium | Wasm bindings, Worker lifecycle, MessagePort behavior, module loading, and multi-worklet fixtures |
| Native platform integration | Native nextest | JACK, CPAL, midir, Carla, subprocess/shared-memory behavior, native atomic storage, and native Tracy capture |
| Browser adapter contracts | Wasm-pack in Chromium | Browser APIs that need a browser but not the packaged application or physical AudioWorklet |
| Packaged-application smoke | Existing production artifact automation | Production bootstrap, artifact wiring, user-gesture policy, real AudioContext/AudioWorklet scheduling, and packaging modes |

### Shared test attribute

Add a test-support attribute used by portable test functions:

- Native expansion retains the Tracy nextest capture behavior.
- Wasm expansion uses `#[wasm_bindgen_test]`.
- Ignored tests, expected panics, synchronous tests, and asynchronous Wasm tests have explicit supported forms.
- Tracy and its native dependencies are absent from the Wasm dependency graph.

Use one test body for native and Wasm assertions. Platform-specific setup may be selected behind small fixture adapters, but assertions and scenarios must not be copied into parallel native and Wasm files.

Each Wasm-capable package exposes a uniform testing-only feature or configuration hook that lets the orchestration command select Node.js or browser mode. The test-support crate supplies the once-per-test-binary `wasm-bindgen-test` configuration so unit and integration test targets behave consistently.

### Runtime selection and orchestration

Provide one repository command with an explicit runtime:

```sh
python3 scripts/run_wasm_tests.py --runtime node --profile ci
python3 scripts/run_wasm_tests.py --runtime chrome --profile ci
```

The orchestrator must:

1. Build the dummy-worklet artifact once for the selected profile.
2. Discover the ordered list of Wasm-capable workspace packages.
3. Invoke `wasm-pack test` once per package with the correct runtime configuration.
4. Pass the worklet artifact to test binaries without compiling it per testcase.
5. Preserve each command's exit status and logs.
6. Emit per-package/runtime JUnit XML and an aggregate summary.
7. Fail closed if test discovery, output parsing, browser startup, Worker startup, or result accounting is incomplete.

Pin compatible `wasm-pack`, `wasm-bindgen-cli`, and `wasm-bindgen-test` versions in CI. Node.js is the fast default; Chromium is a selectable execution environment for the same shared inventory. Browser-only contract tests are additional and explicitly classified.

Because `wasm-pack` operates per package and browser startup occurs per Cargo test binary rather than per test function, retain a modest number of test binaries. Consolidate harness entry points only when measurements show startup dominates; do not duplicate or flatten logical test modules solely to reduce process count.

### Multi-worklet fixture

Add a reusable fixture with an interface equivalent to:

```rust
let fixture = MultiWorkletFixture::spawn(3, ProcessingMode::Explicit).await?;
fixture.worklet(0).process_quantum(128).await?;
fixture.worklet(1).set_processing_mode(ProcessingMode::FreeRunning).await?;
fixture.wait_for_revision(expected_revision).await?;
fixture.shutdown().await?;
```

Required behavior:

- Spawn any practical number of isolated dummy-worklet instances for one testcase.
- Give every instance its own real message-port pair and production command/event transport.
- Instantiate the same dummy driver and engine composition used by portable production code.
- Cache immutable Worker script and compiled Wasm module assets per test binary while creating fresh ports, host state, and driver state per testcase.
- Use `node:worker_threads` `Worker`, `MessageChannel`, and `MessagePort` in Node.js.
- Use browser `Worker`, `MessageChannel`, and `MessagePort` in browser mode.
- Default to explicit processing for deterministic assertions.
- Support free-running processing only for lifecycle, concurrency, backpressure, and restart scenarios.
- Wait on revisions, acknowledgements, callback counters, or bounded deadlines rather than arbitrary sleeps.
- Expose input/output audio buffers, MIDI batches, diagnostics, and callback counters through bounded test APIs.
- Shut down and verify every Worker, port, timer, and in-flight command. Teardown failure fails the testcase.
- Keep a testcase generation identifier so late messages from a failed or completed fixture cannot affect another testcase sharing the page.

The Node.js port implementation is treated as a contract-compatible transport, not proof of browser event-loop or AudioWorklet timing. Browser-mode fixture runs provide the corresponding browser MessagePort evidence.

### Test classification and expected overlap

Every Rust test belongs to exactly one category:

- `shared`: same test source runs under native nextest and the portable Wasm suite.
- `native-driver`: requires a native audio/MIDI/plugin driver or subprocess/shared-memory integration.
- `native-platform`: requires native filesystem guarantees, OS integration, or native Tracy facilities.
- `wasm-runtime`: tests Worker, MessagePort, Wasm binding, or dummy-worklet runtime behavior.
- `browser-adapter`: requires a browser API but not the packaged application.
- `packaged-smoke`: requires production artifact/bootstrap or genuine AudioWorklet behavior.

Generate an inventory report containing native, Node.js, Chromium, shared, and explicitly excluded counts. A test that silently disappears because of `cfg` or feature selection is an error; exclusions require a category and reason.

Based on the current inventory and the relatively small number of clearly native driver, Carla, filesystem, subprocess, and Tracy tests, the expected steady-state overlap is approximately **90–95% of logical Rust testcases**. If the current inventory size remained unchanged, that would be roughly **1,250–1,320 shared tests** running from the same source under native and Wasm. This is planning information, not an acceptance threshold: the inventory report and explicit exclusion reasons are authoritative.

### Packaged-application smoke boundary

Retain only checks that regular Wasm suites cannot represent faithfully:

1. A hosted production artifact boots, paints, accepts the required user gesture, creates a real `AudioContext` and `AudioWorkletNode`, advances real callbacks, and completes one application-to-worklet command and audio route round trip.
2. The self-contained production artifact boots from its supported delivery mode, loads its embedded application and worklet modules, and advances a real callback.
3. One secondary-browser hosted check confirms production bootstrap and real AudioWorklet compatibility where CI supports it.

Move protocol sequencing, saturation, restart replay, deterministic record/playback, session replacement, processor capability rejection, settings behavior, scripting, keyboard control, Web MIDI model behavior, UI model behavior, and multi-worklet scenarios into shared or Wasm runtime suites before deleting their smoke equivalents.

Mocked permission and MIDI API behavior belongs in Wasm-pack browser tests. Keep only browser-policy behavior that depends on production origin, user gesture, permissions, or actual device APIs in packaged smoke tests.

The final smoke suite has at most three primary invocations and targets completion within five minutes on an uncontended CI runner, excluding artifact build and browser installation.

### Reporting

`wasm-pack` and `wasm-bindgen-test-runner` do not natively emit JUnit XML. The orchestration layer therefore captures pinned terse output and emits JUnit with:

- stable testcase names,
- package and runtime properties,
- panic/failure output,
- elapsed suite time where available,
- skipped classifications where representable,
- a synthetic failed testcase for runner crashes, timeouts, malformed output, or result-count mismatch.

Test the converter against successful, failed, ignored, panicking, malformed, and browser-crash fixtures. Preserve raw logs alongside JUnit artifacts.

## Immutable acceptance criteria

- The canonical Node.js and Chromium commands execute actual `wasm32-unknown-unknown` test binaries through `wasm-pack`; neither command substitutes host-native test execution.
- The same portable test inventory can be selected for Node.js or Chromium, with any environment-specific additions reported separately.
- Portable tests use one source body under native nextest and Wasm unless an explicit classification documents why sharing is impossible.
- Multi-worklet tests use real runtime message ports and isolated dummy-driver worklets, support explicit and free-running processing, and verify clean teardown.
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

- [ ] Verify every prerequisite assumption against the refactored production code.
- [ ] Stop and record a blocker for any missing transport, driver, lifecycle, reset, or library seam.
- [ ] Record exact native nextest test inventory and the tests currently selected by the web CI job.
- [ ] Record current web CI duration by build, host-side Rust tests, package validation, and packaged smoke mode.
- [ ] Classify all current test targets provisionally and identify tests that rely on threads, files, subprocesses, environment variables, global allocators, physical drivers, or Tracy.

Verification:

- [ ] A checked-in or reproducibly generated baseline report accounts for every discovered test.
- [ ] No implementation stage begins while a prerequisite remains unresolved.

### Stage 1 — Establish the cross-target test harness

- [ ] Add and pin the test-support and `wasm-bindgen-test` dependencies with target-specific dependency gating.
- [ ] Implement the shared test attribute while preserving native Tracy capture.
- [ ] Implement once-per-binary Node/browser runtime configuration.
- [ ] Move Tracy test dependencies behind non-Wasm target conditions.
- [ ] Port one small package containing synchronous, panic, ignored, and failure-canary coverage.
- [ ] Add dependency-tree checks that reject native backend and Tracy dependencies from Wasm test artifacts.

Verification:

- [ ] The pilot tests run from the same source under native nextest, Node.js Wasm, and Chromium Wasm.
- [ ] Intentional failures are reported correctly in all three environments.
- [ ] Wasm dependency isolation checks pass.

### Stage 2 — Build orchestration and reporting

- [ ] Add the runtime-selectable repository orchestration command.
- [ ] Add deterministic package discovery and stable ordering.
- [ ] Build/cache the dummy-worklet artifact once per profile and make it available to test binaries.
- [ ] Add raw-log capture, fail-closed result accounting, JUnit generation, and aggregate summaries.
- [ ] Add unit tests for output conversion and runner-level failure handling.
- [ ] Add the initial inventory/overlap report and require explicit exclusion reasons.

Verification:

- [ ] Node.js and Chromium commands run the pilot package independently.
- [ ] Successful, failed, panicking, ignored, malformed-output, timeout, and runner-crash fixtures produce correct command status and JUnit.
- [ ] Repeated runs reuse built worklet assets and do not transform or compile them per testcase.

### Stage 3 — Implement the multi-worklet test fixture

- [ ] Implement the common fixture API, readiness handshake, generations, and cleanup checks.
- [ ] Implement Node.js Worker and MessagePort spawning.
- [ ] Implement browser Worker and MessagePort spawning.
- [ ] Implement explicit process-quantum control and free-running lifecycle control.
- [ ] Expose bounded audio, MIDI, event, diagnostic, and callback observations.
- [ ] Add single-worklet, multi-worklet, ordering, saturation, restart, free-running, and teardown-failure contract tests.
- [ ] Verify that multiple worklets have independent state and cannot consume each other's messages.

Verification:

- [ ] The same multi-worklet scenarios pass in Node.js and Chromium.
- [ ] Explicit-mode tests are deterministic across repeated and parallel runs.
- [ ] Free-running tests use bounded observable conditions and leave no Workers, ports, timers, or callbacks alive.

### Stage 4 — Migrate portable Rust coverage

- [ ] Migrate protocol, application API, settings-value, session, scripting, and pure UI/model tests.
- [ ] Migrate engine and backend tests that can use dummy drivers or explicit process quanta.
- [ ] Migrate worklet command, audio, MIDI, capacity, session-transfer, and lifecycle tests onto the shared fixtures where cross-boundary evidence adds value.
- [ ] Migrate application orchestration and multi-backend scenarios without starting the packaged application.
- [ ] Keep pure unit tests pure; do not force a worklet fixture where no boundary behavior is involved.
- [ ] Classify and retain unavoidable native and browser-specific tests with reasons.
- [ ] Compare Node.js and Chromium inventories and resolve unexplained differences.

Verification after each package wave:

- [ ] Native nextest remains green with Tracy capture enabled.
- [ ] Node.js and Chromium Wasm suites pass for the migrated packages.
- [ ] The inventory accounts for every test and reports updated overlap.
- [ ] Warning-denying builds and Wasm dependency isolation remain green.

### Stage 5 — Minimize packaged-application smoke coverage

- [ ] Map each existing smoke assertion to shared, Wasm runtime, browser adapter, or irreducible packaged-smoke coverage.
- [ ] Move each reducible assertion and demonstrate replacement evidence before removing it from smoke automation.
- [ ] Collapse hosted Chrome coverage to one production-bootstrap/real-AudioWorklet round trip.
- [ ] Collapse self-contained coverage to one embedded-asset/real-callback check.
- [ ] Retain one minimal secondary-browser production AudioWorklet check where supported.
- [ ] Remove redundant mode combinations, stress loops, and model-level assertions.
- [ ] Measure invocation count and uncontended runtime.

Verification:

- [ ] Every removed assertion has a passing replacement test reference in the migration record.
- [ ] The retained smoke suite covers only the documented irreducible boundary.
- [ ] It uses at most three primary invocations and completes within five minutes excluding build and installation.

### Stage 6 — Integrate CI and documentation

- [ ] Replace the host-native web-component nextest step with actual Wasm suite execution.
- [ ] Run the full portable suite under Node.js in the fast web CI path.
- [ ] Run the same portable inventory under Chromium in one authoritative web CI path.
- [ ] Retain Wasm package builds, artifact verification, and dependency-isolation checks.
- [ ] Upload raw Wasm logs, JUnit XML, and overlap reports on success and failure.
- [ ] Keep the complete native nextest gate unchanged except for shared test-attribute plumbing.
- [ ] Document prerequisites, pinned tool versions, runtime selection, filtering, fixture diagnostics, and failure reproduction.

Verification:

- [ ] CI demonstrates native, Node.js Wasm, Chromium Wasm, and packaged-smoke gates independently.
- [ ] A failure in any Wasm testcase, Worker, browser runner, result parser, or inventory check fails the owning job and appears in JUnit.
- [ ] Local copyable commands reproduce each CI gate.

### Stage 7 — Final end-to-end validation

- [ ] Run formatting checks.
- [ ] Run warning-denying native workspace builds.
- [ ] Run the complete native nextest suite with required features and missing-backend policy.
- [ ] Run the complete Node.js Wasm suite.
- [ ] Run the complete Chromium Wasm suite.
- [ ] Build both application and worklet Wasm artifacts and inspect dependency isolation.
- [ ] Run the minimized hosted, self-contained, and secondary-browser smoke checks.
- [ ] Verify JUnit, raw logs, inventory, and runtime measurements from a clean checkout.
- [ ] Confirm the documentation commands and tool-version pins on a clean development environment.

Final evidence must show that all acceptance criteria are met and that any remaining platform-specific tests are explicitly classified and justified.
