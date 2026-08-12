# Tiny Synth/FX MIDI CC control implementation plan

## Goals and scope

Implement host-side MIDI learn and MIDI CC control for every continuous control currently exposed by ShoopDaLoop's built-in Tiny Synth/FX editor, without changing `tinyviolin`.

In scope:

- Publish each backend MIDI port's latest received message through the existing lock-free state path.
- Learn assignments from the Tiny Synth/FX track's MIDI input.
- Control master gain, reverb amount, distortion drive, compressor amount, and low/mid/high EQ gain from MIDI CC values.
- Add, replace, remove, clear, display, save, and restore assignments on native and browser/engine backends.
- Keep GUI state and captured current processor state consistent with values changed by MIDI.

Out of scope:

- MIDI control of preset selection, effect enable/disable toggles, panic, or track controls.
- Changes to `tinyviolin` or generic MIDI learn for other processor types.
- User-defined response curves, inversion, min/max ranges, pickup/soft-takeover, or 14-bit CC.

## Immutable acceptance criteria

1. After a MIDI port receives a valid 1-4 byte message, its application-facing state contains that exact payload until a later valid message replaces it; reading state does not clear the payload. The audio/process thread publishes it without locks, allocation, or commands, while existing event counters keep their current consume-on-read behavior.
2. A Tiny Synth/FX track exposes its input port's latest message to the application snapshot. The MIDI learn UI recognizes only three-byte Control Change messages (`0xBn`, controller `0..=127`, value `0..=127`) and identifies both MIDI channel and controller number.
3. Users can assign a learned channel/controller pair to exactly these seven targets: master gain, reverb amount, distortion drive, compressor amount, EQ low, EQ mid, and EQ high. Presets and all boolean toggles remain unavailable as targets.
4. A matching CC controls its target in ShoopDaLoop's realtime Tiny Synth/FX processor path. Values `0` and `127` map exactly and linearly to the target's existing editor range: `-60..=0 dB`, `0..=1`, `1..=20`, `0..=1`, and `-12..=12 dB` for each EQ band respectively.
5. MIDI mapping does not allocate or lock in realtime, and it does not suppress the original MIDI event from `tinyviolin`. Continuous controls still work while their corresponding effect toggle is off, preserving the value for later use.
6. Each target and each channel/controller source has at most one assignment. Assigning an occupied source or target atomically replaces the conflicting assignment(s). Individual removal and clear-all are supported.
7. CC-driven values are reflected in frontend Tiny Synth/FX state and are captured as the current sound state. Assignments themselves are session-level ShoopDaLoop host configuration: session save/load and backend/sample-rate replacement preserve them, while restoring a recorded FX sound snapshot does not replace them.
8. The main Tiny Synth/FX window remains compact. A `MIDI Learn…` control opens a separate, independently closable sub-window showing the latest valid CC (channel, controller, and value), a target selector and assign action, the current assignment list with per-row removal, and a clear-all action. No assignment action is enabled when the latest message is absent or is not a valid CC.
9. Equivalent behavior is covered for the native backend and the browser-compatible `EngineBackend`, and existing sessions without assignments continue to load with an empty assignment set.

## Design rules and constraints

