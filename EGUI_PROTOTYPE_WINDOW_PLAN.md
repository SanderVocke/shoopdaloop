# Implementation Plan: Expand the egui Prototype Window

## Goals and scope

Expand the existing prototype window into a recognizable egui version of the main QML session view while retaining the existing loop widgets. The work includes:

- QML-like track columns with the existing dark colors, editable titles, and an intentionally inert track-menu button.
- Per-track output and input controls, including gain, stereo balance, output mute, input monitoring/mute, level activity, and MIDI activity where available.
- A details pane for the selected loop, including an egui-native audio waveform viewer and useful empty/loading states.
- A ShoopDaLoop logo/version area and live DSP load, xrun, buffer-size, and latency status.
- The global controls represented by `AppControls.qml`, with working stop-all, deselect, clear-loop actions, default record/grab mode, play-after-record, sync, solo, and fixed-cycle controls. The main global-menu button remains intentionally inert.
- QML/frontend adapters that synchronize state and route typed egui actions to the existing QML-side objects and application logic, following the same pattern as the loop-widget integration.

Out of scope:

- Implementing either the track menu or the main global menu.
- Reproducing the Qt waveform rendering algorithm exactly.
- Moving existing QML/backend business logic into `shoop_egui`.
- Building a standalone browser application or replacing the current QML application window.
- Porting the advanced waveform editing, snapping, and composite-loop editing tools from the QML details pane.

## Immutable acceptance criteria

These criteria may not change without explicit user approval.

1. `shoop_egui` remains free of Qt/QML types and dependencies, contains no backend mutations or application business logic, creates no windows, and continues to compile for `wasm32-unknown-unknown`.
2. Each prototype track is visually separated using styling comparable to the QML track area, shows its loops, supports editing its title, and shows a track-menu affordance that causes no application action.
3. Editing a title or changing a track control in egui updates the corresponding QML `TrackWidget` or `TrackControlWidget`; subsequent QML-side state changes are reflected back in egui without feedback loops.
4. Track controls expose the applicable output/input gain, stereo balance, output mute, input monitoring/mute, audio level, and MIDI activity state. Controls that do not apply to a track are hidden or disabled rather than issuing invalid commands.
5. The details pane follows the current QML loop selection, handles no-selection and unavailable-data cases, and renders fetched audio channel data as a responsive waveform. Waveform rendering is implemented with platform-independent egui painting and remains usable when zoomed or resized.
6. The prototype shows the ShoopDaLoop logo/version and live status for DSP load, xruns, buffer size, and calculated latency.
7. Global controls show current QML-side state and invoke the same existing application behavior as their QML counterparts. The main global-menu button is visible but has no effect.
8. Existing egui loop interactions continue to operate, and existing Rust and QML test suites have no regressions.

## Design rules and constraints

- Follow `EGUI_PROTOTYPE_DESIGN_RULES.md` throughout implementation.
- Define plain Rust input models and typed output actions in `shoop_egui`. The crate may render state and retain local presentation state only.
- Keep all QObject, QVariant, signal/slot, asynchronous channel-data fetching, and QML object lookup code in `frontend` and `src/qml`.
- Route user intent outward; QML remains responsible for transitions, selection changes, registry updates, track-control mutations, clears, and all other business logic.
- Treat QML as authoritative. Suppress no-op updates and ensure echoed state does not repeatedly reissue commands.
- Fetch waveform data asynchronously through the existing channel data path. Convert Qt payloads to owned plain Rust samples at the frontend boundary before passing them to `shoop_egui`.
- Downsample or bin waveform data to the visible pixel width so rendering cost is bounded by the viewport rather than the complete recording length.
- Use only browser-compatible dependencies in `shoop_egui`. Embed or otherwise provide logo data without filesystem or native-window assumptions.
- Preserve the current loop-widget behavior and static snapshot assumptions unless a change is required to synchronize one of the newly scoped elements.

## Staged implementation

### Stage 1 — Establish application-level state and action contracts

- [x] Add plain `shoop_egui` models for application/global status, global controls, track presentation and controls, selected-loop details, and waveform channels.
- [x] Add typed actions for title edits, track controls, details visibility/selection where needed, and every functional global control.
- [x] Split the prototype into focused reusable components while retaining `TracksWidget`/`LoopWidget` as the loop-grid foundation.
- [x] Add unit tests for value clamping/conversion, action generation, applicability rules, and waveform binning over empty, short, and long inputs.
- [x] Verify with `cargo test -p shoop_egui` and `cargo check -p shoop_egui --target wasm32-unknown-unknown`.
- [x] Commit the state/action contracts and tests as the first milestone.

Verification: six `shoop_egui` unit tests pass and the crate checks for `wasm32-unknown-unknown`.

### Stage 2 — Implement and connect track presentation

Depends on Stage 1.

