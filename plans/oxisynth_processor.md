# OxiSynth processor implementation plan

## Goals and scope

Add **OxiSynth** as a built-in, SoundFont-based track processor alongside External, Carla, and Tiny Synth/FX. The processor uses the Rust `oxisynth` crate and the embedded `TimGM6mb.sf2` SoundFont, is available in every native and WebAssembly application build, and is operated entirely through incoming MIDI.

This work includes the engine processor, native and AudioWorklet backend integration, shared API/wire/session representations, generic processor discovery and track creation, embedded-asset provenance, documentation, and tests. It does not include an OxiSynth-specific editor, app controls for synth parameters or preset selection, loading user SoundFonts, or changing the SoundFont at runtime.

## Immutable acceptance criteria

- Every supported native and browser build advertises an available processor with stable ID `oxisynth` and display label **OxiSynth**.
- A user can create, save, load, and remove an OxiSynth track through the existing generic processed-track workflow; loading is transactional and never silently substitutes another topology.
- Each OxiSynth track has no dry audio inputs, exactly two wet audio outputs (left/right), and exactly one required MIDI input.
- The implementation uses a pinned `oxisynth` crate dependency and renders `f32` stereo audio at the active backend sample rate.
- Standard MIDI channel messages supported by OxiSynth—including notes, controllers/bank selection, program changes, channel pressure, and pitch bend—are forwarded with their in-block timing preserved. Unsupported or malformed MIDI is ignored safely, and system reset/all-notes-off behavior cannot leave permanently stuck voices.
- `TimGM6mb.sf2` is checked into the repository with documented origin, redistribution/license information, and a pinned digest, and is compiled into the executable/wasm module rather than fetched or read from the filesystem at runtime.
- Native and WebAssembly implementations use the same processor and embedded SoundFont bytes and have equivalent MIDI-to-audio behavior.
- Audio callback/worklet rendering performs no heap allocation, blocking I/O, locks, logging, or panicking; parsing the SoundFont and sizing processor scratch state happen before publication to the realtime graph.
- OxiSynth has no processor-specific UI or non-MIDI parameter/control API in this version. Existing generic track creation, activation/bypass, routing, and deletion affordances remain available.
- Existing session files and existing processor types remain backward compatible.

## Design decisions and constraints

These are deliberate major design choices for the initial implementation:

1. **Model OxiSynth as a synth-only dry/wet processor with shape `0 dry audio / 2 wet audio / 1 dry MIDI`.** Do not overload the matched-channel `TinySynthFx` topology. Extend the generic `DryWetProcessor` contract and add explicit OxiSynth variants only where the persisted and worklet formats currently use closed topology enums.
2. **Use one OxiSynth instance per track.** Configure 16 MIDI channels, stereo output, the backend sample rate, a fixed reviewed polyphony limit, and OxiSynth's built-in chorus/reverb defaults. MIDI bank select and program change choose TimGM6mb presets; channel 10 follows General MIDI percussion behavior.
3. **Keep synthesis state transient.** Advertise `state: false` and `editor: None`; session data persists the processor topology and ordinary track state, but not voices, effects tails, current programs/controllers, or a duplicate SoundFont blob. Recorded MIDI must contain any bank/program/controller setup needed for deterministic playback after a fresh load. Do not create OxiSynth variants in `TrackProcessorEditorState` or `TrackAction`.
4. **Parse the embedded bytes on the control path for each track initially.** This keeps ownership and native/wasm behavior simple and avoids introducing shared mutable synthesizer state. If profiling later proves parsing or memory duplication unacceptable, immutable parsed-SoundFont sharing may be introduced without changing the public/session contract.
5. **Render sample-accurately by sub-blocking at MIDI event offsets.** Translate complete MIDI messages into `oxisynth::MidiEvent`, render up to the next event, apply all events at that frame in stable input order, then render the remainder. Reuse preallocated stereo/scratch buffers and define deterministic handling for events at or beyond the current quantum.
6. **Vendor the exact SoundFont as a reviewed binary asset, not a generated download.** Record its authoritative source URL, version/date if available, SHA-256, size, copyright, and redistribution terms beside it. Redistribution verification is a release gate: implementation must not ship an asset whose terms are missing or incompatible with all app distribution targets.
7. **Pin the crate version and review target support before integration.** Start with crates.io `oxisynth` 0.1.x with default features disabled unless a required feature is identified; do not enable SF3 or randomized/i16 output. Confirm the selected dependency graph builds for `wasm32-unknown-unknown` and document the crate's LGPL obligations in the existing third-party notices/package metadata.
8. **Do not add bespoke UI.** The capability-driven generic add-track selector may show OxiSynth and enforce its fixed shape, but track widgets must not gain an editor button, parameter panel, SoundFont picker, or program selector.

