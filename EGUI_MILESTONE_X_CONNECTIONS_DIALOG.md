# Milestone 4 Plan: Pure egui Track Port Connections Dialog

## Completion status

Implemented. The staged checklist and final evidence below record the completed fourth major milestone under `EGUI_REPLACEMENT_PROJECT.md`.

`EGUI_FEATURE_PARITY_MATRIX.md` is the detailed discovery and parity ledger for this milestone. The filename retains `X` because it was drafted before the already-completed engine and browser-audio milestones established the final ordering; connections are factually recorded as Milestone 4 throughout the current project documents.

## Goals and scope

Add a pure-egui connection-management workflow for application-owned track ports. The per-track options button opens the dialog scoped to that track, while the global/main menu opens the same dialog for all track ports, including the sync track. The dialog discovers compatible external audio and MIDI ports, shows current connection state, and lets the user connect or disconnect individual pairs through the application actor and non-Qt backend boundary.

The milestone includes:

- Stable application identities and typed metadata for every externally connectable track port.
- Immutable connection snapshots and typed set-connected intents in `shoop_app_api`.
- Application-owned port inventory, validation, command ordering, error reporting, and snapshot publication in `shoop_app`.
- Backend discovery, connection-state polling, connect, and disconnect operations in `shoop_backend`, using the active `shoop_engine` driver without Qt/QObject adapters.
- Audio input, audio output, MIDI input, and MIDI output ports created by the current direct and sync track topology.
- Typed role support for the QML dialog's audio input/output/send/return and MIDI input/output/send categories when such ports exist; future track topologies must not require name parsing in the GUI.
- One reusable egui dialog presentation with track and all-tracks scopes.
- The QML-recognizable tabbed connection matrix: local ports as columns, compatible external ports as rows, current connection indicators, unavailable/incompatible cells, client grouping, and usable scrolling.
- Dynamic updates while ports appear, disappear, connect, or disconnect outside the application.
- Representative backend-free preview data and deterministic fake/dummy-backend workflows.
- Continued browser WebAssembly compatibility for the presentation and preview.
- Living updates to the replacement project and feature-parity matrix throughout implementation.

The retained Qt/QML application remains the behavior oracle and regression surface. Pixel-identical rendering is not required.

Out of scope:

- Creating, deleting, renaming, or reordering ports or tracks.
- Editing the internal track/loop/FX routing graph; this dialog manages external driver connections only.
- Creating dry/wet tracks, FX send/return topology, FX chains, or trigger-only tracks. The dialog model must handle those typed roles when present, but their topology remains assigned to a later milestone.
- Persisting external connections, autoconnect rules, regular-expression matching, or reconnect-on-device-appearance behavior.
- Driver selection, driver settings, audio-device settings, or application settings UI.
- MIDI-control ports and other non-track application ports; the global scope matches the QML session dialog by aggregating sync and main track ports.
- Session save/load and archive migration.
- A free-form patchbay, connection graph editor, bulk connect/disconnect commands, or connection presets.
- Other main-menu and track-menu actions such as session I/O, settings, deletion, FX state, monitoring, and profiling.
- Switching the production entry point or packaging away from Qt, or deleting the retained frontend.

## Baseline behavior and evidence to preserve

Initial planning is based on these sources; Stage 1 must turn this into detailed matrix entries and resolve any ambiguity before implementation claims parity:

- `src/qml/ConnectionsControl.qml`: category tabs, 100 ms refresh behavior, union of external candidates, local-port columns, external-port rows, client grouping, connection/incompatibility indicators, and per-cell toggling.
- `src/qml/ConnectionsWindow.qml`: independently closable, resizable connection window and default sizing.
- `src/qml/TrackWidget.qml`: **Connections...** in each track options menu and track-specific port category lists.
- `src/qml/AppControls.qml` and `src/qml/Session.qml`: global **Connections** entry and aggregation of sync plus main track ports.
- `src/qml/AudioPort.qml`, `src/qml/MidiPort.qml`, and the frontend port bridge: external discovery/state and connect/disconnect behavior.
- `src/qml/test/tst_Jack_ports.qml` and `src/qml/test/tst_Cpal_ports.qml`: direction/type filtering, live state, and audio/MIDI connect/disconnect behavior, including application-owned ports as candidates.
- `docs/source/usage.trackcontrols.rst`: user-facing entry point for track connection management.
- `shoop_engine::app_backend`, dummy external-port tests, and JACK application-backend tests: reusable non-Qt discovery, connection cache, external mutation, direction, and data-type behavior.

