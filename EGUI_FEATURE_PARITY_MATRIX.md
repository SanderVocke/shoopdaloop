# egui Replacement Feature-Parity Matrix

## Purpose

This is the living feature-discovery and implementation ledger for the pure egui replacement described in `EGUI_REPLACEMENT_PROJECT.md`. It is intentionally incomplete. Entries are discovered and refined as milestone work reaches each part of the old application.

The initial entries cover the first tracks/loops vertical slice defined in `EGUI_MILESTONE_1_TRACKS_AND_LOOPS_PLAN.md`. Areas outside that slice are listed only coarsely and remain to be explored.

## Maintenance rules

- Update discovery, implementation, and evidence as part of the stage that changes them.
- Add or split entries whenever an independently testable behavior is discovered.
- Cite QML, Rust frontend code, existing tests, user documentation, or recorded observation as baseline evidence.
- `Unexplored` and `Partially explored` do not mean optional.
- Do not mark an entry `Complete` without replacement verification evidence.
- Record intentional differences only after explicit approval.
- At milestone completion, summarize the changed discovery/build extent in `EGUI_REPLACEMENT_PROJECT.md`.

## Status vocabulary

Discovery:

- `Explored for M1`: investigated enough to define the first milestone behavior.
- `Explored for M2`: investigated enough to define the cross-target dummy-engine milestone behavior.
- `Partially explored`: some relevant behavior is known, but later work must continue discovery.
- `Unexplored`: not yet inventoried for replacement.

Implementation:

- `Existing widget`: presentation exists in `shoop_egui`, but is not connected to the new application architecture.
- `Prototype through Qt`: works only through the current Qt/QML host or adapters.
- `Not started`: no replacement implementation.
- `Partial`: replacement exists but does not meet the recorded target.
- `Complete`: target is implemented and has recorded evidence.
- `Deferred`: discovered but outside the current milestone.

Milestone target:

- `Required`: must be complete for milestone 1.
- `Required subset`: only the behavior stated in the notes is required for milestone 1.
- `M2 required`: must be complete for `EGUI_MILESTONE_2_ENGINE.md`.
- `Superseded in M2`: completed Milestone 1 behavior intentionally replaced by the accepted Milestone 2 architecture; its historical evidence remains valid.
- `Deferred`: explicitly outside the active milestone, but not outside the project.

## Baseline sources inspected for milestone 1

The initial matrix is based on:

- Tracks and layout: `src/qml/TracksWidget.qml`, `src/qml/TrackWidget.qml`, and `src/qml/Session.qml`.
- Track creation: `src/qml/NewTrackDialog.qml` and `src/qml/js/generate_session.js`.
- Track behavior: `src/qml/TrackControlWidget.qml` and `docs/source/usage.trackcontrols.rst`.
- Loop behavior: `src/qml/LoopWidget.qml`, `src/qml/AppControls.qml`, and `docs/source/usage.loopcontrols.rst`.
- Existing egui behavior: the models and widgets in `src/rust/shoop_egui/src` plus the Qt adapter in `src/rust/frontend/src/egui_window.rs`.
- Existing integration evidence: `src/qml/test/tst_EguiWindow.qml` and the loop/track QML test suites listed by `src/qml/test`.
- Group transition, target, sync, and solo behavior: `src/qml/test/tst_TwoLoops.qml` and `src/qml/test/tst_ThreeLoops.qml`; grab-only cases remain deferred.
- Direct track topology and controls: `src/qml/test/tst_TrackControl_direct.qml`, `src/qml/test/tst_TrackControlAndLoop_direct.qml`, and the corresponding dry/wet tests used only to identify deferred behavior.

These sources do not exhaustively specify later milestones. The milestone-1 subset was refined through implementation and its focused tests.

## Baseline sources inspected for milestone 2 planning

The initial cross-target dummy-engine plan is based on:

- Current composition roots and browser packaging: `src/rust/shoopdaloop_native`, `src/rust/shoop_egui_preview`, and `.github/workflows/wasm_preview.yml`.
- Application ownership and polling: `src/rust/shoop_app/src/lib.rs`.
- Dummy backend topology and engine translation: `src/rust/shoop_backend/src/lib.rs`.
- Engine feature boundaries and application backend: `src/rust/shoop_engine/Cargo.toml` and `src/rust/shoop_engine/src/app_backend.rs`.
- Worker-owned engine services: `src/rust/shoop_engine/src/graph_scheduler.rs` and `src/rust/shoop_engine/src/content_snapshot/runtime.rs`.
- Dummy cycle semantics: `src/rust/shoop_engine/src/dummy_driver.rs` and existing controlled-driver tests.