## Execution contract

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

## Implementation stages

Stages are ordered; a stage may start only after its dependencies below are complete.

### Stage 1 — Dependency, asset, and realtime feasibility

- [x] Pin `oxisynth` in the workspace and engine manifests with the minimum feature set, update the lockfile, and confirm its license/dependency metadata and `wasm32-unknown-unknown` compatibility.
- [x] Add the exact `TimGM6mb.sf2` binary under a dedicated resource/third-party location plus a human-readable provenance/license notice and machine-checkable SHA-256 metadata.
- [x] Add an asset integrity test that parses the `include_bytes!` payload with OxiSynth and verifies its digest, expected identity/preset availability, and non-empty stereo rendering from a fixed MIDI note.
- [x] Audit or instrument OxiSynth construction, `send_event`, and `write` calls for realtime behavior; select fixed polyphony/buffer limits and record any required upstream workaround before graph integration.

**Verification:** build the engine for native and `wasm32-unknown-unknown`; run the focused asset/render test; run license/metadata checks; prove the render path passes the repository's no-allocation guard after warm-up.

**Completed:** `oxisynth` 0.1.0 is pinned without default features (LGPL-2.1; pure-Rust dependency graph), both engine targets compile, and the embedded 5,969,788-byte GPL SoundFont matches SHA-256 `c5378b62028c920cb11e4803327983fee2f2cdff5dc89c708e39da417e51c854`. The focused 48 kHz stereo render and warmed-up no-allocation assertion pass with 16 MIDI channels and fixed polyphony 256.

### Stage 2 — Engine processor and realtime graph

Depends on Stage 1.

- [x] Add an engine-owned OxiSynth processor wrapper that constructs from embedded bytes off the audio thread, validates sample-rate/channel settings, preallocates scratch storage, translates valid MIDI messages, and renders two output channels without allocation.
- [x] Specify and test MIDI translation for note on/off (including velocity-zero note-on), poly/channel pressure, CC/bank select, program change, pitch bend, all-notes-off/all-sound-off, and MIDI System Reset (`0xFF`); safely reject truncated, SysEx, other realtime, and unsupported system messages, and verify System Reset releases active voices.
- [x] Add an OxiSynth backend variant to the engine processor route, lifecycle/activation hooks, port registration, processing dispatch, and teardown. Preserve ordered sample offsets by splitting rendering at event boundaries and write silence when inactive.
- [x] Add focused engine tests for stereo output, timing at block boundaries, multi-channel program/drum behavior, activation/reset/removal, malformed MIDI, sample-rate variation, bounded event capacity, and no allocation/no realtime lock violations.

**Verification:** run the OxiSynth engine tests plus existing session scheduling, MIDI, no-allocation, and realtime-lock suites on the dummy backend.

**Completed:** the callback-owned processor uses preallocated stereo planes, strict message-length/range validation, stable in-block sub-blocking, explicit reset/all-sound-off handling, and an OxiSynth engine route that resets on deactivation and publishes silence while inactive. Focused translation, stereo, event-offset, malformed-input, reset, and warmed-up note-event allocation tests pass; the shared bounded MIDI staging and route lifecycle continue to provide capacity, activation, removal, and teardown behavior.

### Stage 3 — Shared catalog and native backend

Depends on Stage 2.

- [x] Extend `TrackProcessorConstraints` with minimum or exact audio-channel bounds, update every descriptor, validator, selector, and construction consumer to preserve existing processor behavior, and add acceptance tests proving under- and over-sized shapes are rejected.
- [x] Add `TrackProcessorTypeId::OXISYNTH` and an always-available descriptor with exact fixed constraints (`dry=0`, `wet=2`, required MIDI), no editor, and no persistent processor state/recovery/log features.
- [x] Generalize native processed-track construction where necessary, create the stereo wet ports and MIDI dry port, instantiate OxiSynth transactionally before publishing track state, and include it in every native catalog independently of Carla/native-driver feature flags.
- [x] Ensure generic active/bypass behavior, snapshots, driver switching, session replacement, loop creation, routing, and cleanup recognize OxiSynth without adding processor-specific actions.
- [x] Extend backend contract tests for descriptor constraints, successful and invalid shapes, rollback after construction failure, processor identity, port roles, audio generation, deletion, and driver-switch reconstruction.