Known baseline details:

- Empty port categories are omitted.
- Categories are Audio in, Audio out, Audio send, Audio return, MIDI in, MIDI out, and MIDI send.
- A local input accepts compatible external outputs; a local output connects to compatible external inputs. Audio and MIDI candidates never mix.
- The candidate set for a category is the union returned for its local ports. A cell is disabled when that candidate is not connectable for that local port.
- Full external names are connection identities. Client and short-port components are presentation grouping only.
- The QML dialog refreshes while visible and reflects connections changed by another client.
- The global dialog includes the sync track and every main track, but not unrelated application ports.

## Immutable acceptance criteria

These criteria may not change without explicit user approval.

1. `shoop_egui` remains presentation-only and browser-compatible. It receives immutable plain Rust connection state, keeps only dialog presentation state, emits typed intent, and depends on neither `shoop_app`, `shoop_backend`, `shoop_engine`, the old frontend, nor Qt.
2. Every externally connectable track port has a stable `PortId` plus explicit application-level data type, direction, role/category, owning `TrackId`, and display name. Backend handles and engine enums do not enter GUI snapshots, and no layer derives semantic roles from port-name suffixes.
3. The application actor is authoritative for the track-port inventory and serializes connection mutations. A connection intent identifies the local port by stable ID, the external endpoint by its exact full name, and the desired connected state rather than relying on a positional cell or blind toggle.
4. Activating **Connections...** from any track options menu, including the sync track, opens the connection dialog with only that track's externally connectable ports. No port from another track is shown in that scope.
5. Activating **Connections** from the global/main menu opens the same dialog with the union of externally connectable ports from the sync track and all main tracks. Other deferred menu entries remain unavailable or inert.
6. The dialog omits empty categories and supports Audio in, Audio out, Audio send, Audio return, MIDI in, MIDI out, and MIDI send when represented by its input state. Current direct/sync engine integration must populate audio and MIDI input/output categories correctly.
7. Within each category, the dialog shows deterministic local-port columns and external-port rows, recognizable client grouping and short names, connected and unavailable/incompatible indicators, and both-axis overflow suitable for many ports. It remains usable at the native minimum window size and in the browser preview.
8. Candidate eligibility preserves backend direction and data-type rules: local inputs see outputs, local outputs see inputs, and audio and MIDI do not cross. Application-owned driver ports may appear as external candidates when the active driver reports them, matching QML behavior.
9. Activating an eligible disconnected cell requests a connection; activating an eligible connected cell requests a disconnection; an ineligible cell cannot emit a mutation. The UI converges to subsequently observed backend truth and does not silently claim success merely because a command was accepted.
10. Unknown/stale local IDs, disappeared external endpoints, command saturation, backend rejection, and connection timeout/failure are observable and cannot mutate another cell. Pending state is bounded and eventually resolves to confirmed state or a visible error.
11. While relevant connection data is being monitored, external endpoint appearance/disappearance and out-of-process connection changes become visible at a bounded cadence without per-cell backend calls or GUI-thread blocking. Candidate ordering is deterministic despite backend map order.
12. Closing and reopening the dialog, or switching between track and global scope, cannot reuse stale scope data or redirect a late result. Dialog visibility, selected tab, scroll positions, and window geometry remain presentation state rather than session truth.
13. The backend-free preview includes representative multi-client audio/MIDI candidates, connected, disconnected, unavailable, empty, loading, pending, and error states; it can exercise both scopes and capture connection intents without linking backend, engine, driver, frontend, or Qt code.
14. Deterministic tests cover fake and engine-backed dummy discovery and mutation, direction/type filtering, endpoint churn, stale operations, and failure visibility. Existing engine, Rust workspace, retained QML, wasm, native, and self-contained HTML preview gates have no regressions.
15. `EGUI_FEATURE_PARITY_MATRIX.md` contains explored connections-milestone entries and replacement evidence for the accepted scope, and `EGUI_REPLACEMENT_PROJECT.md` records the milestone's active/completed status, roadmap ordering, and remaining connection-related deferrals.

