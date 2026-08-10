# egui Replacement Feature-Parity Matrix

## Purpose

This is the living feature-discovery and implementation ledger for the pure egui replacement described in `EGUI_REPLACEMENT_PROJECT.md`. It is intentionally incomplete. Entries are discovered and refined as milestone work reaches each part of the old application.

The detailed entries cover the completed tracks/loops, cross-target engine, browser-audio, native JACK/CPAL+midir/dummy driver management, track-port connections, session-persistence/loop-I/O, settings, native Lua, cross-target ports/browser-Lua/omniLua migration, Wasm Web MIDI, and click-track generation milestones, plus the implemented Tiny Synth/FX milestone that is awaiting master integration and final repository-wide gates. Areas outside those slices remain listed coarsely until their milestone discovery begins.

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
- The former backend-free `shoop_egui_preview` fixture package has been removed; its presentation coverage remains in `shoop_egui` tests and the product workflow no longer checks it.

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
- `Explored for click tracks`: investigated enough to define the click-track generation milestone behavior.
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
- `Click-track required`: must be complete for `EGUI_CLICK_TRACK_GENERATION_PLAN.md`.
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

- Milestone-2-era composition roots and browser packaging: `src/rust/shoopdaloop_native`, the since-removed `src/rust/shoop_egui_preview`, and `.github/workflows/wasm_preview.yml`.
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
| TRACK-007 | Dry/wet Add Track choices | QML supports external and Carla processing with dry/wet audio/MIDI topology. | Explored for dry/wet/Carla and Tiny Synth/FX | Dry/wet/processor required | Complete | Capability-driven Regular/Dry + Wet form; native External/Carla/Tiny catalog and browser Tiny catalog; independent External/Carla counts; matched Tiny counts with required MIDI; role-aware cross-target session/media round trips; transactional unavailable-processor rejection |
| TRACK-008 | Trigger-only Add Track choice | QML offers a trigger-only track type intended for composite/script control. | Partially explored | Deferred | Deferred | Later composite milestone |
| TRACK-009 | Track title editing | Finishing an edit updates the track name but not its port names. | Explored for M1 | Required | Complete | Stable-ID track action handling and presentation tests |
| TRACK-010 | Output gain and stereo balance | Applicable audio output controls update the track's output ports. | Explored for M1 | Required | Complete | Track-control widget tests, application control test, and backend contract |
| TRACK-011 | Output mute | Mute affects track outputs and is reflected in the control state. | Explored for M1 | Required | Complete | Typed control tests and backend port mutation/polling implementation |
| TRACK-012 | Input gain and stereo balance | Applicable audio input controls update track input ports. | Explored for M1 | Required | Complete | Track-control widget tests and backend port mutation/polling implementation |
| TRACK-013 | Input monitoring/mute | The monitor control changes input passthrough without preventing recording. | Explored for M1 | Required | Complete | Typed control tests and backend passthrough mutation/polling implementation |
| TRACK-014 | Audio level and MIDI activity display | QML and the standalone egui application aggregate applicable port activity into track controls. | Explored for M1 | Required | Complete | Engine state polling into application snapshots and representative preview test |
| TRACK-015 | Hide inapplicable controls | Audio gain/balance controls are absent or disabled when a track has no applicable channels. | Explored for M1 | Required | Complete | `inapplicable_track_controls_are_not_rendered` and supported-shape test |
| TRACK-016 | Track reordering and width resizing | QML supports drag reordering and per-track width adjustment. | Partially explored | Deferred | Deferred | Later layout-management milestone |
| TRACK-017 | Track options menu | QML track options include connections, deletion, and FX state actions. | Explored for connections | Connections required subset | Partial | **Connections...**, capability-driven processed-track FX controls, logs, and compatible recorded-take restore are complete; deletion remains deferred |
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
| LOOP-014 | Dry playback and dry-to-wet recording controls | QML exposes orange play-dry below play and re-record below record, with mode/timing policy outside the widget. | Explored for loop-control refinement | Loop-control refinement | Complete | Foreground hover-group tests, typed `PlayingDryThroughWet`/`RecordingDryIntoWet` intents, actor scheduling tests, shared routing table, and runnable native External/Carla tracks |
| LOOP-015 | Grab control and behavior | QML supports always-on-ringbuffer capture with selection, sync/immediate, fixed-cycle, target, play-after-record, and solo policy. | Explored for loop-control refinement | Loop-control refinement | Complete | Typed grab intent, actor policy test, all-target backend preflight, bounded audio/MIDI capture, non-zero Web Audio adoption, and protocol/worklet coverage |
| LOOP-016 | Stereo loop balance control | QML exposes a `B` dial beside volume while volume or balance remains hovered. | Explored for loop-control refinement | Loop-control refinement | Complete | Foreground balance popup/reset test, immutable balance snapshots, coherent gain/balance backend factors, fake/engine/worklet/native workflow tests |
| LOOP-017 | Loop context menu and its dialogs | QML provides clear, load/save, click-track, details, composition, and other actions. | Explored for click tracks | Click-track required subset | Partial | Milestone 5 completes exact/WAV audio and exact/standard MIDI import/export; `EGUI_CLICK_TRACK_GENERATION_PLAN.md` completes the click action/dialog, while clear, details, and composition remain deferred |
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
| GLOBAL-009 | Main menu | QML opens connections, session I/O, monitoring, profiling, settings, and developer surfaces. | Explored for settings | Settings required subset | Partial | **Connections**, **Save session…**, **Load session…**, and registry-driven **Settings** are complete; monitoring, profiling, and developer entries remain deferred |
| DETAILS-001 | Details pane selection | Existing egui pane follows the selected loop and handles no selection. | Explored for M1 | Required | Complete | Selection/details application and native workflow tests |
| DETAILS-002 | Audio waveform display | Existing egui waveform renders selected-loop audio data, offsets, loop regions, and play position. | Explored for M1 | Required | Complete | Backend channel-data path, immutable details snapshots, and bounded waveform tests |
| DETAILS-003 | Advanced details editing | QML details windows edit preplay, offsets, MIDI, and composites. | Partially explored | Deferred | Deferred | Later details/editing milestone |
| DIALOG-001 | Add Track dialog | QML has many dialogs; milestone 1 requested Add Track. | Explored for M1 | Required | Complete | Add dialog paint/accept/cancel tests |
| DIALOG-002 | Track-port Connections dialog | QML provides track-scoped and global connection windows. | Explored for connections | Connections required | Complete | Reusable connection dialog, scope/matrix/geometry tests, preview, and native dummy workflow |
| MENU-001 | Deferred context and main-menu actions | QML has track, loop, and global actions beyond connections. | Partially explored | Deferred | Partial | Connections, session Save/Load, Settings, and loop audio/MIDI I/O entries are enabled; deletion, FX, and remaining loop-context actions remain deferred |
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
- The historical backend-free `shoop_egui_preview` fixtures covered every category, sync/main scopes, multiple clients, connected/disconnected/unavailable/pending/error/loading/backend-unavailable states, endpoint churn, and intent confirmation/failure controls. Its dependency tree contained neither application/backend/engine nor frontend/Qt crates. The package has since been removed after its presentation coverage moved into `shoop_egui` tests; its milestone-completion browser evidence remains historical.
- Historical M4 delivery published typed browser local descriptors but reported physical connection management unavailable. The completed cross-target ports follow-up supersedes that limitation with normalized host inventories and mutable authoritative worklet routes proven by Chrome/Firefox production automation. Native real-driver selection remains separate.

