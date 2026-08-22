# Console-first error handling refactoring plan

## Goals

- Make structured console logging the default destination for operational failures, degraded operation, recovery warnings, and developer diagnostics.
- Keep error messages in egui only when they are feature-owned, directly explain a visible user interaction, and can be cleared by that interaction's lifecycle.
- Remove the application-wide notification queue and its floating and tooltip-based UI presentation.
- Replace string-based notification observation with typed feature, task, and self-test state.
- Preserve useful diagnostic context without flooding logs from failures that can recur on every update.

## Scope

This refactor covers native and browser logging, generic application notifications, file-I/O failure reporting, browser runtime and self-test observation, and tests coupled to notification text. Existing feature-local validation and status UI remains in scope only where adaptation is required after removing generic notifications. A broader redesign of settings diagnostics, script logs, connection state, FX status, or audio-driver state is out of scope.

## Immutable acceptance criteria

- No generic floating error, warning, or informational message is rendered over the egui application.
- The application API and snapshots contain no generic notification type or notification collection.
- No backend-status UI presents an unrelated historical application error.
- Every removed notification producer is deliberately replaced by structured logging, typed feature state, or both.
- User-initiated I/O failures remain visible in the active I/O workflow and retain enough state to support retry, cancellation, or a clear terminal outcome where applicable.
- Input validation and actionable failures remain next to their owning controls or inside their owning dialogs.
- Browser runtime status and self-tests use typed state rather than matching human-readable notification strings.
- Recurrent update-loop failures are transition-aware or otherwise rate-limited so that the console is not flooded.
- Routine console output uses `tracing` or the `log` facade; direct stderr output remains only at boundaries where structured logging is unavailable or no longer usable.
- Native and browser builds, the complete Rust test suite, tracing coverage, and relevant browser smoke tests pass.

## Design rules and constraints

- Log broadly and display narrowly: diagnostic relevance is sufficient for a structured log, while UI presentation requires immediate relevance to a visible user interaction.
- Prefer typed, owner-specific state over generic message storage. UI text must be derived from that state at the feature boundary.
- Separate diagnostic detail from user copy. Logs should carry operation names, identifiers, paths, and underlying errors; UI messages should be concise and actionable.
- Use `error` when an operation or subsystem fails, `warn` for degradation, fallback, recovery, or expected rejection, and lower levels for ordinary lifecycle detail.
- Prefer structured tracing fields over interpolated diagnostic strings when identifiers or error sources are available.
- Do not log a failure on every tick. Repeated failures must be logged on state transition, recovery, or through an explicit bounded suppression strategy.
- A feature-local failure may also be logged, but it must not be copied into an application-wide UI channel.
- Preserve the existing feature-owned UI for settings, scripts, connections, click-track operations, FX, audio-driver switching, and form validation unless a typed-state correction is necessary.
- Narrow file-I/O reporting to a typed task failure. Failures without an active task belong in logging or another owning feature, not in a task intent with a missing identifier.
- Tests must assert typed state or captured structured events, not user-facing message substrings, unless the exact copy itself is the behavior under test.
- Keep native and Wasm behavior aligned while respecting that native logs go to the terminal and Wasm logs go to the browser console.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

## Stage 1: Inventory and define routing

This stage is a dependency for all implementation stages.

- [x] Inventory every producer and consumer of application notifications, notification severity, and file-I/O error intents across native, Wasm, application-model, UI, and test code.
- [x] Classify each producer as console-only, feature-state-only, or console plus feature state.
- [x] Identify update-loop producers that can repeat and define their transition or suppression behavior before replacing notification writes with logs.
- [x] Identify direct `eprintln!` calls and classify the few legitimate process-boundary uses separately from calls that should become structured logs.
- [x] Record the typed state that will replace each browser runtime or self-test dependency on notification text.

### Stage 1 verification

- [x] Confirm searches account for every notification type, field, helper, producer, UI consumer, tracing field, browser consumer, and test assertion.
- [x] Confirm every producer has one documented replacement route and every generic consumer has a typed replacement or is intentionally deleted.

### Stage 1 routing record

- Periodic backend, composition, selected-media, and scripting failures become transition-deduplicated structured errors; connection failures additionally retain connection-owned state.
- Intent failures, script/session serialization failures, loop capture/duplication failures, queue failures, audio recovery, and MIDI quantization become structured events at error or warning severity as appropriate.
- I/O completion failures with a task ID retain typed task state and structured diagnostics; picker, URL, dropped-file, scan, and startup failures without a task become console-only.
- The floating area, backend tooltip history, snapshot notification count, and browser status notification suffix are deleted.
- Browser click-track self-tests observe the typed I/O task status and message associated with the request instead of the notification history.
- Non-boundary `eprintln!` calls become structured events. Fatal startup and post-run shutdown reporting remain direct stderr boundaries.

## Stage 2: Establish structured diagnostic coverage

Depends on Stage 1 classifications. This stage must land before generic notification state is removed.

- [x] Replace non-boundary `eprintln!` calls with structured `tracing` events at an appropriate level.
- [x] Add structured events for application-model failures that currently rely on generic notifications as their only diagnostic output.
- [x] Include stable operation names and useful fields such as intent kind, task or request ID, path, subsystem, and underlying error.
- [x] Make polling and periodic-update diagnostics transition-aware or bounded, including a recovery event when that information is useful.
- [x] Retain direct stderr reporting only for logging initialization failure, fatal process boundaries, or shutdown paths where the subscriber cannot be relied upon.
- [x] Verify startup fallback and recovery conditions use warning severity rather than being promoted to generic UI errors.

