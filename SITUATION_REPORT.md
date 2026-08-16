# Situation Report: Fix WASM Loop Duplication and Peak Meter Decay

## Current branch and PR

- Branch: `test/node-remote-app-harness`
- PR: https://github.com/SanderVocke/shoopdaloop/pull/756
- Current CI: fully green on commit `a56e8ac`
- The branch is pushed and the working tree was clean before this report was added.

## What PR 756 adds

A reusable remote application test harness was added under:

- `src/rust/shoop_wasm_runtime_tests/tests/remote_application.rs`
- `src/rust/shoop_wasm_runtime_tests/js/worker_fixture.js`

The harness runs this complete stack:

```text
AppIntent
  -> CooperativeApplicationRuntime
  -> RemoteWorkletBackend
  -> MessageEndpoint / MessagePort
  -> production audio_worker.js
  -> production shoop_audio_worklet.wasm
  -> protocol responses
  -> RemoteBackendControl
  -> application snapshots
```

It works in the required Node WASM setup and also passed in Chromium.

Three tests use it:

1. `remote_application_stack_processes_intents_and_engine_quanta`
2. `remote_loop_duplication_reproduces_async_capture_error`
3. `remote_peak_publication_reproduces_accumulated_maximum`

The latter two are **characterization tests**. They currently pass by asserting the bugs occur. They must be converted into correct-behavior regression tests as part of fixing the bugs.

## Bug 1: dragging a loop to clone/duplicate fails in WASM

### Reproduction now covered

The remote test:

```rust
remote_loop_duplication_reproduces_async_capture_error
```

- creates a real remote audio track;
- generates real click content in a source loop;
- dispatches `AppIntent::Loop` with `LoopAction::DuplicateTo(target)`;
- observes the application notification:
  `asynchronous session capture is not complete`;
- verifies the target remains empty.

This is the same class of error shown at the top of the browser application.

### Likely root cause

`ApplicationModel::duplicate_loop_into()` in `src/rust/shoop_app/src/lib.rs` uses synchronous backend methods for primitive loops:

```rust
backend.capture_session()
backend.replace_loop_content(...)
```

The in-process `EngineBackend` and `FakeBackend` complete these synchronously, which is why existing tests pass.

`RemoteWorkletBackend` implements these operations using multi-message asynchronous transfers:

- `capture_session()` delegates to `capture_session_async()` and returns an error while pending:
  `asynchronous session capture is not complete`
- `replace_loop_content()` delegates to `replace_loop_content_async()` and similarly returns an error while pending.

The application already has asynchronous I/O state machinery in `PendingIo` and repeatedly advances operations such as:

- session capture/load;
- generated click content replacement;
- loop import/export.

Duplication needs an equivalent asynchronous application operation. Do not hide the problem by blocking or spinning in `RemoteWorkletBackend`; Node/browser message delivery requires yielding to the event loop.

### Suggested implementation direction

Add a pending duplication state machine to `ApplicationModel`. It should:

1. Validate source/target and capture source metadata.
2. Call `capture_session_async()` repeatedly until ready.
3. Extract the source loop content.
4. Call `replace_loop_content_async()` repeatedly until ready, or clear the target if appropriate.
5. Apply gain and balance.
6. Remove/replace any previous target composite as needed.
7. Commit target model metadata only after backend success.
8. Report errors without partially committing application state.

Preserve the immediate path for synchronous backends where `BackendAsyncResult::Ready` is returned on the first call.

Be careful about:

- composite duplication, which follows a different path;
- stale source/target IDs while an operation is pending;
- only one backend transfer being active at a time;
- interaction with existing `pending_io` operations;
- committing name, length, media flags, cached audio/MIDI details, repeat-sync, recorded FX state, and composite fields exactly as the existing synchronous implementation does;
- cleanup/rollback after failures.

### Required test conversion

Rename and invert:

```rust
remote_loop_duplication_reproduces_async_capture_error
```

The final test should assert:

- no error notification;
- the operation settles asynchronously;
- target is non-empty;
- target content is equivalent to source;
- target gain and balance match source;
- stable source and target identities are preserved.

Keep the existing GUI drag tests. They already cover synthetic egui drag/drop producing `DuplicateTo`; the remote test starts at the `AppIntent` boundary.

## Bug 2: WASM audio peak meters rise and never decay

### Reproduction now covered

The remote test:

```rust
remote_peak_publication_reproduces_accumulated_maximum
```

- creates a real remote track;
- generates click content;
- plays it through the production Worker/worklet WASM;
- observes a loud loop peak in the application snapshot;
- stops the loop and processes silent quanta;
- verifies the published peak remains exactly at the previous maximum.