| ID | Capability or behavior | Retained baseline | Discovery | Milestone target | Current implementation | Replacement evidence |
|---|---|---|---|---|---|---|
| CONN-ARCH-001 | Presentation dependency isolation | QML calls QObject port methods directly. | Explored for connections | Connections required | Complete | `shoop_egui` consumes API snapshots/intents only; GUI dependency scans and historical preview isolation scan |
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
| CONN-WASM-001 | Browser-compatible presentation | QML path is native; pure egui presentation must compile for Wasm. | Explored for connections | Connections required | Complete | GUI/preview Wasm checks and production hosted/direct-file normalized mutable Web Audio route workflows pass |
| CONN-E2E-001 | Native fake/dummy integrated workflow | Retained tests exercise JACK/CPAL through QML. | Explored for connections | Connections required | Complete | Shared contracts, actor tests, and native sync/audio/MIDI connection workflow |
| CONN-DEF-001 | Persisted external connections and autoconnect | Sessions can hold external connection names; the connections milestone excluded persistence/rules. | Explored for connections | Deferred | Complete | Milestone 5 `.shoop` documents and authoritative app/backend round trips preserve ordered external connection/autoconnect names; runtime reconnect policy remains deferred |
| CONN-DEF-002 | Driver selection/settings and native real-driver composition | Retained frontend owns JACK/CPAL settings and drivers. | Explored for native drivers | Native drivers required | Complete | Native-only `NativeBackend` adapts application-backend JACK/CPAL+midir/dummy; typed catalogs/configs, exact-rate confirmation, `resample_session`, rollback/fatal state, persistent profiles/startup fallback, egui Audio UI, deterministic JACK/CPAL test adapters, optional real-driver switches, Wasm exclusion, and full regression gates |
| CONN-DEF-003 | Dry/wet send/return topology and FX chains | QML dry/wet tracks supply send/return categories. | Explored for dry/wet/Carla | Dry/wet/Carla required | Complete | Native External publishes/restores exact Audio in/send/return/out and MIDI in/send links; Carla keeps indexed FX ports internal while publishing only dry inputs, wet outputs, and dry MIDI input |
| CONN-DEF-004 | MIDI-control and other non-track ports | Global QML session dialog aggregates track ports, not the control port. | Re-explored for cross-target ports/Lua | Superseded target | Complete | Explicit script/registration ownership publishes stable Lua-created logical control ports in global scope, including zero-host browser state |

## Native audio-driver switching discovery and evidence

`EGUI_NATIVE_AUDIO_DRIVER_SWITCHING_PLAN.md` is the implementation and validation ledger for native driver management. The delivered slice adds:

- target-gated `shoop_backend::NativeBackend` composition over the existing application-backend `AudioDriver`/`BackendSession`, with production JACK, CPAL+midir, and dummy/offline discovery while test drivers remain hidden;
- plain API catalogs, configured/resolved profiles, exact target rate/buffer reporting, generation-scoped confirmation and persistence state, dynamic host/device/MIDI refresh, and explicit unsupported Web Audio defaults;
- application-owned capture, `shoop_session::resample_session`, stopped-session replacement, stable application-ID remapping, compatible-link restoration, reconfirmation if negotiation changes, rollback, and fatal double-failure publication;
- native Audio settings with independent profiles, unavailable-selector retention, interruption and exact-rate warning popup, save-after-commit, save retry without a second switch, and persisted-first startup with diagnostic dummy fallback;
- native dummy, JACK-test, CPAL-test, repeated-lifecycle, actor failure/reconfirmation/recording/I/O/remap/rollback/resampling, settings restart/failure/retry, backend-free egui paint, optional real JACK/CPAL/cross-driver, warning-denying native/Wasm, dependency-exclusion, workspace, QML, and package evidence.

Hosted browser Web Audio remains automatic and its dependency tree remains free of native driver and MIDI packages.

Final validation on 2026-08-08 passed 141 focused warning-denying native tests, the complete workspace suite, and all 236 retained QML testcases with no failures or skips. Locked debug/release production and preview Wasm checks, AudioWorklet builds/import inspection, and forbidden native-dependency scans also passed. CPAL discovery exposed `default` and `pipewire`; optional real smoke switched dummy → CPAL → software-backed JACK → a changed JACK client profile, all resolving to 48 kHz. Because this host had no `/dev/snd`, MIDI sequencer, or display server, physical audio/MIDI I/O, hardware-negotiated rate change, and OS-window click-through remain explicit environment skips; deterministic 48→24 kHz resampling, JACK/CPAL adapters, headless paint tests, and QML regression tests provide the corresponding automated evidence.

## Carla subprocess hosting discovery

The native QML Carla path supplied the frontend-independent hosting-mode, processor, supervised worker, bounded-log, shared-memory, checkpoint, and status baseline. `EGUI_DRY_WET_AND_CARLA_TRACKS_PLAN.md` composes that baseline into runnable native egui tracks without introducing a second Carla protocol. The native catalog advertises External, Tiny Synth/FX, and feature-dependent Carla Rack/Patchbay/Patchbay16x. The browser catalog advertises only Tiny Synth/FX while preserving shared UI/application mechanics and transactionally rejecting native-only External/Carla sessions.

## Cross-target Tiny Synth/FX processor