### Stage 2 verification

- [x] Add or update focused tests for any new transition/suppression state.
- [x] Exercise representative intent, backend, startup fallback, script scan, and I/O failures and verify one appropriately leveled structured event is emitted without per-frame repetition.
- [x] Run `cargo fmt --all -- --check`.
- [x] Run `RUSTFLAGS="-D warnings" cargo build --workspace`.
- [x] Run `python3 scripts/check_shoop_test_usage.py` before committing Rust test changes.

### Stage 2 completion record

- Structured diagnostics now cover former notification producers, periodic failures are logged once until recovery, and non-boundary stderr dispatch reporting uses tracing.
- Formatting, the test-usage policy check, the periodic-failure test, the complete `shoop_app` unit suite, and warning-denying package checks passed. The warning-denying workspace build reached all changed packages but the host-only audio-worklet shared-library link is unavailable because the container Tracy archive is not position-independent.

## Stage 3: Narrow file-I/O failure handling

Depends on Stage 2 so failures remain observable after generic publication is removed.

- [ ] Replace the optional-ID file-I/O error intent with an explicitly named I/O-task failure intent that requires a task ID and updates only the matching task.
- [ ] Keep concise failure information in `IoTaskState` so the active I/O dialog explains the failed operation and its terminal state.
- [ ] Update native and browser save paths with active tasks to log diagnostic detail and dispatch the typed task failure.
- [ ] Route picker reads, URL fetches, dropped files, scans, and startup warnings without active tasks to structured logging or their actual feature owner.
- [ ] Define behavior for stale task failure completion and log it without overwriting a newer task.
- [ ] Ensure successful, cancelled, and failed task lifecycles remain distinguishable without consulting generic notifications.

### Stage 3 verification

- [ ] Add or update focused model tests for matching and stale task failures.
- [ ] Verify failed session and loop imports/exports remain visible in the I/O dialog while unrelated file failures do not create UI overlays.
- [ ] Verify native and browser I/O callers no longer dispatch an error intent without a task ID.
- [ ] Run targeted `shoop_app`, `shoop_egui`, and `shoopdaloop` tests covering I/O state and rendering.
- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `RUSTFLAGS="-D warnings" cargo build --workspace`.
- [ ] Run `python3 scripts/check_shoop_test_usage.py` before committing Rust test changes.

## Stage 4: Replace browser notification observation

Depends on the typed I/O outcome from Stage 3.

- [ ] Change browser click-track self-tests to observe request/task IDs and typed click-track or I/O terminal state instead of searching notification messages.
- [ ] Change the browser runtime-status element to report typed audio, MIDI, task, and self-test health without appending a notification string.
- [ ] Add explicit typed failure state where an existing subsystem state cannot distinguish pending, completed, and failed outcomes reliably.
- [ ] Keep human-readable browser status copy derived from typed state and separate from test selectors and data attributes.

### Stage 4 verification

- [ ] Add or update tests proving browser self-tests detect typed audio and MIDI click-track failures without message matching.
- [ ] Verify browser status data attributes expose the required typed terminal states.
- [ ] Build `shoopdaloop` and `shoop_audio_worklet` for `wasm32-unknown-unknown`.
- [ ] Run the relevant browser smoke and self-test scenarios at their supported viewport sizes.
- [ ] Run `python3 scripts/check_shoop_test_usage.py` before committing Rust test changes.

## Stage 5: Remove the generic notification mechanism

Depends on Stages 2 through 4. No notification producer or non-UI observer may remain before this stage starts.

- [ ] Delete the top-center `latest_notification` egui area.
- [ ] Delete the historical “Latest error” entry from the backend-health tooltip.
- [ ] Remove notification severity and notification message types from the application API.
- [ ] Remove notification storage and helper methods from the application model.
- [ ] Remove notifications from application snapshots, constructors, fixtures, and public re-exports.
- [ ] Remove notification count from egui tracing instrumentation.
- [ ] Rewrite remaining tests to assert typed feature/task state or diagnostic behavior.
- [ ] Confirm feature-owned validation, settings diagnostics, connection errors, script logs, FX status, click-track failures, and audio-driver messages still render in their existing contexts.

### Stage 5 verification

- [ ] Confirm repository searches find no generic notification type, collection, helper, renderer, or message-matching test.
- [ ] Run focused egui paint tests for I/O, settings, connections, scripts, FX, click-track, and form validation.
- [ ] Run `python3 scripts/check_tracing_coverage.py --require-closed`.
- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `RUSTFLAGS="-D warnings" cargo build --workspace`.
- [ ] Run `python3 scripts/check_shoop_test_usage.py` before committing Rust test changes.

## Stage 6: Final end-to-end validation

Depends on completion of all prior stages.

- [ ] Manually exercise invalid dialog input and confirm the error remains inline and clears when corrected or the interaction closes.
- [ ] Manually exercise representative backend, script scan, connection, click-track, FX, settings, and I/O failures and confirm each follows its classified route.
- [ ] Confirm no generic error or warning floats over the native or browser egui application.
- [ ] Confirm console diagnostics contain actionable context and recurrent failures do not flood output.
- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `RUSTFLAGS="-D warnings" cargo build --workspace`.
- [ ] Run `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend --profile ci`.
- [ ] Run `python3 scripts/check_tracing_coverage.py --require-closed`.
- [ ] Run `python3 scripts/check_shoop_test_usage.py`.
- [ ] Build the Wasm application and audio worklet and run the documented browser smoke checks when browsers are available.
- [ ] Review the final diff against every immutable acceptance criterion and document any environment-limited validation in the implementation PR.
