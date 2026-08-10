# egui MIDI piano plan

## Status and document role

Status: **In progress**.

Approved clarifications:

- Side-injected piano MIDI has a soft timing requirement. Driver independence is mandatory, but ingestion may occur once per engine process iteration at frame zero; no sample-exact or hard realtime delivery guarantee is required.
- The feature and required build validation are egui-only. Native build testing may be limited to `shoopdaloop_egui --no-default-features`, so Qt, QML, Lilv, and Carla/native-FX dependencies are not prerequisites for this milestone.

This is the implementation contract for adding an on-screen MIDI piano to `shoopdaloop_egui`. It must remain synchronized with:

- `EGUI_FEATURE_PARITY_MATRIX.md` for capability status and verification evidence;
- `EGUI_REPLACEMENT_PROJECT.md` for architecture and coarse project status;
- other current plans whose limitations or evidence are affected.

Completed plans remain historical ledgers; update stale current statements without rewriting their frozen scope or evidence.

## Investigation findings

- `shoop_egui::AppWidget` currently has a 24-pixel bottom toggle strip containing only **details**, and a resizable bottom `DetailsPane`. Presentation receives immutable `AppState` and emits `AppIntent`; this boundary is suitable for a second pane but not for direct backend access.
- `TrackState` exposes `controls.input_monitoring` (`true` means input mute is off) and stable `port_ids`. `AppState.connections.application_ports` identifies track ownership, MIDI type, input direction, and `MidiInput` role, so both the pane and application can derive eligible destinations without relying on physical host connections.
- `shoop_app` owns logical-to-backend track IDs and serial intent handling. It is the correct authority for recomputing destinations and preserving note-on/note-off ownership when track monitoring or topology changes while a note is held.
- The shared `Backend` trait has no track-input injection operation. The underlying native `MidiPort::dummy_queue_msgs` already stages data into an `ExternalMidiPort` through the control queue even for real driver ports, but its API name and backend exposure are test-specific. `EngineBackend` can likewise stage an event directly into a track's external MIDI input.
- Browser physical MIDI currently travels through a host-endpoint route (`PushMidiInput`) before the AudioWorklet stages it. The piano must use a separate track-targeted worklet command so it does not require Web MIDI permission, invent a host endpoint, change connection state, or depend on a physical route.
- The application command queue, worklet transport, and MIDI input staging are bounded. Piano events should use these existing control paths. The side-injected messages may be transferred from control state and ingested only once per engine process iteration, at frame zero of that iteration; GUI timing is intentionally neither sample-exact nor subject to a hard realtime delivery deadline.

## Goals and scope

Add a **piano** button beside **details** and a resizable piano pane containing a playable, horizontally scrollable keyboard for all 128 MIDI notes. Route its MIDI simultaneously into every currently input-monitored track that owns a MIDI input port, through the same input-port processing used by external events, on native and browser builds regardless of active audio/MIDI driver.

In scope:

- reusable piano rendering and pointer interaction in `shoop_egui`;
- plain piano actions in `shoop_app_api` and application-owned destination/lifecycle policy in `shoop_app`;
- direct track-MIDI-input staging in fake, dummy/offline, native JACK/CPAL+midir, Web Audio, and AudioWorklet backend paths;
- focused backend/application/UI tests, native and production-browser workflows, documentation, and plan-ledger maintenance.

Out of scope:

- QML/frontend UI, behavior, build, or self-test changes; this milestone is exclusively for the `shoopdaloop_egui` product and its reusable Rust dependencies;
- computer-keyboard-to-note mapping, MIDI learn, sustain/pitch/modulation controls, channel or velocity configuration, and MIDI output routing;
- changing physical MIDI connection state, creating a virtual host port, or persisting piano state in sessions/settings;
- sample-exact GUI event timing, a hard realtime injection-latency guarantee, or multi-touch support.

## Immutable acceptance criteria

