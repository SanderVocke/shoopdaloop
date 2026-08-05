# Milestone 1 Plan: Pure egui Tracks and Loops Vertical Slice

## Completion status

In progress. Stages 1 and 2 are complete and verified.

This is the first major implementation milestone under `EGUI_REPLACEMENT_PROJECT.md`. `EGUI_FEATURE_PARITY_MATRIX.md` is the detailed discovery and parity ledger for the milestone.

## Goals and scope

Create the first usable pure native egui ShoopDaLoop application slice. It must run without a Qt application, own track/loop business state outside the presentation crate, communicate with the existing engine through a non-Qt backend boundary, and present a QML-recognizable tracks-and-loops workspace.

The milestone includes:

- A thin native egui executable and a backend-free GUI preview executable.
- A small framework-independent application API with stable entity IDs, immutable snapshots, and typed intents.
- An application actor owning the subset of business logic needed by the included controls.
- A non-Qt backend façade sufficient to create and operate direct audio/MIDI tracks and loops.
- A distinct sync track and horizontally arranged main tracks with vertically aligned loop slots and track controls below the loop viewport.
- Every currently implemented `shoop_egui` presentation area: global controls, track widgets, loop widgets, track controls/meters, details/waveform pane, logo, and status.
- An add-track button and one egui Add Track dialog for regular/direct tracks with disabled/mono/stereo/custom audio and optional MIDI.
- An add-loop button on each main track, including QML-compatible slot alignment behavior.
- Functional existing egui actions: track naming and controls; loop selection, targeting, play, record, stop, and gain; global controls; selected-loop details and waveform display.
- Dummy-backend tests and a native smoke workflow.

The old Qt application remains available during this milestone as a behavior oracle and regression surface. Removing the `frontend` crate, QML, Qt dependencies, session persistence, and production Qt entry point is not part of this milestone.

Out of scope:

- Track or loop context menus and their actions.
- Any dialog other than Add Track.
- Track/loop drag reordering, track resizing, or track deletion.
- Dry/wet and trigger-only track creation.
- Connection management, settings, driver configuration UI, FX chains, and FX state.
- Session load/save and audio/MIDI import/export.
- Grab, play-dry, dry-to-wet re-recording, stereo loop balance, and advanced loop details editing.
- Composite-loop creation/editing, Lua, MIDI-control configuration, keyboard parity, monitoring, profiling, and developer tools.
- Switching packaging or the production executable away from Qt.

## Immutable acceptance criteria

These criteria may not change without explicit user approval.

1. A native egui executable opens and operates the milestone workspace without creating a Qt application or depending on `frontend`, QML, CXX-Qt, `egui-cxx-qt`, or Qt helper crates in its dependency graph.
2. `shoop_egui` remains presentation-only and browser-compatible: it receives plain immutable state, owns only presentation state, emits typed intent, creates no native window, and depends on neither the application implementation nor backend/engine implementation.
3. Framework-independent snapshots and intents use stable IDs rather than track/loop positions as identity. One application actor is authoritative for session topology, selection, targeting, and global-control state.
4. The workspace presents a distinct sync track and horizontally scrollable main track columns with editable headers, vertically aligned loop slots, usable vertical overflow, and track controls aligned below the loop viewport. The result must be recognizably equivalent to the QML tracks layout, though not pixel-identical.
5. The GUI includes all currently implemented egui presentation areas: global toolbar, track and loop widgets, track controls/meters, details/waveform pane, logo, version, DSP load, xrun count, buffer size, and latency.
6. The add-track button opens the only implemented dialog. Accepting it creates a regular/direct track with the chosen name, disabled/mono/stereo/custom audio channel count in the supported 0–10 range, optional MIDI, and at least eight empty loop slots or enough slots to match the existing longest main track. Canceling has no effect.
7. Each main track has an add-loop button. Activating it creates a new empty backend loop with the track's channel shape and port wiring and preserves the QML row-alignment rule across tracks. The sync track cannot add loops.
8. Track title, output gain/balance/mute, input gain/balance/monitoring, audio meters, and MIDI activity are bidirectionally reflected between application snapshot and backend state. Inapplicable controls do not issue invalid backend operations.
9. Existing loop-widget actions are functional against the backend: selection, additive/toggle selection, single targeting, play, record, stop, and gain. Live mode, progress, queued transitions, emptiness, sync, selection/target highlighting, audio levels, and MIDI activity flow back into snapshots.
10. Play/record/stop behavior honors the milestone subset of selected-loop grouping, target synchronization, global sync, solo-within-track, fixed recording cycles, and play-after-record semantics documented in the parity matrix.
11. Existing global egui controls are included and functional for stop all, deselect all, clear variants, default record/grab state, play after record, sync, solo, and fixed recording cycles. The main-menu and track-menu affordances remain inert; track and loop context menus do not open. Clear actions do not introduce a confirmation dialog in this milestone.
12. Selecting a loop updates the existing details pane; audio loops expose waveform data without blocking rendering, and no-selection/no-audio/loading states remain usable.
13. A backend-free preview can display representative sync, stereo audio/MIDI, mono audio, and MIDI-only tracks and capture all emitted intents without linking the engine, drivers, Lua, Qt, or the old frontend.
14. Existing Rust and QML test suites have no regressions, and deterministic application/backend tests plus a native dummy-backend workflow verify creation and control of tracks and loops.
15. Throughout implementation, `EGUI_REPLACEMENT_PROJECT.md` reflects coarse project and milestone status, while `EGUI_FEATURE_PARITY_MATRIX.md` records newly discovered behavior, implementation status, intentional deferrals, and verification evidence for this subset.

