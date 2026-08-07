# egui Replacement Feature-Parity Matrix

## Purpose

This is the living feature-discovery and implementation ledger for the pure egui replacement described in `EGUI_REPLACEMENT_PROJECT.md`. It is intentionally incomplete. Entries are discovered and refined as milestone work reaches each part of the old application.

The detailed entries cover the completed tracks/loops, cross-target engine, browser-audio, track-port connections, and session-persistence/loop-I/O milestones. Areas outside those slices remain listed coarsely until their milestone discovery begins.

The retired Qt-hosted egui experiment has been removed. The legacy QML application and standalone egui applications now have independent presentation and dependency paths; QML remains only as the behavior baseline for features not yet replaced.

## Integration-removal status and evidence

The cleanup is complete:

- The QML canvas/window components, four state adapters, launch control, runtime state, dedicated QML test, two frontend adapters, registrations, initialization, and integration-only dependencies are deleted.
- `LoopWidget.qml` always renders the established QML status surface; the current native matrix runs 236 QML testcases, including the Carla subprocess status and dry/wet coverage.
- `frontend` and legacy `shoopdaloop` dependency trees contain no egui package. The standalone production runner and fixture preview retain no frontend/QML/CXX-Qt dependency.
- Focused frontend/API/application/backend/presentation/runner/preview tests pass, as do both standalone Wasm compiler checks.
- Formatting and the warning-denying all-target workspace build pass. The native release archive currently reports 1,100 Rust tests passed, including the shared Carla subprocess lifecycle and realtime guards.
- Source, lockfile, deleted-document-reference, and workflow scans contain no stale integration reference outside the retained execution record.
- Release-browser and cross-platform gates are recorded as passing under the user's explicit instruction to accept those unchanged checks without another run for this documentation closure.

## Current cross-target build and CI status

`EGUI_CI_AND_BUILD_FLAVORS_PLAN.md` is implementing the current product workflow in `.github/workflows/build_and_test_egui.yml`:

- One eight-cell matrix covers Linux x86_64, Windows x86_64, macOS arm64, and WebAssembly in debug and release; no coverage flavor exists yet.
- Every cell builds, packages, uploads, then tests in one job. Native outputs are unsigned application archives. Each web profile emits the authoritative hosted application archive and a separate self-contained HTML file.
- The hosted production bundle contains the complete Web Audio/AudioWorklet microphone path and the connections dialog. Neither product web artifact is a connection-fixture preview.
- `build_worklet.py` follows Trunk's profile, and package verification requires the UI Wasm/glue, worklet shim, and dedicated worklet Wasm while rejecting extra/stale bundle files.
- `Swatinem/rust-cache@v2` is configured per target/profile. `nektos/act` Linux/web debug paths pass build/package/staging and focused checks locally; PR #676 passes every GitHub-hosted target/profile build, package, upload, and test cell.
- `shoop_egui_preview` remains a backend-free fixture/test package and retains a Wasm compiler check, but it is not uploaded by the product workflow.

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
- `Explored for M3`: investigated enough to define the direct browser Web Audio milestone behavior.
- `Explored for connections`: investigated enough to define the track-port connections milestone behavior.
- `Partially explored`: some relevant behavior is known, but later work must continue discovery.
- `Unexplored`: not yet inventoried for replacement.

Implementation:

- `Existing widget`: presentation exists in `shoop_egui`, but is not connected to the new application architecture.
- `Not started`: no replacement implementation.
- `Partial`: replacement exists but does not meet the recorded target.
- `Complete`: target is implemented and has recorded evidence.
- `Deferred`: discovered but outside the current milestone.

Milestone target:

- `Required`: must be complete for milestone 1.
- `Required subset`: only the behavior stated in the notes is required for milestone 1.
- `M2 required`: must be complete for `EGUI_MILESTONE_2_ENGINE.md`.
- `M3 required`: must be complete for `EGUI_MILESTONE_3_BROWSER_AUDIO.md`.
- `Connections required`: must be complete for `EGUI_MILESTONE_X_CONNECTIONS_DIALOG.md`.
- `Superseded in M2`: completed Milestone 1 behavior intentionally replaced by the accepted Milestone 2 architecture; its historical evidence remains valid.
- `Deferred`: explicitly outside the active milestone, but not outside the project.
- `Loop-control refinement`: required by `EGUI_LOOP_HOVER_CONTROLS_AND_EMPTY_TRACKS_PLAN.md` after Milestone 1.

## Baseline sources inspected for the loop-control refinement

The loop-control refinement is based on `LoopWidget.qml`, `AudioDial.qml`, `SmallButtonWithCustomHover.qml`, `CustomHoverDetection.qml`, `TracksWidget.qml`, `docs/source/usage.loopcontrols.rst`, and the existing QML loop tests. It covers foreground hover families, edge-local dial indicators, play-dry, dry-to-wet re-recording, grab policy, stereo loop balance, and first-track onboarding without adding dry/wet track creation or FX topology.

Replacement evidence consists of typed API/application tests, fake and engine-backed balance/grab contracts, protocol/worklet round trips, non-zero Web Audio ringbuffer adoption, egui dial/hover/balance/empty-state interaction tests, stable-ID widget ownership, unified-runner fresh-state/workflow tests, standalone Wasm checks, and the final workspace/QML/browser regression gates recorded in the completed plan.

## Baseline sources inspected for milestone 1

The initial matrix is based on:

- Tracks and layout: `src/qml/TracksWidget.qml`, `src/qml/TrackWidget.qml`, and `src/qml/Session.qml`.
- Track creation: `src/qml/NewTrackDialog.qml` and `src/qml/js/generate_session.js`.
- Track behavior: `src/qml/TrackControlWidget.qml` and `docs/source/usage.trackcontrols.rst`.
- Loop behavior: `src/qml/LoopWidget.qml`, `src/qml/AppControls.qml`, and `docs/source/usage.loopcontrols.rst`.
- Existing egui behavior: the models and widgets in `src/rust/shoop_egui/src`.
- QML behavior evidence: the loop/track QML test suites listed by `src/qml/test`.
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

Milestone 2 intentionally replaced the standalone general-purpose backend-free preview with one engine-backed dummy application for native and browser targets. The user explicitly approved that direction. Milestone 1 evidence remains a historical statement of what passed at its completion boundary. The later connections milestone restores only a narrowly fixture-driven connection presentation surface; it does not duplicate or replace the authoritative consolidated runner.

The portability inventory supported a simpler implementation than adapting the full native application-backend worker layer: `shoop_backend` now uses the target-neutral `shoop_engine::Session` core directly for its dummy-only façade. Topology changes are applied synchronously, loop content is read directly at stable application points, elapsed-time processing is bounded, and the native application actor drives the same backend from its thread. The retained frontend continues using the full threaded `shoop_engine/app_backend` feature and its existing JACK/CPAL/Midir/LV2 paths.

## Milestone-2 replacement evidence

Evidence referenced by the M2 rows consists of:

