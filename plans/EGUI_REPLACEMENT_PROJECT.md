# Pure egui Application Replacement Project

## Document role and maintenance

This is the durable project-level design document for replacing the Qt/QML application with a pure Rust egui application. It records the intended end state, architectural boundaries, migration strategy, and coarse-grained progress. Detailed implementation work belongs in milestone plans rather than here.

Keep this document current throughout the migration:

- Update the coarse status table when a milestone starts, materially changes direction, becomes usable, or completes.
- Record architectural decisions here when they affect more than one milestone.
- Keep the end goal and deletion criteria synchronized with accepted project decisions.
- Update this document in the same change that makes a project-level status or architectural decision obsolete.
- Do not turn an unexamined part of the old application into an implicit non-requirement.

Feature discovery and implementation status are maintained in `EGUI_FEATURE_PARITY_MATRIX.md`. That matrix is a living inventory, not a one-time up-front specification. Each milestone must discover and document the old behavior needed for its scope, extend or split matrix entries as evidence is found, and record implementation and verification evidence. This document must track, at a coarse level, how much of the matrix has been explored and how much has been built.

Milestone plans must reference both this document and the parity matrix, and must include their maintenance as implementation work rather than treating documentation as a final cleanup task.

## Current coarse status

| Area | Status | Notes |
|---|---|---|
| Project architecture | Usable | The cross-target production composition selects persisted native JACK, CPAL+midir, or dummy/offline through the threaded application backend, while hosted builds retain direct Web Audio/AudioWorklet ownership. `shoop_session`, feature-isolated `shoop_settings`, transactional driver/session replacement, the native Carla subprocess baseline, and frontend-independent `shoop_scripting` are real boundaries. Lua states remain application-owner-local, the connection preview remains backend-free, and the browser dependency graph excludes native audio/MIDI packages. |
| Feature-parity discovery | Partially explored | Tracks/loops, cross-target engine/browser audio, native runtime driver/device settings, connections, session/media persistence, application settings, the native Carla subprocess baseline, native Lua/script-created MIDI control, and click-track generated media are explored and built. Runnable egui FX/Carla settings, generic MIDI rule editing, and advanced editing remain deferred or largely unexplored. |
| First major milestone | Complete | The pure-egui tracks/loops vertical slice met all acceptance criteria at its completion boundary. |
| Second major milestone | Complete | `EGUI_MILESTONE_2_ENGINE.md` consolidates the runners in `shoopdaloop_egui`; native and browser targets run the authoritative app/backend/dummy-engine path with cross-target tests and browser artifacts. |
| Third major milestone | Complete | `EGUI_MILESTONE_3_BROWSER_AUDIO.md` delivers direct `web-sys`/AudioWorklet microphone/output in hosted secure runs with bounded protocol/storage, explicit permission, lifecycle recovery, offline dummy selection, and native regression evidence. |
| Fourth major milestone | Complete | `EGUI_MILESTONE_X_CONNECTIONS_DIALOG.md` delivers typed track-port inventory, authoritative connection state/mutation, sync/main and global scopes, the tabbed matrix, dynamic/error behavior, and backend-free preview evidence. |
| Fifth major milestone | Complete | The session/media implementation delivers playback-safe `.shoop` save/load, exact arbitrary-channel loop audio/MIDI I/O, deterministic resampling, transactional native/worklet replacement, and native/browser file services under `docs/session_format_v1.md`. All PR #676 platform/browser and retained Linux Rust/realtime/QML gates pass. |
| Lua scripting and MIDI-controller milestone | Complete | The historical artifact audit maps all native acceptance criteria. It includes exhaustive retained-API/constants evidence and 45/45 retained QML cases, actor-owned lifecycle, keyboard/APC serial+parallel workflows, strict native MIDI pacing and per-rule diagnostics, exact startup/settings/session behavior, egui presentation, packages, and realtime/workspace gates. Its former browser rejection/isolation boundary is superseded by the completed cross-target ports/browser-Lua milestone. Generic MIDI-rule editing remains separate. |
| Loop-control refinement | Complete | `EGUI_LOOP_HOVER_CONTROLS_AND_EMPTY_TRACKS_PLAN.md` delivers legible edge indicators, QML-style foreground hover families, play-dry/re-record/grab behavior, stereo loop balance, and first-track onboarding across native and browser boundaries. |
| Persistent application settings | Complete | The fresh versioned egui document, typed explicit registration including ordered string/toggle lists, native atomic config storage, browser `localStorage`, and one category-tabbed Settings dialog persist Add Track defaults and native Lua startup configuration without importing QML settings. |
| Click-track generated media | Complete | `EGUI_CLICK_TRACK_GENERATION_PLAN.md` completes embedded checked audio/MIDI generation, the stable-ID egui dialog, transactional native/worklet replacement, bounded native/browser preview, exact export/save-load, debug/release artifacts, Chrome/Firefox/self-contained workflows, and the full regression/audit gates. |
| egui presentation | Usable | `shoop_egui` renders plain snapshots and emits typed intents while remaining independently browser-compatible and backend/filesystem-free; loop popups retain stable-ID state, the connection dialog supports typed roles, and the one tabbed Settings dialog contains the full native and bundled-only browser Scripts surfaces while scripting/key presentation uses only plain application data. |
| Framework-independent application API | Complete | `shoop_app_api` owns stable application/host IDs, explicit track/Lua ownership, normalized immutable port/link/pending/error views, typed host-link intents, task/warning/mapping/script/MIDI diagnostics, capabilities, and notifications without framework/backend dependencies. |
| Rust business-logic application core | Usable | `shoop_app` runs tracks/loops, connections, persistence, explicit media mapping, transactional model publication, script/session lifecycle, committed callbacks, and shared GUI/Lua/MIDI reducers through a native actor or bounded cooperative browser runtime. Native session compression is worker-owned; browser audio continues independently in the worklet. |
| Backend façade | Usable | Fake, direct-core dummy/Web Audio, native threaded JACK/CPAL+midir/dummy, and browser-worklet backends expose normalized application/host inventories and confirmed links plus coherent arbitrary-channel audio/MIDI session capture, exact-route transactional replacement/migration, bounded generation-tagged transfer, and stable ID remapping. Native switches retain rollback data and browser Chrome/Firefox evidence confirms mutable authoritative worklet routes. |
| Unified egui executable | Usable | `shoopdaloop_egui` starts the persisted native JACK/CPAL+midir/dummy configuration with diagnostic fallback, or hosted Web Audio, and supplies target file/settings adapters. Runtime native switches are confirmation-gated, transactionally preserve/resample session contents, and publish full success only after durable preference save. Both targets retain bundled keyboard/APC behavior; browser omits native audio settings, user paths/Add-file, and native MIDI. |
| Cross-target egui CI | Usable | One eight-cell workflow builds, packages, uploads, then tests Linux x86_64, Windows x86_64, macOS arm64, and production WebAssembly in debug/release. PR #676 passes every cell, including hosted/direct-file browser persistence workflows. Coverage remains intentionally absent. |
| Browser egui application | Usable | Hosted/direct-file artifacts run negotiated audio host ports, authoritative mutable AudioWorklet routes, omniLua scripts/settings, and explicit Web MIDI. Supported secure browsers publish physical MIDI endpoints for user-managed direct-track recording/playback and owner-managed Lua control through one bounded hub; unsupported/denied states remain usable. Chrome hosted/direct-file Web MIDI and Firefox audio/no-permission regressions pass; direct-file physical APIs remain browser-policy-dependent. |
| Wasm Web MIDI recording and control | Complete | Direct `web-sys` access, canonical endpoint inventory, protocol-v4 track transport, bounded queues/diagnostics, desired-route persistence, unchanged APC control, and debug/release hosted/self-contained Chrome record/save/load/ordered-playback/hotplug/restart evidence pass. Workspace, 236/236 QML, Firefox no-host, package/import, and dependency closure is recorded in `EGUI_WASM_WEBMIDI_PLAN.md`. |
| Cross-target ports, browser Lua, and omniLua migration | Complete | Stages 0–6 deliver pinned omniLua, normalized protocol-v3 routes, owner-managed Lua MIDI ports, browser bundled settings, real keyboard control, active/exact source-bearing sessions, empty browser MIDI, debug/release hosted/standalone packages, and final workspace/realtime/QML/dependency/package/Chrome/Firefox audits. All Lua-specific retained cases pass; the later native-driver work also fixed CPAL test virtual-port registration, and the current full QML suite passes 236/236 with no skips. |
| Qt-hosted egui experiment | Complete | The embedded canvas/window experiment, QML state adapters, launch controls, frontend bridge code, and bridge dependencies are removed. The QML and standalone egui products now build and test independently. |
| Full Qt/frontend removal | Not started | Removing the legacy QML product remains a final migration result, not part of the first four milestones or the completed integration cleanup. |

