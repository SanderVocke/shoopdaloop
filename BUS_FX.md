# Bus FX implementation plan: Carla + Built-in FX, no synth

## 1. Goals, scope, acceptance criteria

### Goals

- Support insert effects on buses, reusing per-track FX concepts: processor descriptors + constraints, `DryWetProcessor`-style insert wiring, `*FxControl` set/active/visible/restore-state pattern, `FxState` snapshots, session `FxChainDocument` persistence.
- Include Carla (Rack / Patchbay / Patchbay 16x) and Built-in FX.
- Explicitly exclude Built-in Synth: buses have no MIDI input path.

### Scope

- `shoop_backend` public API + `EngineBackend` (dummy/web) + `NativeBackend`.
- `shoop_app_api` bus view types, `shoop_app` bus intent handling + session save/load, `shoop_egui` bus strip FX affordances.
- `shoop_session` validation for bus FX chains.
- Tests at backend, app, session-roundtrip, and audio-through-FX levels.

### Immutable acceptance criteria

- [ ] A stereo bus and a mono bus can each run Built-in FX audibly (e.g. Drive enabled changes output; bypass restores dry path).
- [ ] A bus can run a Carla chain where the native backend supports it, using the same chain-type mapping as tracks; unavailable Carla degrades with a reason, never a panic.
- [ ] No bus FX path creates or requires a MIDI port; OxiSynth is not offered for buses anywhere (catalog, validation, session load).
- [ ] Session save/load round-trips bus FX state + MIDI-CC assignments; old sessions without bus FX load unchanged.
- [ ] Mixer routes to an FX bus keep working; removing the bus removes its processor and ports without leaking session processors/ports.
- [ ] CI is green on the PR.

## 2. Design rules and constraints

- Reuse, don't fork: bus FX state reuses `TrackFxState` / `TrackProcessorEditorState` shapes; bus controls mirror `BackendTrackFxControl` minus `OxiSynth`; native bus wiring reuses `processor_chain_type()` + `create_builtin_fx_chain_with_audio_channels()` / `create_oxisynth_chain()` / `create_fx_chain()` dispatch.
- Insert semantics: bus FX is dry = wet = bus channel count. No channel-count conversion on buses in v1.
- Processor titles must be stable IDs, not bus display names: e.g. `bus_<raw_id>_fx`. Bus renames must not retitle processors.
- MIDI policy for buses is `Unsupported`: zero MIDI ports passed to `set_processor_ports()`. Built-in FX parameter/state controls still work; MIDI-CC assignments persist but have no live MIDI driver unless a later global-FX-MIDI routing stage explicitly adds it.
- Built-in FX descriptor currently says `midi: Required`; do not change the track descriptor. Bus catalog is the track catalog filtered to `midi != Required` and `id != OXI_SYNTH`, plus a bus-specific Built-in FX entry with `midi: Unsupported` and `matching_audio_channels: true`.
- No bus latency compensation in v1; document as a known limitation. Do not block on it.
- Goals/acceptance criteria change only with explicit user approval. Design rules/steps may be revised on new evidence with a documented reason. Keep the plan checked off as work progresses; commit per completed stage.

## 3. Stage 0 findings (recorded)

- `Session::set_processor_ports` now accepts 0 or 1 MIDI inputs for Built-in FX processors; tracks keep passing exactly 1, so track behavior is unchanged.
- A new engine test (`builtin_fx_processes_audio_without_midi_ports`) proves MIDI-less audio through Built-in FX.
- Global FX MIDI is no longer gated on per-processor MIDI ports, so a MIDI-less bus processor still receives control mappings; per-track MIDI inputs are unchanged.
- Native `FXChain` port creation derives MIDI ports from the chain kind and already tolerates a MIDI-less consumer: `create_processed_track` only connects `get_midi_input_port(0)` when `dry_midi` is set, and `bind_processor_ports` forwards whatever ports exist, so `dry_midi=false` yields a MIDI-less binding.
- Bus processor title scheme: `bus_<raw_id>_fx`, derived from the stable backend bus id, never the display name.
- Bus catalog rule: track catalog filtered to `midi != Required` and `id != OXI_SYNTH`, plus a bus-specific Built-in FX entry with `midi: Unsupported` and `matching_audio_channels: true`.