- Cooperative backend processing: `shoop_backend` contracts plus exact-frame record/play, fractional-time accumulation, bounded catch-up, and xrun tests.
- Application ownership: `shoop_app` threaded actor tests and cooperative real-engine workflow, waveform refresh, queue-capacity, stale-ID, and failure tests.
- Unified composition: `shoopdaloop_egui` native workflow, shared minimum/common-size paint test, and WebAssembly compiler check.
- Browser runtime: release Trunk bundle plus the Chrome DevTools smoke at 360×200 and 900×600; `?self-test=1` creates a stereo/MIDI track, records real dummy-engine frames, stops, refreshes waveform details, plays, and proves revisions continue advancing without browser exceptions.
- Browser artifacts: `build_single_file_app.py` embeds the application in one HTML file; the current profile-aware packager emits uniquely named hosted archives and standalone HTML files, and the product workflow browser-tests both forms, including direct `file:` behavior.
- Dependency isolation: the Wasm runner tree contains `shoop_app`, `shoop_backend`, and `shoop_engine` but no JACK, CPAL, Midir, LV2, frontend, Qt, X11, or Wayland package; the `shoop_egui` tree remains limited to presentation dependencies and `shoop_app_api`.
- Compatibility gates: warning-denying builds, formatting, all workspace Rust tests with the full engine application backend, and retained QML self-tests pass. The QML suite reports 197 passed, 0 failed, and one environment skip for unavailable CPAL virtual playback ports.
- Native graphical environment note: native construction, real-engine workflows, and 360×200/900×600 paint tests pass. A local Xvfb process could not provide a GLX framebuffer configuration, so OS-window runtime smoke is an environment skip; the unchanged eframe native bootstrap is compiled and its prior M1 Xvfb evidence remains applicable.

## Baseline sources inspected for milestone 3 planning

The direct browser audio plan is based on:

- Browser composition, cooperative runtime, status diagnostics, packaging, and smoke automation: `src/rust/shoopdaloop_egui/src/main.rs`, its HTML/Trunk/single-file tooling, and the current `.github/workflows/build_and_test_egui.yml` successor to the original milestone workflow.
- Synchronous backend and application assumptions: `src/rust/shoop_backend/src/lib.rs` and `src/rust/shoop_app/src/lib.rs`.
- Engine audio-thread ownership, bounded command queues, state mirrors, and real-time guards: `src/rust/shoop_engine/src/engine.rs`, `state_mirror.rs`, `realtime_alloc_guard.rs`, `realtime_lock_guard.rs`, and the no-allocation/lock integration tests.
- Audio ports, recording storage, and topology scheduling constraints: `external_audio_port.rs`, `audio_channel.rs`, `chunked_samples.rs`, `session.rs`, and `graph_scheduler.rs`.
- Native worker paths that cannot be invoked from a browser worklet: `content_snapshot/runtime.rs` and the full `app_backend.rs`.
- Browser platform contracts: `getUserMedia` permission is asynchronous and secure-context-only, while browsers vary in whether and how they treat local files as potentially trustworthy; AudioWorklet runs at the owning `AudioContext` sample rate and normally supplies 128-frame render quantums; browser source/destination boundaries perform device-rate conversion.