The stable processor ID `tiny_synth_fx` and display label **Tiny Synth/FX** identify the dependency-free `tinyviolin 0.1.0` integration. One callback-owned processor handles matched dry/wet audio channels and one MIDI input; MIDI-only zero-audio tracks remain valid. Native and browser compositions share processor state, recorded-take state, routing policy, typed controls, runtime preset discovery, and the embedded egui editor without creating a child window.

| ID | Behavior | Status | Evidence |
| --- | --- | --- | --- |
| TINY-CAP-001 | Cross-target capability and stable identity | Complete | Native/direct-core/WebAudio catalogs, API constraint tests, warning-free native and Wasm checks |
| TINY-DSP-001 | Zero/mono/stereo/arbitrary matched audio with sample-timed MIDI and effects | Complete | Engine processor/backend shape and non-zero audio/MIDI tests, first-active-block allocation guard, worklet render test |
| TINY-STATE-001 | Versioned exact current and recorded-take state | Complete | Strict envelope tests, backend/app/session round trips, transactional malformed-state tests, native dummy restart-style replacement |
| TINY-WEB-001 | AudioWorklet-owned browser processing and bounded typed protocol | Complete | Protocol v5 round trips/coalescing rules, worklet allocation-guarded Tiny processing/snapshot test, browser proxy mapping and Wasm dependency checks |
| TINY-PRES-001 | Embedded editor and capability-driven track controls | Complete | Backend-free editor interaction tests, runtime preset descriptors, stable track-ID window key, no `tinyviolin` dependency in `shoop_egui` |
| TINY-COMPAT-001 | External/Carla/QML and existing session compatibility | In progress | Existing processor fixtures and focused native-FX regressions pass and Tiny uses separate IDs/topology/state while generalized runtime fields retain Carla document representation. The `origin/master` MIDI-keyboard/engine work is integrated; retained QML and the full cross-platform matrix still require validation on the combined commit. |

Tiny implementation evidence current on 2026-08-10 includes hosted Chrome 147 Web Audio and Web MIDI, hosted Firefox 153 Web Audio, Chrome self-contained offline and output-only runs, debug/release web and native packaging, warning-free no-default native/Wasm checks, warning-free default native-FX compilation, and 42/42 native-FX backend tests. After integration, the combined protocol/worklet/backend/engine/app/egui/session no-default-feature suites pass serially, including 610 engine tests and realtime guards, and the no-default-feature product suite passes 23/23. The earlier two timeout-sensitive failures under concurrent Cargo contention did not reproduce. The local all-target/QML gates remain unavailable because Qt is not installed, and the authoritative eight-cell matrix has not yet run for the integrated commit. Therefore the processor is usable and its focused rows are complete, but the cross-target milestone is not closed.

Physical audio/MIDI hardware is not required for deterministic Tiny Synth/FX DSP evidence. Native driver, retained QML, browser artifact, and cross-platform CI evidence must continue to distinguish software coverage from unavailable physical-device click-through.

| ID | Capability or behavior | Current native baseline | Discovery | Current implementation | Evidence |
|---|---|---|---|---|---|
| FX-SUBPROC-001 | Global direct/subprocess policy | QML settings were previously window-owned and Carla was always direct. | Explored for dry/wet/Carla | Complete | Native `carla.hosting_mode` is validated, restart-required, applied before backend construction, and excluded from sessions |
| FX-SUBPROC-002 | One worker generation per Carla chain | Direct hosts share the application process. | Explored for dry/wet/Carla | Complete | Packaged egui executable dispatches hidden worker mode before GUI startup; independent generation/recovery tests and worker handshake pass |
| FX-SUBPROC-003 | Bounded realtime block transfer | Direct Carla ran inline under a callback-visible mutex. | Explored for dry/wet/Carla | Complete | Existing preallocated bounded bridge and realtime/allocation guards serve native egui FX composition unchanged |
| FX-SUBPROC-004 | Checkpoint and click recovery | Session state lives in the live direct host. | Explored for dry/wet/Carla | Complete | Last-confirmed checkpoint fallback, toggle-or-recover, exact current state, and compatible recorded-take restore are application-visible |
| FX-SUBPROC-005 | Worker diagnostics | No per-chain subprocess streams existed. | Explored for dry/wet/Carla | Complete | Capability-driven lifecycle/crash state and bounded per-generation stdout/stderr refresh/copy/clear UI |
| FX-SUBPROC-006 | Pure-egui FX integration | FX chains and dry/wet tracks were deferred. | Explored for dry/wet/Carla | Complete | Native composition, role-aware persistence/media, controls/logs, worker entry, hosting settings, and browser capability rejection are implemented |

Dry/wet/Carla validation on 2026-08-09 passed 200 warning-denying focused unit tests plus the egui executable worker handshake and 1,228 serialized workspace/app-backend tests. Installed-Carla discovery/creation, Linux debug/release native and web builds/packages, dependency scans, Chromium 147 at 360×200 and 900×600, and Firefox 150.0.1 passed. Browser runs explicitly observed the empty disabled Dry + Wet processor surface and transactional External/Carla rejection with continuing callbacks. Hosted PR runs passed all eight Linux/macOS/Windows/Web debug/release egui jobs and all 236 retained QML testcases with zero failures or skips. Final software workflow closure passed a real JACK External dry-send/wet-return chain and installed Carla Rack/Patchbay16x processing in both direct and subprocess modes at 32–1,024 frames with zero deadline misses. This agent environment has no `/dev/snd`, ALSA sequencer, interactive desktop, or native patchbay, so physical-device and desktop click-through remain an explicit environment limitation rather than claimed evidence.

## Milestone-5 persistence and loop-I/O matrix

The completed session/media milestone is specified by the fresh-format contract in `docs/session_format_v1.md` and the implementation/evidence rows below. QML descriptors and tests remain behavior-discovery evidence only: QML-era `.shl`, `session.1`, tar archives, and JSON `.smf` are intentional non-requirements and are rejected without changing the running session.

