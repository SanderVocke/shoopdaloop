# Redesign the connections dialog as a four-column connection graph

## Goals and scope

Replace the connection matrix in `src/rust/shoop_egui/src/connection_dialog.rs` with a readable, interactive graph ordered left-to-right as **System sources → ShoopDaLoop sinks → ShoopDaLoop sources → System sinks**. Users can create routes by dragging from a source to a compatible sink, see confirmed routes as lines, remove user-managed routes, and reduce the graph with independent Audio, MIDI, and track filters. Opening Connections from a track initializes the graph with only that track selected; opening it globally initializes an unfiltered graph.

### In scope

- Four-column grouped endpoint layout, connection-line rendering, drag-to-connect, and an explicit line-based disconnect interaction.
- Independent Audio/MIDI inclusion toggles and a track multi-selection filter.
- Grouping system endpoints by device/client/application and ShoopDaLoop endpoints by track or application owner.
- Presentation of confirmed, pending, failed, unavailable, and owner-managed connection states already exposed by `ConnectionViewState`.
- Egui interaction/layout tests, application open-path tests, and connection UI documentation.

### Out of scope

- Changing backend connection discovery, compatibility, command processing, session persistence, or routing semantics.
- Adding system-to-system or ShoopDaLoop-to-ShoopDaLoop routes; only the two currently supported adjacent lanes are connectable.
- Editing script-owned autoconnection policy from this dialog.
- Persisting dialog filters in application settings.

## Immutable acceptance criteria

1. The graph always orders visible endpoint columns as System sources, ShoopDaLoop sinks, ShoopDaLoop sources, and System sinks; endpoints are listed top-down in stable groups.
2. System ports are grouped by the device/client/application portion of their display name, with a stable fallback for unqualified names. ShoopDaLoop ports are grouped by their owning track or non-track application owner and use user-facing owner names where available.
3. Dragging a source connector onto a compatible, user-managed sink emits exactly one `AppIntent::SetPortConnected { connected: true, .. }`. Invalid, same-direction, wrong-data-type, pending, and owner-managed drops emit no mutation.
4. Every visible confirmed route is drawn between its source and sink. Pending connect/disconnect state, route errors, and owner-managed routes remain visually distinguishable and truthful to `ConnectionViewState`.
5. A confirmed user-managed route has a discoverable line interaction that emits exactly one matching disconnect intent; owner-managed and pending routes cannot be disconnected through the graph.
6. Audio and MIDI filters are independent inclusion toggles. Disabling either removes matching ShoopDaLoop endpoints, system endpoints, pending routes, and confirmed lines from the overview without changing connection state.
7. The track filter supports all tracks or one or more particular tracks. A particular-track selection removes other tracks' ports and non-track application-owner ports; system endpoints with no compatible visible ShoopDaLoop endpoint are also omitted.
8. Opening Connections globally selects all tracks with both data types visible. Opening it from any sync or main track button selects only that track with both data types visible. Filters can then be changed within the open dialog without mutating application state.
9. Loading, backend-unavailable, empty-host, no-filter-results, stale-track, pending, owner-managed, and error states remain usable and do not hide otherwise eligible ShoopDaLoop ports or emit invalid intents.
10. The graph remains usable in a small resizable window through clipped painting and two-axis overflow, and remains practical with large endpoint inventories; connector hit targets and labels retain hover/help text where truncation or interaction meaning is not obvious.
11. Existing native and browser connection behavior, normalized identities, exact desired-state intents, and session routing semantics remain unchanged, and all required project validation gates pass.

## Design rules and constraints

- Derive a frontend graph view from `AppState` and `ConnectionViewState`; keep confirmed backend truth separate from transient UI drag/filter state. Do not introduce a second routing model or cache desired connection truth in the widget.
- Use stable typed identities (`PortId`, `HostPortId`, owner IDs) for grouping, widget IDs, anchors, hit testing, and emitted intents; names are labels, not identities.
- Treat only adjacent source/sink pairs as candidates: host output → application input and application output → host input. Data types must match, and the application endpoint must be `UserManaged` and not pending before mutation.
- Keep application ports visible when the compatible host inventory is empty. After applying data-type and track filters, include only system endpoints compatible with at least one visible application endpoint in their lane.
- Use one shared scrollable graph coordinate space so endpoint anchors and connection curves remain aligned while scrolling. Clip lines, drag previews, and line hit regions to the graph viewport.
- Use consistent Audio/MIDI line styling plus separate confirmed, pending, hovered, owner-managed, and error treatments; do not rely on color alone to communicate mutable versus disabled state.
- Cancel transient drag/hover selection when the dialog closes, filters change, or a snapshot revision removes an endpoint. Never optimistically add or remove a confirmed line before authoritative state changes.
- Keep grouping and filtering logic in testable helper/view-model code, separate from egui painting and pointer handling. Avoid changes to `shoop_app_api` unless implementation evidence shows the existing owner/name metadata is insufficient.
- Do not open or control the desktop application or a browser for automated/manual GUI validation. Validate presentation and gestures through headless egui input/paint tests, application integration tests, and native/Wasm compilation; visual behavior beyond those surfaces is assumed.

