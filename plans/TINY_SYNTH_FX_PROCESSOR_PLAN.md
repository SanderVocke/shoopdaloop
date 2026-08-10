# Tiny Synth/FX processor integration plan

## Status

Implementation, integration, and validation are complete on the `tinyviolin` branch at implementation commit `9e05415f`.

The stage checklists below are the original execution scaffold. The final prompt-to-artifact audit is authoritative: every stage, acceptance criterion, verification bullet, named validation gate, and deliverable has direct evidence. This plan may now be removed in the required separate cleanup commit.

### Final implementation and validation ledger (2026-08-10)

- Pinned dependency-free `tinyviolin = "=0.1.0"`, stable `tiny_synth_fx` identity, exact **Tiny Synth/FX** label, runtime presets, matched `N/N` audio plus required MIDI constraints, and zero-audio support are implemented.
- Callback-owned engine DSP covers mono, stereo, seven-channel, and MIDI-only tracks with sample-timed MIDI, panic, distortion, reverb, bypass, smoothed master gain, bounded state, malformed-input handling, variable blocks, and first-active-block/control allocation guards.
- Native dummy/JACK/CPAL, direct-core/Web Audio, role-bearing topology, processor-generic current/take state, transactional replacement, same/different-rate switching, native/browser transfer, and fresh-runtime restoration are implemented and tested.
- Browser protocol v5, bounded/coalesced controls, pipelined bounded session capture, allocation-guarded worklet rendering, browser catalog/proxy/session mapping, and direct plus Tiny zero-audio Web MIDI routes pass hosted and self-contained workflows.
- Application/session persistence and recorded-take mappings plus the backend-free stable-track-ID embedded egui editor pass exact typed-control, close/visibility, and multi-track isolation tests.
- `shoop_egui` remains backend/filesystem/tinyviolin-free; the browser/worklet closure excludes native driver, plugin, frontend, and Qt dependencies; the raw worklet Wasm has no imports.
- EgUI CI run `31436349108` passes all eight Linux/Windows/macOS/WebAssembly debug/release cells, including warning-denying builds, native/default-feature suites, web packages, dependency scans, Chrome hosted/self-contained audio and MIDI suites, extended lifecycle/saturation/stress modes, and Firefox Web Audio.
- Workspace/QML run `31436349093` passes 1,294/1,294 Rust tests, 236/236 retained QML cases, and 6/6 packaged Carla subprocess cases. Local Qt remains unavailable, so the Qt evidence is correctly attributed to authoritative CI rather than the local host.
- Local deterministic evidence includes 23/23 no-default product tests, 55/55 application tests, realtime allocation and sustained variable-block tests, repeated Chrome/Firefox workflows, ten consecutive self-contained Web MIDI route/save/load/restart runs, native/browser artifacts, and dependency/package audits.
- Physical audio/MIDI hardware click-through remains an explicit environment limitation and is not claimed as deterministic completion evidence.

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

## Final prompt-to-artifact audit

Audit result: **green**. No proxy signal, skipped physical-device check, or uncertain result is used as completion evidence.

### Immutable acceptance criteria

| Criterion | Direct artifact and verification evidence |
| --- | --- |
| 1. Capability and identity | `Cargo.toml` pins `tinyviolin = "=0.1.0"`; `shoop_app_api` owns `TrackProcessorTypeId::TINY_SYNTH_FX`, exact label, presets, constraints, and feature facets; native/direct-core/browser catalogs advertise Tiny while native External/Carla remain separate. `tiny_synth_fx_constraints_require_matched_audio_and_midi`, catalog/session regressions, dependency trees, all six native CI cells, and both web cells pass. |
| 2. Topology and processing | `shoop_engine::tiny_synth_fx`, session routing, backend creation, and worklet mapping implement `0..N` matched dry/wet audio plus one MIDI input. Engine tests cover zero/mono/stereo/seven channels, equal synth mix, sample offsets, malformed/unsupported MIDI, note-off, All Notes Off, All Sound Off, effects, panic, smoothing, and sustained variable blocks; backend/worklet tests cover exact roles and non-zero output. |
| 3. Embedded editor | `shoop_egui/src/tiny_synth_fx_editor.rs` owns an in-surface `egui::Window` keyed by stable track ID and emits only typed intents. `embedded_editor_emits_typed_intents_for_every_control_and_close` and `stable_track_ids_isolate_multiple_embedded_editors` cover runtime presets, Panic, gain, reverb, distortion, title close, and isolation; package/dependency audits show no child-window or backend dependency. |
| 4. State and sessions | `shoop_engine::tiny_synth_fx` implements strict `shoop-tiny-synth-fx:1:` state, finite `-60..=0 dB` gain, bounded canonical tinyviolin bytes, and prepared transactional restore. `shoop_session`, backend, application, native, and worklet tests cover exact current/take state, malformed rollback, fresh runtime, native-browser-native transfer, channel/block-layout changes, and 48 kHz to 44.1 kHz driver switching; fresh load proves editor visibility and live DSP history remain transient. |
| 5. Realtime and browser safety | `tiny_synth_fx_first_block_and_controls_are_allocation_free`, engine realtime lock/allocation suites, and allocation-guarded worklet tests pass. Processor/scratch construction and state coding are control-path work; displaced processors return for off-callback destruction. Protocol capacity/order/coalescing tests pass; session capture uses a bounded eight-command window; the AudioWorklet remains the sole hosted audio clock. |
| 6. Validation evidence | Focused suites, 1,294-test workspace archive, 236 retained QML cases, packaged Carla subprocess cases, local native/Wasm/package/browser checks, and authoritative CI runs `31436349108` and `31436349093` all pass. |

### Stage and verification closure