Milestone 3 completion adds `shoop_audio_protocol`, the dedicated `shoop_audio_worklet` artifact, a direct browser controller in the composition root, bounded physical recording storage, target-specific documentation and CI, and plain API diagnostics without adding backend dependencies to `shoop_egui`. Chrome and Firefox deterministic fake-media workflows prove non-zero record/waveform/playback/output; Chrome additionally covers denial/retry, repeated-start prevention, suspension, media-track loss, processor loss, bounded queue saturation/recovery, zero-owned-track teardown, minimum/common viewports, and sustained recording. Native warning-free build, full workspace, JACK test-backend, real-time guard, and retained QML gates pass. Physical hardware was unavailable on the validation host and Safari remains explicitly untested; neither is claimed as evidence.

The historical connections milestone added application/backend descriptors, desired-state actor commands, bounded pending/error handling, live endpoint churn, menu scopes, and a reusable egui matrix. The completed cross-target follow-up replaced per-port candidates with normalized application ports, one host inventory, confirmed/pending links, typed `HostPortId` mutations, and explicit owner metadata. `shoop_egui_preview` remains fixture-only. Native integration now adapts the existing threaded application-backend JACK, CPAL+midir, and dummy ports into the normalized inventory and supports transactional runtime replacement; hosted Web Audio retains production-proven authoritative routing.

