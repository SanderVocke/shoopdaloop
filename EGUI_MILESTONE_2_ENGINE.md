# Milestone 2 Plan: Unified Native/Web Dummy-Engine Application

## Completion status

Complete. All five stages and every immutable acceptance criterion are implemented and verified. The native OS-window smoke was attempted but the available Xvfb server exposes no GLX framebuffer configuration; native construction, real-engine workflow, and minimum/common-size paint tests pass, and the unchanged native bootstrap retains its Milestone 1 launch evidence.

This is the second major implementation milestone under `EGUI_REPLACEMENT_PROJECT.md`. It replaces the separate `shoopdaloop_native` and backend-free `shoop_egui_preview` runners with one target-neutral egui application package named `shoopdaloop_egui`. `EGUI_FEATURE_PARITY_MATRIX.md` remains the detailed discovery, implementation, and evidence ledger.

Milestone 1 remains a record of what was completed and verified at that boundary. This milestone intentionally supersedes its separate backend-free preview runner with one application that exercises the real application and engine architecture on both native and browser targets.

## Goals and scope

Create one egui composition root that runs the current tracks-and-loops feature set through `shoop_app`, `shoop_backend`, and `shoop_engine` with the dummy driver on both native and `wasm32-unknown-unknown` targets.

The milestone includes:

- Consolidating `shoopdaloop_native` and `shoop_egui_preview` into `shoopdaloop_egui`.
- Sharing the eframe application, widget, intent dispatch, snapshot display, error presentation, and dummy-backend startup across native and browser builds.
- Retaining only target-specific eframe bootstrap and runtime scheduling code.
- Splitting the dummy application-backend feature graph from native JACK, CPAL, Midir, LV2, frontend, and Qt dependencies.
- Refactoring application and engine services that currently require native workers so the browser can advance them cooperatively without blocking its event loop.
- Running actual dummy-driver engine cycles in the browser, rather than simulating loop state in the presentation runner.
- Preserving the existing threaded native dummy workflow and current `shoop_egui` behavior.
- Migrating the browser bundle, self-contained HTML build, CI workflow, and run documentation to the unified package.
- Removing the two superseded runner packages after equivalent native and browser evidence exists.

Out of scope:

- JACK, CPAL, Web Audio, AudioWorklet, audible output, microphone capture, and physical audio-device integration.
- Midir, Web MIDI, or physical MIDI-device integration.
- Wasm threads, shared Wasm memory, nightly `build-std`, or cross-origin-isolation requirements.
- Injecting live external audio/MIDI through the browser application. The dummy engine may process silence and engine-internal/test data.
- Session persistence, media import/export, connections UI, settings, driver selection, FX, scripting, and other deferred parity areas.
- Replacing the retained Qt production executable or removing native engine driver support used by it.
- Moving business state or engine handles into `shoop_egui`.

## Immutable acceptance criteria

These criteria may not change without explicit user approval.

1. One workspace package named `shoopdaloop_egui` replaces both `shoopdaloop_native` and `shoop_egui_preview`; the old runner packages are removed only after their required assets, workflows, documentation, and verification have migrated.
2. The package builds and starts as a native eframe application and as a browser application targeting `wasm32-unknown-unknown`. Both targets render the same `shoop_egui::AppWidget` from snapshots produced by the same application model.
3. Both targets start `shoop_app` with the engine-backed `shoop_backend` dummy implementation. The browser path does not use representative fixture snapshots or a presentation-local intent simulator as application truth.
4. On both targets, dummy engine cycles advance loop timing and apply the current egui intents for direct-track/loop creation, track controls, loop selection/targeting, play, record, stop, gain, global controls, and details publication. Native and browser runs expose equivalent observable application state for the same deterministic command and tick sequence.
5. Browser execution is cooperative and non-blocking: application commands, backend polling, dummy cycles, graph updates, and content-snapshot publication make bounded progress from the browser runtime without calling unsupported native thread, sleep, blocking channel receive, condition-variable wait, or join paths.
6. Cooperative dummy timing uses elapsed time with bounded per-update work and an explicit long-pause/catch-up policy. A throttled or backgrounded tab cannot trigger unbounded processing or freeze the next rendered frame, and the chosen dropped-time/xrun behavior is deterministic and tested.
7. Native execution preserves the current threaded application actor and automatic dummy processing behavior unless shared-runtime evidence justifies a documented revision. Existing engine real-time and no-allocation guarantees are not weakened by the cooperative adapter.
8. The unified runner's Wasm dependency graph contains the dummy-capable engine/application path but excludes JACK, CPAL, Midir, LV2, Lua, `frontend`, QML, CXX-Qt, Qt helper crates, and native window-system dependencies. Native real-driver support required by the retained Qt application remains available through separate features.
9. `shoop_egui` remains presentation-only, browser-compatible, and independently testable with plain snapshots and typed intents. Consolidating the runners must not add `shoop_app`, `shoop_backend`, `shoop_engine`, native windowing, or filesystem dependencies to it.
10. Browser startup, runtime failure, command saturation, and engine/backend errors are observable in the UI or browser diagnostics; no intent or cooperative runtime failure is silently discarded.
11. The browser bundle and self-contained HTML artifact are generated from `shoopdaloop_egui`, load without external application-resource requests where promised, and clearly describe themselves as a dummy-engine application rather than an audio-capable preview.
12. Deterministic runtime contracts and end-to-end workflows prove that native and browser targets create supported direct-track shapes, add loops, advance real dummy-engine cycles through record/play/stop, update details, and remain responsive at minimum and common viewport sizes.
13. Existing Rust and retained QML test suites have no regressions. Native engine backends and production paths outside the new runner retain their existing build and test coverage.
14. `EGUI_REPLACEMENT_PROJECT.md`, `EGUI_FEATURE_PARITY_MATRIX.md`, root/run documentation, and CI descriptions accurately track the consolidated-runner decision, the loss of a standalone backend-free preview, dummy-only limitations, implementation status, and verification evidence throughout the milestone.

