# Tiny Synth/FX processor integration plan

## Status

Implementation complete on the `tinyviolin` branch; final integration and repository-wide validation are in progress.

The stage checklists below are the original execution scaffold. This status and the validation ledger are authoritative. The branch is now rebased onto the `origin/master` change that introduced the MIDI keyboard and related `shoop_engine` work. Do not mark this milestone complete or remove this plan until the combined paths pass the pending gates below.

### Current implementation and validation ledger (2026-08-10)

Implemented and focused-verified:

- pinned dependency-free `tinyviolin = "=0.1.0"`, stable `tiny_synth_fx` identity, exact **Tiny Synth/FX** label, runtime presets, matched `N/N` audio plus required MIDI constraints, and zero-audio support;
- callback-owned engine DSP for mono, stereo, seven-channel, and MIDI-only tracks, including sample-timed MIDI, panic, distortion, reverb, bypass, smoothed master gain, strict bounded state, malformed-input handling, and first-active-block/control allocation guards;
- native dummy/JACK/CPAL composition, direct-core/Web Audio composition, role-bearing topology, processor-generic current/take state, transactional replacement, and same-rate driver-switch preservation;
- browser protocol v5, bounded/coalesced control journal, allocation-guarded worklet rendering, transactional state publication, browser catalog/proxy/session mapping, and zero-audio Web MIDI routing;
- application/session persistence and recorded-take mappings, plus a backend-free stable-track-ID embedded egui editor with typed tests for every control and close/visibility behavior;
- hosted Chrome 147 Web Audio and Web MIDI Tiny workflows, hosted Firefox 153 Web Audio, Chrome self-contained offline and output-only workflows, and debug/release hosted plus self-contained packages;
- warning-free no-default native and Wasm checks, debug/release AudioWorklet builds, no-default native debug/release builds/packages, a warning-free default native-FX build, and 42/42 backend tests with native FX enabled;
- dependency audits showing `shoop_egui` remains backend/filesystem/tinyviolin-free and the browser/worklet closure excludes native driver, plugin, frontend, and Qt dependencies;
- after rebasing onto the MIDI-keyboard changes, the combined protocol/worklet/backend/engine/app/egui/session no-default-feature suites pass serially, including 610 engine tests and realtime guards, and the no-default-feature product suite passes 23/23. The earlier two timeout-sensitive product failures did not reproduce in the required serialized run.

Pending or requiring a clean rerun after integrating `origin/master`:

- rerun focused and broad suites against the integrated MIDI-keyboard and Tiny MIDI/callback paths;
- the warning-denying all-target workspace build reached the retained Qt/CXX-Qt packages and is blocked locally because Qt is not installed; retained QML self-tests are unavailable for the same reason;
- run the serialized full workspace suite on the integrated commit;
- obtain the authoritative Linux/Windows/macOS/WebAssembly debug/release CI matrix for the integrated commit;
- complete the final prompt-to-artifact audit, inspect the integrated diff, and only then perform the separate implementation and plan-cleanup commits required by this plan.

## Goals and scope

Add the published dependency-free `tinyviolin` library as a first-class dry/wet track processor in the pure-egui application under the user-facing name **Tiny Synth/FX**.

The implementation covers:

- native JACK, CPAL+midir, and dummy/offline compositions;
- hosted and self-contained WebAssembly/Web Audio compositions;
- `tinyviolin::AudioProcessor` audio-input mixing, MIDI synthesis, post-effects, panic, preset discovery, and state APIs;
- matched `N`-channel input/output processing for `N = 0`, mono, stereo, and arbitrary supported session channel counts, with one MIDI input path;
- an embedded egui editor window and typed application/backend controls;
- current and recorded-take FX state in `.shoop` sessions, including native/browser and restart round trips.

This does not add Tiny Synth/FX to the retained Qt/QML product, add a native child window, expose manual tinyviolin MIDI-layer editing, or modify the tinyviolin crate. Existing External and Carla behavior remains supported.

## Immutable acceptance criteria

1. **Capability and identity**
   - Native and browser egui builds advertise one stable processor identity whose label is exactly **Tiny Synth/FX**.
   - It is available without JACK, LV2, Carla, a subprocess, filesystem access, or browser-incompatible dependencies.
   - Native External and Carla identities, hosting modes, controls, and session compatibility do not regress.