| ID | Capability or behavior | Retained behavior / new contract | Discovery | M5 target | Current implementation | Replacement evidence |
|---|---|---|---|---|---|---|
| IO-ARCH-001 | Target-neutral persistence boundary | Persistence serializes the authoritative model, never widgets | Explored for M5 | M5 required | Complete | `shoop_session` native/Wasm tests; application mapping; GUI dependency scan |
| IO-FMT-001 | Fresh versioned session container | `.shoop` v1 ZIP64, deterministic JSON manifest, Deflate, hashes | Explored for M5 | M5 required | Complete | Deterministic bit-exact archive round trips and Wasm check |
| IO-FMT-002 | Version/schema rejection and future migration boundary | Unsupported older/future major versions fail before mutation | Explored for M5 | M5 required | Complete | Version/path/resource tests plus application old/future-format rollback |
| IO-FMT-003 | Complete session-scoped state document | Controls, topology, routes, buses, composites, scripts, MIDI control, settings, FX | Explored for M5 | M5 required | Complete | Typed deferred-state fixture and authoritative direct-track model round trip |
| IO-FMT-004 | Exact Carla and recorded FX state | Opaque Carla strings and channel FX-state references are byte-exact | Explored for dry/wet/Carla | Dry/wet/Carla required | Complete | Runnable processed-track save/load preserves Unicode/newline/NUL current/take strings, matching chain references, checkpoint fallback, and compatible restore |
| IO-AUD-001 | Exact compressed session audio | Per-channel little-endian `f32` payloads avoid aggregate codec limits | Explored for M5 | M5 required | Complete | Bit-exact deterministic archive and 300-channel tests |
| IO-AUD-002 | Individual loop audio export | Ordered selected channels; float WAV and `.shoop-audio` on all targets | Explored for M5 | M5 required | Complete | App ordered-selection/WAV/exact test, egui selection paint/routing, browser real-byte round trip |
| IO-AUD-003 | Individual loop audio import | Explicit source-to-destination mapping and optional length adoption | Explored for dry/wet/Carla | Dry/wet/Carla required | Complete | Direct and unequal dry/wet fewer/equal/more/reordered/duplicate mapping tests with role-labeled destinations; browser direct mapping remains unchanged |
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

## Lua scripting and script-created MIDI-control milestone discovery

The native parity contract is frozen in `docs/egui_lua_compatibility_contract.md`. Discovery covers every function and constant installed by the retained session control handler, the shared Lua libraries, the unchanged keyboard and APC Mini scripts, QML script lifecycle/settings, callback payloads, MIDI control ports, autoconnect behavior, and the current native/browser composition boundaries. The completed native milestone remains historical evidence. Its accepted browser omission is superseded by the completed `EGUI_WEB_PORTS_AND_WASM_LUA_PLAN.md` implementation.