The completed cross-target ports, browser Lua, and omniLua milestone superseded those two browser limitations without adding Web MIDI or native egui real-driver selection at that historical boundary. The subsequent Web MIDI follow-up now adds browser physical MIDI without changing that historical scope. Normalized ownership/inventory, mutable authoritative worklet audio routes, Lua-created logical MIDI publication, browser settings, hosted/standalone workflows, and final package scans pass. One pinned omniLua Lua 5.4 runtime serves `shoop_scripting`, production Wasm, and retained frontend/QML, and the former C Lua dependency chain is removed. The frozen contract and closure evidence are in `EGUI_WEB_PORTS_AND_WASM_LUA_PLAN.md`.

The subsequent native audio-driver milestone in `EGUI_NATIVE_AUDIO_DRIVER_SWITCHING_PLAN.md` composes the existing non-Qt application backend behind `shoop_backend`, adds runtime discovery and per-driver settings for JACK, CPAL+midir, and dummy/offline, and switches through generation-scoped confirmation, exact-rate session resampling, rollback, and durable next-start persistence. Browser Web Audio composition and dependency isolation remain unchanged.

Milestone 5 establishes the real `shoop_session` boundary and fresh `.shoop` v1 contract in `docs/session_format_v1.md`. Exact per-channel floating-point payloads, integer-frame MIDI, independent versions/hashes, deterministic resampling, transactional backend/worklet replacement, target file adapters, and application-owned task state now serve native and browser composition. Hosted Firefox normal/stress and self-contained direct-file automation round-trip real produced session/audio/MIDI bytes while worklet callbacks continue. QML-era `.shl`, `session.1`, tar archives, and JSON `.smf` remain intentional unsupported-format inputs; QML is only a behavior-discovery and regression source.

The persistent-settings slice establishes a fresh `shoop-egui-settings` JSON contract rather than importing QML `settings.1`. `shoop_settings` separates its target-neutral typed registry/codec from opt-in native store and legacy features and supports deterministic ordered string/toggle collections. Consumers explicitly register definitions during composition; `shoopdaloop_egui` loads them before runtime/widget construction, owns storage, and publishes immutable revisions only after save. `shoop_egui` renders categories as tabs in one Settings window. Add Track preferences and bundled keyboard/APC toggles are cross-target; native additionally registers user paths, while browser shows the Scripts tab without file-path actions. Driver/device, Carla/FX, generic MIDI-rule, and session-local settings remain with their owning milestones.

The loop-control refinement keeps temporary controls in presentation state while moving their policies through typed intents and the application owner. Foreground overlays reproduce the QML source/child hover lifetime without changing track geometry; engine and worklet contracts carry balance and atomic grab requests, with ten-second bounded always-on input capture. Fresh production state remains one sync track/loop, and the empty main pane now points to Add Track. Dry/wet track construction and FX routing remain assigned to their later topology milestone.