| Stage | Implementation evidence | Verification evidence |
| --- | --- | --- |
| 1. Contracts and persistence | Workspace pin; API constraint/preset/editor/control types; Tiny session topology/chain/state variants; generic processor-state backend fields; session-format/parity/project docs. | API shapes `0/1/2/7` and rejection tests; Tiny current/take codec validation; existing Carla fixtures; `cargo tree` confirms dependency isolation and zero tinyviolin dependencies. |
| 2. Engine adapter | Callback-owned `TinySynthFxProcessor`, zero-channel silent plane, timed MIDI dispatch, effects/gain, prepared restore, control checkpoint, dynamic route ports. | Tiny engine unit suite, 614-test local no-default engine surface, sustained variable blocks, malformed state rollback, and first-active-block/control no-allocation guard pass. |
| 3. Native/direct backends | Tiny catalogs and topology in `EngineBackend` and `NativeBackend`; role-bearing ports; capture/replace/remap; driver switches; External/Carla paths retained. | Shared backend `0/1/2/7` shape/state tests, native dummy audible MIDI and native-browser-native transfer, same/different-rate switch evidence, 44 backend tests in native-FX configurations, and External/Carla regressions pass. |
| 4. Worklet protocol/proxy | Protocol v5 topology, typed controls/snapshots/state, ordered/coalesced journal, bounded transfer, worklet host route, browser prediction/remap. | Protocol round-trip/order/capacity/malformed/version tests; allocation-guarded worklet zero/mono/stereo/seven processing and post-replacement callbacks; CI dependency/import scans pass in debug and release. |
| 5. Application/session ownership | Typed descriptor/topology/control mapping; constrained Add Track; current/take capture/restore; preflight and transactional replacement; generic resampling. | `tiny_synth_fx_round_trips_controls_and_recorded_state`, fresh-runtime and 48 kHz to 44.1 kHz assertions, native/browser transfer, malformed restore retention, save-while-playing browser flow, and existing Direct/External/Carla fixtures pass. |
| 6. Embedded editor | Stable-ID presentation state, header show/hide, close synchronization, runtime selector and all controls, capability-gated Carla/Tiny surfaces. | Two exhaustive backend-free editor tests plus minimum/common viewport and product presentation suites pass; `shoop_egui` dependency audit remains clean. |
| 7. Product closure | Chrome/Firefox automation includes Tiny creation, MIDI wet output, all controls, panic, editor visibility reset, save/load, continuing callbacks, and exact direct plus Tiny Web MIDI route restoration; docs and packages updated. | Debug/release native/web artifact verification, hosted/self-contained microphone/output-only/offline/MIDI/lifecycle/saturation/stress suites, Firefox, package/import checks, and physical-device limitation attribution all pass. |

### Named final validation gates and deliverables

| Plan gate or deliverable | Outcome |
| --- | --- |
| `cargo fmt --all -- --check` and `git diff --check` | Passed on the final implementation tree and in CI. |
| Warning-denying workspace/all-target/default/no-default builds | The local exact all-target command stops only at missing Qt discovery; equivalent authoritative split gates pass with `-D warnings`: the Qt-enabled workspace build/test in `31436349093` and all native/default plus Wasm/no-default cells in `31436349108`. No local Qt success is claimed. |
| Focused tests then full `cargo test --workspace --features shoop_engine/app_backend` surface | Focused package suites pass; the authoritative archived workspace run reports **1,294 run, 1,294 passed, 0 skipped**. Native audio-sensitive workflow tests are serialized in the egui matrix. |
| Realtime guards and sustained variable-block Tiny processing | `tiny_synth_fx_first_block_and_controls_are_allocation_free` and `sustained_variable_block_processing_remains_finite_and_active` pass, together with the full realtime guard archive. |
| Wasm product check and debug/release worklet builds | Local warning-denying product check passes; both web CI cells build/check the product and raw worklet, whose import scan is empty. |
| Debug/release hosted and self-contained artifacts and workflows | CI packages and verifies both profiles; local artifacts exist under `artifacts/tiny-debug`, `artifacts/tiny-release`, and `artifacts/tiny-native`; Chrome and Firefox production workflows pass. |
| Retained product/QML/External/Carla | `31436349093` runs the packaged retained product self-test: 236/236 QML and the additional 6/6 Carla subprocess cases pass. |
| Authoritative native/browser session workflow | Combined engine/backend/app/editor/worklet/browser tests cover mono, stereo, seven, zero, MIDI/effects/panic/visibility, wet take state, save, fresh load, sample-rate switch, take restore, and continuing callbacks with zero budget/protocol regressions. |
| Eight-cell Linux/Windows/macOS/WebAssembly debug/release matrix | All eight jobs in `31436349108` completed successfully. |
| Durable documentation | `docs/session_format_v1.md`, `plans/EGUI_FEATURE_PARITY_MATRIX.md`, `plans/EGUI_REPLACEMENT_PROJECT.md`, and `src/rust/shoopdaloop_egui/README.md` describe the shipped contract and evidence. |
| Runtime/package assets | No Tiny plugin, worker, child window, filesystem resource, or new native runtime asset is required; native/web archives contain only the existing product assets. |
| Compatibility and integrated MIDI keyboard | Both `InjectTrackMidiInput` and host Web MIDI paths pass backend/worklet/application/browser tests; workspace/QML/Carla regressions pass on the rebased combined implementation. |
| Skip/limitation record | Local Qt and physical hardware are unavailable. Qt/QML is covered by authoritative CI; physical click-through is deliberately not claimed. Safari remains outside the named deterministic gate. |
| Commit and cleanup contract | Implementation and evidence are committed through `9e05415f`; this final audit is committed separately before the plan-only cleanup commit. |

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