The plan deliberately uses `web-sys` and a repository-owned minimal AudioWorklet shim rather than CPAL, Firewheel, or ScriptProcessorNode. It keeps the native egui dummy path and retained native production drivers unchanged. Hosted browser audio remains the portable path, while the self-contained artifact embeds its worklet assets and attempts direct-file physical audio where browser policy permits it.

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
| ARCH-001 | Pure native egui process | Production startup creates a Qt application and QML engine. | Explored for M1 | Required | Complete | Native dependency scan and Xvfb runtime smoke |
| ARCH-002 | Presentation/business/backend separation | QML widgets currently own substantial session and control behavior. | Explored for M1 | Required | Complete | Separate API, presentation, application, backend, preview, and native crates with one-way dependencies |
| ARCH-003 | Stable entity identity | QML uses object IDs plus coordinates; the replacement routes actions by stable track and loop IDs. | Explored for M1 | Required | Complete | API identity tests and stable-ID widget/application routing tests |
| ARCH-004 | Immutable snapshot and typed intent flow | Replacement presentation receives plain snapshots and emits typed actions through the application contract. | Explored for M1 | Required | Complete | API intent tests, snapshot-independence test, and bounded dispatch errors |
| ARCH-005 | Backend-free egui preview | No standalone preview executable currently supplies mock application snapshots. | Explored for M1 | Required | Complete | Preview representative-shape test, clean dependency tree, native Xvfb smoke, and deployable browser WebAssembly bundle |
| SHELL-001 | Existing egui application shell | Current `AppWidget` includes global controls, tracks, details, logo, and backend status. | Explored for M1 | Required | Complete | Native workflow and complete application paint tests |
| SHELL-002 | Logo, version, DSP, xrun, buffer, and latency display | QML and the standalone egui application show these live values. | Explored for M1 | Required | Complete | Application/backend status contract and complete application paint tests |
| LAYOUT-001 | Horizontal track columns with vertical loop stacks | QML places tracks in horizontally scrollable columns and loops in aligned vertical slots. | Explored for M1 | Required | Complete | Refactored tracks widget and minimum/common-size paint tests |
| LAYOUT-002 | Track controls remain aligned below the loop viewport | QML renders controls in a separate row below the vertically scrollable loop area. | Explored for M1 | Required | Complete | Separate fixed controls row in `TracksWidget`; application paint tests |
| LAYOUT-003 | Track header and editable title | QML has a title field at the top of each main track; the standalone egui application has an editable title. | Explored for M1 | Required | Complete | Track intent routing and native workflow tests |
| LAYOUT-004 | Sync track has a distinct fixed area and limited presentation | QML renders one non-editable sync track separately from main tracks. | Explored for M1 | Required subset | Complete | Distinct actor model plus non-editable right-side sync presentation |
| LAYOUT-005 | Horizontal and vertical overflow remain usable | QML separates horizontal track scrolling from vertical loop scrolling. | Explored for M1 | Required | Complete | Independent horizontal and vertical scroll areas; minimum/common-size paint tests |
| LAYOUT-006 | Add-track and add-loop affordances occupy QML-like positions | QML places add-track after the track columns and add-loop below each main track. | Explored for M1 | Required | Complete | Dialog tests and add-loop stable-ID intent test |
| LAYOUT-007 | Empty main-tracks onboarding | A fresh session owns only the sync track; the main pane should direct the user to Add Track. | Explored for loop-control refinement | Loop-control refinement | Complete | Application and unified-runner fresh-state assertions plus `empty_main_tracks_show_first_track_instruction_only` |
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
| TRACK-014 | Audio level and MIDI activity display | QML and the standalone egui application aggregate applicable port activity into track controls. | Explored for M1 | Required | Complete | Engine state polling into application snapshots and representative preview test |
| TRACK-015 | Hide inapplicable controls | Audio gain/balance controls are absent or disabled when a track has no applicable channels. | Explored for M1 | Required | Complete | `inapplicable_track_controls_are_not_rendered` and supported-shape test |
| TRACK-016 | Track reordering and width resizing | QML supports drag reordering and per-track width adjustment. | Partially explored | Deferred | Deferred | Later layout-management milestone |
| TRACK-017 | Track options menu | QML track options include connections, deletion, and FX state actions. | Explored for connections | Connections required subset | Partial | **Connections...** is complete for sync/main tracks; deletion and FX actions remain unavailable and deferred |
| LOOP-001 | Add Loop button creates a backend-capable empty loop | QML clones the track's channel shape and port wiring into a new loop slot. | Explored for M1 | Required | Complete | Stable-ID add intent test, backend direct-track contract, and application row test |
| LOOP-002 | Add Loop preserves aligned rows | Adding from a longest track extends tracks that were one row shorter so the grid remains aligned. | Explored for M1 | Required | Complete | `direct_track_creation_and_aligned_rows_are_published` |
| LOOP-003 | Loop names and generated slot labels | New loops receive generated labels such as `(N)` and render generated labels distinctly. | Explored for M1 | Required | Complete | Application row creation and loop-widget rendering tests |
| LOOP-004 | Mode, emptiness, progress, and queued-transition rendering | Loop color, icon, progress, and transition indicator follow live loop state. | Explored for M1 | Required | Complete | Backend polling, actor publication, native workflow, and application paint tests |
| LOOP-005 | Sync, selection, target, and composite highlighting | Borders and icons identify these states. | Explored for M1 | Required subset | Complete | Sync/selection/target actor tests and existing rendering coverage; composite creation remains deferred |
| LOOP-006 | Audio level and MIDI activity display | Loop widgets show mono/stereo levels and MIDI activity when applicable. | Explored for M1 | Required | Complete | Channel-state polling and representative preview/application paint tests |
| LOOP-007 | Play action | Hover control requests normal playback and follows application sync/selection/solo policy. | Explored for M1 | Required | Complete | Actor behavior and native engine-backed workflow tests |
| LOOP-008 | Record action | Hover control requests normal recording and follows fixed-cycle and play-after-record policy. | Explored for M1 | Required | Complete | Solo/fixed-recording application test |
| LOOP-009 | Stop action | Hover control requests stop and follows application sync/selection policy. | Explored for M1 | Required | Complete | Typed action routing, transition policy, and backend contracts |
| LOOP-010 | Loop gain and legible dial indicator | The gain dial updates applicable playback gain; QML leaves its centered `V` label readable. | Explored for loop-control refinement | Loop-control refinement | Complete | Edge-local indicator geometry test, egui gain interaction, backend contract, and actor gain path |
| LOOP-011 | Selection by state-icon click | QML toggles or replaces selection according to modifiers; selected loops participate in grouped transitions. | Explored for M1 | Required subset | Complete | Modifier-carrying API/widget tests and selection/details application test |
| LOOP-012 | Targeting by state-icon double-click | QML maintains at most one targeted loop and uses it as an alternate transition/recording sync source. | Explored for loop-control refinement | Loop-control refinement | Complete | Single-target actor behavior, target-delay test, and targeted grab/re-record policy coverage |
| LOOP-013 | Solo-within-track behavior | With solo enabled, play/record actions stop other applicable loops in the affected track. | Explored for M1 | Required | Complete | `controls_selection_details_solo_and_fixed_recording_are_functional` |
| LOOP-014 | Dry playback and dry-to-wet recording controls | QML exposes orange play-dry below play and re-record below record, with mode/timing policy outside the widget. | Explored for loop-control refinement | Loop-control refinement | Complete | Foreground hover-group tests, typed `PlayingDryThroughWet`/`RecordingDryIntoWet` intents, actor scheduling tests, and backend/worklet mode support; dry/wet track creation remains TRACK-007 |
| LOOP-015 | Grab control and behavior | QML supports always-on-ringbuffer capture with selection, sync/immediate, fixed-cycle, target, play-after-record, and solo policy. | Explored for loop-control refinement | Loop-control refinement | Complete | Typed grab intent, actor policy test, all-target backend preflight, bounded audio/MIDI capture, non-zero Web Audio adoption, and protocol/worklet coverage |
| LOOP-016 | Stereo loop balance control | QML exposes a `B` dial beside volume while volume or balance remains hovered. | Explored for loop-control refinement | Loop-control refinement | Complete | Foreground balance popup/reset test, immutable balance snapshots, coherent gain/balance backend factors, fake/engine/worklet/native workflow tests |
| LOOP-017 | Loop context menu and its dialogs | QML provides clear, load/save, click-track, details, composition, and other actions. | Partially explored | Deferred | Partial | Milestone 5 completes exact/WAV audio and exact/standard MIDI import/export context actions with mapping/selection dialogs; clear, click-track, details, and composition actions remain deferred |
| LOOP-018 | Loop drag reordering/moving | QML supports loop drag/drop within a track and related coordinate updates. | Partially explored | Deferred | Deferred | Later layout-management milestone |
| LOOP-019 | Hover-family overlay lifetime and geometry | QML temporary controls remain visible over source/children and paint outside the loop row without changing layout. | Explored for loop-control refinement | Loop-control refinement | Complete | Foreground `Area` groups with traversal grace/drag retention, stable-ID widget map, hover geometry tests, and dense track paint regressions |
| GLOBAL-001 | Stop all | Stops running loops and respects current sync policy. | Explored for M1 | Required | Complete | Typed global action tests and actor transition policy |
| GLOBAL-002 | Deselect all | Clears loop selection. | Explored for M1 | Required | Complete | Typed global action tests and actor selection/details state |
| GLOBAL-003 | Clear menu actions | Existing egui menu emits clear-recordings/all variants including or excluding sync. | Explored for M1 | Required | Complete | Clear-menu action test and actor include/exclude-sync filtering; no confirmation dialog added |
| GLOBAL-004 | Default record/grab preference | Existing egui control edits application state used by default-trigger behavior. | Explored for loop-control refinement | Loop-control refinement | Complete | Typed global state plus the dedicated loop grab control and actor/backend policy coverage |
| GLOBAL-005 | Play after record | Toggle affects recording completion and control rendering. | Explored for M1 | Required | Complete | Global action test and fixed-recording completion behavior |
| GLOBAL-006 | Sync mode | Toggle determines immediate versus synchronized loop actions. | Explored for M1 | Required | Complete | Global action and snapshot-independence tests plus transition delay policy |
| GLOBAL-007 | Solo mode | Toggle determines whether sibling loops stop on play/record. | Explored for M1 | Required | Complete | Solo/fixed-recording application test |
| GLOBAL-008 | Fixed recording cycles | Numeric control sets infinite or N-cycle recording behavior. | Explored for M1 | Required | Complete | Solo/fixed-recording application test |
| GLOBAL-009 | Main menu | QML opens connections, session I/O, monitoring, profiling, settings, and developer surfaces. | Explored for connections | Connections required subset | Partial | **Connections**, **Save session…**, and **Load session…** are complete; monitoring, profiling, settings, and developer entries remain unavailable/deferred |
| DETAILS-001 | Details pane selection | Existing egui pane follows the selected loop and handles no selection. | Explored for M1 | Required | Complete | Selection/details application and native workflow tests |
| DETAILS-002 | Audio waveform display | Existing egui waveform renders selected-loop audio data, offsets, loop regions, and play position. | Explored for M1 | Required | Complete | Backend channel-data path, immutable details snapshots, and bounded waveform tests |
| DETAILS-003 | Advanced details editing | QML details windows edit preplay, offsets, MIDI, and composites. | Partially explored | Deferred | Deferred | Later details/editing milestone |
| DIALOG-001 | Add Track dialog | QML has many dialogs; milestone 1 requested Add Track. | Explored for M1 | Required | Complete | Add dialog paint/accept/cancel tests |
| DIALOG-002 | Track-port Connections dialog | QML provides track-scoped and global connection windows. | Explored for connections | Connections required | Complete | Reusable connection dialog, scope/matrix/geometry tests, preview, and native dummy workflow |
| MENU-001 | Deferred context and main-menu actions | QML has track, loop, and global actions beyond connections. | Partially explored | Deferred | Partial | Connections, session Save/Load, and loop audio/MIDI I/O entries are enabled; deletion, FX, settings, and remaining loop-context actions remain unavailable/deferred |
| BACKEND-001 | Create direct track ports, loops, and channels | QML descriptor generation plus QObject wrappers constructs corresponding engine entities and wiring. | Explored for M1 | Required | Complete | Engine-backed direct-track contract and native workflow |
| BACKEND-002 | Poll loop, channel, port, and driver state | Legacy frontend update code converts state mirrors into QML properties. | Explored for M1 | Required | Complete | Engine state aggregation, backend contracts, actor publication, and native workflow |
| BACKEND-003 | Dummy-backend deterministic operation | Existing tests use a dummy backend for headless behavior. | Explored for M1 | Required | Complete | Shared contract passes for fake and engine-backed dummy implementations |

