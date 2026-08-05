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
| Project architecture | Planned | The accepted crate boundaries now include one cross-target composition root and native/cooperative application ownership; persistence, scripting, and production real-driver composition remain future architecture work. |
| Feature-parity discovery | Partially explored | The tracks/loops subset and cross-target dummy-engine portability surface are explored and built; settings, persistence, connections, FX, scripting, MIDI control, and advanced editing remain largely unexplored for replacement purposes. |
| First major milestone | Complete | The pure-egui tracks/loops vertical slice met all acceptance criteria at its completion boundary. |
| Second major milestone | Complete | `EGUI_MILESTONE_2_ENGINE.md` consolidates the runners in `shoopdaloop_egui`; native and browser targets run the authoritative app/backend/dummy-engine path with cross-target tests and browser artifacts. |
| egui presentation | Usable | `shoop_egui` renders plain snapshots and emits typed intents while remaining independently browser-compatible and backend-free. |
| Framework-independent application API | Complete | `shoop_app_api` owns stable IDs, snapshots, capability state, and typed milestone intents without framework/backend dependencies. |
| Rust business-logic application core | Usable | `shoop_app` runs the same tracks/loops model through a native actor or a bounded cooperative browser runtime. |
| Backend façade | Usable | `shoop_backend` owns a Wasm-safe dummy façade directly over the engine `Session` core, with deterministic elapsed-time processing, synchronous graph updates, and direct stable content reads. |
| Unified egui executable | Usable | `shoopdaloop_egui` is the only egui runner package and passes native construction/workflow/paint tests plus Wasm build and browser workflow smoke. |
| Browser egui application | Usable | Release and self-contained browser artifacts run the actual dummy engine; scripted Chrome tests pass at minimum and common sizes. |
| Qt/frontend removal | Not started | Removal is a final migration result, not part of the first two milestones. |

Use the status terms `Not started`, `Partially explored`, `Planned`, `In progress`, `Usable`, `Complete`, and `Blocked` consistently. Notes should identify the active milestone or the evidence needed for the next status change.

## End goal

ShoopDaLoop is a pure Rust desktop application whose primary GUI is egui. The final application:

- Does not contain or depend on the `frontend` crate, QML code, Qt, CXX-Qt, `egui-cxx-qt`, Qt helper crates, or Qt packaging.
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
├── shoop_egui ──────────────> shoop_app_api
└── shoop_app ───────────────> shoop_app_api
    ├── shoop_backend ───────> shoop_engine
    ├── shoop_session
    └── shoop_scripting
```

Milestone 2 uses only the engine-backed dummy driver in this composition root. Its façade owns the target-neutral engine `Session` directly: the native application actor and browser event loop drive the same bounded elapsed-time processing, while topology and content operations complete synchronously at stable control points. Native real-driver composition remains later project work; the retained Qt application continues to exercise the full threaded application backend and existing native drivers during the migration.

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
- `.shl` archive loading and saving.
- Audio/MIDI import and export.
- Sample-rate conversion and generated content such as click tracks.

Persistence serializes the authoritative application model. It must not reconstruct session truth by traversing GUI widgets.

### `shoop_scripting`

The Lua runtime, MIDI-control integration, timers, and subscriptions. It communicates through application intents, snapshots, and events rather than QObjects or widget references. Existing built-in scripts and the old public Lua surface are compatibility evidence.

### `shoop_egui`

A presentation-only crate. Widgets receive immutable plain Rust state, retain local presentation state, and return typed user intent. Presentation state includes temporary text edits, scroll positions, pane expansion, dialog drafts, and waveform zoom. Session topology, selection truth, and backend handles do not belong here.

The crate remains host-independent and browser-compatible where practical. It does not create native windows or use native dialogs.

### `shoopdaloop_egui`

The target-neutral egui composition root delivered by Milestone 2. It owns eframe startup and shutdown, application/backend wiring, snapshot delivery, and target-specific runtime scheduling. Native builds own windowing and run the application owner on its actor thread; browser builds own WebRunner startup and cooperative application/dummy-engine progress. Target adapters share one eframe application and do not leak into `shoop_egui`.

This runner supports only the dummy driver. It supersedes both `shoopdaloop_native` and the standalone backend-free `shoop_egui_preview`; the completed Milestone 1 plan remains historical evidence of the earlier arrangement.

## Runtime model

```text
Native driver callback / browser cooperative dummy tick
                    │ state mirrors
                    ▼
               shoop_backend
                    │ observations and command results
                    ▼
shoop_app logical owner ─────────> Arc<AppSnapshot>
(native actor / browser pump)             │
                    ▲                     ▼
                    │ typed intents  shoop_egui
             GUI / Lua / MIDI / CLI
```

Required properties:

- The native real-time callback never waits for the GUI or application actor.
- Browser dummy execution advances through bounded non-blocking ticks; it does not pretend to be a real-time audio callback.
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

The first vertical slice is defined in `EGUI_MILESTONE_1_TRACKS_AND_LOOPS_PLAN.md`. The cross-target dummy-engine consolidation is defined in `EGUI_MILESTONE_2_ENGINE.md`.

## Fast GUI iteration

Fast recompilation is an architectural requirement:

- `shoop_egui` depends only on egui, assets, and small API/value crates.
- Backend, engine, driver, Lua, media-I/O, and native-shell crates stay out of the GUI dependency graph.
- `shoop_egui` tests and fixtures supply representative plain snapshots and capture emitted intents without linking backend, engine, driver, LV2, Lua, or Qt dependencies.
- The Milestone 2 `shoopdaloop_egui` runner deliberately links the real application/backend/dummy-engine path on both native and browser targets; there is no separate fixture-driven executable after consolidation.
- Native renderer and OS-integration dependencies remain target-specific in the composition root.
- GUI tests run with `cargo test -p shoop_egui`; presentation browser compatibility and the engine-backed browser application remain separate checks.
- Presentation-only iteration does not relink the engine unless the consolidated application itself is being built.

Use an in-process architecture initially. A separate backend process is justified only if measured build or runtime evidence outweighs the additional IPC and lifecycle complexity.

## Coarse roadmap

This roadmap gives ordering, not fixed future milestone scope:

1. Tracks/loops vertical slice in a pure egui application. Complete.
2. Consolidated native/browser egui application running the real application/backend path with cooperative dummy-engine processing in the browser. Complete.
3. Session persistence and media workflows required to use the tracks/loops slice across runs.
4. Track topology, connections, settings, and real driver-management workflows.
5. Dry/wet tracks, FX chains, and advanced loop details/editing.
6. Composite-loop creation and editing.
7. Lua scripting, MIDI control, monitoring, profiling, and remaining utility/developer surfaces.
8. Whole-matrix validation, production entry-point switch, packaging migration, and Qt deletion.

Future discovery may reorder or split these areas. Any such change must update this document and the parity matrix.

## Final removal gate

Qt/frontend removal is complete only when:

- Every discovered matrix entry is complete, explicitly deferred to a tracked post-replacement item, or approved as an intentional difference.
- The production executable starts and operates without loading Qt.
- Session compatibility, relevant Lua behavior, and supported audio backends pass their replacement test suites.
- Native end-to-end tests cover critical live-looping workflows.
- The `frontend`, Qt helper crates, QML sources, Qt build steps, Qt tests, and Qt packaging are removed.
- Workspace dependency and source scans find no unintended Qt, QML, CXX-Qt, or `egui-cxx-qt` dependency.
- Build, test, installation, and developer documentation describe only the replacement architecture.
