# Single-preset OxiSynth implementation plan

## Status and execution contract

This document is an implementation plan. No implementation stages are complete yet.

During implementation:

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

## Goals

- Present each OxiSynth processor as one ShoopDaLoop instrument using exactly one selected preset from the embedded SoundFont.
- Give all incoming channel-voice MIDI one logical channel and one shared controller state, independent of the source MIDI channel.
- Prevent recorded, live, and global-control bank/program messages from changing the selected preset.
- Provide the same embedded preset selector and behavior on native, browser AudioWorklet, and browser Worker/dummy runtimes.
- Persist the selected preset in a strictly versioned OxiSynth processor-state string and restore it transactionally with the session.
- Keep OxiSynth's broader API and General MIDI channel model private to the engine adapter.

## Scope

Included:

- A stable catalog of presets in the embedded `TimGM6mb.sf2` SoundFont.
- A narrow engine-level preset identity, control state, state codec, and realtime processor API.
- Logical single-channel MIDI remapping, bank/program filtering, disabled dedicated-drum behavior, preset selection, and panic/reset behavior.
- Native and in-process engine backend control, snapshots, capture, and transactional restoration.
- Audio protocol, AudioWorklet host, and remote worklet client support.
- Application-domain actions, optimistic state, processor descriptors, and an egui preset-selector window.
- Session document version 5, migration of version-4 stateless OxiSynth tracks, validation, documentation, and cross-target tests.

Excluded from this MVP:

- User-supplied or session-embedded SoundFonts.
- Multiple simultaneous presets, MIDI parts, channel-specific controller state, or a dedicated drum channel.
- Exposure of OxiSynth generators, effects, tuning, interpolation, gain, or other library controls.
- MIDI-learn mappings for the OxiSynth preset selector.
- Automatic per-recorded-take OxiSynth `fx_state` snapshots. Dry MIDI is re-rendered with the track's current preset.
- Serialization of voices, notes, controllers, envelopes, oscillator phase, effect tails, or editor visibility.

## Current constraints and target architecture

OxiSynth 0.1 requires at least 16 internal MIDI channels, so the dependency cannot be configured with one physical channel. ShoopDaLoop will retain the required 16 internal channels but make only channel 0 reachable: dedicated-drum behavior is disabled, every accepted channel-voice message is remapped to channel 0, and all other internal channels remain unused. This is an implementation detail below a logically single-channel Shoop API.

The engine adapter will own these concepts:

- `OxiSynthPresetId { bank: u16, program: u8 }` as the stable preset identity.
- A generated, `(bank, program)`-sorted preset catalog with stable IDs and display names.
- `OxiSynthState { preset }` as the version-independent domain state.
- `OxiSynthControlState` for validation, editor snapshots, canonical encoding/decoding, and processor preparation.
- `OxiSynthProcessor` for realtime MIDI filtering/rendering and direct validated preset selection.

The processor API will expose construction from an explicit state, preset selection, panic/reset, bounded processing, and stereo output access. It will not expose `oxisynth::Synth`, `MidiEvent`, SoundFont handles, arbitrary channels, or arbitrary bank/program operations.

Control-side state is the snapshot and persistence authority. Validated UI changes are queued to the render graph at a block boundary and mirrored in control state after queue acceptance. Session restoration decodes and validates state and prepares a complete replacement before publication; malformed state, unknown versions, unknown SoundFonts, and missing presets fail before replacing the running session.

Preset state will use one canonical opaque string:

```text
shoop-oxisynth:1:timgm6mb:<bank>:<program>
```

The codec version is independent of the session document version. The preset name and UI list index are never serialized. The logical SoundFont ID is stable; a future incompatible bundled font must receive a new ID or an explicit processor-state migration.

## Design rules and constraints