## 4. Staged implementation

### Stage 0 — Setup + contract spike (no public API changes)
Depends on: nothing; blocks Stages 1–7.

- [x] Create branch for bus FX work before implementing.
- [x] Confirm MIDI-less Built-in FX runs in `Session` with empty MIDI port vec (audio in/out, bypass, parameter set, state encode/restore).
- [x] Confirm native `FXChain` path works with `dry_midi=false` for Built-in FX and Carla (same shape `create_processed_track` uses).
- [x] Decide bus processor title scheme and bus catalog filter rule.
- [x] Verification: throwaway integration test or REPL-style test proving audio-through-FX with no MIDI ports; record decision in plan.

### Stage 1 — `shoop_backend` bus-FX API types
Depends on: Stage 0. Blocks Stages 2–7.

Changes in `src/rust/shoop_backend/src/lib.rs`:

- [x] Add `BackendBusFxTopology` (or extend `BackendBusRequest` with `fx: Option<BackendBusFxRequest>`), e.g. `{ processor_type, audio_channels }` with `dry_midi=false` invariant.
- [x] Add `BackendBusFxControl` mirroring the Built-in FX subset of `BackendTrackFxControl`: `SetActive/SetVisible/ToggleOrRecover/RestoreState/ClearLogs/BuiltInFx(...)`, plus Carla generic controls if tracks expose them; no `OxiSynth` variant.
- [x] Add `fx: Option<TrackFxState>` (or `BusFxState` alias) to `BackendBusState`; add `processor_state + builtin_fx_midi_cc_assignments` to `BackendSessionBus`.
- [x] Extend `Backend` trait: `bus_processor_catalog()`, `set_bus_fx_control()`, `bus_fx_state_string()`; add `BusFxControl` mutation kind/detail.
- [x] Update `MockBackend`/`FakeBackend`/test fakes for new methods.
- [x] Verification: `cargo build`; new API-level unit tests for normalization/validation (bad channel count, synth rejected, MIDI rejected).

### Stage 2 — `EngineBackend` (dummy/web): Built-in FX on buses
Depends on: Stage 1. Blocks Stages 4–7; independent of Stage 3.

- [x] Store `EngineBusFx { control, active, visible }` on `EngineBus`; create processor in `create_bus_with_ids()` when requested (`prepare_processor_with_channels(sample_rate, buffer_size, channel_count)`).
- [x] Rewire bus graph from `input -> output` to `input -> send -> processor -> receive -> output`, keeping gain/balance/mute on the output port (`apply_bus_control` unchanged in behavior).
- [x] Implement `set_bus_fx_control()` by cloning the Built-in FX arm of `set_track_fx_control()`; implement `bus_fx_state_string()`, include FX in `mixer_snapshot()` + `capture_session_data()` + `build_replacement()` restore.
- [x] `remove_bus_internal()` also calls `session.remove_processor(title)` and drops FX ports.
- [x] Carla on `EngineBackend` stays unsupported (same as Carla tracks there); return a clear error.
- [x] Verification: backend tests for mono/stereo create → control → snapshot → remove; audio test proving Drive-on changes bus output vs bypass.

### Stage 3 — `NativeBackend`: Built-in FX + Carla on buses
Depends on: Stage 1. Blocks Stages 4–7; independent of Stage 2.

Changes in `src/rust/shoop_backend/src/native.rs`:

