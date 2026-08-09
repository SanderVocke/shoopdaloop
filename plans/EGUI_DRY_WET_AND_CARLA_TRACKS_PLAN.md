# egui dry/wet tracks and Carla FX plan

## Pre-implementation evidence

- The QML baseline builds dry/wet tracks in `NewTrackDialog.qml` and `js/generate_session.js`: dry input/send and wet return/output ports for external processing, or dry inputs, internal Carla ports, and wet outputs for Carla Rack, Patchbay, or Patchbay 16x. Loop channels are explicitly `dry` or `wet`; MIDI is supported on the dry side.
- `TrackControlLogic.qml` defines the required live-routing matrix. Monitoring, current loop modes, and next-cycle modes jointly gate dry input passthrough, wet output passthrough, and FX activity; dry-to-wet re-recording forces monitoring off. The QML dry/wet, external, Carla, transition, multiple-loop, and session tests are the behavioral oracle.
- The pure-egui API/application/backend currently models and instantiates only direct tracks. `shoop_session` already represents `DryWetExternal` and `Carla` topology, channel modes, FX-chain descriptors, opaque Carla state strings, and captured FX states, but `shoop_app` deliberately rejects those topologies at load time.
- Native `shoop_engine::app_backend` already exposes `FXChain`, Carla Rack/Patchbay/Patchbay16x creation, external-UI visibility, activation, lifecycle/recovery, generation logs, and exact state save/restore. Its global `CarlaHostingMode` already selects in-process or one supervised subprocess per chain.
- `shoop_backend::NativeBackend` currently enables native audio drivers without LV2, creates only direct ports/channels, sets direct passthrough directly from the monitor control, and writes `carla_state: None`. The browser/core backend and AudioWorklet protocol likewise only construct direct topology. Neither external send/return processing nor Carla is currently viable in the browser, but the shared API and egui dialog must be capability-driven so a future browser-native processor can plug into the same mechanics.
- The native egui executable does not yet recognize the hidden Carla-worker command line used when `SupervisedCarlaProcessor` launches `current_exe()`. That entry path must exist before subprocess mode can work in the egui binary.
- Fresh `.shoop` v1 already stores exact per-channel audio/MIDI payloads and declares opaque Carla strings byte-for-byte significant. The current documentation explicitly marks runtime dry/wet and Carla instantiation as deferred.

## Goals and scope

Deliver QML-equivalent dry/wet looping in the native standalone egui application, using either user-connected external send/return ports or a hosted Carla chain. Keep the dry/wet topology, processor-capability contract, and Add Track/FX presentation cross-target: the current browser advertises no viable processor types, while a future browser-native FX implementation can use the same UI and application mechanics. Make topology, routing, loop media, FX state, external links, driver switching, and UI lifecycle durable through the existing typed application/backend/session boundaries.

In scope:

- cross-target typed direct/dry-wet track specifications, processor-type identities/capabilities, immutable presentation state, and capability-driven Add Track/FX UI;
- native Add Track processing capabilities for External, Carla Rack, Carla Patchbay, and Carla Patchbay16x, with independent dry/wet audio counts and optional dry MIDI;
- dry/wet loop creation, aligned Add Loop behavior, all current/queued loop modes, monitoring, gain/balance/mute, grabs, dry playback, and dry-to-wet re-recording;
- public dry input/send, wet return/output, and dry MIDI input/send connection roles, with internal Carla ports kept out of the host connection matrix;
- native in-process and supervised-subprocess Carla, external UI toggle/recovery, lifecycle/error/log presentation, and exact state capture/restore;
- exact session and loop-media round trips for dry and wet audio and dry MIDI, including role-aware import/export mapping and recorded-wet FX-state references;
- transactional native session load/replacement and audio-driver switching with dry/wet/Carla topology intact;
- an explicit empty browser processor catalog, complete browser-compatible dialog/state/action mechanics, synthetic future-provider presentation tests, and transactional capability errors for currently unrunnable dry/wet sessions;
- focused documentation and parity-matrix updates.

Out of scope:

- browser audio-FX processing in this milestone, including Carla/LV2, external send/return processing, Web MIDI, or treating capture/destination device endpoints as an effects host;
- arbitrary non-Carla plugin hosts, buses, generic FX graphs, plugin parameter editors, or embedding Carla's UI inside egui;
- QML-era `.shl` import or changes to the retained QML product;
- named/manual FX snapshot libraries beyond the existing automatic recorded-wet state and current chain-state persistence;
- track deletion/reordering and unrelated advanced loop/composite editing.

## Immutable acceptance criteria

1. The egui Add Track dialog offers Regular and Dry + Wet on native and browser builds. Dry + Wet always renders independent disabled/mono/stereo/custom dry and wet audio counts, optional dry MIDI, and a processing selector. Native runtime capabilities provide External, Carla Rack, Carla Patchbay, and Carla Patchbay 16x. The current browser capability catalog is empty: it shows an explanatory no-processors-available state and cannot accept the Dry + Wet draft, rather than hiding the mechanics or pretending External/Carla is runnable.
2. Processing choices come from an immutable application/backend capability catalog, not target checks or Carla-specific branching inside `shoop_egui`. Descriptors state channel/MIDI constraints and optional state, show/hide UI, recovery, and log capabilities. A backend-free test can inject a synthetic browser-native processor descriptor and exercise selection, validation, typed acceptance, and applicable FX controls through the same widgets without changing presentation code.
3. Accepted specifications are validated before backend mutation, produce stable track/loop/port identities, preserve the accepted name as the immutable port-name base, create at least eight aligned loop slots, and let later Add Loop operations clone the exact dry/wet channel shape and wiring.
4. Native External tracks publish dry audio inputs and sends, optional dry MIDI input/send, and wet audio returns and outputs. The normalized Connections dialog shows those ports in its existing Audio in/send/return/out and MIDI in/send roles and mutates exact authoritative host links.
5. Carla tracks create the selected existing Carla chain type, connect each available dry audio/MIDI channel to its corresponding internal chain input, connect corresponding chain audio outputs to wet recording/output paths, and do not expose internal FX ports as user-managed host endpoints. QML-compatible unequal channel counts remain deterministic: only indices with corresponding endpoints are internally connected.
6. Each loop has ordered, role-bearing dry audio, wet audio, and optional dry MIDI channels. Normal recording captures live dry input and the contemporaneous wet result; normal playback emits recorded wet content; Play Dry routes recorded dry content through the processor; re-record replaces wet content from recorded dry content without corrupting dry media.
7. Monitoring and processing follow the `TrackControlLogic.qml` truth table for both current and next-cycle modes. Stopped/unmonitored tracks gate live paths, recording can capture without monitoring, normal playback excludes live wet return, dry playback/re-record activates the processor path, and re-record forces monitoring off. Multiple loops and synchronized transition boundaries behave like the retained QML tests.
8. Input/output gain, stereo balance, output mute, meters, MIDI activity, grab, solo, fixed-cycle, selection, target, and queued-transition behavior continue to work for dry/wet tracks without changing direct-track behavior.
9. Loop details and loop audio import/export label channels by role and index. Exact `.shoop-audio`, float WAV, and session save/load can contain dry only, wet only, or both in explicit user-selected order; import maps source channels to every chosen dry/wet destination. Exact/standard MIDI workflows address the dry MIDI channel.
10. `.shoop` capture writes the correct topology, port roles/links, channel modes/media/offsets/preplay/gain, controls, FX descriptor, and the current exact opaque Carla `internal_state`. Loading reconstructs those values transactionally and restores Carla state before the loaded track becomes usable; save/load failures leave the prior session usable.
11. Entering a mode that records a wet channel captures one exact Carla state for that take, references it from the affected wet channels through `recording_fx_state_id`, saves it in `fx_states`, and allows the loop UI to restore that recorded state to a compatible current chain. Stale, duplicate, wrong-chain, or missing references are rejected before commit.
12. Session save uses the last confirmed Carla state when a supervised worker is unavailable. It never substitutes crash text, an empty placeholder, or partially received state for a previously confirmed value; absence of any valid state is reported explicitly.
13. Every Carla track shows an FX control whose state distinguishes active/bypassed, starting/restarting, crashed, and unavailable. Clicking it opens/closes the external Carla UI when healthy or recovers then opens after failure. UI closure, worker generation/crash summary, and bounded per-generation stdout/stderr logs are observable without blocking egui or the audio callback.
14. Native Settings has one global **Carla hosting** choice: **In application process** (default) or **One subprocess per FX chain**. It is a machine preference, not session data, is loaded before backend/FX construction, is marked restart-required, and does not migrate existing chains while the app runs.
15. In subprocess mode the packaged `shoopdaloop_egui` executable recognizes the existing hidden worker arguments before GUI/runtime construction and runs the existing supervised worker protocol. Each chain remains independently recoverable; normal shutdown and UI closure are not reported as crashes.
16. Native audio-driver switching captures and recreates external and Carla dry/wet tracks, media, exact Carla state, controls, and compatible links under the already-confirmed rate transaction. Missing links or unavailable Carla are visible diagnostics, not silent topology conversion to direct tracks.
17. A browser load of any session requiring External, Carla, or another unavailable processor fails capability validation before worklet/backend mutation and leaves the prior browser session usable. Direct browser tracks, files, settings, Lua, ports, and callback progress remain unchanged.
18. Carla/LV2 and worker dependencies remain native feature-gated. `shoop_egui`, `shoop_app_api`, browser UI Wasm, and AudioWorklet dependency trees remain free of LV2/native-process dependencies; all Carla state/UI/control operations remain off realtime callbacks.
19. Deterministic tests cover external and fake-FX signal flow, the complete routing truth table, session/media round trips, state-string identity, worker entry/lifecycle, settings, failures, browser empty/synthetic processor catalogs, and regressions. Installed-Carla UI/audio tests remain opt-in with explicit environment skips.

