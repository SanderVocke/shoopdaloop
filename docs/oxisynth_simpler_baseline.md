# Single-preset OxiSynth baseline

This record supports Stage 0 of `oxisynth_simpler.md`. It describes the behavior and ownership boundaries before the single-preset implementation changes production behavior.

## Embedded SoundFont

- Path: `third_party/timgm6mb/TimGM6mb.sf2`
- Size: 5,969,788 bytes
- SHA-256: `c5378b62028c920cb11e4803327983fee2f2cdff5dc89c708e39da417e51c854`
- Presets: 136, excluding the terminal `EOP` record
- Banks: 0 and 128
- Duplicate `(bank, program)` identities: none
- Programs outside `0..=127`: none
- Current default: bank 0, program 0, `Piano 1`

The source preset headers are not ordered by bank/program. A generated runtime catalog must sort them explicitly.

## Dependency constraint

OxiSynth 0.1 validates `SynthDescriptor::midi_channels` with a minimum of 16. The current processor uses 16 channels and leaves `drums_channel_active` at its default `true`. The accepted target is therefore a logically single-channel adapter: retain 16 internal dependency channels, disable drum-channel behavior, and make only channel 0 reachable after Shoop-side MIDI filtering.

## Current behavior and ownership inventory

### Engine adapter and render graph

- `shoop_engine/src/oxisynth.rs` parses one embedded SoundFont for every processor, creates a 16-channel synth, forwards source channel numbers, accepts Program Change and all valid CC messages, and has no preset catalog or state codec.
- Existing engine tests cover the SoundFont digest, stereo rendering, strict MIDI message lengths/ranges, event sample offsets, bounded polyphony, reset, and allocation-free steady rendering.
- `shoop_engine/src/session.rs` owns OxiSynth routes, activation, reset-on-deactivation, and rendering. It has no OxiSynth processor accessor for scheduled preset control.
- `shoop_engine/src/app_backend.rs` creates a processor with implicit defaults. `FXChainBackendKind::OxiSynth` has no control-state mirror, state capture returns an empty string through the generic fallback, and restore explicitly reports that OxiSynth has no persistent state.

### Backend domain and native adapter

- `shoop_backend/src/lib.rs` advertises a fixed 2-dry/2-wet/1-MIDI topology with default processor features and no editor.
- The in-process `EngineBackend` stores Tiny Synth/FX in `track.fx` but stores OxiSynth only as `oxisynth_active`; snapshots synthesize an editor-free FX state.
- In-process session capture emits no OxiSynth processor state, replacement requires no state, and FX control rejects everything except generic active control.
- `shoop_backend/src/native.rs` special-cases OxiSynth capture to `None`, rejects OxiSynth state during replacement, and omits editor state from snapshots.
- Both backend implementations create OxiSynth transactionally as part of track/session staging, but there is currently no state to validate or restore.

### Browser protocol and remote client

- `shoop_audio_protocol` represents OxiSynth topology but has no OxiSynth FX control or editor-state wire variants.
- `shoop_audio_worklet` converts the topology and generic active control only; snapshots have no OxiSynth editor state.
- `shoop_worklet_client` reserves the fixed ports and topology, but has no preset validation, command conversion, optimistic key, or selected-preset snapshot conversion.
- Backend session transfer already contains a generic optional processor-state string, but all OxiSynth producers and validators require it to be absent.

### Application and persistence

- `shoop_app` treats captured OxiSynth state as an error, writes an empty `FxChainDocument.internal_state`, and reconstructs backend session data without OxiSynth processor state.
- Application optimistic FX state and action-key handling only have processor-specific variants for Tiny Synth/FX.
- `shoop_session` document version 4 introduced OxiSynth and validates that its chain state is empty and has no Tiny Synth/FX MIDI-CC assignments.
- Version 4 is currently the only accepted document version that can contain OxiSynth topology; versions 1 through 3 predate it.
- Current documentation explicitly calls OxiSynth stateless and excludes automatic recorded-take OxiSynth state.

### UI and processor catalog

- `shoop_app_api` has Tiny Synth/FX-specific descriptor/editor/action variants but no OxiSynth editor state or control.
- `shoop_egui` only invokes the Tiny Synth/FX embedded editor. The OxiSynth FX button is disabled because its descriptor advertises neither embedded nor external UI.
- Visibility exists in generic FX state, but the in-process OxiSynth backend rejects visibility/toggle controls.

### Driver switching and transaction surfaces

- Native driver switching and browser session replacement both serialize through `BackendSessionData.processor_state` and construct a staged backend before publication.
- OxiSynth is currently excluded at capture and validated as state-free at replacement in both paths.
- The target implementation can use the existing transaction boundary once all OxiSynth state-free exceptions are removed and state is decoded during staging.

## Baseline verification commands

Run commands in the repository's selected development environment.

```sh
cargo nextest run -p shoop_engine -E 'test(oxisynth)'
cargo nextest run -p shoop_backend -E 'test(oxisynth)'
cargo nextest run -p shoop_app -E 'test(oxisynth)'
python3 scripts/run_wasm_tests.py --runtime node --profile dev --package shoop_engine --filter oxisynth
cargo fmt --all -- --check
RUSTFLAGS="-D warnings" cargo build -p shoop_engine
python3 scripts/check_shoop_test_usage.py
```

The complete workspace, Wasm, tracing, and packaged-browser gates remain final-stage requirements rather than Stage 0 baseline requirements.