- The Shoop adapter, not OxiSynth, defines the public instrument semantics.
- Keep OxiSynth's mandatory 16 internal channels private and route all accepted channel messages to channel 0.
- Set `drums_channel_active` to false. Do not rely on OxiSynth's channel-9 behavior.
- Drop MIDI Program Change and bank-select CC 0/32 at the final processor input boundary so the rule covers live input, loop playback/start state, the on-screen piano, and global FX MIDI.
- Preserve source MIDI bytes in recordings and session media; filtering applies only when OxiSynth consumes events.
- Select presets only through the direct processor control API, never by injecting bank/program MIDI.
- Stop current voices before applying a new preset so old- and new-preset voices cannot overlap.
- Keep all allocations, SoundFont parsing, state decoding, and replacement construction outside realtime `process()`.
- Retain the existing fixed OxiSynth track topology: two dry audio inputs, two wet audio outputs, and one dry MIDI input. Dry audio remains ignored by the synth.
- Generate the catalog from the embedded SoundFont at build time, reject duplicate `(bank, program)` entries or programs outside `0..=127`, and keep native/Wasm catalog ordering identical.
- Use bank/program plus logical SoundFont ID as persistence identity; never use display name or catalog position.
- Reject missing presets and unsupported state versions transactionally. Do not silently fall back except when explicitly migrating a legacy stateless session to the documented default preset.
- Keep current-chain session state separate from automatic recorded-take state; the latter remains excluded for this MVP.
- Keep descriptor, editor-state, control, backend, and wire variants typed rather than introducing a generic plugin-parameter framework.
- Add new wire snapshot fields as defaultable where practical; application and worklet artifacts are still built and deployed as a matched set.
- Preserve fixed memory bounds and allocation-free OxiSynth rendering.

## Immutable acceptance criteria

- Every native, AudioWorklet, and Worker/dummy OxiSynth track exposes the same complete embedded preset list and an embedded selector showing the confirmed selected preset.
- A newly created OxiSynth track starts on one documented default preset that exists in the generated catalog.
- Selecting a catalog preset updates the sound at an audio block boundary, silences voices from the previous preset, and converges in the authoritative editor snapshot.
- Notes, note-offs, polyphonic pressure, channel pressure, pitch bend, and allowed CC messages from every source MIDI channel are delivered to OxiSynth channel 0.
- Program Change and CC 0/32 never reach OxiSynth and cannot alter the selected preset; the original MIDI remains intact in recorded and serialized loop media.
- OxiSynth dedicated-drum behavior is disabled and no source channel receives special drum treatment.
- OxiSynth's required unused internal channels and broader synthesis API are not exposed through Shoop application or backend APIs.
- OxiSynth state has a canonical version-1 encoding containing only logical SoundFont identity and bank/program preset identity.
- State decode rejects malformed envelopes, unsupported versions, unknown SoundFonts, invalid ranges, and unavailable presets without mutating the current processor or session.
- Session saves write non-empty OxiSynth chain state, and native/browser save-load plus driver-switch round trips preserve the exact selected preset.
- Session document version 5 is written. Version-4 stateless OxiSynth sessions migrate to the explicit default state; versions 1 through 3 remain compatible.
- OxiSynth chains still reject Tiny Synth/FX MIDI-CC assignments and do not create automatic recorded-take `fx_state` entries in this MVP.
- Preset selection, processing, and reset remain bounded and allocation-free on the realtime path.
- Existing Tiny Synth/FX, Carla, direct-track, session transaction, native driver, and browser worklet behavior remains intact.
- Formatting, warning-denying native/Wasm builds, the complete shared Rust suite, tracing coverage, and relevant browser validation pass.

## Implementation stages

Stages are sequential unless noted otherwise. Each stage must leave the repository buildable and testable and should be committed independently.

### Stage 0 — Characterize the current contract

- [x] Add or strengthen engine characterization tests for current OxiSynth stereo output, sample offsets, reset, bounded polyphony, and realtime allocation behavior.
- [x] Add backend/session characterization tests identifying every stateless OxiSynth special case in native, in-process engine, application conversion, archive validation, protocol, and remote client paths.
- [x] Record the embedded SoundFont digest, current default bank/program, preset count, banks, and duplicate/range validation results.
- [x] Confirm that OxiSynth 0.1's minimum channel count is enforced and document channel-0-only use as the accepted adapter strategy.
- [x] Record focused native and Wasm commands used as the baseline for later stages.

Verification:

- [x] Existing behavior tests pass before production behavior changes.
- [x] The inventory accounts for processor creation, commands, snapshots, capture/replacement, driver switching, document migration, UI visibility, and native/worklet conversions.
- [x] The chosen default preset is present and renderable in the embedded SoundFont.

Evidence: `docs/oxisynth_simpler_baseline.md` records the digest, 136-preset/two-bank catalog facts, default preset, dependency constraint, full state-free path inventory, transaction boundaries, and baseline commands. Six focused native and Node/Wasm engine tests pass, including the added default-preset and minimum-channel characterizations; focused backend and application stateless tests pass. Formatting, the test-attribute policy, and a warning-denying engine build pass.