1. The egui bottom toggle strip shows **details** and **piano** controls. **piano** opens/closes the piano pane, switching panes does not stack them, and existing details selection/waveform behavior remains functional.
2. The pane renders a recognizable piano with white and black keys for the complete MIDI note range `0..=127`. It is horizontally scrollable to both endpoints and initially centers middle C (MIDI 60, C4) in the available viewport.
3. Every C key has a small, legible scientific-pitch marking using MIDI note 0 = C-1; middle C is marked C4. Labels and hit regions remain correct at the truncated high end of the MIDI range.
4. Pointer press on a key emits one channel-1 note-on (`0x90`, note, velocity 100); release emits its matching note-off (`0x80`, note, velocity 0). Held keys have visible feedback, duplicate frame events are suppressed, black keys win overlapping hit tests, and leaving the key/pane, closing or switching the pane, pointer cancellation, and focus loss cannot leave a GUI-held note without cleanup.
5. At note-on time, eligible destinations are exactly tracks whose current `input_monitoring` is true and which own an application port with MIDI data type, input direction, and `MidiInput` role. This includes any sync, direct, or processed track meeting those conditions and excludes muted, stale, port-less, output-only, Lua-control, and audio-only entries.
6. One piano event is delivered once to every eligible track simultaneously. No eligible track is privileged by selection, ordering, targeting, solo state, physical MIDI connection, or active driver; no ineligible track receives note-on.
7. Note-off and release-all cleanup reach the tracks that received the corresponding note-on even if monitoring, port inventory, pane visibility, or ordering changes while the note is held. Repeated release and stale-track cases are harmless and do not block cleanup of other destinations.
8. Injected messages enter each destination's MIDI input port before that port's ordinary processing, as external input would. It is acceptable to transfer and ingest pending side-injected messages only once per engine process iteration, at frame zero of that iteration. Existing monitoring, recording, passthrough, loop capture, MIDI activity, and downstream processor behavior apply unchanged; the implementation does not write directly to loop media or track outputs.
9. Injection is driver-independent: the feature works with native dummy/offline, JACK, and CPAL+midir compositions and with browser Web Audio and explicit offline mode. Browser use does not require Web MIDI access; native use does not require a configured physical MIDI input or a driver-specific injection implementation.
10. Fanout and transport are bounded. Invalid notes/payloads are rejected, per-destination failure does not prevent attempts to remaining destinations, and failures are observable. Submission and delivery may use ordinary bounded control-side synchronization and process-iteration batching; they need not be lock-free or meet a hard realtime latency bound. Existing realtime processing must remain bounded, and no blocking wait or unbounded work may be added to it.
11. This is an egui-only feature. `shoop_egui` remains presentation-only and browser-compatible; application policy remains in `shoop_app`; driver/worklet mutation remains behind `shoop_backend` and the bounded audio protocol. No QML/frontend dependency or target-specific API enters the reusable widget/API crates, and no QML build or self-test is required for acceptance.
12. Planning and user documentation accurately describe target eligibility, fixed MIDI semantics, default centering, driver independence, timing, and validation evidence when the feature closes.

## Design rules and constraints

- Represent UI output as piano semantics (`press`, `release`, and `release all`), not arbitrary backend bytes or backend IDs. Encode the fixed channel-1 note messages at the application/backend boundary.
- Make `shoop_app` authoritative twice: the pane may display the eligible track names/count from the snapshot, but the application must revalidate current track controls and role-bearing ports before note-on fanout.
- Track note-on recipients by logical `TrackId` in application state. Resolve current backend IDs when staging cleanup so driver/session replacement and stable-ID remapping do not move policy into the GUI.
- Ignore an already-held note-on, make note-off/release-all idempotent, and use per-note release cleanup when the pointer or pane lifecycle cannot identify a normal release. Keep velocity 100 and zero-velocity note-off fixed unless the user separately expands scope.
- Add a driver-neutral incoming-MIDI operation to the backend contract. Reuse `ExternalMidiPort` staging and the native session control queue rather than simulating host links or bypassing port processing.
- Add a distinct bounded AudioWorklet protocol command addressed by backend track ID. Do not overload browser `PushMidiInput`, because that command intentionally requires a configured physical host endpoint and confirmed links.
- Preserve existing finite limits while treating delivery latency as soft. A bounded mailbox/control queue may batch side-injected events and drain them once per engine process iteration; no driver-specific fast path or sample-offset guarantee is required. Validate three-byte note messages, continue fanout after individual failures, and aggregate diagnostics rather than producing an error storm.
- Keep bottom-pane selection and scroll/held-key state as transient `AppWidget`/`PianoPane` state. Do not serialize it or reset the user's scroll position on every snapshot refresh.
- Share one geometric source of truth for painting, C labels, hit testing, initial centering, and tests. Paint white keys before black keys and resolve black-key hits first.
- Keep QML/frontend untouched and outside the required validation surface. Use `shoopdaloop_egui --no-default-features` for the authoritative native build so validation does not require Qt, Lilv, or Carla/native-FX.

## Staged implementation plan

Dependencies are sequential unless stated otherwise. Complete, verify, document, and commit each stage before beginning its dependent stage.

### Stage 0 — Freeze the API and piano interaction contract