- [x] Refactor `create_processed_track()` chain-setup into a helper usable by buses, or add `create_processed_bus()` calling the same `processor_chain_type()` → `create_builtin_fx_chain_with_audio_channels()` / `create_fx_chain()` dispatch with `dry_midi=false`.
- [x] Wire bus inputs → chain audio inputs, chain audio outputs → bus outputs; skip all MIDI port creation.
- [x] Extend native catalog function to expose the bus catalog (Carla entries only under `native-fx`, same availability logic; never OxiSynth for buses).
- [x] Implement native `set_bus_fx_control()` / state-string by reusing the native track FX control arms.
- [x] Verification: `cargo test -p shoop_backend --features native-drivers --lib bus` green (incl. new `native_dummy_bus_fx_builtin_round_trips_state_and_removes_cleanly`: catalog → create stereo Built-in FX bus → Drive-on editor snapshot → capture `processor_type/state` → `RestoreState` → remove cleanup); full `--features native-drivers --lib` suite 111 passed; workspace `cargo check` + `cargo check --features native-drivers --tests` clean.

### Stage 4 — App model + session persistence
Depends on: Stages 1–3. Blocks Stages 5–7.

- [x] `shoop_app_api`: add `fx` to `BusState`; add `BusAction::Fx*` variants (active/visible/toggle/restore/clear/builtin-fx); add bus catalog to the snapshot/view state; never construct an OxiSynth bus action.
- [x] `shoop_app`: `handle_bus_action()` FX arms mirroring `handle_track_action()` FX arms incl. optimistic `desired_bus_fx_controls`; `refresh_bus_view()` propagates `fx`; bus creation accepts an FX spec and validates against the bus catalog.
- [x] Session save: emit bus `FxChainDocument` + topology for FX buses; session load: remove the `bus.fx_chain.is_some()` rejection in `session_bundle_to_backend()`, add `runtime_bus_topology()` + validation paralleling `runtime_track_topology()`; restore via create-bus-with-FX + `RestoreState` + CC assignments.
- [x] `shoop_session`: allow/validate bus FX chains (`Carla` + `BuiltInFx` only; reject synth/test mismatches in `archive.rs`).
- [x] Verification: app unit tests for bus FX intents + optimistic rollback; session bundle round-trip test with an FX bus; old-fixture load test still passes.

### Stage 5 — UI (`shoop_egui/src/bus_controls.rs` + track-editor reuse)
Depends on: Stage 4. Blocks Stages 6–7.

- [x] Add collapsed FX section to the bus strip reusing the Built-in FX editor widgets and Carla status/logs/external-UI affordances; wire to new `BusAction::Fx*`.
- [x] Processor picker for buses lists only the bus catalog (no synth).
- [x] Verification: existing `bus_controls` egui tests still pass; new tests for FX action emission and MIDI-less picker contents.

### Stage 6 — Regression + edge-case tests
Depends on: Stages 1–5. Blocks Stage 7.

- [x] Mixer-route × bus-FX matrix: route connect/disconnect with FX active/inactive; bus remove with routes attached.
- [x] Master-bus FX create/remove; mono/3ch/surround channel counts incl. Carla-16x limits and rejection messages.
- [x] `python3 scripts/check_shoop_test_usage.py`; `cargo fmt --all`; `RUSTFLAGS="-D warnings"` build.
- [x] Verification: full affected test suites green locally.

### Stage 7 — Final end-to-end validation + delivery workflow
Depends on: Stages 0–6.

- [ ] End-to-end: new bus with Built-in FX → route a track → hear/measure effect → bypass → save → reload → FX state intact → remove bus cleanly; repeat smoke for a Carla bus where supported.
- [ ] Run the required test suites before pushing; push branch and create a PR.
- [ ] Work to ensure CI turns green.
- [ ] Check the PR for any automated review coming in; repeatedly address review feedback until the reviewer approves.

## 5. Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

## 6. Suggested file touch list

- `src/rust/shoop_backend/src/lib.rs` — bus FX types, trait, `EngineBackend` wiring/snapshot/restore, fakes.
- `src/rust/shoop_backend/src/native.rs` — native bus chain creation/control/catalog.
- `src/rust/shoop_app_api/src/lib.rs` — `BusState.fx`, `BusAction::Fx*`.
- `src/rust/shoop_app/src/lib.rs` — bus FX intents, optimistic controls, session bundle mapping.
- `src/rust/shoop_session/src/{document,archive}.rs` — allow + validate bus FX chains.
- `src/rust/shoop_egui/src/bus_controls.rs` (+ shared FX editor widgets) — bus FX UI.