The completed native Carla subprocess work establishes reusable frontend-independent semantics for global direct/subprocess policy, one supervised generation per chain, fixed shared-memory audio/MIDI transfer, bounded wet fallback, parent-owned checkpoints, click recovery, generation logs, crash notification, and lifecycle status. Native release evidence covers Windows, Linux, macOS Intel, and macOS ARM. The future egui FX milestone must compose these existing processor/control/realtime boundaries and snapshots through `shoop_backend`/`shoop_app` and register Carla-specific preferences through the delivered settings service; it must not duplicate the worker protocol, serialize hosting mode into sessions, or move supervision into presentation code.

The abandoned path that rendered egui inside the QML application has been deleted. The legacy product continues with its ordinary QML loop presentation and Rust/CXX-Qt frontend, while `shoopdaloop_egui` and `shoop_egui_preview` remain standalone and have no QML/frontend dependency. Formatting, warning-denying all-target builds, focused suites, standalone Wasm checks, the current 1,100-test native Rust archive, and 236-case QML matrix pass on the recorded native release surfaces. Release-browser and cross-platform gates are accepted as passing under the user's explicit documentation-closure instruction. This cleanup does not advance the whole-application parity roadmap or the final Qt deletion gate.

The cross-target CI restructuring is tracked by `EGUI_CI_AND_BUILD_FLAVORS_PLAN.md`. The authoritative application now has explicit debug/release artifact contracts for three native platforms and WebAssembly, profile-matched UI/worklet builds, production-only web deliverables, post-upload tests, and Rust caching. `nektos/act` is the initial Linux/web workflow-development surface; hosted runners remain authoritative for platform, cache, upload, and browser evidence. No coverage build is included yet.

Use the status terms `Not started`, `Partially explored`, `Planned`, `In progress`, `Usable`, `Complete`, and `Blocked` consistently. Notes should identify the active milestone or the evidence needed for the next status change.

## End goal

ShoopDaLoop is a pure Rust desktop application whose primary GUI is egui. The final application:

- Does not contain or depend on the `frontend` crate, QML code, Qt, CXX-Qt, Qt-to-egui bridge crates, Qt helper crates, or Qt packaging.
- Preserves the user-facing capabilities of the old application unless a difference is explicitly reviewed and accepted in the feature-parity matrix.
- Has clear crate boundaries between presentation, business logic, backend control, real-time processing, persistence, and scripting.
- Uses a typed, authoritative application model rather than a GUI object tree as session state.
- Routes GUI, Lua, MIDI-control, and other control sources through the same application command path.
- Keeps the audio callback independent from GUI locks, GUI timing, filesystem operations, and unbounded work.
- Supports fast, backend-free GUI compilation and testing.
- Preserves compatible session data and scripting behavior, or provides explicit migrations for accepted format/API changes.

Pixel-identical rendering is not required. Behavioral parity, recognizable layout, live-performance usability, and explicit review of intentional differences are required.

## Target architecture

```text
shoopdaloop_egui (native + browser)
├── shoop_egui ──────────────> shoop_app_api + shoop_settings core
├── shoop_settings ──────────> native config store / browser adapter
└── shoop_app ───────────────> shoop_app_api
    ├── shoop_backend ───────> shoop_engine
    ├── shoop_session
    └── shoop_scripting

shoop_egui_preview (connection fixtures, native + browser)
└── shoop_egui ──────────────> shoop_app_api + shoop_settings core
```

Milestone 2 uses only the engine-backed dummy driver in this composition root. Its façade owns the target-neutral engine `Session` directly: the native application actor and browser event loop drive the same bounded elapsed-time processing, while topology and content operations complete synchronously at stable control points.

Milestone 3 preserves the native side of that arrangement and automatically selects a direct `web-sys` browser driver in hosted secure browser runs. A dedicated raw Wasm AudioWorklet module owns the browser engine and physical audio clock; the application side communicates through bounded asynchronous commands, snapshots, and revisioned waveform chunks rather than touching or locking the worklet session. `BROWSER_AUDIO_CONTRACT.md` records the versioned limits and the control-task design revision, and `EGUI_MILESTONE_3_COMPLETION_AUDIT.md` maps the completed criteria and gates to evidence. The retained Qt application continues exercising the full native application backend and existing native drivers during the migration.