Milestone 2 intentionally replaces the standalone backend-free preview with one engine-backed dummy application for native and browser targets. The user explicitly approved that direction. Milestone 1 evidence remains a historical statement of what passed at its completion boundary; new rows below track the superseding implementation rather than rewriting that evidence.

The portability inventory supported a simpler implementation than adapting the full native application-backend worker layer: `shoop_backend` now uses the target-neutral `shoop_engine::Session` core directly for its dummy-only façade. Topology changes are applied synchronously, loop content is read directly at stable application points, elapsed-time processing is bounded, and the native application actor drives the same backend from its thread. The retained frontend continues using the full threaded `shoop_engine/app_backend` feature and its existing JACK/CPAL/Midir/LV2 paths.

## Milestone-2 replacement evidence

Evidence referenced by the M2 rows consists of:

- Cooperative backend processing: `shoop_backend` contracts plus exact-frame record/play, fractional-time accumulation, bounded catch-up, and xrun tests.
- Application ownership: `shoop_app` threaded actor tests and cooperative real-engine workflow, waveform refresh, queue-capacity, stale-ID, and failure tests.
- Unified composition: `shoopdaloop_egui` native workflow, shared minimum/common-size paint test, and WebAssembly compiler check.
- Browser runtime: release Trunk bundle plus the Chrome DevTools smoke at 360×200 and 900×600; `?self-test=1` creates a stereo/MIDI track, records real dummy-engine frames, stops, refreshes waveform details, plays, and proves revisions continue advancing without browser exceptions.
- Browser artifacts: `build_single_file_app.py` produces the self-contained `shoopdaloop_egui.html`; the migrated workflow browser-tests and uploads both forms, including opening the single file directly through a `file:` URL.
- Dependency isolation: the Wasm runner tree contains `shoop_app`, `shoop_backend`, and `shoop_engine` but no JACK, CPAL, Midir, LV2, frontend, Qt, X11, or Wayland package; the `shoop_egui` tree remains limited to presentation dependencies and `shoop_app_api`.
- Compatibility gates: warning-denying builds, formatting, all workspace Rust tests with the full engine application backend, and retained QML self-tests pass. The QML suite reports 197 passed, 0 failed, and one environment skip for unavailable CPAL virtual playback ports.
- Native graphical environment note: native construction, real-engine workflows, and 360×200/900×600 paint tests pass. A local Xvfb process could not provide a GLX framebuffer configuration, so OS-window runtime smoke is an environment skip; the unchanged eframe native bootstrap is compiled and its prior M1 Xvfb evidence remains applicable.

## Milestone-1 replacement evidence

Evidence referenced below consists of:

- API and identity: `shoop_app_api` tests and `shoop_egui::tracks_widget` stable-ID routing test.
- Business behavior: `shoop_app` tests for initialization, stale IDs, injected creation failure, supported track shapes, aligned rows, controls, selection/details, solo/fixed recording, target delay, and snapshot independence.
- Backend behavior: shared `shoop_backend` contracts run against both `FakeBackend` and engine-backed `EngineBackend::new_dummy`.
- Presentation: `shoop_egui` action, applicability, dialog, waveform, inert-menu, and minimum/common-size paint tests.
- Native integration: `shoopdaloop_native::tests::dummy_native_workflow_creates_and_controls_tracks_and_loops`.
- Preview isolation: representative-shape test plus dependency-tree inspection showing only eframe/web support, `shoop_app_api`, and `shoop_egui` as direct dependencies and no backend/engine/Qt/Lua subtree; native and browser WebAssembly entry points share the same preview application.
- Runtime smoke: `shoopdaloop_native` stayed operational at 360×200 and 900×600 Xvfb screen sizes and `shoop_egui_preview` at 900×600 until a four-second timeout, with no runtime errors.
- Final compatibility gates: formatting, warning-free workspace build, `wasm32-unknown-unknown` GUI check, serialized full-workspace tests, and retained Qt/QML self-tests all pass. The QML suite reports 192 passed, 0 failed, and one environment skip for unavailable CPAL virtual playback ports.
- Real-backend environment note: no usable real audio device/backend was available (`/dev/snd` and `jackd` absent). Passing JACK engine/QML test backends provide regression evidence, while a real-device GUI launch remains an environment skip rather than an implementation deferral.

## First-milestone matrix