- Keep the feature in ShoopDaLoop layers (`shoop_engine`, `shoop_backend`, `shoop_app`, `shoop_app_api`, `shoop_session`, and `shoop_egui`); do not patch or fork `tinyviolin` behavior.
- Represent a latest MIDI payload as fixed bytes plus length. Pack it into one atomic publication value in `MidiPortStateMirror` so readers cannot observe torn bytes/length; retain it across reads, unlike activity accumulators.
- Update the latest-message state from input events before mute/passthrough gating, so MIDI learn observes what physically reached the application input.
- Represent Tiny Synth assignments as a bounded, fixed-size mapping keyed by the seven target parameters. Do not put vectors, mutexes, allocation, parsing, or backend/frontend work in `process()`.
- Apply mapped values at the MIDI event's block offset before processing later samples. Keep existing master-gain smoothing and existing Tiny effect setter validation.
- Publish CC-driven parameter values through a small atomic Tiny Synth runtime mirror shared with the control side. Synchronize that mirror before editor snapshots and current-state serialization so GUI display and saved sound state do not become stale.
- Keep assignments separate from the opaque `shoop-tiny-synth-fx:1` audio-state envelope. Add a defaultable host-assignment field to the current chain/session representation; do not add assignments to recorded `FxStateDocument` snapshots.
- Use internal zero-based MIDI channels but display channels as 1-16. Validate channel/controller ranges and reject malformed or duplicate persisted data transactionally.
- Keep assignment ordering deterministic by target parameter for stable snapshots, UI, tests, and serialization.
- Preserve existing egui per-track window IDs; give each MIDI learn sub-window a track-scoped ID so multiple Tiny Synth editors cannot interfere.

## Staged implementation

### Stage 1 — Define fixed MIDI and Tiny Synth mapping contracts

- [ ] Add fixed-size latest-message state types at the engine/backend/app API boundaries, with explicit conversion helpers rather than exposing realtime storage internals.
- [ ] Add public Tiny Synth continuous-parameter and CC-assignment types plus assign/remove/clear `TinySynthFxControl` actions and action-kind tracing labels.
- [ ] Add one canonical range/scaling function on the engine side and bounded assignment operations enforcing one source and one target per mapping.
- [ ] Update fixtures/defaults and exhaustive matches for the seven supported targets only.
- [ ] Verify with focused unit tests for CC parsing, endpoint/intermediate scaling, malformed messages, conflict replacement, deterministic ordering, and action kinds.
- [ ] Commit the contract milestone.

### Stage 2 — Publish the latest MIDI input message lock-free

Depends on Stage 1.

- [ ] Extend `MidiPort` and `MidiPortState` in `shoop_engine/src/state.rs` and `midi_port.rs` with persistent latest-input-message state.
- [ ] Extend `MidiPortStateMirror` with a single packed atomic payload and publish the last event in each non-empty input batch without changing event-counter reset semantics.
- [ ] Propagate the field through `shoop_engine::app_backend::MidiPort`, native backend polling, `EngineBackend::poll`, `BackendTrackState`, `shoop_app` snapshot updates, and `TrackControlState`.
- [ ] Refactor native polling to read each MIDI port mirror once per poll, then derive both activity and latest-message state from that result.
- [ ] Verify that no-message defaults, 1/2/3/4-byte payloads, multiple events per block, repeated reads, muted inputs, and replacement by a later message behave identically in direct-engine and application-backend tests.
- [ ] Commit the MIDI state-publication milestone.

### Stage 3 — Implement realtime host-side CC mapping

Depends on Stages 1-2.

- [ ] Add fixed assignment storage and a lock-free runtime parameter mirror to `TinySynthFxControlState`/`TinySynthFxProcessor`; initialize both consistently when preparing or replacing a processor.
- [ ] Inspect processor MIDI events for matching CCs, scale through the canonical target ranges, and call ShoopDaLoop's existing Tiny processor setters at the event offset before forwarding the same event to `tinyviolin`.
- [ ] Ensure matching CCs update parameters even when an individual effect is disabled; arrange processor routing so learned controls are observed when the Tiny processor route is inactive without dispatching inactive synth note traffic.
- [ ] Add assignment mutation methods and preserve mappings when restoring only the opaque sound state.
- [ ] Synchronize mirrored runtime parameter values into editor snapshots and mutable current-state encoding, including correct `Custom` preset presentation after MIDI changes to preset-owned effect values.
- [ ] Verify every target at CC values 0, an intermediate value, and 127; channel/controller matching and nonmatching; unchanged preset/toggle behavior; original MIDI dispatch; inactive/effect-disabled behavior; and sound-state capture after a CC change.
- [ ] Extend `shoop_engine/tests/no_alloc.rs` so assignment mutation and mapped CC processing remain allocation-free in the guarded first/steady-state block.
- [ ] Commit the realtime mapping milestone.