## Design rules and important constraints

- Follow the dependency direction in `EGUI_REPLACEMENT_PROJECT.md`: runner to presentation/application, application to API/backend, and backend to engine. Do not let target adaptation reverse those dependencies.
- Treat the replacement of the backend-free preview as an explicitly approved project decision, but preserve Milestone 1 documents and evidence as historical records. Record the superseding Milestone 2 status in the project document and parity matrix instead of rewriting completed evidence.
- Use `shoopdaloop_egui` for the consolidated composition package. Keep `shoop_egui` as the host-independent presentation crate.
- Extract target-neutral state machines and one-step/tick operations before adding `cfg`-selected adapters. Do not maintain independent native and browser copies of application or engine business behavior.
- Keep browser scheduling cooperative and single-threaded for this milestone. Do not emulate native blocking APIs with busy loops, promises that are synchronously polled, or hidden assumptions that another worker will make progress.
- Keep every cooperative update bounded. Account for elapsed time explicitly, cap catch-up work, and expose dropped processing time as an xrun or another documented status rather than silently allowing timing drift.
- Preserve bounded command delivery and observable errors. Cooperative execution may change who owns a queue, but not its capacity/error semantics.
- Keep the native audio callback/driver path independent of GUI locks. Browser cooperation is an additional execution adapter, not a reason to collapse native real-time boundaries.
- Split Cargo features by capability and target. The dummy application backend must not enable all native backends as a side effect, while the retained frontend's existing full feature remains available.
- Ensure code excluded from Wasm is absent from its dependency graph, not merely unreachable at runtime.
- Preserve deterministic controlled-dummy APIs for tests. Compare native threaded and cooperative observations at stable synchronization points rather than relying on wall-clock races.
- Browser lifecycle code must tolerate animation-frame gaps and shutdown/drop without waiting for unavailable threads.
- The unified browser application is an engine demonstration, not a claim of browser audio/MIDI I/O. UI text and documentation must not imply that recorded silence is microphone capture.
- Keep fast presentation iteration through `shoop_egui` tests and plain snapshot fixtures even though there is no longer a separate fixture-driven executable.
- Treat the generated browser `dist` contents according to the repository's existing source-control policy; do not commit incidental local build artifacts.
- Errors from command saturation, engine startup, cooperative progress, or stale IDs must remain observable; never silently drop user intent.

## Staged implementation

### Stage 1 — Freeze the cross-target contract and isolate the dummy feature graph

No later stage may hide a native-only dependency behind runtime conditionals.

- [x] Review and refine the initial Milestone 2 matrix entries for the unified runner, real dummy-engine browser state, cooperative execution, dependency isolation, and cross-target evidence as the portability inventory adds detail.
- [x] Inventory native-thread, blocking-wait, timer, and platform-FFI assumptions reachable from `shoopdaloop_native` through `shoop_app`, `shoop_backend`, and `shoop_engine`; classify each as shared core, native adapter, cooperative adapter, or unavailable feature.
- [x] Define the target-neutral runtime tick/lifecycle contract, elapsed-time and catch-up rules, synchronization points for deterministic tests, and observable error behavior before changing implementation.
- [x] Isolate the dummy façade from the full `shoop_engine/app_backend` feature by implementing it over engine core, while preserving the existing full native feature for the retained frontend and tests.
- [x] Update `shoop_backend` so the unified runner requests only the dummy-capable engine core.
- [x] Add dependency and compile guards that fail if native driver/plugin crates leak into the Wasm runner graph.
- [x] Update the project document and matrix with the accepted architecture and portability discoveries.