### Stage 1 — Implement the narrow engine adapter and state codec

Depends on Stage 0.

- [x] Add a build-time SoundFont preset-header generator and expose one immutable, deterministically ordered preset catalog.
- [x] Add the preset ID, logical SoundFont ID, state, control-state, editor-state, and strict canonical codec types in the engine OxiSynth module.
- [x] Change processor construction to require an explicit validated preset while retaining OxiSynth's mandatory 16 internal channels and disabling drum-channel behavior.
- [x] Store the loaded SoundFont handle and selected preset inside the processor and implement direct `select_program`-based preset changes.
- [x] Make preset changes silence current voices before applying the new preset.
- [x] Replace MIDI translation with channel-0 remapping and explicit rejection of Program Change and CC 0/32 while preserving strict validation and event offsets.
- [x] Keep internal reset/panic behavior from changing the selected preset; reassert the selected preset if a reset operation can affect program state.
- [x] Add session graph access needed to mutate an OxiSynth processor at a scheduled control boundary without exposing OxiSynth itself.
- [x] Narrow visibility of raw synth-construction and translation helpers where external access is no longer required.

Verification:

- [x] Catalog tests prove deterministic unique IDs, valid ranges, expected digest/default, and successful selection of every generated preset.
- [x] Codec tests cover canonical round trip plus malformed, unsupported-version, unknown-font, out-of-range, and unavailable-preset rejection.
- [x] MIDI tests cover all 16 source channels, Program Change, CC 0/32, ordinary CC, pressure, bend, note-on velocity zero, malformed messages, event ordering, and sample offsets.
- [x] Audio tests demonstrate different representative presets render and a preset switch removes old voices.
- [x] Allocation guards cover steady processing, filtered messages, preset-stable reset/panic, and post-selection processing.
- [x] Run focused `shoop_engine` native and Wasm tests, formatting, and a warning-denying `shoop_engine` build.

Evidence: the engine build script generates 136 sorted, unique descriptors directly from the embedded SoundFont and rejects invalid source identities. The narrow control/processor API, canonical state codec, channel-0 filter, drum disablement, direct preset selection, reset-on-switch, and graph accessor are implemented. Ten focused native and Node/Wasm tests pass, including every-catalog-preset selection, strict codec failures, all-source-channel filtering, render differentiation, voice shutdown, and realtime allocation guards. Formatting, test policy, warning-denying engine/app-backend, and backend builds pass.

### Stage 2 — Add typed controls and backend implementations

Depends on Stage 1.

- [x] Extend application/backend API types with OxiSynth editor state and `SelectPreset`/`Panic` controls using stable preset IDs.
- [x] Advertise persistent state in the OxiSynth processor descriptor while leaving embedded UI disabled until Stage 5.
- [x] Change native `FXChainBackendKind::OxiSynth` to own an `OxiSynthControlState` mirror and implement state capture, restore, editor snapshots, preset selection, and panic.
- [x] Queue native preset mutations through the graph scheduler and prepare complete replacement processors for state restoration.
- [x] Replace the in-process engine backend's stateless OxiSynth fields/special cases with typed OxiSynth control, active, and visible state; use a built-in FX enum or equivalent typed separation so Tiny-only logic cannot consume OxiSynth state.
- [x] Support generic active/visible/toggle behavior for OxiSynth in both backend implementations.
- [x] Include canonical OxiSynth processor state in backend session capture and require/restore it during staged replacement.
- [x] Remove backend assumptions that OxiSynth supports only active control or has no persistent state.
- [x] Keep Tiny Synth/FX MIDI-CC assignment handling processor-specific.

Verification:

- [x] Native and in-process backend tests cover creation/default state, descriptor state capability, preset control, visible/toggle state, panic, capture/restore, malformed restore rollback, and unknown preset rejection.
- [x] Backend session replacement tests prove all OxiSynth processors and state are prepared before publishing a replacement.
- [x] Native and in-process evidence covers the shared catalog, selected-preset snapshots, canonical state, transactional restoration, and rendered output after restoration.
- [x] Existing Tiny Synth/FX and processor-routing suites pass.
- [x] Run focused `shoop_backend` and `shoop_engine` tests, formatting, and warning-denying builds for the changed packages.