## Design rules and important constraints

- Follow the crate boundaries and dependency direction in `EGUI_REPLACEMENT_PROJECT.md`.
- Treat `EGUI_FEATURE_PARITY_MATRIX.md` as a milestone deliverable. Expand the coarse connection area and revise its milestone-specific status vocabulary/columns so connections discovery, target, implementation, and evidence are unambiguous.
- Update the replacement project's coarse roadmap to record connections as Milestone 4, after the already-completed engine and browser-audio milestones; do not leave persistence described as the next milestone.
- Keep dialog visibility, selected tab, scrolling, window placement, and temporary hover/pending presentation in `shoop_egui`. Keep port inventory, eligibility, confirmed connection state, validation, and failures outside widgets.
- Use stable local `PortId` values for egui IDs and intent routing. External full names are backend endpoint keys, not parsed semantic identities; split `client:port` only for display and handle names without a colon.
- Define application-level data type, direction, and role values in the API crate. Convert to engine types only at the backend boundary.
- Assign port roles when typed topology is constructed. Do not reproduce QML's ID regular expressions in Rust business logic or GUI code.
- Represent a desired final connection state (`connected: bool`) rather than a toggle command so stale snapshots cannot invert a newer state.
- Preserve confirmed backend truth separately from pending requests. Coalesce duplicate requests where safe, disable or annotate pending cells, and make rejection/timeout visible through typed state or notifications.
- Never hold GUI/application locks across driver calls, and never perform discovery or connection work on the audio callback. Reuse the engine's asynchronous connection cache/command mechanisms or strengthen them without introducing real-time waits.
- Do not enumerate the backend once per rendered cell. Poll or publish compact port/candidate state at a bounded cadence and use structural revisions/sharing to avoid rebuilding an unchanged matrix every frame.
- Sort local ports, clients, and external endpoints by explicit stable keys before publication or presentation. Never rely on `HashMap`, driver enumeration, or QObject order.
- Keep internal direct-track wiring intact. External connection mutation must not alter loop-channel connections or monitoring paths.
- Make backend errors explicit. If existing engine APIs only log or optimistically cache a failed operation, add an acknowledgement/result path or verify eventual state and report failure; do not copy silent behavior into the new architecture.
- Preserve current direct and sync track topology and controls. Generalize the connection model for future send/return roles without pulling dry/wet/FX implementation into this milestone.
- Keep the preview backend-free and preserve native/web use of the same presentation code. Native window creation and renderer setup remain outside `shoop_egui`.
- Preserve the retained Qt path and its tests. Shared engine changes must not regress QML connection behavior.

## Staged implementation

### Stage 1 — Freeze parity scope and define the connection contract

No later stage may narrow the acceptance criteria because discovery was incomplete.

- [x] Inspect all baseline sources above and exercise the QML track/global dialogs with representative audio and MIDI ports where the environment permits.
- [x] Expand the parity matrix's coarse connection area into independently testable connections entries covering menu entry points, scopes, categories, layout, discovery, compatibility, live state, connect/disconnect, failures, and dynamic endpoint changes.
- [x] Record persistence, autoconnect, non-track ports, driver settings, dry/wet topology, and other explicit deferrals as separate matrix entries rather than omissions.
- [x] Update `EGUI_REPLACEMENT_PROJECT.md` to record connections as the fourth major milestone and next completed roadmap slice.
- [x] Define framework-independent port data type, direction, role/category, local port snapshot, external candidate, eligibility/confirmed/pending state, connection-view revision, and any typed error/status values in `shoop_app_api`.
- [x] Extend track/application snapshots with a structurally shared connection read model keyed by stable `TrackId` and `PortId` without deep-copying the full matrix each frame.
- [x] Define a set-connected application intent containing stable `PortId`, exact external endpoint name, and desired state; keep open/close/tab state out of business intents unless evidence demonstrates a non-presentation requirement.
- [x] Add contract tests for stable identity, role/category mapping, exact endpoint preservation, desired-state semantics, deterministic ordering keys, and snapshot independence.
- [x] Confirm API and GUI dependency trees remain free of backend, engine, frontend, and Qt dependencies.

