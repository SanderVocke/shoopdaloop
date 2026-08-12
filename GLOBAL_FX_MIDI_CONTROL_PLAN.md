# Global FX MIDI Control Input

## Goal and scope

Add one session-wide **Global FX Control MIDI In** application port. Users connect a physical or virtual MIDI source through the existing Connections dialog. Supported control messages fan out to every MIDI-capable FX processor without entering track/loop MIDI channels. Live processors receive controls in their normal process block; intentionally inactive processors retain only the latest control state and apply it when normal processing resumes, without waking DSP solely for MIDI.

This is an end-to-end native and browser feature covering the engine, all backend adapters, AudioWorklet protocol, application model, connection UI, session persistence, tests, and documentation.

## Immutable acceptance criteria

1. Every new session exposes exactly one user-managed, externally connectable MIDI input named **Global FX Control MIDI In** in the all-tracks Connections dialog. It is explicitly application-owned, is absent from track-scoped dialogs, and is not inferred from a port name.
2. The global port accepts only stateful short MIDI controls for FX fanout: CC 0–119, channel pressure, and pitch bend on all 16 channels. Notes, poly pressure, program changes, channel-mode CC 120–127, system common/realtime messages, SysEx, malformed messages, and unsupported payloads do not reach FX processors.
3. Every currently available FX chain with a MIDI input receives supported global controls, including Carla, Tiny Synth/FX, and External chains where their topology exposes MIDI. Processor-specific interpretation and mapping remain the processor's responsibility; this feature adds no mapping editor or fixed CC assignments.
4. A normally processing FX chain receives current global controls in its process block in addition to its ordinary track/playback MIDI. Global controls never enter a loop channel, capture/ringbuffer, dry recording, playback content, MIDI output, or automation path.
5. An intentionally inactive FX chain performs no DSP or synthetic/silent callback because of global MIDI. Instead, it keeps a fixed-capacity per-processor sparse pending state whose `None` entries mean no deferred update. Repeated updates to one key replace the pending value.
6. On the first normally processed block after activation, deferred controls are emitted at frame zero and cleared only when admitted to a processor block. Ordinary processor MIDI retains its existing bounded admission priority; deferred global state drains through remaining capacity over later active blocks and therefore cannot create unbounded work or storage.
7. Within an admitted block, deferred state is initial synchronization, ordinary track/playback MIDI follows, and current global controls follow ordinary MIDI. A current global update supersedes a stale deferred value for the same global key. No duplicate detection or coalescing is attempted between the global and regular track paths.
8. Crashed, unavailable, removed, or unsupported processors do not accumulate deferred controls. Saturation and rejected traffic are bounded and observable through tests/diagnostics; no stale event queue is replayed later.
9. Connecting one host device to both the global port and a regular track MIDI input remains additive. The regular copy retains existing monitoring/recording behavior, while the global copy follows this feature's filtering and deferral rules. The UI warns that absolute controls can be applied twice, relative controls can behave incorrectly, and regular-path controls may be recorded.
10. Session save/load and audio-driver replacement preserve the canonical global port and its exact desired/confirmed host endpoint identities. Older version-1 sessions without the port load with a disconnected canonical port. Malformed documents with conflicting or multiple global FX control ports fail transactionally.
11. Native JACK, CPAL+midir/dummy, Web MIDI/AudioWorklet, in-process Carla, subprocess Carla, Tiny Synth/FX, and supported External-chain routing retain bounded realtime behavior. The audio/render callback adds no ordinary mutex, allocation, blocking operation, browser call, or control-path Carla call.
12. Existing sessions, track ports, processor state, recording/playback, connection truth, browser hotplug recovery, and processor CPU-sleep behavior remain compatible.

## Design rules and constraints

- Treat the global input as a first-class owned application port throughout backend, wire, app, and persistence models. Do not identify it by display name, numeric position, or `MidiInput` role alone.
- Keep physical connection truth in the existing normalized model: backend snapshots provide confirmed links, application state owns pending requests, and Web MIDI desired identities survive hotplug.
- Use a dedicated pending-control abstraction rather than changing `MidiStateTracker`'s playback semantics. Its sparse state covers 16 × 120 CC values plus per-channel pressure and pitch bend, with deterministic iteration and preallocated emission storage.
- Filtering, pending-state mutation, fanout, merge order, and capacity handling are callback-authoritative engine behavior shared by processor kinds. Backend/UI code must not independently emulate it.
- Keep global and ordinary MIDI lanes distinct until processor-block assembly. This proves non-recordability and prevents ordinary track traffic from contaminating deferred global state.
- Relative encoders are not detectable from MIDI bytes. CC 0–119 use absolute last-value semantics while a processor sleeps; dual routing of relative controls is supported only with the documented warning, not deduplication heuristics.
- Preserve per-source event order and current frame offsets where possible. Deferred values are state restoration, not historical events, and are emitted at frame zero.
- A processor-block capacity limit may delay deferred state but must not allocate, loop without a bound, or displace traffic that the ordinary processor path currently admits.
- External-chain delivery must be scheduled before its exposed MIDI send is finalized; do not rely on writes after an output port has already been processed.
- The global port has no capture storage and does not silently acquire default host connections.
- Use the existing `SessionDocument.global_ports` field with a narrowly validated canonical shape; do not introduce a format-major change unless implementation evidence proves the existing representation insufficient.
- Increment the AudioWorklet protocol version when its command/snapshot/session payload changes. Host and worklet must reject mismatched versions as they do today.
- Follow the repository realtime guards, tracing inventory, style rules, and transactional session-replacement contract.