## Design rules and constraints

- Keep `shoop_egui` presentation-only and `shoop_app_api` framework/backend-independent. Publish plain topology, processor capability/constraint, and FX lifecycle state and emit typed track/FX actions; never expose engine handles or Carla objects. The widget must render an empty catalog and synthetic future processor descriptors without target-specific code.
- Replace direct-only count fields with one typed topology model shared by creation, snapshots, persistence conversion, import/export labels, and backend replacement. Do not infer dry/wet roles from names or vector positions.
- Preserve `.shoop` major-version compatibility. Extend or migrate the current Carla topology representation in a backward-compatible minor/document step if independent dry/wet counts cannot be represented; old `Carla { audio_channels, midi }` data must retain a documented equal-count interpretation.
- Strengthen `shoop_session::validate_bundle` so topology/channel/port/FX shapes and all `recording_fx_state_id` references are validated before backend staging. Opaque state strings must not be parsed, normalized, or regenerated.
- Generalize the backend contract around typed track topology and an explicit processor capability catalog rather than adding parallel one-off methods for every permutation. Native implementations advertise External/Carla; the current browser/worklet advertises none and rejects unsupported topology before mutation. Future browser-native processors must be addable by supplying a capability/implementation, not redesigning egui.
- Encode the QML routing truth table once as a tested target-neutral derivation from monitoring plus current/next loop modes. Backend implementations apply the resulting passthrough/FX-active commands; do not duplicate policy independently in egui, native code, and the worklet.
- Use existing `BackendSession`, `AudioPort`, `MidiPort`, loop channel modes, and `FXChain` APIs for native construction. Reuse existing Carla processor/control/subprocess code and state checkpoints; do not create another LV2 host or worker protocol.
- Distinguish externally connectable application ports from internal FX ports. Only the former enter normalized host connection snapshots and session external-link restoration.
- Treat FX construction/state restore as staged session replacement. A missing Carla installation may produce an explicit unavailable FX state for newly created tracks, but a loaded session must never be reported complete with dropped state or silently altered topology.
- Keep channel order stable and role-labeled across backend capture, session media indices, waveform details, and loop import/export. Sample-rate conversion continues through `shoop_session` only.
- Capture current and recorded-wet FX state on the control side. Never request state, allocate media, perform IPC, or show/hide UI from the audio callback.
- External and Carla dry/wet processing are native capabilities in this milestone. The browser keeps the shared topology/UI/action model but reports an empty processing catalog; it must not expose External merely because capture/destination host ports exist, and it must not emulate Carla.
- Register the Carla hosting preference only in native composition. Preserve settings v1 unknown-key/recovery/atomic-save behavior and default old/missing values to in-process.
- Keep the plan, `EGUI_FEATURE_PARITY_MATRIX.md`, `EGUI_REPLACEMENT_PROJECT.md`, `docs/session_format_v1.md`, and egui README synchronized with implementation evidence.