Evidence: typed OxiSynth controls/editor state now span the application and backend APIs. Native FX chains and the in-process engine backend own validated control mirrors, publish active/visible/editor state, queue or directly apply preset and panic controls, capture canonical state, and prepare replacement processors before commit. Focused native/in-process tests cover preset `0:40`, MIDI rendering, visibility, panic, malformed-state rollback, capture, and replacement. All 60 backend tests with native drivers pass, as do formatting, test policy, and warning-denying backend/application builds. The parity verification wording was clarified to reflect the actual split evidence: rendering is exercised by the in-process backend, while both implementations exercise the same catalog, codec, snapshots, and transactional restore contract.

### Stage 3 — Extend the worklet protocol and remote client

Depends on Stage 2.

- [x] Add wire representations for OxiSynth selected-preset editor state and preset-selection/panic controls.
- [x] Make new OxiSynth snapshot fields defaultable without weakening validation of processor/editor variant consistency.
- [x] Add complete conversions in the AudioWorklet host and remote worklet client.
- [x] Validate remote preset IDs against the advertised OxiSynth catalog before submission.
- [x] Give preset selection its own supersedable optimistic-control key; keep panic ephemeral and non-journaled.
- [x] Carry OxiSynth state through chunked backend session capture/replacement rather than the former stateless exception.
- [x] Ensure replay, restart, and stale-generation handling converge to the authoritative selected preset.

Verification:

- [x] Protocol JSON round-trip and control-coalescing tests cover all new variants without changing existing variant encodings.
- [x] OxiSynth conversion tests plus generic transport/transaction tests cover selection, panic, snapshots, acknowledgement/rejection, replay/restart, stale responses, capture/restore, and malformed-state rollback.
- [x] A worklet render test proves bank/program MIDI is ignored and source-channel notes use the selected preset.
- [x] Run focused protocol, worklet, and client native/Wasm tests plus warning-denying Wasm checks for `shoopdaloop` and `shoop_audio_worklet`.

Evidence: the protocol has defaultable OxiSynth snapshot state, durable/coalescible selection, and ephemeral panic variants. Worklet and client conversions publish and validate stable preset IDs and encode canonical state. All 40 native protocol/worklet/client tests pass, including OxiSynth worklet rendering from source channel 15 while bank/program messages leave preset `0:0`, selection to `0:40`, panic, remote snapshot/state conversion, valid/invalid remote selection, protocol round trips, and coalescing. Generic transport tests cover replay, restart, stale/rejected responses, and chunked transaction behavior shared by the new variants. Native suites pass 6 protocol, 14 worklet, and 20 client tests; Node/Wasm suites pass 6 protocol, 14 worklet, and 19 client tests. Warning-denying Wasm application check and release audio-worklet build pass, as do formatting and the test policy check.

### Stage 4 — Persist and migrate OxiSynth session state

Depends on Stages 2 and 3.

- [x] Increment `SESSION_DOCUMENT_VERSION` from 4 to 5.
- [x] Add a version-4 migration that replaces every valid stateless OxiSynth chain's empty state with the canonical default OxiSynth state.
- [x] Require version-5 OxiSynth chains to have matching chain identity, non-empty processor state, and no Tiny Synth/FX MIDI-CC assignments.
- [x] Update application capture/conversion to store backend OxiSynth state in `FxChainDocument.internal_state` and pass it into backend preparation on load.
- [x] Remove application and native/browser replacement branches that reject or discard OxiSynth processor state.
- [x] Keep semantic state decoding in transactional backend preparation so malformed or unavailable presets fail before session publication.
- [x] Preserve the MVP rule that OxiSynth state is not copied into automatic recorded-take `fx_states`.
- [x] Update `docs/session_format_v1.md` with document version 5, processor-state version 1, migration, validation, failure behavior, and the current-chain versus recorded-take distinction.

Verification:

- [x] Archive tests cover new version-5 output, version-4 default migration, versions 1–3 compatibility, structural state requirements, mismatched chain types, forbidden MIDI mappings, and unknown future versions; backend/application tests cover semantic malformed state.
- [x] Application and backend tests cover save/load plus native, in-process, remote, and driver-switch preservation with a non-default preset.
- [x] Transaction tests prove failed OxiSynth state preparation leaves the prior application/backend session and processing progress intact.
- [x] Saved manifests contain canonical non-empty OxiSynth state and no automatic OxiSynth take-state records.
- [x] Run focused `shoop_session`, `shoop_app`, backend, and worklet-client tests plus the Rust test-attribute policy check.