## Milestone-2 planned matrix

These rows track the completed `EGUI_MILESTONE_2_ENGINE.md` implementation.

| ID | Capability or behavior | Current baseline | Discovery | M2 target | Current implementation | Replacement evidence |
|---|---|---|---|---|---|---|
| M2-ARCH-001 | One native/browser composition package | M1 used separate native and fixture-preview runners. | Explored for M2 | M2 required | Complete | `shoopdaloop_egui` shared source, native workflow/paint tests, Wasm check, and browser smoke |
| M2-ARCH-002 | General-purpose backend-free preview superseded without losing presentation isolation | M1 delivered a backend-free preview executable; M2 replaced that general runner while retaining backend-free `shoop_egui` tests and contracts. | Explored for M2 | Superseded in M2 | Complete | M1 runner was removed at M2; the later connection-only fixture preview leaves unified authoritative composition intact |
| M2-BUILD-001 | Dummy-only cross-target dependency graph | The full engine application-backend feature enables native drivers and plugins. | Explored for M2 | M2 required | Complete | `shoop_backend` uses engine core without `app_backend`; Wasm forbidden-package scan passes while full native feature builds/tests pass |
| M2-RUNTIME-001 | Shared application pump with threaded and cooperative adapters | M1 exposed only a native application actor. | Explored for M2 | M2 required | Complete | Shared model/update path, native actor tests, cooperative capacity/failure tests, and real-engine workflow |
| M2-RUNTIME-002 | Cooperative browser dummy-engine cycles | M1's browser fixture had no engine. | Explored for M2 | M2 required | Complete | Exact-frame audio/MIDI-capable loop test, elapsed-time tests, Wasm build, and browser scripted record/play workflow |
| M2-RUNTIME-003 | Cooperative graph and content progress | The full native backend uses graph and content workers. | Explored for M2 | M2 required | Complete | Dummy façade applies core `Session` graph changes synchronously and reads stable channel content directly; waveform workflow passes |
| M2-RUNTIME-004 | Bounded browser pause/resume behavior | M1 had no engine-backed browser timing. | Explored for M2 | M2 required | Complete | Eight-cycle per-update cap, fractional remainder, ten-second gap/xrun test, and continuing browser revisions |
| M2-SHELL-001 | Browser uses authoritative app/engine snapshots and intents | M1's preview mutated representative state locally. | Explored for M2 | M2 required | Complete | Browser self-test reaches authoritative add-track/record/stop/details/play snapshots with no exceptions |
| M2-SHELL-002 | Unified browser bundle and self-contained artifact | M1 tooling belonged to the preview package. | Explored for M2 | M2 required | Complete | Trunk bundle, self-contained HTML, package README, and current cross-target egui workflow |
| M2-TEST-001 | Equivalent native/cooperative dummy observations | M1 had native dummy and fake contracts only. | Explored for M2 | M2 required | Complete | Backend exact-frame contracts, native actor workflow, cooperative app workflow, native runner workflow, and two-size browser smoke |
| M2-ARCH-003 | Presentation remains independently backend-free | `shoop_egui` accepts plain snapshots and emits typed intents. | Explored for M2 | M2 required | Complete | GUI tests/Wasm check pass and dependency tree contains `shoop_app_api` but no app/backend/engine implementation |

## Milestone-3 replacement evidence

Milestone 3 is complete. The frozen architecture and limits are recorded in `BROWSER_AUDIO_CONTRACT.md`; the criterion-by-criterion and staged completion ledger is `EGUI_MILESTONE_3_COMPLETION_AUDIT.md`. Evidence referenced below consists of:

- Dedicated worklet core: `shoop_audio_protocol` and the raw-import-free `shoop_audio_worklet.wasm` artifact, with protocol ordering/malformed/shutdown tests and an allocation-guarded 128-frame full-duplex record/monitor/waveform/playback test.
- Engine path: physical `ExternalAudioPort` staging, deterministic mono/stereo mapping and mixing, actual-quantum processing, ten-second hard-bounded channel storage, visible low/exhausted counters, and tests proving exhaustion stops safely without render allocation.
- Browser controller: direct target-gated `web-sys` `AudioContext`, `getUserMedia`, `MediaStreamAudioSourceNode`, `AudioWorklet`, `AudioWorkletNode`, lifecycle listeners, generation-safe retry, and teardown. The UI backend's elapsed-time `advance` is a no-op.
- Hosted browser evidence: Chrome 147 at 360×200 and 900×600 and Firefox 150 at 900×600 use deterministic non-silent fake microphones. They click enable, create mono and stereo tracks, monitor, record non-zero data, transfer a non-zero waveform, play non-zero output, and retain callback progress with zero protocol overflow.
- Failure/stress evidence: Chrome denial followed by same-page retry, repeated-start prevention, context suspend/resume, media-track end/retry, forced processor termination/retry, bounded command saturation/recovery, explicit shutdown with zero owned media tracks, and a 1,500-callback sustained recording workflow all pass without console exceptions or unexpected callback-budget diagnostics.
- Artifact evidence: Trunk reproducibly builds/copies the dedicated worklet. The hosted bundle passes full-duplex and output-only automation; the self-contained file embeds both Wasm modules and the worklet script, passes direct-file output-only and microphone automation in Chrome, and retains explicit `?offline=1` dummy mode. Dependency trees exclude CPAL, Firewheel, Midir, JACK, LV2, frontend, Qt, X11, and Wayland from the worklet, and module inspection reports no Wasm imports.
- Native compatibility: warning-denying full workspace build, serialized full workspace tests with `shoop_engine/app_backend`, native egui workflow/paint tests, engine real-time lock/no-allocation tests, JACK test-backend paths, and offscreen QML tests pass. QML reports 236 passed, 0 failed, and one environment skip for unavailable CPAL virtual playback ports.
- Environment/browser scope: this host has no `/dev/snd`, so physical microphone/headphone hardware was unavailable; deterministic browser fake capture is the acceptance I/O evidence. Safari is untested and is an explicit compatibility limitation rather than a support claim.