This means the failure is upstream of the egui decay algorithm: egui never receives a lower target value, so it correctly has nothing to decay toward.

### Likely root cause

Engine audio-port/channel peaks are running maxima until explicitly reset. Existing low-level tests document this behavior.

`EngineBackend::poll()` in `src/rust/shoop_backend/src/lib.rs` reads:

- track input port peaks;
- track output port peaks;
- loop audio-channel output peaks;

but does not appear to reset those accumulators after publishing the snapshot.

Relevant APIs exist in the engine:

- audio port `reset_input_peak()`
- audio port `reset_output_peak()`
- audio channel `reset_output_peak()`

Global backend `input_peak` and `output_peak` are already reset during processing, but track/loop port and channel accumulators need explicit publication-window handling.

### Suggested implementation direction

Treat `EngineBackend::poll()` as the end of a peak measurement window:

1. Read all track and loop peaks into the returned snapshot.
2. Reset each corresponding accumulator after reading it.
3. Ensure shared ports/channels are reset once after all required snapshot values have been collected.
4. Avoid borrow conflicts by collecting port/channel IDs and resetting in a separate pass if necessary.

Confirm desired semantics for repeated polls without processing:

- first poll after loud processing publishes the loud peak;
- a later processing window containing silence should publish silence;
- polling twice without any processing should not accidentally retain a lifetime maximum.

Consider both:

- local `EngineBackend::new_dummy()` / `new_web_audio()` behavior;
- production worklet behavior, since the worklet serializes `EngineBackend::poll()` snapshots.

### Required test conversion

Rename and invert:

```rust
remote_peak_publication_reproduces_accumulated_maximum
```

The final test should assert that after stopping and processing silent quanta, the application snapshot peak falls to the silence floor rather than remaining at the loud maximum.

Also add a focused in-process backend regression test:

```text
process loud quantum
-> poll reports loud peak
-> process silent quantum
-> poll reports silence
```

The existing egui tests in `src/rust/shoop_egui/src/meter_ballistics.rs` already verify hold/release arithmetic with explicit timestamps. A later improvement could test repaint scheduling and painted geometry over successive egui frames, but fixing backend publication is the primary issue.

## Harness notes

`RemoteAppHarness` owns:

- production fixture JS object;
- `CooperativeApplicationRuntime`;
- `RemoteBackendControl`;
- a persistent Rust closure receiving Worker protocol messages;
- callback error collection.

Important behavior:

- tests run concurrently under `wasm-bindgen-test`;
- fixture shutdown must not assert global Worker counts are zero because peer tests can still own Workers;
- the harness waits for a published 48 kHz sample rate before tests generate click content;
- `drive_step()` ticks the app, yields to the JS event loop, ticks again to consume responses, and yields again;
- explicit engine processing uses 128-frame quanta.

Do not replace the Node fixture with browser-only `web_sys::Worker`; Node compatibility is a requirement.

## Existing coverage distinction

There is extensive real-backend coverage already:

- application tests against in-process `EngineBackend::new_dummy()`;
- backend contract tests against the real engine;
- native full-application dummy workflow tests;
- remote backend tests against mock endpoints;
- production Worker tests using raw protocol commands.

The new harness fills the previously missing combination:

```text
application intents + RemoteWorkletBackend + real MessagePort + real Worker/worklet engine
```

## Validation and CI

Before committing Rust changes, project instructions require:

```sh
cargo fmt --all
RUSTFLAGS="-D warnings" cargo build --workspace
```

If Rust tests change:

```sh
python3 scripts/check_shoop_test_usage.py
```

This phone cannot perform the full build. Push updates to PR 756 and monitor with:

```sh
gh pr checks 756 --watch --interval 30
```

Useful CI commands and evidence:

```sh
gh run list --branch test/node-remote-app-harness
gh run view <run-id> --job <job-id> --log-failed
gh run download <run-id> -n wasm-test-reports-debug-<run-id> -D <dir>
```

The authoritative successful run for the current harness was:

- Build/test run: `31958522271`
- All matrix jobs passed.
- Both Node and Chromium reports showed all three remote tests passing as characterization tests.

## Project instructions already identified

Read before continuing:

- `AGENTS.md`
- `.agents/index.md`
- `.agents/rules/mandates.md`
- `.agents/rules/style.md`
- `.agents/info/test.md`
- `.agents/info/ci-debug.md`

The current PR is open, non-draft, and mergeable, but the bug tests still encode broken behavior. The next objective is to fix both implementations and turn those two tests into correct-behavior regression tests while keeping Node WASM CI green.