Evidence: document version 5 writes canonical OxiSynth chain state; version-4 empty chains migrate to `shoop-oxisynth:1:timgm6mb:0:0`, while nonempty legacy state and future document versions are rejected. Structural validation requires a matching nonempty chain and forbids Tiny mappings; semantic decoding remains in staged backend preparation. The application round-trip test selects `0:40`, confirms no recorded-take state during recording, saves/loads it, rejects a malformed load without losing `0:40`, and preserves it over a 48 kHz→44.1 kHz driver switch. Session/application suites pass all 133 tests, backend all 60, and protocol/worklet/client all 40; formatting and test policy pass. Documentation now specifies document/state versions, migration, identity, errors, and take-state scope.

### Stage 5 — Add the embedded OxiSynth editor

Depends on Stages 2 through 4.

- [ ] Add the OxiSynth editor descriptor with the generated preset catalog and enable `embedded_ui` in the processor descriptor.
- [ ] Add application actions, optimistic selected-preset updates, control matching/coalescing, and backend dispatch for OxiSynth selection and panic.
- [ ] Implement a per-track egui OxiSynth editor window using the existing FX visibility mechanism.
- [ ] Provide a scrolling/searchable preset selector whose labels disambiguate bank/program and name, plus a Panic button.
- [ ] Render the authoritative/optimistic selected preset without retaining a second independent UI selection model.
- [ ] Keep editor visibility transient and preserve existing FX lifecycle/color behavior.
- [ ] Update track-control usage documentation for single-preset, channel-flattened, bank/program-filtered OxiSynth behavior on native and browser builds.

Verification:

- [ ] Descriptor and UI tests prove the FX button opens the editor, every catalog item is selectable, bank/program labels are unambiguous, Panic dispatches once, and closing/reopening reflects snapshot state.
- [ ] Application optimism tests cover rapid preset changes, supersession, backend rejection rollback, and convergence after polling.
- [ ] Native and remote UI fixtures produce identical actions and state.
- [ ] Tiny Synth/FX editor, FX logs, recovery, and track-header tests remain unchanged and passing.
- [ ] Run focused `shoop_app` and `shoop_egui` tests, formatting, warning-denying builds, and the Rust test-attribute policy check.

### Stage 6 — Final end-to-end validation

Depends on all prior stages.

- [ ] Create OxiSynth tracks in native dummy/offline, a native physical driver where available, browser Worker/dummy, and browser AudioWorklet runtimes; confirm identical catalog/default/editor behavior.
- [ ] Send notes and controller messages on multiple source channels and confirm one shared instrument/controller state with no channel-9 drum special case.
- [ ] Send live and looped Program Change plus CC 0/32, including start-state and global FX routes, and confirm the selected preset and sound do not change.
- [ ] Change presets while notes are sounding and confirm old voices stop before the new preset renders.
- [ ] Save a non-default preset, reload it in native and browser runtimes, and switch native/browser driver modes; confirm exact preset preservation.
- [ ] Load a version-4 OxiSynth session and confirm migration to the documented default; try malformed, future-version, unknown-font, and missing-preset state and confirm transactional rejection.
- [ ] Confirm dry MIDI media retains its original channel, bank, and program bytes after processor playback and session round trip.
- [ ] Confirm OxiSynth take-specific state remains absent and document the resulting current-preset re-render behavior.
- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `RUSTFLAGS="-D warnings" cargo build --workspace`.
- [ ] Run `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend --profile ci`.
- [ ] Run `python3 scripts/check_shoop_test_usage.py`.
- [ ] Run `python3 scripts/check_tracing_coverage.py --require-closed`.
- [ ] Run the complete Node Wasm suite and the relevant Chrome Wasm packages for engine, backend, protocol, worklet, client, application, session, and egui behavior.
- [ ] Build `shoopdaloop` and `shoop_audio_worklet` for `wasm32-unknown-unknown` and run the packaged browser smoke commands when browser tooling is available.
- [ ] Review documentation, repository searches, and dependency boundaries for stale claims that OxiSynth is stateless, has no editor, preserves MIDI parts, or accepts bank/program changes.

Final verification is complete only when all immutable acceptance criteria have direct automated evidence where practical, manual cross-runtime checks cover the irreducible audio/UI behavior, and the repository is clean after the final milestone commit.