## Design rules and important constraints

- Follow the architecture and dependency direction in `EGUI_REPLACEMENT_PROJECT.md`.
- Treat `EGUI_FEATURE_PARITY_MATRIX.md` as a deliverable. Before implementing a behavior, confirm or refine its baseline entry; after verification, record the evidence.
- Do not port QML object registries, property bindings, QObject identity, QVariant payloads, or widget references into the new application model.
- Use a small API crate for stable IDs, read models, capabilities, intents, notifications, and shared values. It must not depend on egui or `shoop_engine`.
- Keep business intent above backend mechanics. The GUI requests actions such as triggering a loop or changing a track gain; the application actor decides affected loops, timing, validation, and backend commands.
- Keep the audio callback independent of GUI and actor locks. Use existing command queues and state mirrors through a non-Qt façade.
- Publish immutable, structurally shared snapshots. Separate structural revisions from bounded-cadence live values where useful; do not deep-copy waveform samples or the full session on every frame.
- Use stable IDs for egui persistence IDs and action routing. Positional coordinates may remain display/query data but are not identity.
- The Add Track dialog owns a temporary draft only. No backend/application mutation occurs until acceptance.
- Restrict Add Track to direct tracks in this milestone. Keep dry/wet and trigger-only options absent rather than exposing nonfunctional choices.
- Preserve current egui widget behavior unless an acceptance criterion or matrix entry requires extending its action/state contract.
- The main menu, per-track menu, and loop right-click surfaces must be inert. The existing clear drop-down is a toolbar menu, not a context menu, and remains in scope.
- Keep native windowing and renderer dependencies in the native runner. Prefer a lightweight renderer configuration and do not add native-shell dependencies to `shoop_egui`.
- The preview runner must depend only on the GUI/API side of the graph so presentation iteration does not relink backend, driver, LV2, Lua, or Qt code.
- Preserve the old Qt path during the milestone. Shared engine changes must continue to satisfy its tests.
- Errors from command saturation, backend creation, or stale IDs must be observable; never silently drop user intent.

## Staged implementation

### Stage 1 — Freeze the milestone contract and establish application API types

No implementation stage may narrow acceptance criteria based on incomplete parity discovery.

- [x] Review every `Required` and `Required subset` matrix entry against relevant QML, frontend Rust, user documentation, and tests; split entries where independently testable behavior is found.
- [x] Record known ambiguities or intentional milestone limitations in the matrix rather than hiding them in implementation.
- [x] Create the small framework-independent application API crate.
- [x] Define stable opaque IDs for tracks, loops, ports/channels as needed, snapshot revisions, and asynchronous data generations.
- [x] Move or replace shared application-facing state/action contracts currently owned by `shoop_egui`; define typed intents for all milestone controls plus add-track/add-loop.
- [x] Represent capability/applicability in snapshots so egui never derives backend validity from names or engine handles.
- [x] Convert `shoop_egui` action routing and persistent UI IDs from positional identity to stable IDs.
- [x] Add contract tests for ID stability, state clamping, direct-track validation, modifier-carrying intent construction, and compatibility routing; application-level stale-ID rejection is verified in Stage 2.
- [x] Update the project document's coarse status and matrix implementation/evidence columns with the completed contract work.