## Staged implementation

### Stage 1 — Build the filtered, grouped graph view

- [x] Replace role-tab/matrix preparation with internal graph-view structures for the four endpoint columns, owner/client groups, visible routes, pending/error metadata, and stable source/sink identities.
- [x] Add independent Audio/MIDI filter state and an all-tracks-or-selected-track-set filter. Make `ConnectionScope::AllTracks` and `ConnectionScope::Track` deterministic open presets rather than permanent hard scopes.
- [x] Resolve track and script display names from `AppState`, preserve stable ordering, and prune only system endpoints that cannot connect to a currently visible application endpoint.
- [x] Add focused tests for four-column classification, direction/type compatibility, group naming/order, global and multi-track filters, Lua/non-track exclusion under track filters, stale tracks, empty inventories, and route visibility.
- [x] Verify with `cargo test -p shoop_egui connection_dialog` and commit the completed view-model/filter milestone.

### Stage 2 — Replace the matrix with the grouped four-column layout

- [x] Add a compact filter bar with independent Audio and MIDI toggles plus a track multi-select control with clear All tracks, single-track, and multi-track summaries.
- [x] Render fixed-order column headers and grouped endpoint rows with connector anchors on the lane-facing edge, owner/client headers, truncated-label hover text, and disabled/managed affordances.
- [x] Put all four columns in one two-axis scrollable graph viewport; provide useful lane-specific empty messages while retaining visible application ports when host inventories are empty.
- [x] Update open-path behavior in `AppWidget` as needed so global, sync-track, and main-track buttons apply the required presets every time they open the dialog.
- [x] Add layout and interaction-state tests at minimum and common window sizes, with mixed Audio/MIDI, multiple tracks, script ports, duplicate display names, and large inventories.
- [x] Verify with targeted headless `shoop_egui` paint/resize/scroll tests, then commit the layout/filter milestone.

### Stage 3 — Add connection curves and pointer interactions

- [x] Record connector anchors during layout and paint clipped source-to-sink curves for confirmed and pending routes, with Audio/MIDI and managed/pending/error styling defined by the design rules.
- [x] Implement source drag state, live drag preview, compatible target highlighting, cancellation, and drop resolution for both host-source → application-sink and application-source → host-sink lanes.
- [x] Implement nearest-visible-curve hover/hit testing and the discoverable disconnect interaction for confirmed user-managed routes; suppress mutation for owner-managed or pending routes and surface explanatory hover text.
- [x] Ensure each successful gesture emits one exact typed intent and waits for authoritative pending/confirmed snapshots instead of changing route truth locally.
- [x] Add pointer-sequence tests for both lane directions, connect, disconnect, incompatible drops, owner-managed routes, pending routes, overlapping/nearby curves, drag cancellation, filter changes during a drag, and scrolled/clipped content.
- [x] Verify with `cargo test -p shoop_egui connection_dialog` and relevant `AppWidget` tests, then commit the graph-interaction milestone.

### Stage 4 — Documentation and integration coverage

- [x] Update `docs/port_model.md` and `src/rust/shoopdaloop/README.md` to describe the graph, grouping, filters, drag-to-connect, disconnect interaction, owner-managed routes, and unchanged authoritative connection contract; remove matrix-specific wording.
- [x] Update or replace existing matrix-oriented tests without weakening exact-intent, browser MIDI, empty-host, owner-managed, and large-inventory coverage.
- [x] Exercise global and per-track opening through the application widget, including sync tracks and deletion of a selected track while the dialog is open.
- [x] Verify targeted UI/application tests and browser compilation, then commit the documentation/integration milestone.

### Stage 5 — Final end-to-end validation

- [x] Cover representative Audio and MIDI inventories in headless tests: grouped devices/apps, direct/sync/main ownership, owner-managed Lua MIDI ports, exact connect/disconnect intents, pending/error feedback, filtering, scrolling, resizing, stale scopes, and close/reopen presets. Do not open or control a desktop GUI or browser.
- [x] Run `cargo fmt --all -- --check`.
- [x] Run `RUSTFLAGS="-D warnings" cargo build --workspace`.
- [x] Run `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo test --workspace --features shoop_engine/app_backend -- --test-threads=1`.
- [x] Run `python3 scripts/check_tracing_coverage.py --require-closed`.
- [x] Build `shoopdaloop` and `shoop_audio_worklet` for `wasm32-unknown-unknown`; do not run browser GUI automation.
- [x] Confirm the final diff contains no backend/session routing changes unless separately justified by implementation evidence, and commit the validated feature.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