Verification:

- Targeted native builds prove both the dummy-only and retained full native engine feature sets compile.
- A `wasm32-unknown-unknown` dependency inspection shows no JACK, CPAL, Midir, LV2, frontend, Qt, or native-window subtree in the intended runner graph.
- Contract tests cover timing accumulation, catch-up caps, dropped-time accounting, and lifecycle state independent of eframe.

Commit the feature-boundary and runtime contract before changing worker ownership.

### Stage 2 — Add cooperative dummy-engine services

Depends on Stage 1.

Implementation revision: the inventory showed that the milestone dummy façade needs only the target-neutral `shoop_engine::Session` core. Using it directly avoids adapting or invoking the full backend's graph, content, connection-cache, and dummy-driver workers in the browser. The retained frontend still builds and tests those native worker paths, while the native egui actor and browser pump drive the same simpler dummy façade.

- [x] Implement one dummy engine processing primitive owned by `shoop_backend` and driven from either the native application actor or browser cooperative runtime.
- [x] Implement elapsed-time accumulation into buffer-sized dummy cycles, including fractional remainder handling, an eight-cycle maximum per update, deterministic long-pause dropping, and xrun accounting.
- [x] Apply dummy façade graph changes synchronously at topology control points without condition-variable waits or queued work requiring another thread.
- [x] Read selected-loop content directly at stable application control points so waveform/details publication needs no browser worker.
- [x] Remove the egui dummy façade's dependency on engine command/query and `wait_process` paths that sleep for another thread.
- [x] Make startup, shutdown, and drop safe with no engine worker handles and across browser update gaps.
- [x] Preserve and re-run the retained native full-backend worker, real-time lock, and no-allocation paths.
- [x] Add deterministic backend tests for topology, transitions, exact frames, audio content, MIDI-capable loops, fractional timing, bounded catch-up, and xrun status.
- [x] Record the revised engine-core approach and evidence in the matrix and project document.

Verification:

- Targeted `shoop_engine` tests cover cooperative startup, graph mutation, exact frame advancement, transition timing, content publication, bounded catch-up, pause/resume, and shutdown.
- Existing dummy-driver, app-backend, real-time lock, and no-allocation tests pass on native.
- The dummy-capable engine and backend compile for `wasm32-unknown-unknown` without thread/FFI-only dependencies.

Commit cooperative engine execution before adapting the application actor.

### Stage 3 — Share application pumping across threaded and cooperative runtimes

Depends on Stage 2.

- [x] Extract one target-neutral application update path that owns ordered intent reduction, backend advancement/polling, notification publication, snapshot revisions, and bounded work.
- [x] Retain the native threaded runtime adapter and add a browser cooperative adapter invoked by the eframe update loop.
- [x] Keep typed intent and immutable snapshot behavior identical across adapters, including stable IDs, selection/target policy, topology mutation, global controls, details, and error notifications.
- [x] Ensure browser dispatch cannot re-enter mutable application/backend state and cannot block while engine work is required.
- [x] Define bounded repaint scheduling and cooperative command limits without an unconditional busy loop.
- [x] Add shared runtime contracts and real-engine workflows covering fake, native actor, and cooperative dummy paths at stable snapshot points.
- [x] Add stale-ID, queue-capacity, backend-failure, long-frame-gap, waveform refresh, and shutdown/drop coverage for the cooperative path.
- [x] Update matrix evidence and project status after the application/backend path passed its Wasm check.

Verification:

- `cargo test -p shoop_app`
- `cargo test -p shoop_backend`
- Native runtime workflow tests pass with the threaded dummy adapter.
- Cooperative runtime tests execute direct-track creation, add-loop, record/stop/play, controls, selection/details, and global policy against actual engine state.
- `shoop_app` and `shoop_backend` check for `wasm32-unknown-unknown` with dummy-only features.

Commit the shared application runtime before consolidating executable packages.

### Stage 4 — Create `shoopdaloop_egui` and retire the separate runners

Depends on Stages 1–3.