| ID | Capability or behavior | Old application baseline | Discovery | M1 target | Current implementation | Replacement evidence |
|---|---|---|---|---|---|---|
| ARCH-001 | Pure native egui process | Production startup creates a Qt application and QML engine; the prototype is a Qt-hosted egui canvas. | Explored for M1 | Required | Complete | Native dependency scan and Xvfb runtime smoke |
| ARCH-002 | Presentation/business/backend separation | QML widgets currently own substantial session and control behavior. | Explored for M1 | Required | Complete | Separate API, presentation, application, backend, preview, and native crates with one-way dependencies |
| ARCH-003 | Stable entity identity | QML uses object IDs plus coordinates; the egui prototype routes actions by track and loop indices. | Explored for M1 | Required | Complete | API identity tests and stable-ID widget/application routing tests |
| ARCH-004 | Immutable snapshot and typed intent flow | The prototype has plain state/actions but receives snapshots and emits actions through QObject adapters. | Explored for M1 | Required | Complete | API intent tests, snapshot-independence test, and bounded dispatch errors |
| ARCH-005 | Backend-free egui preview | No standalone preview executable currently supplies mock application snapshots. | Explored for M1 | Required | Complete | Preview representative-shape test, clean dependency tree, native Xvfb smoke, and deployable browser WebAssembly bundle |
| SHELL-001 | Existing egui application shell | Current `AppWidget` includes global controls, tracks, details, logo, and backend status. | Explored for M1 | Required | Complete | Native workflow and complete application paint tests |
| SHELL-002 | Logo, version, DSP, xrun, buffer, and latency display | QML and the prototype show these live values. | Explored for M1 | Required | Complete | Application/backend status contract and complete application paint tests |
| LAYOUT-001 | Horizontal track columns with vertical loop stacks | QML places tracks in horizontally scrollable columns and loops in aligned vertical slots. | Explored for M1 | Required | Complete | Refactored tracks widget and minimum/common-size paint tests |
| LAYOUT-002 | Track controls remain aligned below the loop viewport | QML renders controls in a separate row below the vertically scrollable loop area. | Explored for M1 | Required | Complete | Separate fixed controls row in `TracksWidget`; application paint tests |
| LAYOUT-003 | Track header and editable title | QML has a title field at the top of each main track; the egui prototype has an editable title. | Explored for M1 | Required | Complete | Track intent routing and native workflow tests |
| LAYOUT-004 | Sync track has a distinct fixed area and limited presentation | QML renders one non-editable sync track separately from main tracks. | Explored for M1 | Required subset | Complete | Distinct actor model plus non-editable right-side sync presentation |
| LAYOUT-005 | Horizontal and vertical overflow remain usable | QML separates horizontal track scrolling from vertical loop scrolling. | Explored for M1 | Required | Complete | Independent horizontal and vertical scroll areas; minimum/common-size paint tests |
| LAYOUT-006 | Add-track and add-loop affordances occupy QML-like positions | QML places add-track after the track columns and add-loop below each main track. | Explored for M1 | Required | Complete | Dialog tests and add-loop stable-ID intent test |
| TRACK-001 | Add Track dialog opens from the add-track button | QML opens a modal Add Track dialog with a generated default name. | Explored for M1 | Required | Complete | Add dialog paint/accept/cancel tests and Xvfb smoke |
| TRACK-002 | Create regular/direct tracks | QML can create direct tracks with configurable audio channels and optional MIDI. | Explored for M1 | Required | Complete | Fake and engine-backed direct-track contracts plus native workflow |
| TRACK-003 | Direct-track audio choices | QML offers disabled, mono, stereo, and custom 0–10 audio channels; stereo is the initial choice. | Explored for M1 | Required | Complete | API validation and supported-shape application test |
| TRACK-004 | Direct-track MIDI choice | QML offers an optional direct MIDI channel. | Explored for M1 | Required | Complete | Supported-shape application and backend contract tests |
| TRACK-005 | New-track naming and stable port-name base | QML defaults to `Track N`; the accepted name determines the initial port-name base and later title edits do not rename ports. | Explored for M1 | Required | Complete | Application creation/name logic and track action tests |
| TRACK-006 | New track receives aligned empty loop slots | QML creates at least eight slots and no fewer than the current maximum row count. | Explored for M1 | Required | Complete | `direct_track_creation_and_aligned_rows_are_published` |
| TRACK-007 | Dry/wet Add Track choices | QML supports external and Carla processing with dry/wet audio/MIDI topology. | Partially explored | Deferred | Deferred | Later FX/topology milestone |
| TRACK-008 | Trigger-only Add Track choice | QML offers a trigger-only track type intended for composite/script control. | Partially explored | Deferred | Deferred | Later composite milestone |
| TRACK-009 | Track title editing | Finishing an edit updates the track name but not its port names. | Explored for M1 | Required | Complete | Stable-ID track action handling and presentation tests |
| TRACK-010 | Output gain and stereo balance | Applicable audio output controls update the track's output ports. | Explored for M1 | Required | Complete | Track-control widget tests, application control test, and backend contract |
| TRACK-011 | Output mute | Mute affects track outputs and is reflected in the control state. | Explored for M1 | Required | Complete | Typed control tests and backend port mutation/polling implementation |
| TRACK-012 | Input gain and stereo balance | Applicable audio input controls update track input ports. | Explored for M1 | Required | Complete | Track-control widget tests and backend port mutation/polling implementation |
| TRACK-013 | Input monitoring/mute | The monitor control changes input passthrough without preventing recording. | Explored for M1 | Required | Complete | Typed control tests and backend passthrough mutation/polling implementation |
| TRACK-014 | Audio level and MIDI activity display | QML and the prototype aggregate applicable port activity into track controls. | Explored for M1 | Required | Complete | Engine state polling into application snapshots and representative preview test |
| TRACK-015 | Hide inapplicable controls | Audio gain/balance controls are absent or disabled when a track has no applicable channels. | Explored for M1 | Required | Complete | `inapplicable_track_controls_are_not_rendered` and supported-shape test |
| TRACK-016 | Track reordering and width resizing | QML supports drag reordering and per-track width adjustment. | Partially explored | Deferred | Deferred | Later layout-management milestone |
| TRACK-017 | Track deletion and track context menu | QML track options include connections, deletion, and FX state actions. | Partially explored | Deferred | Deferred | Inert affordance retained; no context menus in M1 |
| LOOP-001 | Add Loop button creates a backend-capable empty loop | QML clones the track's channel shape and port wiring into a new loop slot. | Explored for M1 | Required | Complete | Stable-ID add intent test, backend direct-track contract, and application row test |
| LOOP-002 | Add Loop preserves aligned rows | Adding from a longest track extends tracks that were one row shorter so the grid remains aligned. | Explored for M1 | Required | Complete | `direct_track_creation_and_aligned_rows_are_published` |
| LOOP-003 | Loop names and generated slot labels | New loops receive generated labels such as `(N)` and render generated labels distinctly. | Explored for M1 | Required | Complete | Application row creation and loop-widget rendering tests |
| LOOP-004 | Mode, emptiness, progress, and queued-transition rendering | Loop color, icon, progress, and transition indicator follow live loop state. | Explored for M1 | Required | Complete | Backend polling, actor publication, native workflow, and application paint tests |
| LOOP-005 | Sync, selection, target, and composite highlighting | Borders and icons identify these states. | Explored for M1 | Required subset | Complete | Sync/selection/target actor tests and existing rendering coverage; composite creation remains deferred |
| LOOP-006 | Audio level and MIDI activity display | Loop widgets show mono/stereo levels and MIDI activity when applicable. | Explored for M1 | Required | Complete | Channel-state polling and representative preview/application paint tests |
| LOOP-007 | Play action | Hover control requests normal playback and follows application sync/selection/solo policy. | Explored for M1 | Required | Complete | Actor behavior and native engine-backed workflow tests |
| LOOP-008 | Record action | Hover control requests normal recording and follows fixed-cycle and play-after-record policy. | Explored for M1 | Required | Complete | Solo/fixed-recording application test |
| LOOP-009 | Stop action | Hover control requests stop and follows application sync/selection policy. | Explored for M1 | Required | Complete | Typed action routing, transition policy, and backend contracts |
| LOOP-010 | Loop gain | The existing egui gain control updates applicable playback channel gain. | Explored for M1 | Required | Complete | Backend direct-track contract and actor gain path |
| LOOP-011 | Selection by state-icon click | QML toggles or replaces selection according to modifiers; selected loops participate in grouped transitions. | Explored for M1 | Required subset | Complete | Modifier-carrying API/widget tests and selection/details application test |
| LOOP-012 | Targeting by state-icon double-click | QML maintains at most one targeted loop and uses it as an alternate transition/recording sync source. | Explored for M1 | Required subset | Complete | Single-target actor behavior and `target_delay_is_derived_from_target_and_sync_lengths`; grab remains deferred |
| LOOP-013 | Solo-within-track behavior | With solo enabled, play/record actions stop other applicable loops in the affected track. | Explored for M1 | Required | Complete | `controls_selection_details_solo_and_fixed_recording_are_functional` |
| LOOP-014 | Dry playback and dry-to-wet recording controls | QML dry/wet loops expose orange play-dry and re-record controls. | Partially explored | Deferred | Deferred | Later dry/wet milestone |
| LOOP-015 | Grab control and behavior | QML supports always-on-ringbuffer capture with sync, fixed-cycle, target, and play-after-record policy. | Partially explored | Deferred | Deferred | Later loop-control milestone |
| LOOP-016 | Stereo loop balance control | QML exposes balance in addition to loop gain for stereo loops. | Partially explored | Deferred | Deferred | Later loop-control milestone |
| LOOP-017 | Loop context menu and its dialogs | QML provides clear, load/save, click-track, details, composition, and other actions. | Partially explored | Deferred | Deferred | No context menus in M1 |
| LOOP-018 | Loop drag reordering/moving | QML supports loop drag/drop within a track and related coordinate updates. | Partially explored | Deferred | Deferred | Later layout-management milestone |
| GLOBAL-001 | Stop all | Stops running loops and respects current sync policy. | Explored for M1 | Required | Complete | Typed global action tests and actor transition policy |
| GLOBAL-002 | Deselect all | Clears loop selection. | Explored for M1 | Required | Complete | Typed global action tests and actor selection/details state |
| GLOBAL-003 | Clear menu actions | Existing egui menu emits clear-recordings/all variants including or excluding sync. | Explored for M1 | Required | Complete | Clear-menu action test and actor include/exclude-sync filtering; no confirmation dialog added |
| GLOBAL-004 | Default record/grab preference | Existing egui control edits application state used by default-trigger behavior. | Explored for M1 | Required subset | Complete | Typed global control and actor snapshot state; dedicated grab button remains deferred |
| GLOBAL-005 | Play after record | Toggle affects recording completion and control rendering. | Explored for M1 | Required | Complete | Global action test and fixed-recording completion behavior |
| GLOBAL-006 | Sync mode | Toggle determines immediate versus synchronized loop actions. | Explored for M1 | Required | Complete | Global action and snapshot-independence tests plus transition delay policy |
| GLOBAL-007 | Solo mode | Toggle determines whether sibling loops stop on play/record. | Explored for M1 | Required | Complete | Solo/fixed-recording application test |
| GLOBAL-008 | Fixed recording cycles | Numeric control sets infinite or N-cycle recording behavior. | Explored for M1 | Required | Complete | Solo/fixed-recording application test |
| GLOBAL-009 | Main menu | QML opens connections, session I/O, monitoring, profiling, settings, and developer surfaces. | Partially explored | Deferred | Deferred | Inert affordance retained; no main-menu implementation in M1 |
| DETAILS-001 | Details pane selection | Existing egui pane follows the selected loop and handles no selection. | Explored for M1 | Required | Complete | Selection/details application and native workflow tests |
| DETAILS-002 | Audio waveform display | Existing egui waveform renders selected-loop audio data, offsets, loop regions, and play position. | Explored for M1 | Required | Complete | Backend channel-data path, immutable details snapshots, and bounded waveform tests |
| DETAILS-003 | Advanced details editing | QML details windows edit preplay, offsets, MIDI, and composites. | Partially explored | Deferred | Deferred | Later details/editing milestone |
| DIALOG-001 | Only Add Track is implemented as a dialog | QML has many dialogs; milestone 1 requests only Add Track. | Explored for M1 | Required | Complete | Add dialog paint/accept/cancel tests and source inspection; no other native `egui::Window` |
| MENU-001 | No track or loop context menus | QML has both; milestone 1 explicitly excludes them. | Explored for M1 | Deferred | Deferred | Main/track affordances are inert and loop context is absent |
| BACKEND-001 | Create direct track ports, loops, and channels | QML descriptor generation plus QObject wrappers constructs corresponding engine entities and wiring. | Explored for M1 | Required | Complete | Engine-backed direct-track contract and native workflow |
| BACKEND-002 | Poll loop, channel, port, and driver state | QObject update code currently converts state mirrors into QML properties and prototype snapshots. | Explored for M1 | Required | Complete | Engine state aggregation, backend contracts, actor publication, and native workflow |
| BACKEND-003 | Dummy-backend deterministic operation | Existing tests use a dummy backend for headless behavior. | Explored for M1 | Required | Complete | Shared contract passes for fake and engine-backed dummy implementations |