Verification:

- `cargo test -p shoop_app_api`
- `cargo test -p shoop_egui`
- `cargo check -p shoop_egui --target wasm32-unknown-unknown`
- Dependency inspection confirms `shoop_app_api` and `shoop_egui` do not pull in backend, engine, native windowing, Lua, or Qt crates.

Commit the application contract and stable-identity milestone before proceeding.

### Stage 2 — Establish the non-Qt backend façade and application actor

Depends on Stage 1.

- [x] Create the non-Qt backend crate/boundary around the reusable Rust application-backend API without moving Qt compatibility types into it.
- [x] Expose the Stage 2 operations and observations needed for driver/session startup, loop construction/control, state polling, and status; direct port/channel and waveform operations are added with their Stage 3 topology.
- [x] Provide a fake backend for deterministic application tests and an engine-backed implementation using the dummy driver.
- [x] Create the application crate and single-owner actor/handle model with bounded command delivery, explicit busy/disconnected errors, and immutable snapshot publication.
- [x] Model the sync track, ordered main tracks, ordered loop slots, stable IDs, selection, target, and global control state independently of widgets.
- [x] Implement initialization of a minimal session with one distinct sync track/loop and no required main tracks.
- [x] Convert available backend driver/loop state into structural and live snapshot sections at bounded cadence; port/channel state is verified with Stage 3 topology.
- [x] Add application/backend contract tests demonstrating snapshot-reader independence, observable stale-ID rejection, and identical fake/dummy basic backend behavior without exposing the real-time engine to readers.
- [x] Update the project status and matrix with newly discovered backend behavior and verification evidence.

Verification:

- Targeted tests for the API, application, backend façade, and `shoop_engine` application backend.
- Dummy-driver tests create the sync loop, poll state, submit a transition, and observe the resulting snapshot.
- `RUSTFLAGS="-D warnings" cargo build` succeeds for the new crates and existing workspace path.

Commit the backend/application skeleton before adding topology mutations.

### Stage 3 — Implement direct track/loop topology and milestone business rules

Depends on Stage 2.

- [ ] Define and validate the direct-track draft/specification used by the Add Track dialog: name, audio channel count 0–10, and optional MIDI.
- [ ] Reimplement direct-track topology generation in typed Rust, including stable persistent names, external/internal port roles, ringbuffer requirements, and loop-channel wiring needed by the engine.
- [ ] Implement transactional track creation so failed backend construction does not publish a partially usable track.
- [ ] Create at least eight initial empty slots or enough to match the longest existing main track.
- [ ] Implement add-loop using the owning track's channel/port shape and the QML row-alignment rule.
- [ ] Implement track title and input/output gain, balance, mute/monitor commands with capability validation and no-op suppression.
- [ ] Implement loop selection and targeting using stable IDs, including ordinary replacement selection and modifier-driven additive/toggle selection.
- [ ] Implement play, record, and stop policy for selected groups, target synchronization, global sync, solo, fixed cycles, and play-after-record.
- [ ] Implement loop gain, stop-all, deselect-all, clear variants, and global state changes.
- [ ] Fetch selected-loop audio data asynchronously with generation checks and publish details/waveform states without blocking actor or draw paths.
- [ ] Add deterministic reducer/application tests for success, invalid/stale IDs, partial backend failure, row alignment, applicability, transition timing, and global policy combinations.
- [ ] Extend the matrix when tests reveal additional QML behavior and record which subset is deliberately deferred.

Verification:

- Application tests exercise stereo audio/MIDI, mono audio, MIDI-only, and audio-disabled direct tracks.
- Dummy-backend integration tests create multiple tracks, add aligned loops, operate every milestone control, and observe state mirrors.
- Waveform tests cover stale asynchronous results, empty/no-audio loops, and bounded presentation data.
- Existing targeted `shoop_engine` tests pass.

Commit track/loop topology and business behavior as one or more meaningful milestones.

### Stage 4 — Build the native shell, preview, QML-like layout, and Add Track dialog

Depends on Stages 1–3 for the real runner; the preview may begin after Stage 1.