| ID | Capability or behavior | Retained baseline / milestone contract | Discovery | Milestone target | Current implementation | Replacement evidence |
|---|---|---|---|---|---|---|
| LUA-ARCH-001 | Frontend-independent Lua ownership | QML wraps one `mlua` state per script; egui target is `shoop_scripting` on the application owner | Explored for Lua milestone | Required | Complete | Native actor owns non-`Send` managers; retained frontend shares environment/print setup; application reducers, settings/session composition, and target-gated GUI/browser dependency scans pass |
| LUA-RUN-001 | Lua 5.4 execution and bundled `require` | Sandboxed execution exposes Shoop print functions and only preloaded Shoop modules | Explored for Lua milestone | Required | Complete | Production sources are embedded/syntax-checked; isolated environment/require/print/control/callback/lifecycle/error tests and bundled end-to-end workflows pass |
| LUA-LIFE-001 | Isolated start/stop/restart/status | Each script can finish or remain listening; destruction removes its callbacks and MIDI rules | Explored for Lua milestone | Required | Complete | Stable IDs, source loading/reload, lifecycle/error snapshots, actor commands, restart/stop/forget, callback/timer/MIDI teardown, and isolation tests pass |
| LUA-API-001 | Loop selector/query surface | Coordinates are `{track,row}`, sync is `{-1,0}`, and getters preserve list shapes/order | Explored for Lua milestone | Required | Complete | All 61 control functions are actually invoked in the frontend-independent table; shapes/order/sync/list/reorder/read-your-writes/invalid selectors pass; retained QML table passes 45/45 |
| LUA-API-002 | Loop mutation/transition surface | Trigger, explicit transition, grab/adopt, gain/balance, selection/target, clear, and repeat-sync | Explored for Lua milestone | Required | Complete | Complete operation table, sentinel/error cases, committed application reducer, GUI/bundled Fake workflows, representative Engine timing, stale-ID handling, and retained QML cases pass |
| LUA-API-003 | Track query/mutation surface | Integer/list selectors include sync `-1`; linear gain, fader, balance, mute, and input controls | Explored for Lua milestone | Required | Complete | Complete selectors/shapes/conversions/clamps/errors, authoritative application/backend operations, APC fader/mute workflow, and retained QML cases pass |
| LUA-API-004 | Global control surface | Fixed cycles, solo, sync, play-after-record, and default record/grab are synchronous queries/mutations | Explored for Lua milestone | Required | Complete | Synchronous shadows plus complete function table, shared GUI reducers, keyboard/APC workflows, callbacks, invalid values, and retained QML cases pass |
| LUA-COMP-001 | Composition API required by APC | APC creates and extends regular compositions serially or in parallel | Explored for Lua milestone | Required | Complete | Coordinator-owned sections execute serially and wrap without global sync, parallel sources start together, state/length/position derive from the active section, and Fake/Engine/APC/session tests pass |
| LUA-EVENT-001 | Loop/global subscriptions | Committed changes produce typed non-reentrant callbacks with retained payloads | Explored for Lua milestone | Required | Complete | All five loop kinds/every field, global/key payloads, cloned non-reentrant registration, duplicate committed-state suppression, queued callback operations, and cross-script failures pass |
| LUA-TIMER-001 | One-shot timers | Script-owned callback fires once after a non-negative delay | Explored for Lua milestone | Required | Complete | Monotonic due order, registration-order ties, zero-timer deferral, 256 callback cap, stop cancellation, callback operations, and error isolation pass |
| LUA-KEY-001 | Keyboard press/release bridge | Qt key/modifier constants, no repeat, and release-sensitive sampler behavior | Explored for Lua milestone | Required | Complete | Retained constants, raw egui translation, typed intents, modifier/repeat/text-entry/focus-loss tests, and production-source navigation/mode/number/target/grab/record-next/overdub/sampler-release command workflow pass |
| LUA-UI-001 | Native Settings Scripts tab | Lifecycle, errors, docs, callbacks/timers, logs, MIDI diagnostics, file actions, and enablement share the one tabbed Settings dialog | Explored for Lua milestone | Required | Complete | Category-tab rendering/actions/logs/help pass at minimum/common sizes; each MIDI rule shows direction/pattern/matched/connected endpoints/latest failure; source scans prove no second Scripts dialog |
| LUA-MIDI-001 | Script-created logical MIDI inputs | Auto-open input connects matching external outputs and forwards exact bytes | Explored for Lua milestone | Required | Complete | Fake and native services, exact-byte delivery, direction/multi-match, callback cap, hotplug, and platform-gated virtual-MIDI tests pass or explicitly skip without host support |
| LUA-MIDI-002 | Script-created logical MIDI outputs | Open/connected callbacks receive `send`; output broadcasts to matching external inputs | Explored for Lua milestone | Required | Complete | Port callbacks, bounded FIFO, two-endpoint broadcast, exact bytes/order, hotplug/reconnect, failures, rate pacing, and teardown pass |
| LUA-MIDI-003 | Full-name regex autoconnect and hotplug | Non-empty patterns are anchored; direction/type filter, all matches, disconnect, and reconnect are explicit | Explored for Lua milestone | Required | Complete | Revisioned discovery snapshots, stable endpoint IDs, anchored patterns, empty/invalid/partial/full cases, duplicate prevention, 500 ms polling, and 250 ms retry tests pass |
| LUA-MIDI-004 | Bounded and rate-limited MIDI | Input/output queues are bounded; requested positive Hz is enforced and failures are observable | Explored for Lua milestone | Required | Complete | Queue/message/callback bounds, strict 99/100 ms fake-clock boundary, no delayed-pump catch-up burst, `0` unthrottled mode, aggregate counters, and per-rule endpoint/failure diagnostics pass |
| LUA-BUNDLE-001 | Production keyboard/APC workflows | Embedded shared sources control authoritative state with no egui script forks | Explored for Lua milestone | Required | Complete | Embedded keyboard and 8×8 APC workflows drive authoritative state; APC proves separate serial/parallel composition paths, one-message positive-rate pumps, reconnect, diagnostics, and cleanup |
| LUA-SET-001 | Machine-wide script settings | Fresh egui keys store bundled toggles and ordered user path/enabled entries; keyboard defaults enabled | Explored for Lua milestone | Required | Complete | Typed list codec/validation/unknown retention, settings drafts and atomic manager, committed add/toggle/remove reconciliation, failed-save no-runtime-change, and rejected-slot/duplicate-name exact path association pass |
| LUA-SESSION-001 | Source-bearing session scripts | `.shoop` stages script source before commit and preserves machine scripts | Explored for Lua milestone | Required | Complete | Exact ID/name/source/enabled round trip, pre-commit syntax rejection, post-commit activation/replacement, sample-rate cancellation rollback, machine/session separation, and production browser active/exact-resave checks pass |
| LUA-PRES-001 | egui Scripts settings tab | Native supports Add, enable, stop, restart, forget, help, status, errors, listening, and MIDI diagnostics; browser supports bundled toggles/status without machine paths | Explored for Lua milestone | Required | Complete | One Settings entry point/window; native retains the full path/action surface, while browser exposes persisted keyboard/APC controls and omits only nonfunctional user-path actions |
| LUA-BUILTIN-001 | Unchanged keyboard script | Every documented key command and release behavior uses the compatibility API | Explored for Lua milestone | Required | Complete | Production embedded source workflow covers navigation/expand/shrink, modes, default action, target, clear, grab, next/overdub, targeted recording, multi-digit cycles, and sampler release |
| LUA-BUILTIN-002 | Unchanged APC Mini MK1 script | Grid/modifiers, globals, faders, LEDs, timer, composition, MIDI I/O, and reconnect all work | Explored for Lua milestone | Required | Complete | Deterministic 8×8 workflow proves modifiers/globals/faders/LEDs/timer, separate serial/parallel composition, one-message rate pumps, reconnect/cleanup; native virtual smoke follows policy and shared N-cycle fix is documented |
| LUA-WASM-001 | Browser Lua with explicit Web MIDI limitation | Production `wasm32-unknown-unknown` uses pure-Rust omniLua while Web MIDI remains out of scope | Re-explored for cross-target ports/Lua | Superseded target | Complete | Hosted/direct-file production artifacts run shared scripting and bundled/session scripts with an intentionally empty MIDI host inventory |
| LUA-SEC-001 | Trusted local extension model | Error containment is required; hostile-code sandbox hardening is not a milestone gate | Explored for Lua milestone | Required subset | Complete | Syntax/runtime/callback failures remain script-local and observable; queue/message bounds and cross-script continuation tests pass; trusted-code model is documented |
## Completed cross-target ports, browser Lua, and omniLua migration follow-up

`EGUI_WEB_PORTS_AND_WASM_LUA_PLAN.md` is the immutable implementation contract and completion ledger for the rows below. Stock vendored `mlua 0.11` failed for `wasm32-unknown-unknown`; pinned omniLua 0.7.1 passes shared native and retained frontend/QML compatibility suites and runs in the production browser composition. Normalized ports, authoritative worklet routes, browser/artifact evidence, and Lua control-port/settings integration are complete. These rows do not claim Web MIDI or native egui real-driver composition.