## Staged implementation

### Stage 1 — Engine control-state and routing contract

- [x] Add a fixed-size `PendingMidiControlState` (or equivalently named type) for CC 0–119, channel pressure, and pitch bend, with classification, overwrite, sparse clearing, deterministic bounded draining, and no invented defaults.
- [x] Add unit tests for all channels/types, explicit zero versus `None`, unsupported message rejection, repeated-value replacement, partial drains, stale-key replacement by current global input, and clear-after-admission behavior.
- [x] Represent the global control lane explicitly in `Session` and processor routes; add/remove/rebind operations must remain topology-safe and preserve prepared schedule ownership.
- [x] Make every relevant processor node depend on the global input while keeping that input outside all channel mappings and internal passthrough recording routes.
- [x] Assemble processor MIDI in the specified order and admission policy, caching only supported global updates while intentionally inactive and never invoking inactive processor DSP.
- [x] Adapt hosted Carla, Tiny Synth/FX, test processors, and External processor sends to the shared contract; make External output scheduling explicit where needed.
- [ ] Add engine tests proving live fanout, inactive overwrite-and-restore, multi-block bounded restore, unsupported-message filtering, no loop recording, removal/crash behavior, External send ordering, and no cross-lane deduplication.
- [ ] Extend allocation/lock tests to cover active and inactive global fanout, a full pending state, activation, and Carla bridge/subprocess-compatible block limits.

**Stage verification**

- [ ] Run targeted `shoop_engine` MIDI-state, session processor, graph scheduling, Carla bridge, subprocess, and realtime guard tests.
- [ ] Verify with instrumentation/fakes that an inactive processor receives zero process calls while its pending state changes, then receives only admitted latest values on real activation blocks.

### Stage 2 — Backend ownership and native lifecycle

- [x] Extend backend contracts with explicit global-port ownership/creation and include global ports in session capture and replacement mappings separately from track ports.
- [x] Create the canonical native driver MIDI input with zero ringbuffer/capture, register it in normalized connection snapshots, and bind it to the engine global lane.
- [ ] Include all MIDI-capable processed tracks in fanout as they are created, and remove their pending state cleanly with track/session replacement.
- [ ] Preserve global endpoint identities and connection truth across native audio-driver switches, including intentional disconnection and endpoint loss.
- [x] Implement the equivalent canonical port and routing in `EngineBackend` for dummy/offline and physical Web Audio modes.
- [ ] Make connection failure explicit when a host API cannot open one hardware endpoint for both a track and global port; do not fake confirmation or fall back to software deduplication.
- [ ] Update fake backends and fixtures so app-level tests can observe global ownership, connection mutation, filtering, deferred delivery, and failure paths.

**Stage verification**

- [ ] Run targeted native backend tests for JACK/dummy/CPAL+midir abstractions, processor creation/removal, driver switching, exact connection capture, and dual-connected host endpoints.
- [ ] Run `EngineBackend` tests proving a shared host source fans independently to a regular track and global lane, only the regular copy records, and inactive Tiny/External chains do not process solely for controls.

### Stage 3 — Browser protocol and AudioWorklet

- [x] Extend the wire protocol with explicit global-port ownership/lifecycle and any required create/replace payloads; increment `PROTOCOL_VERSION` and update serialization/supersession tests.
- [x] Publish the global application port and confirmed Web MIDI links from worklet snapshots without merging it with track ports.
- [x] Route `PushMidiInput` from one Web MIDI source independently to every connected track port and the global port, retaining bounded refusal counters and frame-zero next-quantum timing.
- [ ] Carry the global port and desired Web MIDI endpoint IDs through worklet restart and transactional session replacement.
- [ ] Add AudioWorklet no-allocation tests for global fanout, inactive deferral, saturation, hotplug, restart, and dual routing.

**Stage verification**

- [ ] Run targeted `shoop_audio_protocol`, `shoop_audio_worklet`, and browser backend tests, including protocol-version mismatch and command saturation cases.
- [ ] Build both Wasm targets and prove that global MIDI operation does not require audio input permission and does not introduce Carla/native dependencies.

### Stage 4 — Application model and Connections UI