## Milestone-2 planned matrix

These rows track the completed `EGUI_MILESTONE_2_ENGINE.md` implementation.

| ID | Capability or behavior | Current baseline | Discovery | M2 target | Current implementation | Replacement evidence |
|---|---|---|---|---|---|---|
| M2-ARCH-001 | One native/browser composition package | M1 used separate native and fixture-preview runners. | Explored for M2 | M2 required | Complete | `shoopdaloop_egui` shared source, native workflow/paint tests, Wasm check, and browser smoke |
| M2-ARCH-002 | Standalone backend-free preview superseded without losing presentation isolation | M1 delivered a backend-free preview executable; M2 intentionally replaces that runner while retaining backend-free `shoop_egui` tests and contracts. | Explored for M2 | Superseded in M2 | Complete | Old packages removed; source/workflow/document scans and presentation dependency scan pass |
| M2-BUILD-001 | Dummy-only cross-target dependency graph | The full engine application-backend feature enables native drivers and plugins. | Explored for M2 | M2 required | Complete | `shoop_backend` uses engine core without `app_backend`; Wasm forbidden-package scan passes while full native feature builds/tests pass |
| M2-RUNTIME-001 | Shared application pump with threaded and cooperative adapters | M1 exposed only a native application actor. | Explored for M2 | M2 required | Complete | Shared model/update path, native actor tests, cooperative capacity/failure tests, and real-engine workflow |
| M2-RUNTIME-002 | Cooperative browser dummy-engine cycles | M1's browser fixture had no engine. | Explored for M2 | M2 required | Complete | Exact-frame audio/MIDI-capable loop test, elapsed-time tests, Wasm build, and browser scripted record/play workflow |
| M2-RUNTIME-003 | Cooperative graph and content progress | The full native backend uses graph and content workers. | Explored for M2 | M2 required | Complete | Dummy façade applies core `Session` graph changes synchronously and reads stable channel content directly; waveform workflow passes |
| M2-RUNTIME-004 | Bounded browser pause/resume behavior | M1 had no engine-backed browser timing. | Explored for M2 | M2 required | Complete | Eight-cycle per-update cap, fractional remainder, ten-second gap/xrun test, and continuing browser revisions |
| M2-SHELL-001 | Browser uses authoritative app/engine snapshots and intents | M1's preview mutated representative state locally. | Explored for M2 | M2 required | Complete | Browser self-test reaches authoritative add-track/record/stop/details/play snapshots with no exceptions |
| M2-SHELL-002 | Unified browser bundle and self-contained artifact | M1 tooling belonged to the preview package. | Explored for M2 | M2 required | Complete | Trunk bundle, self-contained HTML, migrated README and `wasm_egui.yml` workflow |
| M2-TEST-001 | Equivalent native/cooperative dummy observations | M1 had native dummy and fake contracts only. | Explored for M2 | M2 required | Complete | Backend exact-frame contracts, native actor workflow, cooperative app workflow, native runner workflow, and two-size browser smoke |
| M2-ARCH-003 | Presentation remains independently backend-free | `shoop_egui` accepts plain snapshots and emits typed intents. | Explored for M2 | M2 required | Complete | GUI tests/Wasm check pass and dependency tree contains `shoop_app_api` but no app/backend/engine implementation |

## Coarsely listed future areas

These areas remain `Unexplored` for whole-feature replacement and must be expanded before their milestones set acceptance criteria:

| Area | Discovery | Implementation |
|---|---|---|
| Session save/load, archive compatibility, schema migration, and resampling | Unexplored | Deferred |
| Audio and MIDI import/export and click-track generation | Unexplored | Deferred |
| Connections, autoconnect, buses, and external-port monitoring | Unexplored | Deferred |
| Driver and application settings | Unexplored | Deferred |
| Dry/wet topology and FX-chain hosting/state management | Partially explored | Deferred |
| Composite-loop creation, scheduling, editing, and nesting | Partially explored | Deferred |
| Lua scripting API and built-in scripts | Unexplored | Deferred |
| MIDI control configuration, learning, filtering, and control ports | Unexplored | Deferred |
| Keyboard control parity | Unexplored | Deferred |
| Monitoring, profiling, logging, crash/developer tools, and first-run UX | Unexplored | Deferred |
| Packaging, installation, and platform integration after Qt removal | Unexplored | Deferred |