| ID | Capability or behavior | Current baseline | Discovery | Milestone target | Current implementation | Replacement evidence |
|---|---|---|---|---|---|---|
| XPORT-ARCH-001 | Explicit application-port and host-port inventories | API/backend/worklet snapshots now separate owner-typed application ports, one host inventory, confirmed links, pending links, and failures | Explored for cross-target ports/Lua | Required | Complete | Structural-sharing/API, fake/dummy/application, egui, backend route, and worklet protocol tests pass |
| XPORT-OWNER-001 | Track and Lua-control app-port ownership | Sync/main tracks and active Lua MIDI registrations publish distinct stable owners with tested lifecycle/policy | Explored for cross-target ports/Lua | Required | Complete | Native tests plus production browser global-dialog/settings workflows pass |
| XPORT-EMPTY-001 | App ports remain visible with no host endpoints | Normalized UI preserves track/control ports; browser null MIDI and offline dummy publish zero MIDI hosts | Explored for cross-target ports/Lua | Required | Complete | Native no-host/global-scope tests and hosted/direct-file APC settings evidence pass |
| XPORT-WEB-001 | Browser destination and microphone host endpoints | Negotiated channels publish stable `webaudio:capture_N`/`destination_N` descriptors | Explored for cross-target ports/Lua | Required | Complete | Chrome hosted/direct-file and Firefox hosted production artifacts pass with normalized topology diagnostics |
| XPORT-WEB-002 | Mutable authoritative worklet audio routes | Protocol v3 mutations change real output to silence/non-zero; only worklet snapshots confirm; failures remain nonfatal | Explored for cross-target ports/Lua | Required | Complete | Allocation-free unit/worklet evidence and Chrome/Firefox global-dialog workflows pass |
| XPORT-WEB-003 | Visible initial mapping and route persistence | Defaults are explicit confirmed links; exact-none replacement and legacy migration pass | Explored for cross-target ports/Lua | Required | Complete | Unit migration plus hosted/direct-file save/load with callback continuity pass |
| XPORT-MIDI-001 | Browser MIDI track ports with empty host inventory | MIDI app descriptors remain visible while browser MIDI host count is zero | Explored for cross-target ports/Lua | Required | Complete | Global dialog, settings, hosted/direct-file, and target-neutral tests pass without Web MIDI |
| XPORT-CTRL-001 | Lua-created logical MIDI ports in global connections | Deterministic owner-managed ports, raw-ID hosts, confirmed truth, and lifecycle cleanup are delivered | Explored for cross-target ports/Lua | Required | Complete | Native connected/zero-host tests and hosted/direct-file APC-on/zero-host evidence pass |
| XLUA-RUNTIME-001 | One omniLua Lua 5.4 compatibility implementation | Pinned omniLua 0.7.1 serves shared scripting, production browser, and retained frontend/QML | Explored for cross-target ports/Lua | Blocking prerequisite | Complete | Native 61-function/callback/timer/error suites, QML evidence, and production browser execution pass |
| XLUA-FRONTEND-001 | Retained frontend/QML on omniLua | Frontend engine, conversions, callbacks, ownership, session control, and MIDI bridges use omniLua directly | Explored for omniLua migration | Required | Complete | 33 frontend Rust tests and all Lua-specific retained QML self-tests passed; the historical closure run was 235/236, and the later CPAL test-port fix brings the current offscreen suite to 236/236 with no skips |
| XLUA-REMOVE-001 | Workspace-wide former-runtime removal | Manifests, Rust, metadata, lockfile, trees, release archive, and packaged Wasm scans contain only omniLua's pure-Rust chain | Explored for omniLua migration | Required | Complete | Final source/dependency/package scans and the 1,415-test workspace run pass |
| XLUA-WASM-001 | Browser application-owner scripting | Production hosted/direct-file artifacts run cooperative scripting with `supported == true` and empty native-MIDI dependencies | Explored for cross-target ports/Lua | Required | Complete | Chrome and Firefox production workflows pass |
| XLUA-BUNDLE-001 | Embedded keyboard/APC in hosted and standalone HTML | Packaged Wasm contains both unchanged sources; keyboard defaults on/APC off | Explored for cross-target ports/Lua | Required | Complete | Required-marker scans, authoritative browser keys, and APC-on zero-host workflows pass |
| XLUA-SET-001 | Browser bundled-script settings | Scripts tab has persisted bundled toggles/runtime state and no user-path action | Explored for cross-target ports/Lua | Required | Complete | Hosted/direct-file `localStorage` save/reload/rejection/failure and runtime reconciliation pass |
| XLUA-SESSION-001 | Browser source-bearing session scripts | Browser transaction loads/activates an enabled source script and resaves exact name/source/enabled fields | Explored for cross-target ports/Lua | Required | Complete | Production hosted/direct-file session workflows pass with callback continuity |
| XLUA-KEY-001 | Browser keyboard control independent of audio permission | Real browser key events traverse egui translation and embedded `keyboard.lua` into authoritative selection | Explored for cross-target ports/Lua | Required | Complete | Chrome hosted/direct-file workflow plus focus/text/release unit tests pass |
| XPORT-E2E-001 | Cross-target artifacts and regression closure | Production source/Wasm contains omniLua, scripting, normalized routing, bundled settings, and the empty MIDI service | Explored for cross-target ports/Lua | Required | Complete | Debug/release hosted/standalone artifacts, Chrome/Firefox/direct-file mode matrix, realtime guards, 1,415-test workspace run, retained Lua-specific QML cases, and dependency/package audits passed; the historical CPAL exception was subsequently fixed and the current full QML suite is 236/236 |

## Wasm Web MIDI follow-up

`EGUI_WASM_WEBMIDI_PLAN.md` supersedes the current no-host limitation without rewriting the completed historical cross-target milestone. Direct `web-sys` access is explicitly permission-gated and main-thread-owned. One bounded hub fans physical input to worklet track routes and Lua subscriptions; the AudioWorklet remains the authoritative track router and never accesses browser APIs.

| ID | Capability or behavior | Previous baseline | Discovery | Milestone target | Current implementation | Replacement evidence |
|---|---|---|---|---|---|---|
| WMIDI-PLAT-001 | Explicit browser Web MIDI access and hotplug inventory | Wasm used `NullMidiService` and published zero MIDI hosts | Explored for Web MIDI | Required | Complete | Direct `web-sys` permission/SysEx flow, opaque direction-qualified IDs, state-change refresh, denial/retry, hotplug, deterministic hub tests, and hosted/self-contained Chrome workflows |
| WMIDI-TRACK-001 | Direct-track MIDI recording, monitoring, and output | Worklet created inert dummy MIDI application ports | Explored for Web MIDI | Required | Complete | Protocol v4 endpoint/input/output batches, physical `ExternalMidiPort` staging, authoritative links, allocation-guarded backend/worklet record-monitor-play tests, exact production-browser note-pair round trip |
| WMIDI-CTRL-001 | Browser Lua controller input/output | Browser APC could run only with zero endpoints | Explored for Web MIDI | Required | Complete | Wasm `MidiControlService` adapter, shared canonical host rows, unchanged APC authoritative solo/LED workflow, rate-limited output, hotplug reconnect, and audio-independent access |
| WMIDI-BOUND-001 | Bounded timing, payload, and failure contract | No browser MIDI event transport existed | Explored for Web MIDI | Required | Complete | `docs/web_midi_contract.md`; 256 subscriptions, 1,024-message queues, 128-message batches, 256-event per-track staging, 4-byte track/256-byte control limits, refusal/drop diagnostics, next-quantum timing, stale-input nonfatal recovery, realtime guards |
| WMIDI-PERSIST-001 | Desired route persistence through missing devices | Browser sessions contained no physical MIDI IDs | Explored for Web MIDI | Required | Complete | Canonical route capture/load, missing-endpoint replacement/reconnect backend test, production browser save/load/playback, hotplug and worklet-generation route recovery |
| WMIDI-E2E-001 | Production artifact and browser closure | Browser artifacts intentionally had no physical MIDI | Explored for Web MIDI | Required | Complete | Debug/release hosted and self-contained Chrome track/control/session/hotplug/restart workflows pass, including denial/retry, generation-safe open-failure removal/reopen, send-failure visibility, saturation recovery, and callback continuity; 1,204-test workspace, 236/236 QML, Firefox no-host regression, package/import/dependency scans, and debug/release artifacts pass |

