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
- `Deferred`: explicitly outside milestone 1, but not outside the project.

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

These sources do not exhaustively specify even the milestone subset. Stage 1 of the milestone plan must continue discovery and add more precise test references where behavior is subtle.

## First-milestone matrix

| ID | Capability or behavior | Old application baseline | Discovery | M1 target | Current implementation | Replacement evidence |
|---|---|---|---|---|---|---|
| ARCH-001 | Pure native egui process | Production startup creates a Qt application and QML engine; the prototype is a Qt-hosted egui canvas. | Explored for M1 | Required | Prototype through Qt | Pending |
| ARCH-002 | Presentation/business/backend separation | QML widgets currently own substantial session and control behavior. | Explored for M1 | Required | Partial — API/presentation boundary exists; application/backend layers remain | `shoop_app_api` has no dependencies; `shoop_egui` depends on it but not the engine/backend |
| ARCH-003 | Stable entity identity | QML uses object IDs plus coordinates; the egui prototype routes actions by track and loop indices. | Explored for M1 | Required | Complete | `shoop_app_api::tests::ids_retain_raw_identity_and_invalid_is_distinct` and stable-ID routing in `TracksWidget` |
| ARCH-004 | Immutable snapshot and typed intent flow | The prototype has plain state/actions but receives snapshots and emits actions through QObject adapters. | Explored for M1 | Required | Partial — framework-independent snapshot/intent types and GUI routing are complete; actor publication remains | `shoop_app_api::tests::intents_preserve_stable_ids_and_selection_modifiers`; `cargo test -p shoop_egui` |
| ARCH-005 | Backend-free egui preview | No standalone preview executable currently supplies mock application snapshots. | Explored for M1 | Required | Not started | Pending |
| SHELL-001 | Existing egui application shell | Current `AppWidget` includes global controls, tracks, details, logo, and backend status. | Explored for M1 | Required | Existing widget | Pending |
| SHELL-002 | Logo, version, DSP, xrun, buffer, and latency display | QML and the prototype show these live values. | Explored for M1 | Required | Prototype through Qt | Pending |
| LAYOUT-001 | Horizontal track columns with vertical loop stacks | QML places tracks in horizontally scrollable columns and loops in aligned vertical slots. | Explored for M1 | Required | Partial — the existing widget has horizontal columns and vertical stacks | Pending |
| LAYOUT-002 | Track controls remain aligned below the loop viewport | QML renders controls in a separate row below the vertically scrollable loop area. | Explored for M1 | Required | Partial — existing controls are inside each scrolling track card | Pending |
| LAYOUT-003 | Track header and editable title | QML has a title field at the top of each main track; the egui prototype has an editable title. | Explored for M1 | Required | Existing widget | Pending |
| LAYOUT-004 | Sync track has a distinct fixed area and limited presentation | QML renders one non-editable sync track separately from main tracks. | Partially explored | Required subset | Partial — the prototype treats supplied tracks uniformly | Pending; exact sizing may be visually adapted while preserving distinction |
| LAYOUT-005 | Horizontal and vertical overflow remain usable | QML separates horizontal track scrolling from vertical loop scrolling. | Explored for M1 | Required | Partial — the existing widget uses one combined scroll area | Pending |
| LAYOUT-006 | Add-track and add-loop affordances occupy QML-like positions | QML places add-track after the track columns and add-loop below each main track. | Explored for M1 | Required | Not started | Pending |
| TRACK-001 | Add Track dialog opens from the add-track button | QML opens a modal Add Track dialog with a generated default name. | Explored for M1 | Required | Not started | Pending |
| TRACK-002 | Create regular/direct tracks | QML can create direct tracks with configurable audio channels and optional MIDI. | Explored for M1 | Required | Not started | Pending |
| TRACK-003 | Direct-track audio choices | QML offers disabled, mono, stereo, and custom 0–10 audio channels; stereo is the initial choice. | Explored for M1 | Required | Not started | Pending |
| TRACK-004 | Direct-track MIDI choice | QML offers an optional direct MIDI channel. | Explored for M1 | Required | Not started | Pending |
| TRACK-005 | New-track naming and stable port-name base | QML defaults to `Track N`; the accepted name determines the initial port-name base and later title edits do not rename ports. | Explored for M1 | Required | Not started | Pending |
| TRACK-006 | New track receives aligned empty loop slots | QML creates at least eight slots and no fewer than the current maximum row count. | Explored for M1 | Required | Not started | Pending |
| TRACK-007 | Dry/wet Add Track choices | QML supports external and Carla processing with dry/wet audio/MIDI topology. | Partially explored | Deferred | Not started | Later FX/topology milestone |
| TRACK-008 | Trigger-only Add Track choice | QML offers a trigger-only track type intended for composite/script control. | Partially explored | Deferred | Not started | Later composite milestone |
| TRACK-009 | Track title editing | Finishing an edit updates the track name but not its port names. | Explored for M1 | Required | Prototype through Qt | Pending |
| TRACK-010 | Output gain and stereo balance | Applicable audio output controls update the track's output ports. | Explored for M1 | Required | Prototype through Qt | Pending |
| TRACK-011 | Output mute | Mute affects track outputs and is reflected in the control state. | Explored for M1 | Required | Prototype through Qt | Pending |
| TRACK-012 | Input gain and stereo balance | Applicable audio input controls update track input ports. | Explored for M1 | Required | Prototype through Qt | Pending |
| TRACK-013 | Input monitoring/mute | The monitor control changes input passthrough without preventing recording. | Explored for M1 | Required | Prototype through Qt | Pending |
| TRACK-014 | Audio level and MIDI activity display | QML and the prototype aggregate applicable port activity into track controls. | Explored for M1 | Required | Prototype through Qt | Pending |
| TRACK-015 | Hide inapplicable controls | Audio gain/balance controls are absent or disabled when a track has no applicable channels. | Explored for M1 | Required | Existing widget | Pending |
| TRACK-016 | Track reordering and width resizing | QML supports drag reordering and per-track width adjustment. | Partially explored | Deferred | Not started | Later layout-management milestone |
| TRACK-017 | Track deletion and track context menu | QML track options include connections, deletion, and FX state actions. | Partially explored | Deferred | Existing widget — inert menu affordance | No context menus in M1 |
| LOOP-001 | Add Loop button creates a backend-capable empty loop | QML clones the track's channel shape and port wiring into a new loop slot. | Explored for M1 | Required | Not started | Pending |
| LOOP-002 | Add Loop preserves aligned rows | Adding from a longest track extends tracks that were one row shorter so the grid remains aligned. | Explored for M1 | Required | Not started | Pending |
| LOOP-003 | Loop names and generated slot labels | New loops receive generated labels such as `(N)` and render generated labels distinctly. | Explored for M1 | Required | Partial — rendering exists; creation is not started | Pending |
| LOOP-004 | Mode, emptiness, progress, and queued-transition rendering | Loop color, icon, progress, and transition indicator follow live loop state. | Explored for M1 | Required | Prototype through Qt | Pending |
| LOOP-005 | Sync, selection, target, and composite highlighting | Borders and icons identify these states. | Explored for M1 | Required subset | Prototype through Qt | Composite creation/editing remains deferred |
| LOOP-006 | Audio level and MIDI activity display | Loop widgets show mono/stereo levels and MIDI activity when applicable. | Explored for M1 | Required | Prototype through Qt | Pending |
| LOOP-007 | Play action | Hover control requests normal playback and follows application sync/selection/solo policy. | Explored for M1 | Required | Prototype through Qt | Pending |
| LOOP-008 | Record action | Hover control requests normal recording and follows fixed-cycle and play-after-record policy. | Explored for M1 | Required | Prototype through Qt | Pending |
| LOOP-009 | Stop action | Hover control requests stop and follows application sync/selection policy. | Explored for M1 | Required | Prototype through Qt | Pending |
| LOOP-010 | Loop gain | The existing egui gain control updates applicable playback channel gain. | Explored for M1 | Required | Prototype through Qt | Pending |
| LOOP-011 | Selection by state-icon click | QML toggles or replaces selection according to modifiers; selected loops participate in grouped transitions. | Partially explored | Required subset | Prototype through Qt | M1 must support ordinary single selection and additive/toggle selection from egui modifiers |
| LOOP-012 | Targeting by state-icon double-click | QML maintains at most one targeted loop and uses it as an alternate transition/recording sync source. | Partially explored | Required subset | Prototype through Qt | M1 requires target state and transition synchronization used by existing play/record/stop actions; advanced grab behavior is deferred |
| LOOP-013 | Solo-within-track behavior | With solo enabled, play/record actions stop other applicable loops in the affected track. | Explored for M1 | Required | Prototype through Qt | Pending |
| LOOP-014 | Dry playback and dry-to-wet recording controls | QML dry/wet loops expose orange play-dry and re-record controls. | Partially explored | Deferred | Not present in current egui widget | Later dry/wet milestone |
| LOOP-015 | Grab control and behavior | QML supports always-on-ringbuffer capture with sync, fixed-cycle, target, and play-after-record policy. | Partially explored | Deferred | Current egui widget has no grab button | Later loop-control milestone |
| LOOP-016 | Stereo loop balance control | QML exposes balance in addition to loop gain for stereo loops. | Partially explored | Deferred | Current egui widget exposes gain only | Later loop-control milestone |
| LOOP-017 | Loop context menu and its dialogs | QML provides clear, load/save, click-track, details, composition, and other actions. | Partially explored | Deferred | Not started | No context menus in M1 |
| LOOP-018 | Loop drag reordering/moving | QML supports loop drag/drop within a track and related coordinate updates. | Partially explored | Deferred | Not started | Later layout-management milestone |
| GLOBAL-001 | Stop all | Stops running loops and respects current sync policy. | Explored for M1 | Required | Prototype through Qt | Pending |
| GLOBAL-002 | Deselect all | Clears loop selection. | Explored for M1 | Required | Prototype through Qt | Pending |
| GLOBAL-003 | Clear menu actions | Existing egui menu emits clear-recordings/all variants including or excluding sync. | Explored for M1 | Required | Prototype through Qt | M1 executes accepted action without adding a confirmation dialog |
| GLOBAL-004 | Default record/grab preference | Existing egui control edits application state used by default-trigger behavior. | Partially explored | Required subset | Prototype through Qt | Control/state required; dedicated grab button remains deferred |
| GLOBAL-005 | Play after record | Toggle affects recording completion and control rendering. | Explored for M1 | Required | Prototype through Qt | Pending |
| GLOBAL-006 | Sync mode | Toggle determines immediate versus synchronized loop actions. | Explored for M1 | Required | Prototype through Qt | Pending |
| GLOBAL-007 | Solo mode | Toggle determines whether sibling loops stop on play/record. | Explored for M1 | Required | Prototype through Qt | Pending |
| GLOBAL-008 | Fixed recording cycles | Numeric control sets infinite or N-cycle recording behavior. | Explored for M1 | Required | Prototype through Qt | Pending |
| GLOBAL-009 | Main menu | QML opens connections, session I/O, monitoring, profiling, settings, and developer surfaces. | Partially explored | Deferred | Inert egui affordance | No main-menu implementation in M1 |
| DETAILS-001 | Details pane selection | Existing egui pane follows the selected loop and handles no selection. | Explored for M1 | Required | Prototype through Qt | Pending |
| DETAILS-002 | Audio waveform display | Existing egui waveform renders selected-loop audio data, offsets, loop regions, and play position. | Explored for M1 | Required | Prototype through Qt | Pending |
| DETAILS-003 | Advanced details editing | QML details windows edit preplay, offsets, MIDI, and composites. | Partially explored | Deferred | Not present in current egui pane | Later details/editing milestone |
| DIALOG-001 | Only Add Track is implemented as a dialog | QML has many dialogs; milestone 1 requests only Add Track. | Explored for M1 | Required | Not started | Verify no other new dialog can open |
| MENU-001 | No track or loop context menus | QML has both; milestone 1 explicitly excludes them. | Explored for M1 | Deferred | Existing widget — track affordance is inert and loop context is absent | Verify right-click and track menu affordance cause no action |
| BACKEND-001 | Create direct track ports, loops, and channels | QML descriptor generation plus QObject wrappers constructs corresponding engine entities and wiring. | Explored for M1 | Required | Not started | Pending |
| BACKEND-002 | Poll loop, channel, port, and driver state | QObject update code currently converts state mirrors into QML properties and prototype snapshots. | Explored for M1 | Required | Partial — native Rust state APIs exist; replacement integration is not started | Pending |
| BACKEND-003 | Dummy-backend deterministic operation | Existing tests use a dummy backend for headless behavior. | Explored for M1 | Required | Partial — engine support exists without replacement contract tests | Pending replacement contract tests |

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