| ID | Capability or behavior | Previous baseline | Discovery | M3 target | Current implementation | Replacement evidence |
|---|---|---|---|---|---|---|
| M3-AUDIO-001 | Automatic target-selected browser audio driver | The unified browser runner always constructed the cooperative dummy backend. | Explored for M3 | M3 required | Complete | Hosted runs construct only `WebAudioBackend` and wait for enable; native workflow remains threaded dummy; `?offline=1` is explicit |
| M3-AUDIO-002 | Microphone permission and browser-origin lifecycle | No browser media permission or physical device was requested. | Explored for M3 | M3 required | Complete | Chrome grant, deny/retry, repeated start, suspend, media-track end/retry, forced failure/retry, zero-owned-track shutdown, explicit offline, and best-effort direct-file microphone tests |
| M3-AUDIO-003 | AudioWorklet-owned Shoop engine clock | Browser engine cycles followed UI elapsed time with a catch-up cap. | Explored for M3 | M3 required | Complete | Browser proxy `advance` is a no-op; worklet callback/frame counters and suspend/resume evidence prove callback-only advancement |
| M3-AUDIO-004 | Full-duplex microphone/monitor/loop/output path | Browser dummy inputs contained silence and produced no audible destination output. | Explored for M3 | M3 required | Complete | Chrome and Firefox fake-media workflows prove non-zero capture, monitor, recording, waveform, playback, and destination output |
| M3-AUDIO-005 | Browser-owned sample rate and channel mapping | Dummy processing was fixed at 48 kHz/256 frames and had no device channel negotiation. | Explored for M3 | M3 required | Complete | Actual 48 kHz/128-frame browser evidence plus native mono duplication/stereo mapping/full-duplex contracts; no engine device resampling |
| M3-AUDIO-006 | Output-only and self-contained physical audio | Browser startup always requested a microphone and the standalone artifact omitted worklet assets. | Explored after M3 | Follow-up | Complete | Separate output-only action creates a zero-input worklet without media tracks; hosted and Chrome direct-file callback tests pass; standalone HTML embeds the main and worklet Wasm plus worklet script |
| M3-RUNTIME-001 | Bounded asynchronous application/worklet protocol | Browser application called a session-owning backend synchronously on the UI thread. | Explored for M3 | M3 required | Complete | Versioned JSON values, stable-ID verification, 256-command/event bounds, strict sequence tests, malformed/stale errors, journal replay, and observable overflow |
| M3-RUNTIME-002 | Worklet real-time topology and recording storage | M2 synchronously rebuilt graphs and had a small implicit recording reserve. | Explored for M3 | M3 required | Complete | Control-task graph preparation, pre-sized render scratch, hard-bounded ten-second storage, allocation-guarded processing, and exhaustion-stop tests |
| M3-RUNTIME-003 | Bounded state, meter, and waveform publication | M2 directly polled session state and copied selected-loop data on the browser UI thread. | Explored for M3 | M3 required | Complete | 50 ms requested snapshots and revisioned ordered 512-sample waveform chunks; stress workflow maintains callback progress |
| M3-RUNTIME-004 | Audio lifecycle and failure recovery | M2 handled elapsed-time tab gaps but had no context, stream, track, or worklet lifecycle. | Explored for M3 | M3 required | Complete | Chrome suspend/resume, denial/retry, media-track-end/retry, processor-loss/retry, repeated-start rejection, handler detachment, track stop, and context close evidence |
| M3-BUILD-001 | Direct `web-sys` worklet artifact and isolated features | Browser artifact contained one UI Wasm module and no audio worklet. | Explored for M3 | M3 required | Complete | Trunk pre-build worklet production, raw no-import module inspection, and clean target dependency scans |
| M3-SHELL-001 | Permission, monitoring, driver, and error presentation | Browser status described a dummy engine only. | Explored for M3 | M3 required | Complete | Enable/retry control, typed driver state, callback/rate/quantum/activity/limit DOM attributes, egui diagnostics, and Web MIDI absence label |
| M3-TEST-001 | Physical-audio evidence without native regressions | Browser smoke proved dummy progression but not non-zero device I/O. | Explored for M3 | M3 required | Complete | Two-size Chrome, Firefox, denial, lifecycle, queue-saturation recovery, stress, offline, and direct-file limitation workflows plus warning-free/full-workspace/QML regression gates |
| M3-ARCH-001 | Presentation/native isolation remains intact | `shoop_egui` was backend-free and native egui used the threaded dummy actor. | Explored for M3 | M3 required | Complete | `shoop_egui` dependency scan stays backend/Web-Audio-free; native tests and retained full backend/QML suites pass |

## Track-port connections milestone discovery and evidence

The connections slice is explored from `ConnectionsControl.qml`, `ConnectionsWindow.qml`, the category/entry-point wiring in `TrackWidget.qml`, `AppControls.qml`, and `Session.qml`, the frontend audio/MIDI port bridge, `tst_Jack_ports.qml`, `tst_Cpal_ports.qml`, and the engine dummy/JACK connection tests. The retained behavior is: omit empty categories in Audio in/out/send/return then MIDI in/out/send order; aggregate sync and main tracks globally; scope a track menu to that one track; use opposite direction and matching data type; union endpoint rows; group full endpoint names by client for presentation; show ineligible cells; mutate exact full-name pairs; and refresh external truth while visible.

The replacement deliberately uses a resizable egui window and a both-axis scrolling matrix rather than QML's rotated fixed header and vertical-only scroll. Full endpoint names remain identities; the first `:` is split only for robust client/short-name presentation, including names with no colon or additional colons.

Replacement evidence referenced below consists of:

- `shoop_app_api` identity/contract and structural-sharing tests for typed data type, direction, role, desired state, exact endpoint preservation, and immutable connection views.
- Shared `shoop_backend` contracts against `FakeBackend` and `EngineBackend::new_dummy`, including audio/MIDI descriptors, direction/type filtering, application-owned dummy candidates, idempotent connect/disconnect, missing endpoints, endpoint churn, out-of-process changes, deferred completion, and injected failure.
- `shoop_app` actor/cooperative tests for sync/main ownership, stable app/backend mapping, deterministic snapshots, pending/confirmation/failure, churn, stale IDs, timeout, saturation, and retained confirmed truth.
- `shoop_egui` tests for global and sync/main track menu entry points, stable scope routing, category order/omission, exact cell intents, unavailable cells, first-colon display handling, and matrix painting at 360×200 and 900×600.
- `shoopdaloop_egui::tests::native_dummy_workflow_creates_records_and_controls_tracks_and_loops`, which now connects sync and main audio/MIDI ports, observes confirmations, disconnects, and continues the existing track/loop workflow.
- The restored backend-free `shoop_egui_preview`, whose fixtures contain every category, sync/main scopes, multiple clients, connected/disconnected/unavailable/pending/error/loading/backend-unavailable states, endpoint churn, and intent confirmation/failure controls. Its dependency tree contains neither application/backend/engine nor frontend/Qt crates. Its milestone-completion regular/self-contained browser evidence remains historical; the current product workflow retains its compiler check without uploading it as an application artifact.
- The direct hosted Web Audio backend publishes typed local audio/MIDI descriptors but explicitly reports arbitrary external connection management unavailable; its existing browser-default physical routing remains unchanged. The native dummy connection path is the integrated mutation evidence until native real-driver selection is added.