2. **Topology and processing**
   - A Tiny Synth/FX track has `N` dry audio inputs, `N` wet audio outputs, and one dry MIDI input, where `N` may be zero and mismatched dry/wet counts are rejected before mutation.
   - Mono, stereo, and an arbitrary count greater than two process each input channel independently through one `AudioProcessor`; MIDI synthesis is mixed equally into all `N` channels before distortion/reverb and master gain.
   - The zero-audio/MIDI-only shape is valid and remains bounded and stable even though it has no audible output.
   - Supported sample-timed MIDI note, note-off, All Notes Off, and All Sound Off messages reach tinyviolin without allocating or locking in the audio callback. Unsupported MIDI does not fail or silence the audio quantum.
3. **Embedded editor**
   - The track FX control opens/closes an `egui::Window` inside the existing application surface; no OS/native child window is created on any target.
   - The window obtains preset IDs/names from tinyviolin and provides preset selection, Panic, master gain, reverb enable/amount, and distortion enable/drive.
   - Closing the window and the track FX show/hide action remain synchronized and are isolated by stable track ID when multiple Tiny Synth/FX tracks exist.
4. **State and sessions**
   - Current processor state and compatible recorded-take state preserve tinyviolin mappings/selected preset and effect settings plus Shoop-owned master gain exactly through save/load and application restart.
   - Sessions move between native and browser builds without flattening or substituting the processor.
   - Processor state is validated and restored transactionally before the replacement session is published; malformed, unsupported, or unavailable state leaves the running session usable.
   - Voices, oscillator phase, effect tails, panic history, and editor visibility remain transient and are not session data.
5. **Realtime and browser safety**
   - `Session::process` performs no allocation, deallocation, locking, I/O, logging, or state encoding/decoding for Tiny Synth/FX, including the first processed block and control changes.
   - AudioProcessor construction, channel scratch sizing, state serialization/deserialization, and replacement preparation happen off the realtime path; bounded commands hand prepared changes to callback-owned DSP state.
   - The AudioWorklet remains the sole hosted-browser audio clock, and UI/control traffic remains bounded and cannot block rendering.
6. **Validation evidence**
   - Focused engine/backend/application/session/presentation/protocol/worklet tests, realtime guards, native and Wasm builds, session round trips, browser workflows, packaging checks, the full Rust workspace suite, and retained QML regressions pass.

## Design rules and constraints

- Use the investigated crates.io baseline `tinyviolin = "=0.1.0"` (Rust 1.85, no dependencies) and its public `AudioProcessor`, preset enumeration, panic, effect, and state APIs; do not copy its preset table or DSP into ShoopDaLoop. A later version requires an explicit state/API compatibility review and a documented design-rule revision.
- Keep `shoop_egui` presentation-only and free of a tinyviolin dependency. Put plain preset/editor state and typed intents in `shoop_app_api`; adapt those types to tinyviolin below the backend boundary.
- Extend processor constraints with explicit channel-coupling and MIDI-cardinality policy. Tiny Synth/FX requires equal dry/wet audio counts and one MIDI path; External and Carla retain their current independent-count/optional-MIDI policies.
- Use stable IDs in contracts and session documents, for example `tiny_synth_fx`; use **Tiny Synth/FX** only as the display label.
- Treat bytes from `AudioProcessor::serialize_state` as the canonical tinyviolin payload. Store them in a versioned, text-safe Shoop processor-state envelope together with master gain; decode with strict size/version/range checks before calling `load_state`.
- Implement master gain after `AudioProcessor` output with allocation-free smoothing. Follow tinyviolin's showcase convention unless implementation evidence warrants a documented design-rule revision: `-60..=0 dB`, `-6 dB` default, with short click-free smoothing.
- Select the first runtime-advertised built-in preset for a new chain so MIDI produces sound without hard-coding a preset enum; preserve stable preset IDs in state.
- Keep callback-owned DSP and control/UI state coherent without sharing a GUI mutex with the callback. State capture reads an authoritative control-side checkpoint; prepared processor replacement and displaced-storage reclamation must not allocate or free in `process`.
- Preserve the existing dry/wet routing policy for monitoring, record, play-dry, and dry-to-wet re-record. Tiny Synth/FX adds a processor implementation, not a second routing model.
- Generalize internal names such as `carla_state` when they begin carrying non-Carla state; do not encode Tiny Synth/FX behind a Carla-specific document/runtime variant.
- Bump the browser audio protocol when adding processed topology, FX controls, or FX snapshot state. Coalesce only supersedable parameter updates; never coalesce Panic, topology, state restore, or transactional session commands.
- Keep processor availability target-driven: Tiny Synth/FX is cross-target; External remains native; Carla remains native and feature/discovery dependent.
- Preserve the transaction and resource-limit rules in `docs/session_format_v1.md`. Unknown future processor/state versions fail explicitly rather than being interpreted as another processor.