**Verification:** run shared API and backend tests with the minimal, native-driver, and native-fx feature combinations; run warning-denying native builds to prove OxiSynth is present with and without Carla.

**Completed:** shared constraints now enforce lower and upper audio-channel bounds without changing existing descriptor behavior. Native and engine catalogs always expose the stateless/no-editor `oxisynth` descriptor; native construction validates the fixed shape before building an embedded processor, publishes exactly two output ports plus one MIDI input, and uses the generic active/routing/removal lifecycle. Focused API, descriptor, invalid-shape, native port-role, activation, and removal tests pass with the native-driver feature set.

### Stage 4 — Web protocol, worklet, and client

Depends on Stage 3.

- [x] Add an OxiSynth topology to the serialized audio protocol while retaining compatibility with existing wire messages; update conversion/exhaustiveness tests.
- [x] Advertise the same descriptor from the browser client, validate the fixed shape, and transport OxiSynth creation/session replacement requests without introducing editor/control messages.
- [x] Instantiate and run the same engine wrapper inside `shoop_audio_worklet`, including the embedded SoundFont in the actual worklet wasm artifact; retain bounded quantum processing and transactional error reporting.
- [x] Extend protocol, client, worklet, and browser tests for creation, MIDI-driven stereo output, program changes, activation, removal/recreation, malformed input, rollback, and native/browser snapshot parity.

**Verification:** run wasm unit/browser tests and dependency-isolation checks; build actual debug and release web artifacts and verify they contain no runtime SoundFont fetch/file access and remain within the repository's artifact-size budget (updating the documented budget deliberately for the embedded asset if required).

**Completed:** protocol version 13 adds the explicit `oxisynth` topology and stateless processor snapshots while preserving all prior variants. The browser client advertises identical fixed constraints and reserves two output/one MIDI resources; the worklet constructs the shared embedded engine processor transactionally, routes its stereo output to Web Audio, and accepts only generic activation controls. Protocol round trips and wasm test compilation pass. The release worklet wasm is 8,234,254 bytes (SHA-256 `c3f68a0743bbc79c7b2877b22184e846ff054423c06df48c8f78c59994ec4028`) and contains the embedded asset with no fetch or filesystem path.

### Stage 5 — Session persistence and generic application integration

Depends on Stages 3 and 4.

- [ ] Introduce session document version 4 for the additive `OxiSynth` track topology and chain type, update archive dispatch/current-version metadata, and add explicit migrations from the currently supported version-1 through version-3 documents; represent only the fixed channel shape and processor identity.
- [ ] Save/load OxiSynth tracks without processor-state entries; reject mismatched chain/topology, illegal channel layouts, unavailable runtimes, or unexpected OxiSynth state transactionally while preserving all older documents.
- [ ] Include OxiSynth in the capability-driven generic track-creation flow with its fixed stereo/MIDI shape, and ensure the ordinary track widget does not offer an editor action when `editor: None`.
- [ ] Add round-trip, malformed-document, session replacement, recorded MIDI playback, generic selector, and no-editor regression tests for native and browser paths. Update the session-format and user/developer documentation, third-party attribution, and build/package descriptions.

**Verification:** run session, app model/controller, and egui tests; load representative old Direct/External/Carla/Tiny sessions; save and reload an OxiSynth session on native and browser backends and compare topology, routing, and audible output.

### Stage 6 — End-to-end validation

Depends on all previous stages.

- [ ] Run formatting, warning-denying workspace builds, repository policy scripts, the complete native test suite, and all supported native feature/build variants.
- [ ] Run the complete wasm test suite and browser smoke matrix against the actual packaged worklet/application artifacts, including MIDI-only startup followed by audio startup/restart.
- [ ] Exercise a real native MIDI input and Web MIDI input: select programs/banks, play melodic and channel-10 percussion notes, use sustain/pitch bend/controllers, record and replay the MIDI loop, toggle active state, switch/restart the audio driver, and verify no stuck notes or crashes.
- [ ] Inspect native and wasm release packages to confirm the exact pinned SoundFont is embedded, no separate/runtime download is required, required OxiSynth/SoundFont notices ship, Carla-optional builds still advertise OxiSynth, and package-size changes are documented.
- [ ] Profile sustained high-polyphony native and AudioWorklet playback for callback time, allocations, locks, underruns, event-capacity behavior, and memory use; resolve regressions before declaring the feature complete.

**Final evidence:** record the tested native platform/feature matrix, browser matrix, exact commands, artifact hashes/sizes, SoundFont digest/license review, realtime profiling results, and a saved-session round trip. All immutable acceptance criteria must have direct passing evidence.