- [x] Add target-neutral piano action types and an `AppIntent` variant in `shoop_app_api`, including explicit press, release, and release-all semantics with MIDI-range validation.
- [x] Define and test keyboard geometry for notes 0–127, white/black placement, C octave labels (C-1 through C9), hit-test precedence, fixed channel/velocity bytes, and the middle-C centering target.
- [x] Add planned piano capability rows to `EGUI_FEATURE_PARITY_MATRIX.md`, mark this slice in `EGUI_REPLACEMENT_PROJECT.md`, and audit current plan cross-references when execution starts.

Verification:

- [x] `cargo test -p shoop_app_api -p shoop_egui` passes 85 tests covering range endpoints, all C labels, MIDI 60/C4, black-key overlap, deterministic geometry, and initial center calculation; lifecycle interaction coverage remains assigned to Stage 3.
- [x] `cargo check -p shoop_egui --target wasm32-unknown-unknown` confirms the contract/geometry remains target-neutral.
- [x] `cargo fmt --all` and `RUSTFLAGS="-D warnings" cargo check -p shoop_app_api -p shoop_egui -p shoop_app` pass before the contract milestone commit; the milestone build is `RUSTFLAGS="-D warnings" cargo build -p shoopdaloop_egui --no-default-features`.

### Stage 1 — Add driver-independent track input injection

- [x] Extend `Backend` with a bounded track-targeted MIDI-input staging operation and add fake-backend operations for exact target/message assertions.
- [x] Give the threaded native MIDI port a driver-neutral incoming-event API built on its existing bounded control queue and `ExternalMidiPort::push_incoming`; ingest pending messages once per engine process iteration and implement the backend operation for direct and dry-MIDI native tracks without requiring host connections.
- [x] Implement the same soft-latency operation in `EngineBackend` by resolving either dummy or external track MIDI input and staging frame-zero events for the next engine process iteration.
- [x] Add protocol-v5 `InjectTrackMidiInput` to `shoop_audio_protocol`; submit it from `WebAudioBackend`, validate it in `shoop_audio_worklet`, and forward it to the shared engine backend without consulting browser MIDI endpoints or links.
- [x] Implement explicit unsupported/stale/invalid/overflow errors and continue-safe per-track semantics; retain existing physical Web MIDI routing unchanged.

Verification:

- [x] `shoop_backend` tests prove exact note-pair recording on the next available process iterations without host endpoints/links plus stale, audio-only, nonzero-frame, and backend-shape rejection.
- [x] All 39 native-driver backend tests pass; dummy exact-output injection plus JACK-test and CPAL-test injection exercise the shared native input-port operation. All 2 protocol and 7 worklet tests pass, including no-Web-MIDI-endpoint recording.
- [x] Existing allocation/saturation tests pass. `cargo build -p shoop_audio_worklet --target wasm32-unknown-unknown` and `cargo check -p shoopdaloop_egui --no-default-features --target wasm32-unknown-unknown` pass with the one-process-iteration/frame-zero contract.
- [x] `cargo fmt --all -- --check`, `RUSTFLAGS="-D warnings" cargo build -p shoopdaloop_egui --no-default-features`, warning-denying Wasm worklet build, and `git diff --check` pass before the backend/protocol milestone commit.

### Stage 2 — Implement application fanout and note lifecycle

- [x] Add application state for active piano-note recipients keyed by logical note and `TrackId`.
- [x] On press, derive current eligible tracks from monitoring state plus owned role-bearing MIDI input ports, ignore duplicate presses, stage note-on to all destinations, and retain successful recipients.
- [x] On release/release-all, resolve and stage note-off for original recipients even if current eligibility changed; remove stale recipients safely and continue after individual backend errors.
- [x] Aggregate one bounded error per action for total/partial staging failure without flooding snapshots or starving other application intents.
- [x] Flush active note-offs before a confirmed driver transition and actor shutdown, clear stale lifecycle state on loaded-session replacement, and retain logical recipients across ordinary backend-ID remapping.

Verification:

- [x] Fake application tests cover one/many destinations, direct and processed dry-MIDI tracks, monitor-off and audio-only exclusions, duplicate press/release, newly eligible and newly muted tracks while held, partial failure, release-all, and original-recipient cleanup. Eligibility requires track-owned input-role MIDI ports from the application map, which excludes Lua/output-only entries; current production sync topology is audio-only and the policy has no sync exclusion.
- [x] Engine-backed cooperative integration records the exact `[0x90, note, 100]`/`[0x80, note, 0]` pair into each of two monitored MIDI tracks while excluding an audio-only monitored track and requiring no physical connections.
- [x] All 54 `shoop_app` tests and 26 target-neutral `shoop_backend` tests pass, including unchanged Lua keyboard and physical Web MIDI coverage.
- [x] Formatting, `git diff --check`, and `RUSTFLAGS="-D warnings" cargo build -p shoopdaloop_egui --no-default-features` pass before the application-policy milestone commit.

