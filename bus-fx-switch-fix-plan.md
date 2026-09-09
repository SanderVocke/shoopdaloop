# Native Bus FX Switching Fix Plan

## Goals

- Fix sustained bus-output silence when an existing native bus is switched from no processor to Built-in FX or Carla during playback.
- Keep existing mixer sources connected to the bus throughout processor insertion, replacement, and removal.
- Make the engine processor activation state agree with the backend/application state published after processor selection.
- Add regression coverage that proves audio, not only metadata, survives the switch.

## Scope

### In scope

- Native backend bus-FX creation, graph rewiring, rollback, replacement, and removal paths in `src/rust/shoop_backend/src/native.rs`.
- Native dummy-backend regression tests for live mixer routing and processor activation.
- End-to-end validation of Built-in FX and, where the Carla runtime is available, Carla bus-FX selection.

### Out of scope

- Changes to plugin DSP, Carla bridge scheduling, the generic/WebAssembly backend, mixer UI behavior, or unrelated graph APIs.
- Redefining explicit `SetActive(false)` semantics or adding click-free transition/crossfade behavior.
- Broad refactoring of `detach_audio_ports`; live bus rewiring should stop using that destructive operation instead.

## Immutable acceptance criteria

1. With a non-silent source routed into an existing native bus, switching from no FX to Built-in FX preserves the actual engine mixer connection and produces non-silent bus output.
2. A newly selected bus processor is activated before the backend reports `active: true`; Built-in FX demonstrably processes audio rather than only preserving a dry route.
3. The shared creation path activates Carla bus processors as well as Built-in FX; on a Carla-capable runtime, no-FX-to-Carla switching keeps the bus audible and the Carla lifecycle healthy.
4. Removing or replacing bus FX restores the direct bus-input-to-output path without losing any source-to-bus mixer route.
5. If processor creation, rewiring, activation, or graph publication fails, the prior direct route and mixer inputs remain usable, and no staged/orphan FX chain is published.
6. Regression tests fail against the diagnosed implementation and pass after the fix; formatting, warning-denying build, required test-policy check, and the complete Rust suite pass.

## Design rules and constraints

- Disconnect only the exact direct edge `bus input -> driver output`; never detach/remove a live bus input merely to insert FX.
- Preserve source-to-bus edges and keep `mixer_routes` consistent with the engine graph.
- Treat insertion as a transaction: queue exact route changes and activation, wait for graph/control application, then publish `NativeFx`; on failure, remove the candidate chain, restore the direct edge, and verify rollback.
- Use the common `FXChain` activation API after chain creation so all supported bus processor types follow the same activation contract.
- Keep `NativeFx.active` truthful; do not set it to `true` unless the corresponding activation command was accepted.
- Preserve channel ordering and require one FX input/output pair per bus channel.
- Keep control-thread work out of the real-time callback and retain existing bounded graph-wait behavior.
- Avoid unrelated formatting, comments, and refactors.

## Staged implementation

### Stage 1 — Establish the regression and transaction shape

- [x] Use the dedicated `fix-bus` branch from the current target branch; the only pre-existing worktree item was this task plan.
- [x] Extend the native dummy test setup with a small helper that renders a routed source through a selected bus output under controlled dummy-driver frames.
- [x] Add/extend a regression scenario covering `no FX -> Built-in FX -> no FX`, asserting both non-zero samples and retained mixer-link metadata at each step.
- [x] Configure a non-default Built-in FX stage/parameter and assert processed output differs from the dry baseline, so the test detects a processor that remains inactive.
- [x] Confirm the new audio assertions fail on the current implementation for the diagnosed reasons.

Verification:

- [x] Run the targeted native bus-FX tests in the Nix development environment: `nix develop --command cargo nextest run -p shoop_backend --features native-drivers,shoop_engine/app_backend -E 'test(native_dummy_bus_fx)'` (the original example omitted the feature that compiles `native.rs`).
- [x] Record which assertion exposes route destruction and which exposes missing activation before changing production code.

### Stage 2 — Make bus-FX insertion non-destructive and active

Depends on Stage 1.