## Staged implementation

Stages are ordered; a later stage depends on the contracts and evidence from earlier stages.

### Stage 1 — Freeze cross-layer contracts and persistence representation

- [ ] Add the pinned tinyviolin workspace dependency and stable `TrackProcessorTypeId::TINY_SYNTH_FX` identity.
- [ ] Extend `shoop_app_api` with:
  - [ ] equal-channel and required/optional/unsupported MIDI constraint semantics;
  - [ ] runtime preset descriptors;
  - [ ] plain Tiny Synth/FX editor state;
  - [ ] typed preset, gain, effect, panic, and visibility actions;
  - [ ] an embedded-editor capability distinct from Carla's external UI/recovery/log capabilities.
- [ ] Add `TinySynthFx` topology/chain/state identities to `shoop_session`, retaining decoding of existing Direct, External, Carla, and Test documents.
- [ ] Define the versioned text-safe processor-state envelope and resource limits for tinyviolin bytes plus master gain.
- [ ] Rename internal backend session fields from Carla-specific to processor-generic terminology and update wire/session fixtures without changing existing `.shoop` Carla representation.
- [ ] Update `docs/session_format_v1.md`, `plans/EGUI_FEATURE_PARITY_MATRIX.md`, and `plans/EGUI_REPLACEMENT_PROJECT.md` with the frozen identity, topology, persistence, and target-capability contract.

**Verification**

- [ ] API tests accept `N/N + one MIDI` for `N = 0, 1, 2, 7`, reject mismatched counts or missing MIDI, and leave External/Carla validation unchanged.
- [ ] Session codec/validator tests round-trip Tiny Synth/FX current and recorded state, reject malformed/mismatched state, and continue decoding existing Carla fixtures byte-exactly.
- [ ] Dependency checks confirm tinyviolin has no native-only transitive dependency and `shoop_egui` still does not depend on it.

### Stage 2 — Build the callback-safe tinyviolin engine adapter

- [ ] Add a callback-owned Tiny Synth/FX route in `shoop_engine` that constructs `AudioProcessor` and all planar audio/MIDI/gain scratch storage before activation.
- [ ] Support logical zero-channel operation without asking `AudioProcessor::new` for its invalid zero-channel layout; advance or safely discard MIDI/DSP work through a prepared silent adapter while exposing no audio ports.
- [ ] Route matched input planes through `AudioProcessor::render_range`/MIDI dispatch at sample offsets, then apply smoothed master gain and write matching wet planes.
- [ ] Filter/handle tinyviolin's supported MIDI subset so malformed or unsupported Shoop MIDI cannot abort the block.
- [ ] Add bounded controls for preset, effects, gain, panic, active state, and prepared state replacement; publish a small authoritative editor snapshot.
- [ ] Extend the threaded application-backend FX handle with dynamically sized Tiny Synth/FX ports and control-side state checkpoints, while leaving Carla direct/subprocess handling separate.
- [ ] Ensure setup, serialization, deserialization, and displaced DSP destruction run outside realtime processing.

**Verification**