- [x] Replace the simple grouped track heading in `src/rust/shoop_egui/src/tracks_widget.rs` with a QML-like track card: dark track background, editable title, inert menu button, loop stack, and a reserved controls area.
- [x] Implement output/input gain sliders, stereo balance controls, output mute, input monitor/mute, peak meters, and MIDI indicators from the plain track-control state.
- [x] Extend `src/rust/frontend/src/egui_window.rs` with invokables for track/control state and signals for typed track actions, keeping conversion and queueing in the frontend crate.
- [x] Add a QML track-state bridge analogous to `EguiLoopStateBridge.qml`; bind it to each `TrackWidget` and its `TrackControlWidget`, and route egui signals to existing setters/properties.
- [x] Ensure title/control changes made by either UI are reflected by the other and do not create update loops.
- [x] Add focused tests for frontend state conversion and QML-side track-control routing where practical.
- [x] Verify with `cargo test -p shoop_egui`, `RUSTFLAGS="-D warnings" cargo build`, and a prototype integration test using mono, stereo, audio, and MIDI tracks.
- [x] Commit the completed track presentation and integration milestone.

Verification: the warning-free workspace build and crate tests pass; `tst_EguiWindow.qml` initializes stereo, mono, and MIDI tracks and verifies all title/control routing handlers.

### Stage 3 — Add the application shell, logo/status, and global controls

Depends on Stage 1 and should be integrated after the track layout is stable.

- [x] Add a top-level `shoop_egui` window-content component that lays out the global toolbar, tracks, details pane, and logo/status area within the host-provided egui surface.
- [x] Add the ShoopDaLoop logo/version presentation using a browser-compatible embedded asset path.
- [x] Add read-only DSP load, xruns, buffer-size, and latency presentation from plain status state.
- [x] Implement the global control toolbar, including functional stop-all, deselect, clear variants, record/grab default, play-after-record, sync, solo, and fixed-cycle controls, plus the inert main-menu button.
- [x] Expose status/global-state invokables and global-action signals in the frontend bridge.
- [x] In `EguiWindow.qml`/`Session.qml`, pass the backend and existing `AppControls` context needed to mirror state and dispatch actions; factor reusable QML functions out of inline handlers when both UIs need the same behavior.
- [x] Verify every global action routes to the same QML functions as the QML control and that registry state changes flow through the shared state bridge.
- [x] Run focused Rust tests, `RUSTFLAGS="-D warnings" cargo build`, and the global-control/status integration test.
- [x] Commit the application shell and global integration milestone.

Verification: the warning-free workspace build, crate tests, wasm check, and `tst_EguiWindow.qml` pass. The QML test verifies every global action route and all registry-backed global settings.

### Stage 4 — Add selected-loop details and waveform rendering

Depends on Stages 1 and 3.

- [ ] Implement a reusable egui waveform component using viewport-width min/max or peak bins, with clear channel labels, center lines, and playback/loop-region markers when the supplied state provides them.
- [ ] Implement the details pane with selection title, collapsible/resizable presentation, per-audio-channel waveforms, and no-selection/loading/no-audio placeholders.
- [ ] Add a QML details bridge that tracks the same selected-loop registry data as `DetailsPane.qml`, observes channel metadata/data dirtiness, and requests channel data through the existing asynchronous fetch mechanism.
- [ ] Add frontend QVariant/shared-channel-data conversion that owns the resulting samples in plain Rust state and requests repaint without blocking the egui draw path.
- [ ] Guard against stale asynchronous results when selection or channel identity changes.
- [ ] Verify selection changes, recording/data updates, empty loops, mono/stereo loops, window resizing, and large recordings.
- [ ] Run waveform unit tests, `cargo check -p shoop_egui --target wasm32-unknown-unknown`, and `RUSTFLAGS="-D warnings" cargo build`.
- [ ] Commit the details-pane and waveform milestone.

### Stage 5 — End-to-end validation and polish

Depends on all prior stages.

- [ ] Confirm at common and minimum prototype-window sizes that controls remain reachable, horizontal/vertical scrolling is intentional, and no component creates an external/native window.
- [ ] Confirm both directions of synchronization for loop state, track names/controls, selection/details, global controls, and live status.
- [ ] Confirm the track and global menu buttons are present and inert.
- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `RUSTFLAGS="-D warnings" cargo build`.
- [ ] Run `cargo test --workspace --features shoop_engine/app_backend`.
- [ ] Build and run `target/debug/shoopdaloop_dev.sh --self-test` for the frontend/QML suite.
- [ ] Run `cargo check -p shoop_egui --target wasm32-unknown-unknown` as the final browser-compatibility gate.
- [ ] Manually launch the application, open the egui prototype from a loaded session, exercise all acceptance criteria, and record the tested session/track configurations in this plan.
- [ ] Commit any final fixes and the completed-plan status.

## Execution contract

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