## Staged implementation

Stages are sequential unless noted otherwise. Complete and verify each stage before starting dependent work.

### Stage 0 — Freeze topology, routing, persistence, and capability contracts

- [x] Add application/backend topology, channel-role, stable processor-type identity, channel/MIDI constraints, optional state/UI/recovery/log facets, FX lifecycle, and capability-catalog value types alongside the direct compatibility constructor; Stage 3 migrates the Add Track intent and application model fully.
- [x] Define deterministic port/channel ordering and QML-compatible unequal dry/wet/processor endpoint mapping, including which ports are public versus internal.
- [x] Port the QML current/next-mode routing truth table to a target-neutral pure function and table-driven fixtures.
- [x] Validate fresh-session topology counts, channel modes/types, public/internal port references, FX descriptor/type, and captured FX-state references without requiring a format-version change.
- [x] Add fake-backend empty/synthetic processor catalogs and unsupported-topology validation seams; operational state/UI/worker failure injection follows the backend that owns those operations in Stage 2.
- [x] Update the parity matrix from “deferred” to the exact in-progress rows without marking implementation complete.

Verification:

- [x] `RUSTFLAGS="-D warnings" cargo test -p shoop_app_api -p shoop_session -p shoop_backend -p shoop_app -p shoop_egui -p shoopdaloop_egui` passes 159 focused tests, including contract, compatibility, malformed-document, opaque-string, routing-table, publication, and existing presentation/runner coverage.
- [x] Existing direct session documents still decode to the same runtime topology; the deferred Carla fixture retains its documented equal dry/wet `audio_channels` interpretation.
- [x] Commit the contract/schema milestone.

### Stage 1 — Native External dry/wet backend topology

- [ ] Generalize `Backend` creation, session DTOs, replacement maps, and capture records to carry typed topology, selected processor identity, and role-bearing audio/MIDI channel content.
- [ ] Implement External dry/wet ports, channels, internal wiring, ringbuffers, loop creation, controls, polling, capture, and replacement in native `NativeBackend`.
- [ ] Keep fake/test backends able to exercise the generic topology and routing contracts without making External a production browser capability; leave the browser AudioWorklet topology unchanged in this milestone.
- [ ] Apply the shared routing derivation whenever monitoring or relevant current/queued loop state changes; preserve sample-accurate engine channel-mode semantics and pre-boundary activation.
- [ ] Extend native normalized connection snapshots and restoration for Audio send/return and MIDI send roles without exposing internal ports.

Verification:

- [ ] Shared fake and native-dummy backend contracts prove port shape, exact external links, all loop modes, monitoring, multiple loops, transition pre-activation, grab, and dry/wet media capture.
- [ ] Allocation/lock guards pass for native External dry/wet processing; existing AudioWorklet callback guards remain unchanged.
- [ ] Browser backend/protocol tests publish an empty processor catalog, reject External/Carla track creation or replacement before command transfer, and preserve the previous worklet generation/session.
- [ ] Commit the external-topology milestone.

### Stage 2 — Native Carla composition and worker entry