Milestone 5 adds versioned one-generation session capture and staged replacement to that boundary. Native compression and path I/O run on workers; browser session DTOs move through generation-tagged 2 KiB chunks while Web Audio callbacks continue independently. The application owns validation, warnings, mapping, task state, stable-ID remapping, and publication-after-commit. `shoop_session` owns target-neutral documents, archives, exact/standard media, and deterministic sample-domain conversion; composition-root adapters own platform file objects.

These names describe intended responsibilities. A milestone may establish a boundary before all implementation has physically moved into its final crate, but dependency direction must not be compromised.

### `shoop_app_api`

A small, framework-independent contract crate containing:

- Stable track, loop, port, channel, and task identifiers.
- Immutable application read models and capability flags.
- Typed user intents and application notifications.
- Shared value types required by both the application and presentation layers.

It must not depend on egui, Qt, an audio driver, the engine implementation, Lua, native windowing, or filesystem services. If an engine enum is needed by both layers, move the value type to a small shared crate or define an application-level type and convert at the backend boundary; do not pull `shoop_engine` into the GUI dependency graph.

### `shoop_app`

The authoritative business-logic layer. It owns:

- Session topology and persistent application state.
- Tracks, loops, ports, selection, targeting, and global control policy.
- Interpretation and validation of user intents.
- Sync, solo, recording, grabbing, and composite-loop policies.
- Coordination of backend mutations and asynchronous services.
- Snapshot publication and user-visible errors/task progress.
- The common command/query surface used by GUI, scripting, MIDI control, and tests.

A single logical application owner holds mutable business state and processes commands in order. Native composition may place it on an actor thread; browser composition may drive the same state machine cooperatively. Long-running media or filesystem work runs on appropriate asynchronous services and reports typed results back through the same command path.

### `shoop_backend`

The non-Qt control boundary around `shoop_engine`. It owns:

- Driver startup, fallback, shutdown, and status.
- Engine session and object-handle lifetimes.
- Low-level loop, channel, port, connection, and FX operations.
- State-mirror polling and backend data requests.
- Translation between application/backend commands and engine operations.

It must not own selection, dialogs, track menus, solo policy, or other application semantics. The existing application-backend implementation can be wrapped and incrementally moved, but no QObject compatibility layer is part of the target.

### `shoop_session`

Typed persistence and media services, including:

- Session document types, schema validation, and migration.
- Fresh `.shoop` archive loading and saving with explicit future migration boundaries.
- Audio/MIDI import and export.
- Sample-rate conversion and generated content such as click tracks.

Persistence serializes the authoritative application model. It must not reconstruct session truth by traversing GUI widgets.

### `shoop_settings`

A Qt- and egui-independent application-preference boundary containing stable typed keys, explicit definition registration, registry metadata, versioned DTO decoding/encoding, ordered migration dispatch, immutable snapshots, drafts, validation, and diagnostics. Native config-directory/atomic-file support and retained QML compatibility helpers are opt-in features so presentation-only builds do not acquire filesystem dependencies.

The composition root aggregates registrations, loads settings before constructing consumers, owns platform storage and save scheduling, and publishes a new revision only after persistence succeeds. Browser adapters use origin-scoped `localStorage`. Settings UI actions remain separate from session/business `AppIntent`; machine preferences are not `.shoop` state.

### `shoop_scripting`

The Lua runtime, MIDI-control integration, timers, and subscriptions. It communicates through application intents, snapshots, and events rather than QObjects or widget references. Existing built-in scripts and the old public Lua surface are compatibility evidence.

### `shoop_egui`

A presentation-only crate. Widgets receive immutable plain Rust state, retain local presentation state, and return typed user intent. Presentation state includes temporary text edits, scroll positions, pane expansion, dialog drafts, and waveform zoom. Session topology, selection truth, and backend handles do not belong here.

The crate remains host-independent and browser-compatible where practical. It does not create native windows or use native dialogs.

### `shoopdaloop_egui`

The target-neutral egui composition root owns eframe startup and shutdown, application/backend wiring, snapshot delivery, and target-specific runtime scheduling. Native builds own windowing and run the application owner on its actor thread. Hosted browser builds own permission-aware Web Audio startup and a worklet-backed engine; direct-file `?offline=1` builds explicitly select cooperative dummy progress. Target adapters share one eframe application and do not leak into `shoop_egui`. The completed cross-target ports/Lua milestone preserves this composition while adding cooperative application-owner scripting and explicit worklet-owned host-channel routing; browser Lua readiness does not depend on audio permission.