- [ ] Engine tests prove input preservation/mixing, non-zero MIDI synthesis on every output, post-effects, gain, panic, bypass/active behavior, and sample-timed events for mono, stereo, seven-channel, and zero-channel layouts.
- [ ] State tests prove tinyviolin payload and host gain round-trip across different block sizes and channel counts, and invalid state leaves the prior processor unchanged.
- [ ] Existing allocation and lock guards cover active Tiny Synth/FX processing, first-block processing, parameter changes, panic, and graph/session routing without adding a realtime exception.

### Stage 3 — Integrate native and direct-core backends

- [ ] Advertise Tiny Synth/FX from both `NativeBackend` and `EngineBackend`, unconditionally with respect to the `native-fx`/LV2 feature; query its preset catalog from tinyviolin.
- [ ] Generalize `EngineTrack`/loop channel bookkeeping from direct-only channels to role-bearing dry/wet audio and dry MIDI, reusing the existing dry/wet routing state machine.
- [ ] Implement Tiny Synth/FX track creation, dynamic internal processor ports, controls, snapshots, current-state capture, and compatible recorded-state restore in the native threaded backend.
- [ ] Implement the same topology and processor controls in direct-core dummy/Web Audio `EngineBackend` so the browser worklet and native deterministic tests use the same engine DSP route.
- [ ] Extend native driver switching and both backend session replacement paths to stage, restore, and remap Tiny Synth/FX tracks, media roles, connections, controls, and state transactionally.
- [ ] Keep External/Carla catalog entries and native behavior unchanged, including Carla logs, recovery, external UI, and hosting mode.

**Verification**

- [ ] Shared backend contracts create/process/capture/replace Tiny Synth/FX tracks at 0, 1, 2, and more-than-2 channels and verify exact role/port ordering.
- [ ] Native threaded tests cover dummy plus available native driver compositions, current and recorded-state restore, dry playback/re-record, driver replacement, and builds with and without `native-fx`.
- [ ] Regression tests cover External unequal shapes and all Carla variants/hosting controls.

### Stage 4 — Extend the Web Audio/AudioWorklet protocol and proxy

- [ ] Bump `shoop_audio_protocol` and carry typed processed-track topology, Tiny Synth/FX controls, processor/editor snapshots, and processor-generic session state.
- [ ] Extend `WorkletHost` to create Tiny Synth/FX tracks, dispatch bounded controls, return state, and accept transactional Tiny Synth/FX session replacement while rendering remains callback-owned.
- [ ] Extend `WebAudioBackend` port prediction/remapping and snapshots for dry input, wet output, and one MIDI input; advertise Tiny Synth/FX while continuing to reject External and Carla.
- [ ] Coalesce high-rate gain/effect slider commands by track/parameter and retain strict ordering for preset, panic, visibility, restore, and session transfer.
- [ ] Preserve arbitrary internal track counts independently of the two-channel physical Web Audio host boundary and existing deterministic host-channel mixing.

**Verification**

- [ ] Protocol round-trip, ordering, capacity, coalescing, malformed-command, and version-mismatch tests cover every new message.
- [ ] Allocation-guarded worklet tests create mono, stereo, seven-channel, and zero-channel Tiny Synth/FX tracks; process audio/MIDI/effects; update controls; panic; capture; and replace sessions while callbacks continue.
- [ ] `wasm32-unknown-unknown` dependency scans still reject JACK, CPAL, Midir, LV2/Carla, Qt/frontend, native windowing, and Wasm imports from the raw worklet module.

### Stage 5 — Complete application ownership and session workflows

- [ ] Map Tiny Synth/FX descriptors, topology, snapshots, and typed actions through `shoop_app` without exposing engine handles to presentation.
- [ ] Make Add Track automatically enforce equal audio counts and one MIDI input when Tiny Synth/FX is selected while preserving independent External/Carla drafts.
- [ ] Capture Tiny Synth/FX state when wet recording starts, restore only matching recorded state, and report stale/malformed/capacity errors as typed notifications.
- [ ] Save/load current and referenced recorded-take state through the new Tiny-specific session variants, including resampling and native driver/session replacement.
- [ ] Preflight processor availability, channel shape, and state before backend mutation on both native and browser targets.

**Verification**