Verification:

- `cargo test -p shoop_app_api`
- `cargo test -p shoop_egui`
- `cargo check -p shoop_egui_preview --target wasm32-unknown-unknown`
- Dependency-tree scans for `shoop_app_api` and `shoop_egui`.
- Matrix review shows no connections-required behavior left only in the coarse future-area row.

Commit the connection contract, parity expansion, and project-status update before backend integration.

### Stage 2 — Add backend port inventory, discovery, and mutation

Depends on Stage 1.

- [x] Introduce backend-stable port identity and typed port descriptors for the audio/MIDI input/output handles created with each direct or sync track.
- [x] Return created port descriptors with track construction and retain handle mappings inside `shoop_backend`; do not expose engine handles to the application or GUI.
- [x] Add backend operations to obtain compatible external candidates and confirmed state for a local port and to request a desired external connection state.
- [x] Preserve direction/type filtering through explicit conversions at the backend boundary and keep internal track wiring unchanged.
- [x] Reuse the engine dummy external-connection registry behind the current target-neutral session façade, publish it through bounded actor polling, invalidate structural revisions on topology changes, and distinguish unavailable data from a confirmed empty candidate set.
- [x] Provide immediate rejection plus confirmed-snapshot/deferred-failure convergence for connect/disconnect requests instead of relying on log output or optimistic cache mutation.
- [x] Extend `FakeBackend` with deterministic local ports, external candidates, externally initiated state changes, endpoint churn, pending completion, and injected failures.
- [x] Extend the engine-backed dummy test fixture with mock external audio/MIDI input/output ports and connection inspection.
- [x] Add shared backend contract tests for discovery, compatibility, idempotent desired-state requests, connect/disconnect, missing endpoints, endpoint removal, and refresh convergence.
- [x] Run focused existing `shoop_engine` connection, dummy-port, JACK application-backend, and no-real-time-allocation/lock tests after shared engine changes.
- [x] Update matrix implementation/evidence cells and the project status for the completed backend boundary.

Verification:

- `cargo test -p shoop_backend`
- Targeted `shoop_engine` dummy-port, application-backend, external-port, JACK, and real-time-safety tests.
- Shared backend contracts pass against `FakeBackend` and `EngineBackend::new_dummy`.
- A test proves discovery and mutation happen outside the audio callback and do not require Qt/frontend types.
- `RUSTFLAGS="-D warnings" cargo build --workspace --all-targets`.

Commit the backend connection boundary before actor and GUI integration.

### Stage 3 — Publish authoritative connection state through the application actor

Depends on Stage 2.

- [x] Assign stable application `PortId` values transactionally when sync/direct track topology is created and map them to backend port IDs.
- [x] Store typed port ownership, names, direction, data type, and role in the authoritative track model; include sync-track ports in the same inventory rules.
- [x] Poll or subscribe to backend connection state at a bounded display cadence and publish immutable revisioned connection snapshots with deterministic ordering and structural sharing.
- [x] Build track-scoped and all-track views by stable ownership metadata, not display positions or names.
- [x] Handle desired-state intents in actor order, validate local ownership and current eligibility, suppress safe duplicates, and track bounded pending operations until confirmation or failure.
- [x] Reject stale IDs, removed candidates, incompatible direction/type pairs, and malformed/empty endpoint names without invoking an unrelated backend handle.
- [x] Publish visible notifications/status for saturation, rejection, timeout, and backend failure while retaining the last confirmed state.
- [x] Handle candidate appearance/disappearance and externally changed connections without requiring the dialog to reconstruct business truth.
- [x] Add actor tests for sync/main ownership, per-track/global filtering, stable IDs across refreshes, pending-to-confirmed transitions, external churn, stale late results, and failure recovery.
- [x] Verify track creation remains transactional if port descriptor publication fails and that M1 topology/control workflows are unchanged.
- [x] Update the matrix and project status with actor evidence.