- [x] Create the `shoopdaloop_egui` package with one eframe application implementation and target-selected native-window/WebRunner bootstrap and threaded/cooperative runtime adapters.
- [x] Start the same dummy `EngineBackend`, application model, snapshot flow, and `AppWidget` on both targets.
- [x] Migrate the browser canvas shell, Trunk configuration, Wasm dependencies, self-contained HTML tooling, logging, and application resources from the preview package.
- [x] Replace preview-only representative snapshots, local intent application, and intent-log behavior with authoritative application snapshots, dispatch results, notifications, and browser diagnostics.
- [x] Preserve minimum/common viewport behavior and continuous-but-bounded repaint/progress scheduling.
- [x] Add target-neutral construction/workflow/paint tests and a browser self-test proving engine cycles, waveform content, and snapshot progress.
- [x] Update the root README, package README, development run instructions, CI workflow names/paths/artifact names, and browser text to use `shoopdaloop_egui` and state the dummy-only limitation.
- [x] Remove `shoopdaloop_native` and `shoop_egui_preview` after native tests, browser launch, migrated artifact generation, and replacement workflow tests passed.
- [x] Update the project document and matrix for the consolidated implementation.

Verification:

- `cargo run -p shoopdaloop_egui` launches the dummy-engine application natively.
- `cargo check -p shoopdaloop_egui --target wasm32-unknown-unknown` succeeds.
- `trunk build --release` succeeds from the consolidated package and the migrated self-contained HTML tool produces a loadable artifact.
- Browser-target runtime tests and a headless browser smoke verify canvas startup, engine frame advancement, and a scripted add-track/record/stop/play/details workflow without console errors.
- Source/workspace scans find no remaining package, workflow, or current documentation reference that treats either old runner as active.

Commit the unified package and old-runner removal as separately reversible meaningful milestones where practical.

### Stage 5 — Complete parity evidence and end-to-end validation

Depends on all prior stages.

- [x] Exercise every currently functional egui intent through application/backend tests and native/browser dummy workflows and record stable snapshot outcomes.
- [x] Verify oversized elapsed-time gaps remain bounded and visible as xruns, and browser revisions resume advancing.
- [x] Verify waveform/details publication, no-selection, empty-audio, and recorded-silence states without blocking rendering.
- [x] Inspect native and Wasm dependency trees for forbidden target leakage and confirm `shoop_egui` remains backend-free.
- [x] Run `cargo fmt --all -- --check`.
- [x] Run `RUSTFLAGS="-D warnings" cargo build`.
- [x] Run `cargo test --workspace --features shoop_engine/app_backend` with serialized/missing-backend allowances required by the environment.
- [x] Build and run `target/debug/shoopdaloop_dev.sh --self-test` offscreen to confirm the retained Qt/QML application has no regressions.
- [x] Run the consolidated runner's native construction/workflow and minimum/common-size paint tests; document the Xvfb GLX environment skip for OS-window smoke.
- [x] Run the Wasm compiler check, release Trunk build, self-contained artifact build, and browser self-test at 360×200 and 900×600.
- [x] Confirm every Milestone 2 matrix row has accurate discovery, implementation, and evidence status without erasing Milestone 1 history.
- [x] Update `EGUI_REPLACEMENT_PROJECT.md`, the root/package run documentation, and this plan with final status and evidence.
- [x] Commit the completed implementation and final validation documentation.

Final validation demonstrates one maintained `shoopdaloop_egui` composition package, actual dummy-engine progression on native and browser targets, bounded cooperative browser behavior, isolated target dependencies, migrated browser artifacts/CI, and no regressions in retained native/Qt paths.

Final validation evidence:

- Formatting and the warning-denying build pass.
- The serialized full workspace suite with `SHOOP_ALLOW_MISSING_BACKENDS=1` passes, including 621 `shoop_engine` unit tests, all engine integration suites, and the application/backend/presentation/unified-runner tests.
- The retained offscreen Qt/QML self-test reports 197 passed, 0 failed, and one environment skip for unavailable CPAL virtual playback ports.
- `shoopdaloop_egui`, `shoop_app`, `shoop_backend`, and `shoop_egui` check for `wasm32-unknown-unknown` as applicable.
- Wasm dependency scans exclude JACK, CPAL, Midir, LV2, frontend, Qt, X11, and Wayland packages; `shoop_egui` remains free of app/backend/engine implementation dependencies.
- The release Trunk bundle and approximately 9 MB self-contained HTML artifact build successfully.
- Headless Chrome self-tests at 360×200 and 900×600 create a stereo audio/MIDI track, disable sync, record real dummy-engine frames, stop, refresh non-empty waveform data, play, and continue advancing snapshot revisions without browser exceptions.
- Native tests construct the unified application, create every direct-track shape, record/control a loop against the real dummy engine, and paint at 360×200 and 900×600.
- Native OS-window smoke was attempted with the available Xvfb server, but it exposes no GLX framebuffer configuration. This is recorded as an environment skip rather than weakening the acceptance criteria; the eframe native bootstrap is unchanged from its passing M1 launch path.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