| ID | Capability or behavior | Retained baseline | Discovery | Milestone target | Current implementation | Replacement evidence |
|---|---|---|---|---|---|---|
| CONN-ARCH-001 | Presentation dependency isolation | QML calls QObject port methods directly. | Explored for connections | Connections required | Complete | `shoop_egui` consumes API snapshots/intents only; GUI and preview dependency scans |
| CONN-MODEL-001 | Stable typed local-port identity and ownership | QML objects/descriptor IDs and regex-derived categories identify ports. | Explored for connections | Connections required | Complete | Stable `PortId`, owning `TrackId`, explicit type/direction/role/name; API and actor tests |
| CONN-MODEL-002 | Immutable revisioned connection state | QML reconstructs per-port maps every 100 ms. | Explored for connections | Connections required | Complete | `Arc<ConnectionViewState>` and shared port arrays are reused until structural state changes |
| CONN-ENTRY-001 | Sync/main track **Connections...** entry | Every QML track options menu opens its own window. | Explored for connections | Connections required | Complete | Track-widget menu interaction test and stable track-scope routing |
| CONN-ENTRY-002 | Global **Connections** entry | QML main menu opens an aggregate window. | Explored for connections | Connections required | Complete | Global-controls menu interaction test and `AllTracks` scope |
| CONN-SCOPE-001 | Track scope isolation | Track QML passes only that track's category lists. | Explored for connections | Connections required | Complete | Dialog scope filtering by owning `TrackId`; scope/paint tests include sync/main fixtures |
| CONN-SCOPE-002 | Global sync/main aggregation | Session QML flattens sync plus all main-track categories. | Explored for connections | Connections required | Complete | `AllTracks` filters no tracked ports; unrelated non-track ports are absent from app inventory |
| CONN-CAT-001 | Ordered non-empty role tabs | QML order is Audio in/out/send/return, MIDI in/out/send. | Explored for connections | Connections required | Complete | Explicit `PortRole::ORDERED`; category and preview tests cover all seven roles |
| CONN-TOPO-001 | Current direct/sync audio and MIDI input/output inventory | Descriptor generation creates externally connectable direct ports. | Explored for connections | Connections required | Complete | Engine/Fake/Web Audio descriptors and app ownership tests; send/return topology remains deferred |
| CONN-DISC-001 | Compatible endpoint discovery | Port state maps expose compatible opposite-direction endpoints. | Explored for connections | Connections required | Complete | Compact backend poll snapshots; shared fake/engine discovery contracts |
| CONN-COMPAT-001 | Direction and data-type eligibility | Inputs see outputs; outputs see inputs; audio/MIDI do not mix. | Explored for connections | Connections required | Complete | Backend contract assertions and ineligible-cell GUI fixtures |
| CONN-DISC-002 | Application-owned driver ports may be candidates | JACK QML test includes `shoop:*` opposite ports. | Explored for connections | Connections required | Complete | Engine dummy registry publishes typed `shoop:*` candidates and contract verifies them |
| CONN-LAYOUT-001 | Local columns, endpoint rows, grouping, indicators | QML uses rotated columns, grouped client labels, circles/cancel icons. | Explored for connections | Connections required | Complete | egui grid with deterministic columns/rows, client groups, connected/open/unavailable/pending/error indicators |
| CONN-LAYOUT-002 | Large-matrix overflow and small-window usability | QML scrolls endpoint rows; replacement requires both axes. | Explored for connections | Connections required | Complete | Both-axis scroll area and 360×200/900×600 paint tests |
| CONN-MUT-001 | Exact desired-state connect/disconnect intent | QML performs a blind toggle on the clicked object/name. | Explored for connections | Connections required | Complete | `SetPortConnected { PortId, external_port, connected }`; API/GUI/backend tests |
| CONN-MUT-002 | Actor validation and command ordering | QObject calls bypass the new actor model. | Explored for connections | Connections required | Complete | Actor validates stable local ID, exact eligible endpoint, and serializes backend calls |
| CONN-STATE-001 | Confirmed truth remains separate from pending | QML refreshes immediately after commands. | Explored for connections | Connections required | Complete | Pending desired state overlays confirmed snapshot and clears only on observation/failure/timeout |
| CONN-STATE-002 | External changes and endpoint churn | Visible QML timer refreshes every 100 ms. | Explored for connections | Connections required | Complete | 16 ms bounded app poll; actor test adds/removes endpoints and changes connection externally |
| CONN-ERR-001 | Stale ID, disappearance, incompatibility, rejection | Old UI has no typed per-cell failure contract. | Explored for connections | Connections required | Complete | Typed error kinds, notifications, backend validation, actor churn/failure tests |
| CONN-ERR-002 | Saturation and timeout visibility | Old bridge can log/optimistically cache failure. | Explored for connections | Connections required | Complete | Saturation error publication and deterministic two-second cooperative timeout test |
| CONN-PRES-001 | Close/reopen/scope presentation safety | QML creates independent windows with local selected/scroll state. | Explored for connections | Connections required | Complete | One egui window with stable scope IDs, scope-specific scroll IDs, stable-key intents, and stale-track state |
| CONN-PREVIEW-001 | Backend-free native/browser preview | The old standalone M1 preview was removed by M2. | Explored for connections | Connections required | Complete | Connection-focused fixture restored without app/backend/engine dependencies; fixture/intent tests, retained Wasm check, and historical milestone browser artifacts |
| CONN-WASM-001 | Browser-compatible presentation | QML path is native; pure egui presentation must compile for Wasm. | Explored for connections | Connections required | Complete | GUI/preview Wasm checks; Web Audio reports unsupported arbitrary external mutation without changing default routing |
| CONN-E2E-001 | Native fake/dummy integrated workflow | Retained tests exercise JACK/CPAL through QML. | Explored for connections | Connections required | Complete | Shared contracts, actor tests, and native sync/audio/MIDI connection workflow |
| CONN-DEF-001 | Persisted external connections and autoconnect | Sessions can hold external connection names; the connections milestone excluded persistence/rules. | Explored for connections | Deferred | Complete | Milestone 5 `.shoop` documents and authoritative app/backend round trips preserve ordered external connection/autoconnect names; runtime reconnect policy remains deferred |
| CONN-DEF-002 | Driver selection/settings and native real-driver composition | Retained frontend owns JACK/CPAL settings and drivers. | Explored for connections | Deferred | Deferred | Typed unavailable state in hosted Web Audio; native egui real-driver composition remains roadmap work |
| CONN-DEF-003 | Dry/wet send/return topology and FX chains | QML dry/wet tracks supply send/return categories. | Explored for connections | Deferred | Deferred | Role model/preview support is complete; topology creation remains FX milestone work |
| CONN-DEF-004 | MIDI-control and other non-track ports | Global QML session dialog aggregates track ports, not the control port. | Explored for connections | Deferred | Deferred | Explicitly excluded from authoritative track inventory |