- [ ] Create a thin native egui runner with application actor startup/shutdown, snapshot delivery, intent dispatch, repaint scheduling, and visible error/task reporting.
- [ ] Create a separate backend-free preview runner with representative sync, stereo audio/MIDI, mono audio, and MIDI-only snapshots and an intent log.
- [ ] Keep renderer/windowing dependencies out of `shoop_egui` and verify the preview's dependency graph excludes backend, engine, drivers, Lua, Qt, and frontend.
- [ ] Refactor the tracks presentation into a distinct sync area, horizontally scrollable main track columns, vertically aligned loop viewport, and fixed aligned track-control row comparable to QML.
- [ ] Preserve usability at minimum/common window sizes with intentional independent horizontal and vertical scrolling.
- [ ] Add QML-positioned add-track and per-main-track add-loop affordances.
- [ ] Implement the egui Add Track modal draft, validation, accept, and cancel behavior; expose direct tracks only.
- [ ] Include the existing global toolbar, details/waveform pane, logo, version, and backend status in the native workspace.
- [ ] Keep main/track menu affordances inert and ensure right-clicking loops opens no context menu.
- [ ] Add direct egui tests for add-button intents, dialog validation/accept/cancel, stable-ID routing after insertion, layout painting, scroll reachability, and inert excluded surfaces.
- [ ] Update the matrix and project status as the preview and native workspace become usable.

Verification:

- `cargo test -p shoop_egui`
- `cargo check -p shoop_egui --target wasm32-unknown-unknown`
- Preview dependency inspection and launch with all representative track shapes.
- Native runner launches with the dummy backend and paints at minimum and common sizes.
- Direct interaction tests prove that only Add Track opens a dialog.

Commit the preview and native presentation in meaningful, independently buildable steps.

### Stage 5 — Complete control integration and parity evidence

Depends on Stages 2–4.

- [ ] Route every existing egui loop, track, global, and details action through typed application intents; remove any milestone path that still expects a QObject/QML adapter.
- [ ] Verify live snapshots update all mode/progress/transition, selection/target, meter/activity, track-control, details, and status presentation.
- [ ] Verify Add Track and Add Loop against the real engine-backed dummy implementation, including rollback/error visibility.
- [ ] Verify selection modifiers, target synchronization, selected groups, solo, sync, fixed-cycle recording, and play-after-record against matrix expectations.
- [ ] Verify clear variants and stop-all while preserving the sync inclusion/exclusion choices.
- [ ] Verify no context menu, excluded dialog, connection/settings/FX surface, or nonfunctional Add Track option is reachable.
- [ ] Add replacement evidence to every M1 `Required` and `Required subset` matrix row. No required row may remain `Not started`, `Partial`, or without evidence.
- [ ] Update the project document to accurately report matrix exploration and built extent before final validation.

Verification:

- Focused application/backend/UI integration suites pass.
- A scripted dummy-backend workflow creates each supported direct-track shape, adds loops, drives all controls, selects a waveform-bearing loop, and observes expected snapshots.
- Manual comparison against the QML workspace confirms recognizable layout and documents any accepted visual adaptations in the matrix.

Commit the completed integrated vertical slice before final validation.

### Stage 6 — End-to-end validation

Depends on all prior stages.

- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `RUSTFLAGS="-D warnings" cargo build`.
- [ ] Run `cargo test --workspace --features shoop_engine/app_backend` using documented serialization or missing-backend allowances where required by the environment.
- [ ] Build and run `target/debug/shoopdaloop_dev.sh --self-test` to confirm the retained Qt/QML application has no regressions.
- [ ] Run `cargo check -p shoop_egui --target wasm32-unknown-unknown`.
- [ ] Inspect the native and preview dependency trees for forbidden dependencies.
- [ ] Launch the native application with the dummy backend and complete the full creation/control/details workflow at minimum and common window sizes.
- [ ] Launch with each supported real backend available in the development environment; document environment-related skips rather than weakening acceptance criteria.
- [ ] Confirm all milestone matrix rows contain accurate discovery, implementation, and evidence status.
- [ ] Mark this plan complete and update `EGUI_REPLACEMENT_PROJECT.md` with the achieved coarse status and remaining unexplored feature areas.
- [ ] Commit final validation fixes and document completion evidence.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