Verification:

- `cargo test -p shoop_app`
- Fake-backend actor tests cover every required state transition and error path deterministically.
- Engine-backed dummy workflow creates sync, audio-only, MIDI-only, and audio/MIDI tracks, discovers compatible mock endpoints, connects/disconnects each applicable port, and observes snapshots.
- Existing M1 application and native workflow tests pass unchanged or with additive assertions.

Commit authoritative port and connection state before enabling the menus.

### Stage 4 — Build the reusable egui dialog and enable both menu entry points

Depends on Stage 1 for preview work and Stages 2–3 for real integration.

- [x] Replace the inert per-track options button with an egui menu containing **Connections...** for both sync and main tracks; keep deferred track actions absent or visibly unavailable.
- [x] Replace the inert global/main menu button with a menu containing **Connections**; keep unrelated future menu actions absent or inert.
- [x] Store one reusable dialog's open state and `AllTracks`/`Track(TrackId)` scope in `AppWidget`; safely close or show an unavailable state if a scoped track disappears.
- [x] Implement a resizable, independently closable `egui::Window` with a scope-appropriate title and remembered presentation state.
- [x] Render only non-empty role tabs in QML order and preserve a valid selected tab as categories appear or disappear.
- [x] Render deterministic local-port headers, grouped external client/port labels, connected and unavailable indicators, pending/error feedback, and click targets for eligible cells.
- [x] Add horizontal and vertical scrolling without losing row/column correspondence at minimum and common window sizes; use pinned labels/headers if needed for usability based on observed evidence.
- [x] Emit desired-state intents keyed by stable IDs/names and prevent ineligible or already-pending cells from emitting duplicate mutations.
- [x] Add loading, no local ports, no compatible external ports, backend unavailable, and stale-scope states.
- [x] Extend the backend-free preview with direct/sync tracks and representative input/output/send/return fixtures, multi-client audio/MIDI endpoints, all required cell states, endpoint churn controls, and intent logging/application.
- [x] Add egui interaction and geometry tests for both menu entry points, scope isolation, category omission/order, scrolling, stable cell routing, connected/disconnected actions, disabled cells, pending suppression, closing/reopening, and minimum/common-size painting.
- [x] Keep the browser preview functional and verify that the dialog adds no native-only dependency to `shoop_egui`.
- [x] Update matrix evidence and mark the egui presentation usable in the project document.

Verification:

- `cargo test -p shoop_egui`
- `cargo test -p shoop_egui_preview`
- `cargo check -p shoop_egui_preview --target wasm32-unknown-unknown`
- Native preview interaction at minimum and common sizes with enough ports to require both scroll axes.
- `trunk build --release` plus `python3 build_single_file_preview.py dist`; both regular and self-contained browser previews open the dialog without external app-resource requests.
- Dependency inspection confirms preview isolation.

Commit the preview/dialog and menu entry points in meaningful, independently testable steps.

### Stage 5 — Complete backend integration and parity evidence

Depends on Stages 2–4.