- [ ] Application tests cover creation, typed control routing, snapshot publication, panic, visibility, exact current/take state, incompatible take rejection, and rollback on failed restore.
- [ ] Native-to-browser, browser-to-native, restart-style fresh-runtime, same/different sample-rate, and save-while-playing round trips preserve topology/media/state; malformed input retains the old generation.
- [ ] Existing direct, External, and Carla application/session fixtures remain green.

### Stage 6 — Add the custom embedded egui editor

- [ ] Add a stable-ID Tiny Synth/FX editor window owned by `TrackWidget` presentation state and driven only by immutable snapshots/typed intents.
- [ ] Make the track FX button show/hide the editor; make the window close control emit the corresponding visibility action.
- [ ] Render the runtime preset selector, Panic button, master-gain control, reverb toggle/amount, and distortion toggle/drive, disabling dependent controls when their effect is bypassed.
- [ ] Keep Carla's external editor/recovery behavior and log context menu capability-driven and unchanged.
- [ ] Ensure multiple tracks, track reorder, narrow/common viewport painting, session replacement, and stale track removal do not cross-wire or retain orphan windows.

**Verification**

- [ ] Backend-free `shoop_egui` tests click every editor control and assert exact typed intents without a tinyviolin/backend dependency.
- [ ] Presentation tests prove header open, title-bar close, stable-ID isolation, runtime preset labels, no log/recovery controls for Tiny Synth/FX, and no extra native window API.
- [ ] Minimum/common viewport paint tests remain panic-free and preserve existing track/loop controls.

### Stage 7 — Product automation, documentation, and milestone closure

- [ ] Extend Chrome and Firefox automation to create Tiny Synth/FX tracks, feed MIDI, observe non-zero wet output, change gain/effects/preset, panic, close/reopen the editor, save/load, and verify continuing callbacks.
- [ ] Cover hosted and self-contained output-only and microphone artifacts; retain Web MIDI and lifecycle/saturation workflows.
- [ ] Add native deterministic workflow coverage with `--no-default-features` and default/native-FX builds so Tiny Synth/FX availability is proven independent of Carla.
- [ ] Update the egui README, session format, processor capability text, browser behavior, package checks, parity matrix, and project coarse status with actual evidence and remaining limitations.

**Verification**

- [ ] Debug/release native and web artifacts contain the feature and no new native runtime asset, plugin, worker, or child-window requirement.
- [ ] Browser package/import/dependency checks and native package verification pass.
- [ ] Recorded evidence distinguishes deterministic software/browser validation from any unavailable physical-device testing.

## Final end-to-end validation

- [ ] Run `cargo fmt --all -- --check`.
- [ ] Build the workspace with `RUSTFLAGS="-D warnings" cargo build --workspace --all-targets --features shoop_engine/app_backend` and build `shoopdaloop_egui` both with default features and `--no-default-features`.
- [ ] Run focused package tests while iterating, then `cargo test --workspace --features shoop_engine/app_backend` with serialized execution where required by native audio tests.
- [ ] Run engine realtime allocation/lock guards and Tiny Synth/FX sustained variable-block processing.
- [ ] Run `cargo check -p shoopdaloop_egui --no-default-features --target wasm32-unknown-unknown` and debug/release `shoop_audio_worklet` Wasm builds.
- [ ] Build and verify debug/release hosted and self-contained browser artifacts, inspect dependency closure/imports, and run the Tiny Synth/FX Chrome/Firefox workflows alongside the retained audio/MIDI/lifecycle/session suites.
- [ ] Build the retained product and run `target/debug/shoopdaloop_dev.sh --self-test` to prove QML/External/Carla regressions remain green.
- [ ] Confirm the authoritative native/browser session workflow: create mono, stereo, arbitrary-channel, and zero-channel Tiny Synth/FX tracks; process audio/MIDI; edit all controls; panic; show/hide the embedded editor; record wet state; save; restart into a fresh runtime; load; restore a recorded take; and continue processing without xrun/protocol/storage regressions.
- [ ] Run or obtain the authoritative eight-cell Linux/Windows/macOS/WebAssembly debug/release egui CI matrix before marking the feature complete.
- [ ] Record commands, counts, browser/platform versions, artifacts, skips, and limitations in this plan and the parity/project documents, then mark the milestone complete only when every acceptance criterion has evidence.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