## Persistent egui settings discovery

The completed persistent-settings implementation and `docs/settings_format_v1.md` define the fresh application-preference slice. Discovery covers the existing `shoop_settings`/QML settings separation, egui composition and menu/widget boundaries, Add Track defaults, the cross-target product runner, `directories` platform paths, browser origin storage, and the versioning pattern established by `shoop_session`. The replacement deliberately does not import the QML `settings.1` format. Machine/user preferences remain separate from `.shoop` session state.

| ID | Capability or behavior | Retained/new contract | Discovery | Milestone target | Current implementation | Replacement evidence |
|---|---|---|---|---|---|---|
| SET-ARCH-001 | Explicit settings registration near consumers | QML settings are window-owned; new consumers register typed definitions during composition | Explored for settings | Settings required | Complete | Add Track keys/definitions/reads are colocated; explicit builder aggregation and duplicate/type/default tests |
| SET-ARCH-002 | Presentation/persistence separation | `shoop_egui` remains free of filesystem and browser APIs | Explored for settings | Settings required | Complete | Feature-isolated `shoop_settings`; source/tree scans and native/Wasm warning-denying checks |
| SET-FMT-001 | Fresh egui settings identity | No QML-format compatibility is required | Explored for settings | Settings required | Complete | `docs/settings_format_v1.md`, distinct path/key, and QML-document rejection fixture |
| SET-FMT-002 | Version checks and migration dispatch | Envelope-first format/major/minor/document checks and ordered pure migrations | Explored for settings | Settings required | Complete | Full format/document version corpus and ordered/failing migration harness |
| SET-FMT-003 | Missing, invalid, and unknown values | Registered defaults, typed warnings, and unknown same-version key retention | Explored for settings | Settings required | Complete | Deterministic codec, invalid/default diagnostic, and unknown-value manager round trips |
| SET-API-001 | Typed immutable access | Stable `SettingKey<T>` reads from revisioned snapshots | Explored for settings | Settings required | Complete | Typed getter/mismatch, draft, default, ordering, and revision tests |
| SET-NATIVE-001 | Standard native config location | One `ProjectDirs` identity across Linux, Windows, and macOS | Explored for settings | Settings required | Complete | Path identity test, displayed resolved location, and cross-platform-targeted CI matrix |
| SET-NATIVE-002 | Transactional native write | Same-directory temporary file, flush, atomic replace, no publish on failure | Explored for settings | Settings required | Complete | Commit-failure/no-clobber/temp-cleanup tests and composition restart round trip |
| SET-WEB-001 | Browser-origin persistence | Stable `localStorage` key for hosted/self-contained Wasm | Explored for settings | Settings required | Complete | Hosted and direct-file Chrome real save/reload plus unavailable/set-failure automation |
| SET-ERR-001 | Observable load/save/recovery state | Defaults keep the app usable; rejected input is not silently overwritten | Explored for settings | Settings required | Complete | Manager/dialog tests and Chrome invalid/future/unavailable/failure workflows |
| SET-PRES-001 | Main-menu settings dialog | One registry-generated, resizable, category-tabbed dialog with Save/Cancel/reset/help/effect state | Explored for settings | Settings required | Complete | One-window source scan, tab routing, stable draft/save/reset tests, complete native Scripts tab, diagnostics, and 360×200/900×600 paint |
| SET-USE-001 | Default new-track audio channels | Existing Add Track draft starts at stereo | Explored for settings | Settings required | Complete | Native next-open/open-draft test and hosted/direct-file reload-to-consumer automation |
| SET-USE-002 | Default new-track MIDI state | Existing Add Track draft starts with MIDI disabled | Explored for settings | Settings required | Complete | Native next-open/open-draft test and hosted/direct-file reload-to-consumer automation |
| SET-E2E-001 | Cross-target persistence | Native and browser reload use real persisted text and authoritative consumers | Explored for settings | Settings required | Complete | Temporary-path native restart and hosted/direct-file product artifact workflows |
| SET-OLD-001 | Retained QML isolation | Existing QML settings/Carla path remains independently regression-tested | Explored for settings | Settings required | Complete | Legacy feature tests, fresh-format rejection, and retained QML final gate |

Generic MIDI-rule editing and session-local overrides remain assigned to their owning milestones. Runnable native Carla/FX settings are complete; the browser supports the built-in Tiny Synth/FX processor but native plugin hosting remains intentionally unavailable. Native driver/device selection is complete under `EGUI_NATIVE_AUDIO_DRIVER_SWITCHING_PLAN.md`. Bundled Lua startup toggles are cross-target; native alone registers user-script paths, which browser preserves as unknown values.

## Completed click-track generation discovery

`EGUI_CLICK_TRACK_GENERATION_PLAN.md` is the completed immutable implementation contract for this slice. Discovery covered `ClickTrackDialog.qml`, its `LoopWidget.qml` context-menu wiring, the Rust QML generator/bridge, the four repository click WAVs, and the replacement's existing session-media transaction, loop context menu, native/browser composition, and package boundaries. The implementation preserves the visible legacy contract while applying the explicitly approved fractional-BPM and MIDI velocity-127 corrections.