- [x] Route dialog intents through the native application's bounded actor channel and verify confirmed/pending/error snapshots repaint the open dialog.
- [x] Run end-to-end fake and engine-backed dummy scenarios from both track and global scopes, including sync ports and multiple tracks with overlapping external candidates.
- [x] Verify application-owned compatible ports can appear as candidates and remain distinct by full name.
- [x] Verify external connection changes and endpoint additions/removals become visible while the dialog remains open without user interaction.
- [x] Verify stale snapshots, rapid repeated clicks, scope changes, closing during a pending operation, and command/back-end failures cannot affect the wrong port pair.
- [x] Compare category visibility, labels/grouping, compatibility, and interaction behavior against the QML dialog; record justified visual adaptations and unresolved environment skips.
- [x] Run retained QML JACK/CPAL connection tests, engine JACK application-backend checks, and any other supported real-driver checks available in the environment.
- [x] Add replacement evidence to every connections-required matrix row; no required row remains unexplored, partial without rationale, or without verification evidence.
- [x] Update `EGUI_REPLACEMENT_PROJECT.md` with achieved status and explicitly remaining persistence, autoconnect, driver-settings, dry/wet, and non-track-port work.

Verification:

- Focused API/backend/application/GUI suites pass together.
- A scripted native dummy-backend workflow exercises both scopes and audio/MIDI connect/disconnect while observing confirmed snapshots and visible injected failure.
- Manual native and browser comparison confirms recognizable parity and usable large matrices.
- Matrix and project-document review is complete before the final gate.

Commit the integrated milestone before final validation.

### Stage 6 — End-to-end validation

Depends on all prior stages.

- [x] Run `cargo fmt --all -- --check`.
- [x] Run `RUSTFLAGS="-D warnings" cargo build --workspace --all-targets`.
- [x] Run `cargo test --workspace --features shoop_engine/app_backend -- --test-threads=1` with documented missing-backend allowances only where required by the environment.
- [x] Build and run `target/debug/shoopdaloop_dev.sh --self-test`; confirm retained QML connection and full-application suites have no regressions.
- [x] Run `cargo check -p shoop_egui_preview --target wasm32-unknown-unknown`.
- [x] Build the release Trunk bundle and self-contained `preview.html`, then load both in a browser and exercise both dialog scopes.
- [x] Inspect `shoop_egui`, preview, and native dependency trees for forbidden dependency direction or Qt/frontend leakage.
- [x] Launch the native application and preview under a graphical smoke environment at minimum and common sizes; exercise a large connection matrix and both menu entry points.
- [x] Exercise each supported real backend available in the development environment; document unavailable-device/backend skips rather than weakening acceptance criteria.
- [x] Confirm every connections matrix row has accurate discovery, implementation, target, and evidence status.
- [x] Mark this plan complete and update `EGUI_REPLACEMENT_PROJECT.md` with the achieved coarse status and next remaining roadmap area.
- [x] Commit and push final validation/documentation changes.

Final evidence must summarize exact test counts/results, browser/native smoke results, dependency scans, real-backend coverage or environment skips, and the matrix rows completed or deferred.

## Final completion evidence

Completed implementation and audit evidence:

| Acceptance criterion | Concrete artifact and verification |
|---|---|
| 1. Presentation-only/browser-compatible GUI | `shoop_egui` depends on `shoop_app_api` only; GUI/preview dependency scans and three Wasm checks pass. |
| 2. Stable typed port metadata | API `PortId`, data type, direction, role, owner/name snapshots; fake/engine/Web Audio descriptor construction and API/actor tests. |
| 3. Authoritative actor and desired-state intent | `ApplicationModel` maps app/backend IDs and handles `SetPortConnected` in actor order; stale/duplicate tests pass. |
| 4. Per-track entry and isolation | Sync/main track menu interaction test plus `ConnectionScope::Track` filtering/paint tests. |
| 5. Global entry and aggregation | Main-menu interaction test plus `ConnectionScope::AllTracks`; native workflow includes sync and main ports. |
| 6. Ordered optional categories | `PortRole::ORDERED`, direct/sync input/output descriptors, and preview coverage of all seven roles. |
| 7. Deterministic usable matrix | Sorted stable columns/rows, first-colon client grouping, indicators, both-axis scrolling, 16×50 matrix and minimum/common-size tests. |
| 8. Direction/type eligibility | Shared backend contract proves opposite direction and audio/MIDI separation; preview/dialog union supplies unavailable cells; engine dummy exposes `shoop:*` candidates. |
| 9. Exact connect/disconnect convergence | GUI emits exact desired state; backend/app/native workflow verifies connect, confirmation, and disconnect without optimistic truth. |
| 10. Observable bounded failures | Typed stale/disappeared/incompatible/saturated/rejected/timeout errors; deferred failure and two-second timeout tests retain confirmed state. |
| 11. Bounded live monitoring | One 16 ms app poll publishes compact deterministic snapshots; fake control test verifies external mutation and endpoint appearance/removal. |
| 12. Scope/late-result safety and presentation state | Stable IDs, scope-specific scroll IDs, pending keys, and close/reopen/scope-switch/stale-track test. |
| 13. Backend-free representative preview | `shoop_egui_preview` fixtures and controls cover both scopes, all roles/states, churn, intent log, confirm/fail, native/Wasm/self-contained forms. |
| 14. Deterministic/regression gates | Focused fake/engine/native tests, 1,010-test workspace gate, 236-pass QML gate, warning-denying build, Wasm/Trunk/browser/dependency evidence below. |
| 15. Living project documents | `CONN-*` matrix rows and Milestone 4 project status/roadmap/deferrals are current. |