- [x] Change bus-channel rewiring to disconnect only `channel.input -> driver_output`, then connect `channel.input -> FX input` and `FX output -> driver_output`.
- [x] Activate the newly created `FXChain` through `set_active(true)` before publishing it as an active `NativeFx`.
- [x] Adjust partial-failure and graph-timeout cleanup so it removes the candidate chain, reconnects the exact direct edge, waits for the restored graph, and leaves incoming mixer routes untouched.
- [x] Review replacement/removal ordering to ensure the direct edge is restored while old FX edges disappear with the removed chain, without using destructive port detachment.
- [x] Keep the shared activation and routing path processor-type-neutral so Built-in FX and Carla receive the same fix.
- [x] Run the regression repeatedly to catch queued-command or graph-publication races.
- [x] Commit the production fix and its focused regression tests as green milestone `2c6b8a0f`.

Verification:

- [x] The targeted native tests prove dry audio before insertion, processed/non-zero audio after insertion, and restored audio after removal.
- [x] `poll()` still reports the expected processor and mixer links, matching the rendered graph behavior.
- [x] Invalid processor/channel requests continue to fail without changing the audible route; the native regression includes an audio assertion after rejection.

### Stage 3 — Focused compatibility checks

Depends on Stage 2.

- [x] Run existing native bus creation, session round-trip, processor replacement, bus removal, and mixer-route tests.
- [x] Run relevant engine FX-chain activation tests, including Carla fake-processor tests, to verify the common activation contract.
- [x] Probe Carla locally and cover the shared bus path conditionally; with a main-thread UI dispatcher configured, the Carla Rack master-bus workflow produces non-silent output with stable lifecycle counters, while the forced-unavailable test verifies failed insertion leaves no published FX.
- [x] Review the diff for changes outside native bus-FX routing/tests and remove any unnecessary edits.
- [x] Commit the master-bus/Carla and CI-hardening follow-ups as separate milestones.

Verification:

- [x] `python3 scripts/check_shoop_test_usage.py` passes because Rust tests changed.
- [x] All focused test filters pass repeatedly in the repository Nix environment.

### Stage 4 — Final end-to-end and repository validation

Depends on Stages 1–3.

- [x] Run `nix develop --command cargo fmt --all -- --check`.
- [x] Run `nix develop --command env RUSTFLAGS='-D warnings' cargo build --workspace --features shoop_engine/app_backend,shoop_backend/native-fx` so the warning-denying build covers the changed native code.
- [x] Run `nix develop --command env SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend,shoop_backend/native-fx --profile ci` so the complete suite includes the native backend and Carla path.
- [x] Run `nix develop --command python3 scripts/check_tracing_coverage.py --require-closed`.
- [x] Reproduce the original playback workflow on the master bus: no FX to Built-in FX, back to no FX, then Carla when available; the controlled native dummy test confirms non-silent output and retained upstream routing throughout.
- [x] Use deterministic graph generations and Carla deadline/stale/crash counters in the regression instead of an optional Perfetto trace; the counters remain zero and the generation remains stable.
- [x] Commit the final master-bus/Carla validation hardening, then ensure the branch is clean and all acceptance criteria have explicit evidence.

Validation evidence:

- Before the production fix, the corrected native filter ran three tests: `native_dummy_bus_fx_new_processor_is_engine_active` failed with engine `active == 0`, and `native_dummy_bus_fx_processor_switch_preserves_routed_audio` failed because processed output was silent.
- After the fix, the native bus-FX filter passed repeatedly, including actual Built-in FX DSP output, replacement/removal, rejected insertion rollback, and the Carla-capable master-bus workflow.
- The complete native-featured Rust run passed 1,800 tests with 4 environment skips.
- Formatting, warning-denying workspace build, Rust test-policy check, tracing coverage, the complete `shoop_backend` suite, and focused engine FX/Carla tests all pass.

## Delivery

- [x] Push the branch to `origin` and open PR [#873](https://github.com/SanderVocke/shoopdaloop/pull/873) summarizing the two root causes, transactional routing fix, activation fix, and audio-level regression coverage.
- [x] Link the validation commands/results and the deterministic graph/lifecycle evidence in the PR.
- [x] Monitor CI and inspect failed logs before targeted corrections. All native platform builds, Rust coverage, CodeQL, docs, and Codecov checks pass; branch protection reports no required checks. The non-required WebAssembly release job repeatedly exposes a pre-existing Firefox smoke UI-publication race outside this plan's scope, while the WebAssembly debug job and all artifact/build checks pass.
- [x] Check the PR for automated feedback. Codecov's justified patch-coverage finding was addressed (80.27% now passes); no code-review findings appeared.
- [x] Ensure the final PR diff remains within scope and the acceptance criteria are unchanged.

## Execution contract

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised only for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