- [ ] Add a native-FX backend feature that enables existing LV2/Carla support without contaminating browser or presentation dependency graphs.
- [ ] Extend native track storage/construction to own one existing `FXChain`, wire dry inputs/MIDI and wet outputs by index, and apply shared active/bypass routing state.
- [ ] Expose typed control operations for UI visibility/toggle-or-recover, state capture/restore, lifecycle/generation/crash status, and bounded generation logs.
- [ ] Add a reusable native worker-entry parser/dispatcher at the backend/engine boundary and invoke it in `shoopdaloop_egui` before eframe, settings, audio drivers, or the application actor start.
- [ ] Preserve the existing supervised processor's last-confirmed state and independent generation/recovery semantics through capture and teardown.

Verification:

- [ ] Deterministic `Test2x2x1`/fake-Carla tests prove audio/MIDI wiring, active gating, wet silence on failure/deadline miss, state checkpoint identity, recovery, logs, and clean shutdown.
- [ ] An integration test launches the egui executable in hidden fake-worker mode and completes the existing handshake/shared-memory protocol without creating a GUI.
- [ ] Optional installed-Carla Rack/Patchbay/Patchbay16x and external-UI smokes skip with explicit reasons when LV2/UI support is unavailable.
- [ ] Native dependency scans include LV2 only in the intended product/backend feature; Wasm/worklet/presentation scans remain clean.
- [ ] Commit the native-Carla milestone.

### Stage 3 — Application model, actions, and Add Track UI

- [ ] Store topology and role metadata in authoritative track/loop models and immutable snapshots; derive control applicability, stereo state, waveform labels, and connection ownership from it.
- [ ] Generalize Add Track, aligned Add Loop, load/remap, audio-driver switch, loop state publication, and script control paths without regressing direct/sync tracks.
- [ ] Implement the egui Regular/Dry + Wet draft UI from the capability catalog—not `cfg`/target branches—with independent channel controls, processor constraints, empty-catalog explanation/disabled acceptance, validation, cancellation, and typed acceptance.
- [ ] Add capability-driven FX status/controls to processed track headers. Render show/hide and recovery only when advertised; for Carla, also render crash details and process logs with refresh/clear/copy and stale-track handling.
- [ ] Expose a loop action to restore its compatible recorded-wet FX state.

Verification:

- [ ] Application tests cover every accepted native spec, stable IDs, at-least-eight alignment, Add Loop cloning, stale actions, empty browser capabilities, unsupported browser External/Carla, FX lifecycle/actions, and audio-driver replacement.
- [ ] Backend-free egui interaction/paint tests cover native catalogs, the actual empty browser catalog, injected synthetic browser-native processors with and without UI/state facets, validation, applicable FX states/logs, minimum/common viewports, and absence of FX controls on direct/external tracks.
- [ ] Existing direct-track, loop-control, connection, settings, and script tests remain behaviorally unchanged.
- [ ] Commit the application/UI milestone.

### Stage 4 — Role-aware session and loop media I/O

- [ ] Convert role-bearing backend capture into `Direct`/`Dry`/`Wet` `ChannelDocument`s with exact stable media, port references, metadata, and FX descriptors rather than forcing `Direct` topology.
- [ ] Convert validated dry/wet/Carla documents back into backend DTOs without flattening roles; stage Carla construction and exact state restore before application publication.
- [ ] Capture one FX state when wet recording begins, retain its exact string/type as `FxStateDocument`, associate affected wet channels, and garbage-collect only unreferenced automatic states on save.
- [ ] Make waveform/detail labels, audio export selection, and import destination mapping role-aware; support ordered dry-only, wet-only, or mixed exports and imports. Route MIDI import/export to dry MIDI.
- [ ] Preserve topology, media, Carla state, captured take state, public links, and controls through native driver switches and sample-rate conversion.
- [ ] Remove the documented runtime capability rejection only for processor types supported by the active backend. Retain transactional capability rejection for browser External/Carla, unknown future processors, buses, and other deferred features.

Verification:

- [ ] Codec/application/backend round trips assert exact float bits, MIDI bytes/timing, channel modes/order, offsets/preplay/gain, current and recorded Carla strings (including Unicode/newlines/NUL), and exact external host IDs.
- [ ] Import/export tests cover dry-only, wet-only, mixed/reordered/duplicated source mapping, unequal channel counts, standard WAV/MIDI, exact formats, resampling confirmation, and cancellation.
- [ ] Failure tests prove malformed topology/state references, unavailable Carla, restore failure, recording-time save, and replacement failure leave the prior session usable.
- [ ] Commit the persistence/media milestone.

### Stage 5 — Carla hosting setting and startup orchestration

- [ ] Register a stable native `carla.hosting_mode` setting with in-process default, validated string-to-enum helpers, and restart-required effect; document that it is global and excluded from sessions.
- [ ] Load and apply the setting through a `shoop_backend` adapter before constructing `NativeBackend` or any FX chain; ordinary Save persists it without migrating running chains.
- [ ] Ensure fallback/recovery diagnostics distinguish missing LV2, in-process host failure, subprocess launch/handshake failure, crash, and stale state while preserving settings recovery behavior.
- [ ] Add native packaging checks ensuring the main executable can serve as its own worker on Linux, Windows, and macOS.

Verification:

- [ ] Settings tests cover default, both modes, invalid fallback/diagnostic, unknown-key preservation, restart persistence, and no session serialization.
- [ ] Runner/backend tests prove the selected mode is applied before first FX creation and changing the setting has no runtime effect until restart.
- [ ] In-process and fake-subprocess workflows create independent chains, save/restore state, toggle UI, recover one failed chain, and shut down without orphan workers.
- [ ] Commit the settings/startup milestone.

### Stage 6 — QML-parity integration and documentation

- [ ] Translate the retained QML dry/wet direct, external, transition, multiple-loop, Carla activation/MIDI-gating, and session-save/load cases into the narrowest Rust backend/application integration tests.
- [ ] Add native egui end-to-end workflows for external ports and fake/installed Carla: create, connect, monitor, record, play wet, play dry, re-record wet, show/hide UI, save, replace, reload, and restore recorded FX state.
- [ ] Extend browser automation to open the Dry + Wet form, verify all shared fields/mechanics remain visible, observe an empty processing selector with disabled acceptance, and prove External/Carla session rejection leaves direct tracks/media and AudioWorklet callback progress intact.
- [ ] Update `docs/session_format_v1.md`, `docs/settings_format_v1.md`, `docs/egui_port_model.md`, `shoopdaloop_egui/README.md`, user track/Carla documentation, project roadmap, and parity evidence.

Verification:

- [ ] Focused native workflows satisfy the runnable dry/wet criteria; browser workflows satisfy the shared-mechanics, empty-catalog, capability-rejection, and non-regression criteria; installed-Carla checks have explicit environment skips.
- [ ] The retained QML self-test suite still passes and remains the regression oracle; no QML behavior is changed.
- [ ] Commit the parity/documentation milestone.

### Stage 7 — Final end-to-end validation

- [ ] Run `cargo fmt --all -- --check` and `git diff --check`.
- [ ] Run focused warning-denying tests for `shoop_app_api`, `shoop_session`, `shoop_backend` with native FX, `shoop_app`, `shoop_egui`, `shoop_settings`, and `shoopdaloop_egui`.
- [ ] Run `RUSTFLAGS="-D warnings" cargo build --workspace --features shoop_engine/app_backend`.
- [ ] Run `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo test --workspace --features shoop_engine/app_backend`.
- [ ] Build first, then run `target/debug/shoopdaloop_dev.sh --self-test`.
- [ ] Run locked debug/release native builds and packages plus production Wasm UI, preview, AudioWorklet builds, forbidden-dependency scans, and Chrome/Firefox workflows from `.github/workflows/build_and_test_egui.yml`.
- [ ] Manually exercise one native external chain and, when installed, both in-process and subprocess Carla: create tracks, connect ports, record dry/wet, play wet/dry, re-record, toggle UI, crash/recover worker, save/reload, restore take state, and switch audio driver.
- [ ] Record exact platform/LV2/UI/audio environment evidence and residual limitations in this plan and the parity matrix.
- [ ] Commit the completed validation/documentation milestone.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