- Focused Rust suites pass: `shoop_app_api` 7, `shoop_backend` 7, `shoop_app` 13, `shoop_egui` 20, `shoop_egui_preview` 3, and `shoopdaloop_egui` 3 tests (53 total, 0 failed).
- `RUSTFLAGS="-D warnings" cargo build --workspace --all-targets` passes. The toolchain emits only its external gold-linker deprecation warning; no Rust warning is accepted.
- `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo test --workspace --features shoop_engine/app_backend -- --test-threads=1` passes 1,010 tests across 69 result suites with 0 failed. The first run without the allowance identified only the host's absent `/dev/snd/seq` virtual-MIDI facility; the allowance is therefore evidence-scoped rather than masking a product failure.
- Retained `target/debug/shoopdaloop_dev.sh --self-test` passes 236 tests, fails 0, and skips one CPAL virtual-playback test because no usable CPAL device exists. `JackPorts::test_available_ports` passes, as do all six `jack_app_backend` Rust tests.
- `cargo fmt --all -- --check` and Wasm compiler checks for `shoop_egui`, `shoop_egui_preview`, and `shoopdaloop_egui` pass.
- Release Trunk builds succeed for the authoritative browser application and backend-free connection preview. Their self-contained builders produce `shoopdaloop_egui.html` and `preview.html` respectively.
- Chrome 147 browser smoke passes the preview's all-track and track scopes at exact 360×200 and 900×600 device viewports, and passes both scopes from the directly opened self-contained `file:` artifact at 900×600 without console exceptions.
- Native construction/workflow and egui paint tests cover 360×200 and 900×600, including a 16-column × 50-row connection matrix. OS-window Xvfb smoke was attempted but this shell lacks the dynamically loaded `libXcursor.so`; this is an environment skip, not replacement evidence. Browser graphical smoke and native egui paint/workflow evidence cover the presentation behavior.
- Dependency scans show `shoop_egui` contains no application/backend/engine/frontend/Qt crate and `shoop_egui_preview` contains no application/backend/engine/frontend/Qt crate. Discovery/mutation runs on the app owner/control path, while existing real-time lock/no-allocation suites pass in the full workspace gate.
- Real-backend scope on this host consists of the retained JACK test backend, which passes audio/MIDI connection and topology tests. Physical audio hardware, CPAL virtual playback, and virtual Midir are unavailable; hosted Web Audio continues to use browser-default routing and explicitly publishes arbitrary external connection management as unavailable.
- `EGUI_FEATURE_PARITY_MATRIX.md` records `CONN-ARCH-001` through `CONN-E2E-001` complete and records persistence/autoconnect, native driver settings/composition, dry/wet topology, and non-track ports as `CONN-DEF-001` through `CONN-DEF-004`. `EGUI_REPLACEMENT_PROJECT.md` records connections as the completed fourth major milestone and persistence as the next roadmap area.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