The native runner supports persisted and runtime-switchable JACK, CPAL+midir, and dummy/offline configurations through the native driver-management milestone. Hosted secure browser runs still select the Milestone 3 Web Audio driver automatically; offline artifacts retain only explicitly described dummy/unsupported behavior. The runner supersedes `shoopdaloop_native` and remains the only authoritative application composition. A later connection-focused `shoop_egui_preview` is fixture-only and does not replace or duplicate that runtime.

## Runtime model

```text
Native dummy actor                 Browser AudioWorklet
       │                          Engine + Session + audio I/O
       │                                   ⇅ bounded protocol
       └───────────────> shoop_backend / browser proxy
                                  │ observations and completions
                                  ▼
shoop_app logical owner ─────────────────> Arc<AppSnapshot>
(native actor / browser pump)                      │
                    ▲                              ▼
                    │ typed intents           shoop_egui
             GUI / Lua / MIDI / CLI
```

Required properties:

- The native real-time callback never waits for the GUI or application actor.
- Explicit browser offline dummy execution advances through bounded non-blocking ticks and does not pretend to be a real-time callback.
- The AudioWorklet exclusively owns hosted browser engine state and advances it only from Web Audio render quantums; browser UI time never drives physical-audio cycles.
- Browser application/worklet communication is asynchronous, bounded, and non-blocking in both directions.
- The GUI does not poll or mutate individual engine handles.
- Commands are bounded or apply explicit backpressure and are never silently lost.
- Structural snapshots use stable IDs and structural sharing rather than positional identity and full deep copies every frame.
- Live meters and positions may update at a bounded display cadence independently of structural session updates.
- Large waveform data is revisioned and cached in a representation whose draw cost is bounded by display resolution.

## Feature-parity management

`EGUI_FEATURE_PARITY_MATRIX.md` is the detailed migration ledger. Its maintenance rules are:

1. Record discovered behavior before or while designing the replacement behavior.
2. Cite the old implementation, tests, user documentation, or observed behavior that supports each baseline entry.
3. Distinguish discovery status from implementation status. “Not discovered” must never be interpreted as “not required.”
4. Split broad entries when implementation reveals independently testable behavior.
5. Mark intentional differences only with an explicit rationale and approval.
6. Attach replacement verification evidence before marking an entry complete.
7. At every milestone boundary, update:
   - entries explored by that milestone;
   - entries implemented, partial, deferred, or blocked;
   - tests or manual evidence supporting completed entries;
   - this document's coarse feature-discovery and implementation status.

The matrix is initially explored only for the first tracks/loops vertical slice. Whole-application parity is therefore neither fully known nor estimable yet. Future milestone planning begins with additional discovery in the area it intends to replace.

## Migration strategy

Use incremental vertical slices rather than translating QML file by file.

1. Establish the small application API and stable identities.
2. Create a pure native executable and application actor early.
3. Wrap the reusable engine-facing Rust API behind a non-Qt backend boundary.
4. Reimplement one coherent user workflow at a time, including behavior, presentation, and tests.
5. Keep the Qt application available as a behavior oracle and regression suite while replacement coverage is incomplete.
6. Move reusable non-Qt algorithms only after removing QObject and QVariant assumptions; do not reproduce the old QObject architecture in Rust.
7. Translate tests to the narrowest appropriate layer:
   - business rules in deterministic application tests;
   - engine integration against a dummy backend;
   - typed interaction and layout behavior in egui tests;
   - a small number of native end-to-end workflows.
8. Delete old code only after its matrix area is complete or an explicit difference has been accepted.

The first vertical slice is defined in `EGUI_MILESTONE_1_TRACKS_AND_LOOPS_PLAN.md`. The cross-target dummy-engine consolidation is defined in `EGUI_MILESTONE_2_ENGINE.md`. Direct browser microphone/output integration is defined in `EGUI_MILESTONE_3_BROWSER_AUDIO.md`. Track-port connection management is defined in `EGUI_MILESTONE_X_CONNECTIONS_DIALOG.md` and is the fourth completed major milestone. The completed post-M1 loop-control refinement is defined in `EGUI_LOOP_HOVER_CONTROLS_AND_EMPTY_TRACKS_PLAN.md`. Explicit cross-target application/host ports, mutable Web Audio routing, and browser Lua are completed in `EGUI_WEB_PORTS_AND_WASM_LUA_PLAN.md`. Generated audio/MIDI click-loop content and preview are completed in `EGUI_CLICK_TRACK_GENERATION_PLAN.md`.