- [x] Add `ApplicationPortOwner::GlobalFxControl` (or an equally explicit API identity), map backend ownership into stable app port state, and update exhaustive owner handling.
- [x] Register the canonical global port during startup and loaded-session commit without assigning it a track ID; keep stable app/backend/document ID mappings transactional.
- [x] Show it in the main/all-tracks Connections dialog with a clear owner label and hide it from every track-scoped dialog.
- [x] Derive and display a non-blocking warning whenever one confirmed host source feeds both the global port and any regular track MIDI input. Explain duplicate absolute updates, unsafe relative/event semantics, and regular-path recording without preventing the connection.
- [ ] Retain existing user-managed pending/confirmed/error cell behavior for global links, including disappearance, timeout, rejection, and reconnect.
- [ ] Update UI and application tests for empty host inventories, sorting, owner labels, warning appearance/removal, connection intents, dual links, and backend failure reporting.

**Stage verification**

- [ ] Run targeted `shoop_app_api`, `shoop_app`, and `shoop_egui` tests.
- [ ] Exercise the all-tracks and track-scoped dialogs with native and browser snapshots and verify that the warning is based on confirmed truth rather than pending requests.

### Stage 5 — Session persistence and compatibility

- [x] Capture the canonical port in `SessionDocument.global_ports` with its stable ID, MIDI/input role, external connectability, zero capture frames, and exact desired/confirmed endpoint identities as appropriate to the backend contract.
- [x] Validate load documents to allow exactly the canonical global FX control shape while continuing to reject unrelated deferred global-port capabilities; reject duplicate IDs, multiple control ports, wrong type/direction/role, internal links, capture state, and unsupported mutable fields before backend mutation.
- [x] Migrate older version-1 documents with no global control port to one disconnected canonical port, choosing a collision-free stable ID without changing track/loop/media identity.
- [x] Extend backend session data and native/browser replacement mappings so global creation, endpoint restoration, rollback, and driver switching are atomic with tracks.
- [ ] Add round-trip, legacy-load, malformed-document, endpoint-hotplug, browser restart, native switch, resampling, and failed-commit tests. Confirm pending runtime controller values are transient and are not serialized as MIDI media or processor state.

**Stage verification**

- [ ] Run targeted `shoop_session`, app session codec, native replacement, and browser transfer tests.
- [ ] Save/load a dual-connected device case and prove exact link restoration without recording or replaying global controller history.

### Stage 6 — Documentation and diagnostics

- [ ] Update the port model, track/connection usage documentation, session-format contract, architecture notes, application README, and user-facing help text.
- [ ] Document supported messages, inactive last-value behavior, bounded delayed restore under saturation, no automation/recording, dual-routing duplication, relative encoder limitations, and host APIs that may reject opening a device twice.
- [ ] Document that processor mappings remain processor-owned: Carla mappings are configured in Carla, while Tiny/External processors act only on controls they already support.
- [ ] Add bounded diagnostics/tracing for rejected global messages, pending overwrites/drains, and capacity deferrals at an existing appropriate publication boundary; do not add per-message logging on the callback.
- [ ] Update tracing coverage inventory where new runtime files or boundaries require it.

**Stage verification**

- [ ] Run documentation/build checks and `python3 scripts/check_tracing_coverage.py --require-closed`.
- [ ] Audit terminology and code references for the canonical feature name and absence of claims that arbitrary MIDI events are replayed or deduplicated.

### Stage 7 — Final end-to-end validation

- [ ] On a native backend, connect one controller to the global port and multiple Carla/Tiny/External chains; prove supported controls fan out, muted chains consume no DSP, latest deferred values apply on normal reactivation, and unsupported messages do not cross the global lane.
- [ ] Connect the same controller to a regular track input and the global port; prove additive live delivery, no deduplication, warning visibility, ordinary control recording only once through the regular path, and no stuck/doubled notes from the filtered global path.
- [ ] Exercise in-process and subprocess Carla with mapped parameters, inactivity/reactivation, deadline/failure recovery, and a session containing enough sleeping processors to demonstrate that controller movement does not wake their process callbacks or introduce callback budget regressions.
- [ ] Run browser Web MIDI validation for permission, connection UI, dual fanout, Tiny inactivity/reactivation, hotplug reconnect, worklet restart, saturation counters, session save/load, and continued render callbacks.
- [ ] Run formatting: `cargo fmt --all -- --check`.
- [ ] Run warning-denying build: `RUSTFLAGS="-D warnings" cargo build --workspace`.
- [ ] Run the complete Rust suite: `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo test --workspace --features shoop_engine/app_backend -- --test-threads=1`.
- [ ] Run tracing inventory: `python3 scripts/check_tracing_coverage.py --require-closed`.
- [ ] Build `shoopdaloop` and `shoop_audio_worklet` for `wasm32-unknown-unknown`, then run the documented Chrome/Firefox Web MIDI smokes where browsers are available.
- [ ] Record any unavailable hardware/platform validation as explicit evidence and rely on the authoritative CI matrix rather than weakening acceptance criteria.

## Execution contract

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