## Carla subprocess hosting discovery

The native QML Carla path now supplies a complete frontend-independent hosting-mode setting, processor seam, supervised worker lifecycle, bounded generation logs, shared-memory audio/MIDI transport, checkpoint recovery, and status/log adapter baseline. Native package, process, deadline, cleanup, QML, and benchmark evidence passes on Windows, Linux, macOS Intel, and macOS ARM. Milestone 5 adds byte-exact typed persistence for deferred FX/Carla state but intentionally capability-rejects runtime instantiation; dry/wet topology, FX intents/snapshots, settings presentation, runnable FX session loading, and native real-driver composition remain deferred. A future egui FX milestone must reuse these engine/application semantics rather than introduce a second worker protocol. In the rows below, `Partial` describes replacement parity, not the completed shared native baseline.

| ID | Capability or behavior | Current native baseline | Discovery | Current implementation | Evidence |
|---|---|---|---|---|---|
| FX-SUBPROC-001 | Global direct/subprocess policy | QML settings were previously window-owned and Carla was always direct. | Partially explored | Partial | Shared native baseline complete: startup-owned typed settings default old files to direct mode and current QML exposes restart-scoped selection; egui settings presentation remains deferred |
| FX-SUBPROC-002 | One worker generation per Carla chain | Direct hosts share the application process. | Partially explored | Partial | Shared native baseline complete: self-spawned workers, independent-process/crash/restart tests, and installed-package evidence pass on Windows, Linux, macOS Intel, and macOS ARM; egui FX composition remains deferred |
| FX-SUBPROC-003 | Bounded realtime block transfer | Direct Carla ran inline under a callback-visible mutex. | Partially explored | Partial | Shared native baseline complete: lock-free single-writer endpoint, preallocated audio/MIDI pools, three generation-tagged slots, bounded fallback, authenticated notification, allocation/lock guards, and all-platform 2/16-channel six-size measurements pass |
| FX-SUBPROC-004 | Checkpoint and click recovery | Session state lives in the live direct host. | Partially explored | Partial | Shared native baseline complete: parent checkpoint retention, classified supervised restart, QML toggle-or-recover, and direct/subprocess dry/wet activation/MIDI suites pass; pure-egui FX remains deferred |
| FX-SUBPROC-005 | Worker diagnostics | No per-chain subprocess streams existed. | Partially explored | Partial | Shared native baseline complete: bounded generation-tagged stdout/stderr, inspect/copy/clear, truncation, crash notification, status, keyboard, and accessibility tests pass; pure-egui presentation remains deferred |
| FX-SUBPROC-006 | Pure-egui FX integration | FX chains and dry/wet tracks are deferred. | Partially explored | Deferred | Shared core is frontend-independent; no egui FX/settings API or presentation is claimed |

## Milestone-5 persistence and loop-I/O matrix

Milestone 5 is implemented by `EGUI_MILESTONE_5_SESSION_PERSISTENCE_AND_LOOP_IO.md` and the fresh-format contract in `docs/session_format_v1.md`. QML descriptors and tests remain behavior-discovery evidence only: QML-era `.shl`, `session.1`, tar archives, and JSON `.smf` are intentional non-requirements and are rejected without changing the running session.

| ID | Capability or behavior | Retained behavior / new contract | Discovery | M5 target | Current implementation | Replacement evidence |
|---|---|---|---|---|---|---|
| IO-ARCH-001 | Target-neutral persistence boundary | Persistence serializes the authoritative model, never widgets | Explored for M5 | M5 required | Complete | `shoop_session` native/Wasm tests; application mapping; GUI dependency scan |
| IO-FMT-001 | Fresh versioned session container | `.shoop` v1 ZIP64, deterministic JSON manifest, Deflate, hashes | Explored for M5 | M5 required | Complete | Deterministic bit-exact archive round trips and Wasm check |
| IO-FMT-002 | Version/schema rejection and future migration boundary | Unsupported older/future major versions fail before mutation | Explored for M5 | M5 required | Complete | Version/path/resource tests plus application old/future-format rollback |
| IO-FMT-003 | Complete session-scoped state document | Controls, topology, routes, buses, composites, scripts, MIDI control, settings, FX | Explored for M5 | M5 required | Complete | Typed deferred-state fixture and authoritative direct-track model round trip |
| IO-FMT-004 | Exact Carla and recorded FX state | Opaque Carla strings and channel FX-state references are byte-exact | Explored for M5 | M5 required | Complete | Byte-exact codec fixture; unavailable Carla runtime is rejected as a capability, not dropped |
| IO-AUD-001 | Exact compressed session audio | Per-channel little-endian `f32` payloads avoid aggregate codec limits | Explored for M5 | M5 required | Complete | Bit-exact deterministic archive and 300-channel tests |
| IO-AUD-002 | Individual loop audio export | Ordered selected channels; float WAV and `.shoop-audio` on all targets | Explored for M5 | M5 required | Complete | App ordered-selection/WAV/exact test, egui selection paint/routing, browser real-byte round trip |
| IO-AUD-003 | Individual loop audio import | Explicit source-to-destination mapping and optional length adoption | Explored for M5 | M5 required | Complete | App fewer/equal/more/duplicate mapping and length test; browser mapping round trip; dry/wet runtime topology remains capability-rejected |
| IO-MIDI-001 | Exact loop/session MIDI | Integer source frames, duration, start state, equal-frame order, exact bytes | Explored for M5 | M5 required | Complete | Exact archive/order/start-state/resampling tests and app/browser exact-media round trips |
| IO-MIDI-002 | Standard MIDI interoperability | Tempo-map import and disclosed high-resolution export quantization | Explored for M5 | M5 required | Complete | Tempo/SysEx/order encode/decode tests; 7,650-tick/s export and measured quantization notification |
| IO-CHAN-001 | Arbitrary channels per loop | `u32` format/API counts; no old 10-channel persistence ceiling | Explored for M5 | M5 required | Complete | 300-channel codec, 12-channel app session, `u32` backend/protocol, custom-channel UI tests, and deterministic four-loop-channel-to-stereo Web Audio playback mix |
| IO-RATE-001 | Session sample-rate warning/conversion | Confirm before deterministic audio and sample-domain conversion | Explored for M5 | M5 required | Complete | 48↔44.1/32/96 codec cases and app warning/cancel/accept/round-trip test |
| IO-RATE-002 | Loop media sample-rate warning/conversion | The same warning and conversion policy applies to loop imports | Explored for M5 | M5 required | Complete | Exact MIDI/audio app warning and deterministic frame conversion test |
| IO-SAVE-001 | Coherent playback-safe save | One settled content epoch; playing continues through compression/output | Explored for M5 | M5 required | Complete | Active-recording rejection, native background compression, worklet generation/chunk test, Firefox normal/stress callback continuity |
| IO-LOAD-001 | Transactional session replacement | Validate/stage/finalize then one commit; abort retains old session | Explored for M5 | M5 required | Complete | Shared Fake/engine rollback contract, worklet stale/incomplete/abort tests, app publish-after-commit test |
| IO-TASK-001 | Progress, cancellation, and typed errors | Bounded immutable task state; no large bytes in snapshots | Explored for M5 | M5 required | Complete | App warning/cancel/stale-task/queue tests, typed file errors, task dialog paint/routing |
| IO-FILE-001 | Native file service | Async read/write, temporary sibling, atomic replacement | Explored for M5 | M5 required | Complete | Composition-root worker reads/writes and atomic replace/cleanup/failure test |
| IO-FILE-002 | Hosted and self-contained browser files | Async upload/download fallback and browser file handles | Explored for M5 | M5 required | Complete | `rfd` async adapters; hosted Firefox and direct-file self-contained real-produced-byte automation |
| IO-PRES-001 | Main session I/O controls | Enabled Save/Load actions, warnings, progress, actionable failures | Explored for M5 | M5 required | Complete | Global-controls typed menu tests, task dialog paint, native/browser workflows |
| IO-PRES-002 | Loop context media controls | Audio/exact MIDI/standard MIDI load/save and mapping surfaces | Explored for M5 | M5 required | Complete | Stable-ID context intents, ordered selection/mapping UI, app stale-ID validation, browser loop-byte round trip |
| IO-SEC-001 | Untrusted archive/resource safety | Reject traversal, duplicates, bombs, overflows, and hash mismatches | Explored for M5 | M5 required | Complete | Adversarial path/duplicate/hash/size/count/reference/version corpus in `shoop_session` |
| IO-OLD-001 | QML-era archive handling | Old archive/media formats are deliberately unsupported | Explored for M5 | M5 required | Complete | Codec rejection and application no-mutation/error evidence |
| IO-E2E-001 | Authoritative native/browser round trip | Save/load/play and loop export/import under real runtimes | Explored for M5 | M5 required | Complete | Native dummy save/load/play workflow; Firefox Web Audio session/audio/MIDI exact-byte workflow; self-contained direct-file session/media workflow |