## Fast GUI iteration

Fast recompilation is an architectural requirement:

- `shoop_egui` depends only on egui, assets, and small API/value crates.
- Backend, engine, driver, Lua, media-I/O, and native-shell crates stay out of the GUI dependency graph.
- `shoop_egui` tests and fixtures supply representative plain snapshots and capture emitted intents without linking backend, engine, driver, LV2, Lua, or Qt dependencies.
- The Milestone 2 `shoopdaloop_egui` runner deliberately links the real application/backend/dummy-engine path on both native and browser targets. The later connection milestone restores one narrowly fixture-driven executable for backend-free matrix/state iteration; it is not an alternative production composition root.
- Milestone 3 keeps Web Audio and worklet packaging out of `shoop_egui`; the dedicated worklet artifact links only its narrow protocol host and engine DSP dependencies.
- Native renderer and OS-integration dependencies remain target-specific in the composition root.
- GUI tests run with `cargo test -p shoop_egui`; presentation browser compatibility and the engine-backed browser application remain separate checks.
- Presentation-only iteration does not relink the engine unless the consolidated application itself is being built.

Use an in-process architecture initially. A separate backend process is justified only if measured build or runtime evidence outweighs the additional IPC and lifecycle complexity.

## Coarse roadmap

This roadmap gives ordering, not fixed future milestone scope:

1. Tracks/loops vertical slice in a pure egui application. Complete.
2. Consolidated native/browser egui application running the real application/backend path with cooperative dummy-engine processing in the browser. Complete.
3. Direct Web Audio/AudioWorklet microphone and output driver selected automatically in hosted browser runs. Complete.
4. Track-port connections dialog and authoritative discovery/mutation workflow. Complete.
4a. QML-style loop hover families, grab, stereo balance, and empty-tracks onboarding. Complete.
5. Session persistence and media workflows required to use the tracks/loops slice across runs. Complete with fresh `.shoop` v1, exact loop media, resampling, and native/browser adapters.
6. Persistent application-settings foundation, category-tabbed dialog, Add Track preferences, native Lua startup settings, and confirmation-gated native JACK/CPAL+midir/dummy driver/device management. Complete. Remaining track topology and other feature-specific settings stay in later milestones.
6a. Generated audio/MIDI click-loop content, loop-length fitting, and non-mutating native/browser preview. Completed under `EGUI_CLICK_TRACK_GENERATION_PLAN.md`.
7. Dry/wet tracks, FX chains, and advanced loop details/editing.
8. Composite-loop creation and editing.
9. Native Lua scripting, keyboard control, and script-created MIDI control. Complete as the historical native slice and extended cross-target by item 9a; generic MIDI rule editing remains separate.
9a. Project-wide `mlua`-to-omniLua migration, cross-target application/host port normalization, mutable Web Audio routes, visible empty-host MIDI/control ports, and browser Lua/bundled keyboard support. Complete under `EGUI_WEB_PORTS_AND_WASM_LUA_PLAN.md`.
9b. Explicit Web MIDI permission, direct-track MIDI recording/playback, and browser Lua controller I/O. In progress under `EGUI_WASM_WEBMIDI_PLAN.md`.
10. Whole-matrix validation, production entry-point switch, packaging migration, and Qt deletion.

Future discovery may reorder or split these areas. Any such change must update this document and the parity matrix.

## Final removal gate

Qt/frontend removal is complete only when:

- Every discovered matrix entry is complete, explicitly deferred to a tracked post-replacement item, or approved as an intentional difference.
- The production executable starts and operates without loading Qt.
- Session compatibility, relevant Lua behavior, and supported audio backends pass their replacement test suites.
- Native end-to-end tests cover critical live-looping workflows.
- The `frontend`, Qt helper crates, QML sources, Qt build steps, Qt tests, and Qt packaging are removed.
- Workspace dependency and source scans find no unintended Qt, QML, CXX-Qt, or Qt-to-egui bridge dependency.
- Build, test, installation, and developer documentation describe only the replacement architecture.