### Stage 4 — Wire both backends, application actions, and persistence

Depends on Stage 3.

- [ ] Handle assign/remove/clear controls in native `FXChain`, native backend dispatch, and browser-compatible `EngineBackend`, updating control and realtime processor copies in command order.
- [ ] Include current assignments in `TinySynthFxState` returned by both backend implementations and pass the input port's latest message into the app-facing track state.
- [ ] Route new `TrackAction::TinySynthFx` variants through `shoop_app` with the existing error-reporting behavior.
- [ ] Add a defaultable Tiny Synth MIDI-assignment list to `FxChainDocument` and the backend session-transfer model; validate bounds, supported targets, uniqueness, and processor type before mutation.
- [ ] Save current-chain assignments deterministically, restore them when constructing replacement backends, preserve them across sample-rate/driver replacement, and deliberately exclude them from recorded-take `FxStateDocument` restore.
- [ ] Update `docs/session_format_v1.md` to document the optional ShoopDaLoop host mapping and backward-compatible empty default.
- [ ] Verify focused native and `EngineBackend` tests for action dispatch, realtime CC response, frontend state reflection, current-state capture, session round-trip, legacy empty defaults, malformed persisted mappings, transactional rejection, sample-rate replacement, and recorded FX-state restore preserving current assignments.
- [ ] Commit the backend/application/persistence milestone.

### Stage 5 — Add the Tiny Synth/FX MIDI learn sub-window

Depends on Stage 4.

- [ ] Add a compact `MIDI Learn…` button to the existing editor and track local open/selected-target state in `TinySynthFxEditor`.
- [ ] Build a track-scoped secondary egui window that shows either `Channel N · CC M · Value V` or a clear no-valid-CC status, offers only the seven continuous targets, and enables assignment only for a valid latest CC.
- [ ] Render assignments in deterministic target order with channel/controller labels, per-row `Remove`, and guarded `Remove all`; make both windows independently closable and prevent the learn view from appearing during normal use unless requested.
- [ ] Emit only the new Tiny Synth actions and rely on backend snapshots for authoritative assignment/value state rather than optimistic persistent copies.
- [ ] Extend egui tests for open/close behavior, track-scoped IDs, valid and invalid latest messages, target selection, assign, replacement display, individual removal, clear-all, disabled actions, and isolation between two editor instances.
- [ ] Commit the GUI milestone.

### Stage 6 — End-to-end validation and documentation check

Depends on all prior stages.

- [ ] Exercise a complete scenario on the dummy/native path: inject a CC into a Tiny Synth track input, observe it in the learn window, assign it, inject endpoint and intermediate values, observe audio/editor-state changes, remove it, and confirm further CCs no longer control the parameter.
- [ ] Repeat the backend-level scenario through `EngineBackend`, including session save/load, to cover browser behavior without relying only on native handles.
- [ ] Re-read changed public/session documentation and update tracing inventory if new instrumented action paths require it.
- [ ] Run formatting and warnings gates: `cargo fmt --all -- --check` and `RUSTFLAGS="-D warnings" cargo build --workspace`.
- [ ] Run the complete Rust suite: `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo test --workspace --features shoop_engine/app_backend -- --test-threads=1`.
- [ ] Run `python3 scripts/check_tracing_coverage.py --require-closed`.
- [ ] Build `shoopdaloop` and `shoop_audio_worklet` for `wasm32-unknown-unknown`; run the documented browser smoke checks when browsers are available, recording an explicit skip reason otherwise.
- [ ] Confirm `git diff` contains no `tinyviolin` changes and no unrelated generated artifacts.
- [ ] Commit the final validation/documentation milestone.

## Execution contract

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