## Persistent egui settings discovery

`EGUI_PERSISTENT_SETTINGS_PLAN.md` defines the fresh application-preference slice. Discovery covers the existing `shoop_settings`/QML settings separation, egui composition and menu/widget boundaries, Add Track defaults, the cross-target product runner, `directories` platform paths, browser origin storage, and the versioning pattern established by `shoop_session`. The replacement deliberately does not import the QML `settings.1` format. Machine/user preferences remain separate from `.shoop` session state.

| ID | Capability or behavior | Retained/new contract | Discovery | Milestone target | Current implementation | Planned evidence |
|---|---|---|---|---|---|---|
| SET-ARCH-001 | Explicit settings registration near consumers | QML settings are window-owned; new consumers register typed definitions during composition | Explored for settings | Settings required | Not started | Registry composition and duplicate/type tests |
| SET-ARCH-002 | Presentation/persistence separation | `shoop_egui` remains free of filesystem and browser APIs | Explored for settings | Settings required | Not started | Dependency/source scans and Wasm checks |
| SET-FMT-001 | Fresh egui settings identity | No QML-format compatibility is required | Explored for settings | Settings required | Planned | `docs/settings_format_v1.md`, rejection fixtures, path/key scans |
| SET-FMT-002 | Version checks and migration dispatch | Envelope-first format/major/minor/document checks and ordered pure migrations | Explored for settings | Settings required | Not started | Version corpus and migration-chain harness |
| SET-FMT-003 | Missing, invalid, and unknown values | Registered defaults, typed warnings, and unknown same-version key retention | Explored for settings | Settings required | Not started | Deterministic codec/registry tests |
| SET-API-001 | Typed immutable access | Stable `SettingKey<T>` reads from revisioned snapshots | Explored for settings | Settings required | Not started | Typed getter/default/revision tests |
| SET-NATIVE-001 | Standard native config location | One `ProjectDirs` identity across Linux, Windows, and macOS | Explored for settings | Settings required | Not started | Injected-path tests and cross-platform CI |
| SET-NATIVE-002 | Transactional native write | Same-directory temporary file, flush, atomic replace, no publish on failure | Explored for settings | Settings required | Not started | Failure injection and restart round trip |
| SET-WEB-001 | Browser-origin persistence | Stable `localStorage` key for hosted/self-contained Wasm | Explored for settings | Settings required | Not started | Adapter tests and browser save/reload workflow |
| SET-ERR-001 | Observable load/save/recovery state | Defaults keep the app usable; rejected input is not silently overwritten | Explored for settings | Settings required | Not started | Malformed/future/storage-failure UI and manager tests |
| SET-PRES-001 | Main-menu settings dialog | Registry-generated, resizable dialog with Save/Cancel/reset/help/effect state | Explored for settings | Settings required | Not started | Typed routing and 360×200/900×600 paint tests |
| SET-USE-001 | Default new-track audio channels | Existing Add Track draft starts at stereo | Explored for settings | Settings required | Not started | Save/restart/next-open integration tests |
| SET-USE-002 | Default new-track MIDI state | Existing Add Track draft starts with MIDI disabled | Explored for settings | Settings required | Not started | Save/restart/next-open integration tests |
| SET-E2E-001 | Cross-target persistence | Native and browser reload use real persisted text and authoritative consumers | Explored for settings | Settings required | Not started | Native temporary-config restart and product browser automation |
| SET-OLD-001 | Retained QML isolation | Existing QML settings/Carla path remains independently regression-tested | Explored for settings | Settings required | Not started | Mutual rejection tests and retained QML suite |

Driver/device selection, MIDI-control and script settings, runnable Carla/FX settings, and session-local overrides remain assigned to their owning milestones; this slice establishes the service and two currently usable application preferences only.

## Coarsely listed future areas

These areas remain `Unexplored` for whole-feature replacement and must be expanded before their milestones set acceptance criteria:

| Area | Discovery | Implementation |
|---|---|---|
| Click-track generation beyond loop media I/O | Partially explored | Deferred |
| Runtime reconnect policy and runnable bus topology | Partially explored | Deferred |
| Driver/device settings and native real-driver composition beyond the persistent-settings foundation | Partially explored | Deferred |
| Dry/wet topology and FX-chain hosting/state management | Partially explored | Deferred |
| Composite-loop creation, scheduling, editing, and nesting | Partially explored | Deferred |
| Lua scripting API and built-in scripts | Unexplored | Deferred |
| MIDI control configuration, learning, filtering, and control ports | Unexplored | Deferred |
| Keyboard control parity | Unexplored | Deferred |
| Monitoring, profiling, logging, crash/developer tools, and first-run UX | Unexplored | Deferred |
| Packaging, installation, and platform integration after Qt removal | Unexplored | Deferred |