| ID | Capability or behavior | Retained baseline / milestone contract | Discovery | Milestone target | Current implementation | Planned evidence |
|---|---|---|---|---|---|---|
| CLICK-ENTRY-001 | Primitive loop context entry and kind applicability | **Click loop...** targets sync/main primitive loops and limits Audio/MIDI by channel shape | Explored for click tracks | Click-track required | Complete | Primitive audio/MIDI applicability, composite/channel-less omission, stable LoopId propagation through sync/main widget paths, and dialog target tests pass |
| CLICK-CAT-001 | Built-in click catalog and defaults | Sorted installed WAV stems; repository defaults resolve to `click_high` primary and `click_low` secondary | Explored for click tracks | Click-track required | Complete | Four embedded WAVs decode in native/Wasm target-neutral tests; application publishes the stable sorted catalog; dialog selectors/default/reconciliation tests pass |
| CLICK-TIME-001 | Tempo, count, pattern, and odd-click timing | Primary plus N secondary clicks cycles at clicks/minute; odd clicks receive 0–100% interval delay | Explored for click tracks | Click-track required | Complete | Checked shared timing tests cover fractional BPM, exact floor duration/starts, 0/50/100% delay, zero/NaN/overflow, and fixed frame/click limits |
| CLICK-AUD-001 | Generated audio loop media | First source channel is resampled, mixed/truncated, copied to every target audio channel, and adopts loop length | Explored for click tracks | Click-track required | Complete | Deterministic 44.1/48 kHz generation, fake/native/worklet transactions, all-channel/offset/opposite-media/stable-ID tests, exact export and save/load, and Chrome/Firefox production playback pass |
| CLICK-MIDI-001 | Generated MIDI loop media | Note-on/off clicks are copied to every target MIDI channel and adopt loop length | Explored for click tracks | Click-track required | Complete | Exact note/order/boundary generation, fake/native/worklet transactions, velocity 127/offset/opposite-media/save-load tests, and Chrome/Firefox exact exported-byte workflows pass |
| CLICK-FILL-001 | Fill current loop length | Derives BPM from current loop frames, click count, and backend sample rate | Explored for click tracks | Click-track required | Complete | Loop length is plain snapshot data; egui tests prove 120 BPM fitting plus zero-length/rate/count disabled reasons and fractional precision |
| CLICK-PREV-001 | Non-mutating audio preview | Audio draft can be heard without loading the target loop | Explored for click tracks | Click-track required | Complete | Capacity-one application/native queues, request generations, no-mutation/stale/error tests, Chrome/Firefox active-context success, and self-contained offline fallback-context success pass; native audible output is an explicit no-device environment skip |
| CLICK-TXN-001 | Transactional target-loop replacement | Replacement must preserve stable IDs, unrelated media/session state, and fail without partial mutation | Explored for click tracks | Click-track required | Complete | Capture/prepare/replace/remap passes mixed-content, all-channel, sync/main, conflict/recording, injected failure/no-mutation, stable-ID, native `NativeBackend`, worklet, exact export, and save/load workflows |
| CLICK-XTARGET-001 | Native/browser resources, preview, and artifacts | Legacy is native/resource-directory based; replacement must compile assets into every product target | Explored for click tracks | Click-track required | Complete | Debug/release native, hosted, and self-contained packages verify all four compiled markers; native workflow, Chrome/Firefox/self-contained production runs, import inspection, and forbidden-dependency scans pass |

## egui MIDI piano discovery

`EGUI_MIDI_PIANO_PLAN.md` is the completed implementation contract for this egui-only slice. The piano is an application input source rather than a physical host endpoint: presentation emits typed key lifecycle actions, the application selects monitored role-bearing MIDI tracks, and each backend stages bounded messages into ordinary track input ports with accepted process-iteration timing.

| ID | Capability or behavior | New contract | Discovery | Milestone target | Current implementation | Planned evidence |
|---|---|---|---|---|---|---|
| PIANO-ENTRY-001 | Bottom piano pane | **piano** sits beside **details** and selects one resizable bottom pane | Explored for MIDI piano | MIDI piano required | Complete | Backend-free open/close/switch/no-stack tests, details paint retention, and minimum/common product paint pass |
| PIANO-RANGE-001 | Full-range piano geometry | Scrollable notes 0–127, C-1 through C9 labels, and initial MIDI 60/C4 centering | Explored for MIDI piano | MIDI piano required | Complete | Endpoint/C-label/black-hit geometry, actual C4 viewport center and retained offset, horizontal overflow, active paint, and 360×200/900×600 evidence pass |
| PIANO-FANOUT-001 | Monitored MIDI-track fanout | Send once to every input-monitored track owning a role-bearing MIDI input, independent of selection/routes | Explored for MIDI piano | MIDI piano required | Complete | Snapshot destination summary, exact app intents, fake direct/processed eligibility, engine two-track exact recording, and native product recording pass without host links |
| PIANO-LIFE-001 | Paired held-note lifecycle | Fixed channel-1 velocity-100 press and zero-velocity release follow original recipients through eligibility changes | Explored for MIDI piano | MIDI piano required | Complete | Duplicate/changed-eligibility/partial-failure/release-all policy plus pointer leave/release/gone, focus loss, pane switch, driver transition/load reset, and shutdown cleanup pass |
| PIANO-XTARGET-001 | Driver-independent track input staging | Native dummy/JACK/CPAL and browser Web Audio/offline ingest bounded side-injected MIDI at frame zero of the next available engine process iteration, without physical MIDI/Web MIDI or a hard realtime latency guarantee | Explored for MIDI piano | MIDI piano required | Complete | Shared dummy/external/native staging; dummy/JACK-test/CPAL-test injection; protocol-v5 no-endpoint worklet recording; native product exact recording; browser self-test callback lifecycle; Trunk/self-contained/package and dependency-isolation checks pass, with runtime browsers explicitly unavailable |

## Coarsely listed future areas

These broader future areas remain outside the currently explored milestone contracts and must be expanded before their own milestones set acceptance criteria:

| Area | Discovery | Implementation |
|---|---|---|
| Runtime reconnect policy and runnable bus topology | Partially explored | Deferred |
| Dry/wet topology and FX-chain hosting/state management | Explored for dry/wet/Carla | Implemented and validated under `EGUI_DRY_WET_AND_CARLA_TRACKS_PLAN.md` |
| Composite-loop creation, scheduling, editing, and nesting beyond the Lua-required append path | Partially explored | Deferred |
| Generic MIDI control configuration, learning, filtering, and non-script rule editor | Unexplored | Deferred |
| Monitoring, profiling, logging, crash/developer tools, and first-run UX | Unexplored | Deferred |
| Packaging, installation, and platform integration after Qt removal | Unexplored | Deferred |