### Stage 3 — Add the egui piano pane and compose workflows

- [x] Add and export a reusable `PianoPane` in `shoop_egui` without backend/platform dependencies.
- [x] Replace the details boolean with transient bottom-pane selection; render **details** and **piano** side by side, preserve details behavior, and emit release-all before hiding/switching away from a held piano.
- [x] Render the fixed-size full-range keyboard in a horizontal scroll area, center C4 only on first initialization, retain subsequent scroll state, show active keys, and display current eligible destination names or a clear no-target hint.
- [x] Implement press/hold/release, drag-between-key, leave, pointer-gone, pane-switch, and focus-loss handling using shared geometry, black-key hit priority, and non-dragging scroll content.
- [x] Extend the native dummy product workflow to record exact piano bytes and the production browser self-test to inject a paired note across separate callback iterations without Web MIDI.
- [x] Document the pane, eligibility, fixed bytes, soft timing, and driver-independent behavior in `src/rust/shoopdaloop_egui/README.md` and synchronize planning status.

Verification:

- [x] All 78 backend-free `shoop_egui` tests pass, including button open/close/switch, no stacking, details preservation, app-intent routing, destination roles, full geometry/C labels, black-key precedence, C4 centering/retention, active lifecycle, and focus/pointer cleanup.
- [x] Piano paint/overflow tests pass at 360×200 and 900×600; geometry/hit tests cover endpoint notes 0/127 and all C labels through C9.
- [x] All 23 `shoopdaloop_egui --no-default-features` tests pass. The native dummy product workflow records exact velocity-100 note-on/zero-velocity note-off bytes; application integration separately proves simultaneous two-track recording and audio-only exclusion, while deterministic dummy/JACK-test/CPAL-test backend injection passes.
- [x] The production browser self-test now waits across AudioWorklet callbacks for piano press and release without Web MIDI. Debug Trunk and self-contained/package builds, Wasm UI/worklet checks, and forbidden native-dependency scans pass; Chrome/Firefox execution is an explicit environment skip because neither browser executable is installed (`google-chrome` returned `ENOENT`).
- [x] Formatting, `git diff --check`, warning-denying native egui/Wasm checks, debug Trunk build, self-contained generation, and package generation pass before the presentation/composition milestone commit.

### Stage 4 — Final end-to-end validation and closure

- [ ] Run `cargo fmt --all -- --check` and `git diff --check`.
- [ ] Run focused tests for `shoop_app_api`, `shoop_backend`, `shoop_app`, `shoop_audio_protocol`, `shoop_audio_worklet`, `shoop_egui`, and `shoopdaloop_egui` on native and Wasm-relevant surfaces.
- [ ] Run `RUSTFLAGS="-D warnings" cargo build -p shoopdaloop_egui --no-default-features` and focused tests for the egui dependency path. Do not substitute a full workspace, Qt/QML, Lilv, or native-FX build as a required acceptance gate.
- [ ] Confirm source/dependency scans show no implementation changes in `src/qml`, `src/rust/frontend`, or other retained frontend paths; QML build and self-test execution are explicitly not required for this egui-only feature.
- [ ] Build and verify debug/release native, hosted WebAssembly, self-contained HTML, and AudioWorklet artifacts; run existing Chrome/Firefox audio, Web MIDI, lifecycle, settings, session/media, offline, and direct-file regressions in addition to the piano workflow.
- [ ] Manually validate side-by-side eligible MIDI tracks on each available native driver: full-range scrolling, C labels, default C4 center, note press/release, monitoring, recording/playback, pane/focus cleanup, mute/topology changes while held, and driver switching. Record explicit environment skips rather than claiming unavailable hardware.
- [ ] Record exact commands, test counts, platform/browser/driver evidence, skips, and the accepted soft timing contract (up to one engine process iteration before ingestion, frame-zero placement) in this plan; reconcile `EGUI_FEATURE_PARITY_MATRIX.md`, `EGUI_REPLACEMENT_PROJECT.md`, the runner README, and every affected current plan before the closure commit.

Final acceptance evidence must include one authoritative native workflow and one production-browser workflow where a piano note pair is injected into at least two monitored MIDI tracks, excluded from ineligible tracks, observed through ordinary monitoring and recording, and cleaned up after a held-note lifecycle interruption. Browser evidence must run without Web MIDI permission, native evidence must run without a physical MIDI source, and both must retain application/audio progress after the interaction.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
