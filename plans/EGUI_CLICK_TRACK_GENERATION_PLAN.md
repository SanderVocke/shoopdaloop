# egui click-track generation plan

## Status and document role

Status: **Complete**. Stages 0–4 and every immutable acceptance criterion are implemented and audited against the evidence recorded below.

This is the implementation contract for adding the legacy loop-scoped click-track workflow to the pure-egui product. It depends on the session/media transaction delivered by the persistence milestone and must remain synchronized with:

- `EGUI_FEATURE_PARITY_MATRIX.md` for capability-level discovery, status, and evidence;
- `EGUI_REPLACEMENT_PROJECT.md` for coarse project architecture and progress;
- every other document in `plans/` whose current cross-reference, limitation, or status is affected. Completed milestone plans remain historical ledgers: update stale current statements, but do not rewrite their frozen scope or evidence.

## Investigation findings

- The legacy entry point is **Click loop...** in every primitive loop context menu, including the sync loop. `ClickTrackDialog.qml` limits the kind to the target's available audio/MIDI channels and defaults to Audio when both exist.
- The legacy draft defaults are primary `click_high`, secondary `click_low`, three secondary clicks per primary, 100 clicks/minute, four clicks, zero percent odd-click delay, MIDI note 64, and a 0.1-second note. Its installed catalog is the sorted set of WAV stems under `resources/clicks/`; the repository currently contains `click_high`, `click_low`, `shaker_primary`, and `shaker_secondary`.
- A primary click followed by N secondary clicks repeats cyclically. Odd zero-based clicks are delayed by the configured percentage of one click interval. The loop duration remains `floor(clicks * 60 / bpm * sample_rate)` frames, so click tails at the end are truncated.
- Audio generation reads one channel from each source WAV, resamples each source to the active backend rate, mixes overlapping clicks, copies the generated mono waveform to every audio channel, resets audio start offset/preplay, and sets the loop length.
- MIDI generation writes note-on/note-off pairs to every MIDI channel, resets MIDI start offset/preplay, and sets the same click-grid loop length. The visible QML contract supplies velocity 127 and permits fractional BPM, but the current Rust bridge accidentally reads velocity from the note list and accepts BPM as an integer. This plan proposes honoring the visible inputs—velocity 127 and fractional BPM—rather than preserving those implementation defects. That proposed intentional difference must be explicitly approved with this plan before implementation.
- **Fill loop length** derives BPM from the current loop length, selected click count, and active sample rate. Audio-only Preview plays the generated result without changing the target loop; preview failure is currently logged rather than represented in application state.
- The egui loop context menu already owns exact/WAV audio and exact/standard-MIDI I/O intents. `shoop_app` already performs generation-adjacent work through a serialized I/O task, captures the backend session, changes only target-loop media, replaces transactionally, and remaps stable application IDs.
- `shoop_session` is the intended home for generated media and already owns target-neutral WAV decoding, deterministic resampling, `LoopAudio`, and `ExactMidi`. `shoop_egui` must remain presentation-only and cannot decode assets or generate media.
- The egui product is cross-target. Built-in click WAVs therefore need to be compiled into the relevant target-neutral crate rather than discovered from a native filesystem. Preview playback needs target adapters: native playback must stay off the application/audio callback, while browser playback must obey Web Audio gesture/lifecycle policy.

## Goals and scope

Deliver a loop context action and egui dialog that generate the same configurable audio or MIDI click content as the legacy visible workflow, including built-in click selection, primary/secondary patterns, tempo, click count, odd-click delay, loop-length fitting, and non-mutating audio preview.

In scope:

- target-neutral generation in `shoop_session` using the four repository click assets;
- plain click catalog/configuration/status types and typed intents in `shoop_app_api`;
- application-owned validation, preview requests, serialized generation, transactional target-loop replacement, and failure reporting in `shoop_app`;
- loop context entry and stable-`LoopId` dialog state in `shoop_egui`;
- native and browser preview adapters and production composition in `shoopdaloop_egui`;
- fake/dummy, native-driver, Web Audio/AudioWorklet, packaging, browser, retained-QML, and documentation regression evidence;
- continuous maintenance of all documents in `plans/` affected by implementation status or evidence.

Out of scope:

- a continuously running metronome independent of loop content;
- user-installed click packs or runtime filesystem discovery beyond the four compiled-in repository sounds;
- per-click velocity/channel sequences, tempo maps, time signatures, or generated standard-MIDI files;
- changing the `.shoop`, `.shoop-audio`, or `.shoop-midi` formats;
- changing legacy QML behavior or deleting its generator in this milestone;
- click generation for regular/script composite loops, which have no primitive target channels.

## Immutable acceptance criteria

1. A primitive sync or main loop with audio and/or MIDI channels exposes **Generate click track...** in its egui context menu. Composite or channel-less loops do not offer a nonfunctional action.
2. The dialog offers only kinds supported by the target loop and defaults to Audio when both are available. It exposes primary/optional secondary audio sound, secondary clicks per primary, clicks per minute, number of clicks, odd-click delay percentage, MIDI note, and MIDI note length with inline validation and Generate/Cancel actions.
3. A newly initialized draft matches the legacy visible defaults: `click_high`, `click_low`, three secondary clicks, 100 clicks/minute, four clicks, 0% odd delay, MIDI note 64, channel 0, velocity 127, and 0.1-second note length. Draft ownership is keyed by stable `LoopId`, so another loop cannot receive a stale dialog's output.
4. Timing uses the active backend sample rate and deterministic checked arithmetic. The output duration is `floor(click_count * 60 / bpm * sample_rate)` frames; each click starts at `floor(index * 60 / bpm * sample_rate)`, with odd indices additionally delayed by the selected interval percentage. Fractional BPM remains effective.
5. Audio generation embeds and exposes all four repository WAVs by stable sorted stem, converts each to mono using its first source channel, resamples it to the active rate, cycles the selected primary/secondary pattern, sums overlaps, and truncates at the exact output duration. Missing, malformed, non-finite, overflowing, or over-limit input fails visibly without partial output.
6. Generating audio copies the same mono result into every direct audio channel of the target loop, resets those channels' start offset and preplay to zero, and adopts the generated duration. Existing MIDI content and unrelated loops/tracks remain byte/value-equivalent.
7. Generating MIDI writes ordered note-on/note-off events with the selected note, channel 0, velocity 127, and selected duration into every target MIDI channel, resets those channels' start offset and preplay to zero, and adopts the click-grid duration. Invalid or out-of-range event timing is rejected or bounded before backend mutation; existing audio content and unrelated loops/tracks remain unchanged.
8. **Fill loop length** computes `bpm = click_count * 60 * sample_rate / current_loop_frames`, preserves fractional precision, and is disabled with a clear reason for a zero-length loop, zero/unknown sample rate, or invalid click count.
9. Audio Preview uses the current audio draft and active sample rate, never mutates session/loop state, and does not enter the MIDI generation path. Playback is asynchronous and bounded; native and browser success/failure are reported without blocking egui or an audio callback, and stale preview completions cannot overwrite newer status.
10. Generate is one application-owned, generation-checked transaction. It rejects stale loops, unsupported kinds, active recording/replacement, and conflicting session/media/generation tasks before mutation. Capture/preparation/replace failure leaves the prior session usable and reports an actionable error; success publishes only after backend replacement and stable-ID remapping complete.
11. Successful generation preserves target track/loop application IDs, name, gain, balance, selection/target state, topology, port identities, compatible host links, scripts, and global state. Transport follows the existing loop-media replacement contract and does not introduce a second mutation path.
12. Generation and preview have explicit finite click-count, frame, event, and byte limits validated before allocation or integer conversion. Decoding, resampling, generation, preview setup, and backend replacement run only on control/application/platform paths; no filesystem access, allocation, lock, media decoding, or platform playback is added to a realtime callback.
13. `shoop_egui` continues to depend only on plain API/settings/presentation crates. Click assets and generation stay in `shoop_session`; application policy stays in `shoop_app`; backend/session mutation stays behind the existing backend contract; native/browser playback APIs stay in `shoopdaloop_egui`.
14. Native JACK/CPAL/dummy and browser Web Audio/offline compositions generate equivalent media at the same sample rate. Hosted and self-contained browser artifacts contain the built-in sounds through compiled Wasm data, require no source checkout/network asset fetch, and retain existing native-package and Wasm dependency isolation.
15. `EGUI_FEATURE_PARITY_MATRIX.md` and `EGUI_REPLACEMENT_PROJECT.md` are updated in every stage that changes discovery, architecture, status, or evidence. Before each stage commit, audit every other file in `plans/` and update affected current statements/cross-references while preserving historical acceptance criteria and evidence.

## Design rules and constraints

- Treat this as generated loop media, not a transport/metronome subsystem. Reuse `LoopAudio`, `ExactMidi`, session resampling, capture/replacement, task publication, and stable-ID remapping rather than adding direct engine-handle mutation.
- Keep generator DTOs internal to `shoop_session`; convert framework-independent API request types at the application boundary instead of making `shoop_session` depend on `shoop_app_api`.
- Publish only small immutable catalog/configuration/progress data in `AppSnapshot`. Generated samples and preview payloads use bounded out-of-band queues analogous to file outputs and never enter snapshots.
- Embed the source WAV bytes once from `resources/clicks`. Use one catalog implementation for native and Wasm, stable filename-stem IDs, deterministic ordering, and explicit decode errors; do not add a native-only resource scan.
- Preserve the current media transaction's unrelated-channel behavior: audio generation changes audio only, MIDI generation changes MIDI only, and both update loop length. Do not serialize generator settings into sessions; generated media persists as ordinary loop content.
- Centralize timing and checked-limit validation so audio and MIDI cannot disagree about beat starts or output length. Validate finite positive BPM, nonzero click count/sample rate, delay in 0–100%, MIDI 0–127, note duration in the visible 0–10 second range, and repository-defined resource limits.
- If the final note-off would fall outside the loop, use one documented deterministic boundary rule that cannot leave a stuck note; lock it with tests before UI integration.
- Preview is a platform service, not a backend route or hidden loop. Native may reuse the existing target-gated playback stack; browser must use a user-gesture-compatible Web Audio path and must not restart, replace, or steal ownership from the production AudioWorklet.
- Keep completed plans historical. New evidence belongs in this plan and the living project/matrix documents unless a completed plan contains a genuinely stale current-status statement.

## Staged implementation plan

Dependencies are sequential unless explicitly stated otherwise. Complete, verify, document, and commit each stage before beginning its dependent stage.

### Stage 0 — Freeze the generated-media contract and baseline fixtures

- [x] Add plain API types for click kind, sound descriptors/IDs, validated draft/request data, preview state, generation task kind, and typed preview/generate intents; expose loop length frames needed by **Fill loop length** without exposing backend handles.
- [x] Add `shoop_session` timing, audio, and MIDI generator APIs with checked limits and structured errors. Embed and decode the four WAV fixtures, and convert API requests in `shoop_app` rather than coupling the crates.
- [x] Characterize the legacy formulas, default audio pattern, odd-click delay, tail truncation, source-rate conversion, and MIDI bytes in deterministic tests. Record the approved visible-contract corrections for fractional BPM and velocity 127.
- [x] Choose and test the final-note boundary rule and exact finite capacities before allowing generation requests.
- [x] Revalidate and refine the planned catalog/dialog, timing/audio, MIDI, fill, preview, transaction, and cross-target parity rows against implementation findings; mark this milestone **In progress** in the project document when execution begins and audit all other plan documents.

Verification:

- [x] `cargo test -p shoop_app_api -p shoop_session` passes defaults, identity, validation, overflow/NaN/zero, exact frame starts/duration, 0/100% odd delay, overlap/truncation, 44.1↔48 kHz fixtures, pattern cycling, MIDI order/boundary, and deterministic repeated-generation tests (10 API and 21 session tests in the recorded focused run).
- [x] Native and `wasm32-unknown-unknown` checks prove the generator uses no Qt/frontend, filesystem discovery, native decoder, or platform playback dependency; the focused Wasm check and forbidden dependency scan pass.
- [x] Commit the contract/generator milestone and synchronized planning updates.

### Stage 1 — Add application-owned preview output and transactional loop replacement

- [x] Extend application state/runtime handles with a bounded out-of-band preview queue and small revisioned preview status; keep PCM out of immutable snapshots.
- [x] Handle preview intents by validating the current catalog/rate/config, generating mono PCM, assigning a request generation, and enqueueing one bounded platform payload without changing I/O/session state.
- [x] Add click generation to the serialized I/O state machine. Reject conflicts and active recording/replacement, generate target media, capture the current backend session, modify only the matching target channels/length/offset/preplay, and reuse the existing replace/remap commit path.
- [x] Support every target audio or MIDI channel rather than assuming one MIDI channel; preserve the opposite media kind and all unrelated session data.
- [x] Add stale-request, backend-pending, replacement-failure, and preview-completion reporting with bounded notifications and no optimistic success.

Verification:

- [x] Fake-backend and application actor/cooperative tests cover sync/main loop foundations, mixed audio/MIDI targets, stable IDs, all-audio-channel copies, all-target implementation paths, opposite-kind preservation, exact length, stale/conflict/active-recording rejection, injected replacement failure/no mutation, backend pending behavior, and successful remap.
- [x] Preview tests prove no capture/replacement or loop mutation, bounded queue behavior, generation ordering, and stale platform completion suppression.
- [x] Existing session/media import/export and native-driver switch tests remain green in the 46-test application, 20-test backend, 21-test session, and 10-test API focused run; commit the application transaction milestone and update all affected planning status/evidence.

### Stage 2 — Implement the egui context action and dialog

- [x] Add **Generate click track...** to applicable loop context menus and route the exact stable target ID through `TrackWidget`, `TracksWidget`, and the sync-track path.
- [x] Add one resizable egui dialog with kind-specific controls, the legacy defaults, catalog-driven selectors, numeric validation, **Fill loop length**, audio-only Preview, Generate, and Cancel.
- [x] Retain presentation drafts by stable loop ID, reconcile removed/stale loops and changed capabilities safely, and prevent context menus or reordered tracks from retargeting an open draft.
- [x] Render preview/generation running, completion, and actionable failure state without blocking; keep ordinary media-I/O dialogs and settings/connections independent.
- [x] Update loop-control user documentation for the generated content, timing, defaults, preview, and session persistence behavior; update planning documents in the same stage.

Verification:

- [x] Backend-free egui tests cover menu applicability, sync/main stable routing, default values, audio/MIDI capability reconciliation, catalog selection, fractional BPM, invalid fields, fill calculation/disabled states, preview enablement, exact Generate/Preview intents, Cancel draft retention, stale target, and dialog reopen behavior.
- [x] Paint tests pass at 360×200 and 900×600 with validation/status surfaces and both kind-specific control paths.
- [x] `cargo test -p shoop_egui -p shoop_app_api` passes 51 presentation and 10 API tests, and the focused Wasm compiler check passes without adding session/backend/platform dependencies; commit the presentation milestone and synchronize final user docs with working platform preview in Stage 3.

### Stage 3 — Compose native/browser preview and production assets

- [x] Add a native preview adapter that consumes application preview payloads off the actor/UI critical path, owns playback resources until completion, bounds concurrent previews, and returns generation-tagged success/failure.
- [x] Add a browser preview adapter that starts only from the Preview click gesture, uses/respects the current Web Audio lifecycle, remains separate from render callbacks and session routes, and reports unsupported/denied/suspended failures truthfully.
- [x] Consume preview outputs and dispatch completions in the unified runner for native threaded and browser cooperative runtimes, including clean shutdown and stale-completion handling.
- [x] Extend native workflow and browser automation to open the real dialog, preview without mutation, generate audio and MIDI into the authoritative engine/worklet session, export/inspect exact results, and continue callback progress.
- [x] Extend package/marker checks to prove compiled-in clicks are present in native, hosted, and self-contained products with no external resource-directory dependency.
- [x] Update the runner README and every affected plan document with supported preview behavior, browser policy, and current evidence.

Verification:

- [x] The 23-test runner suite proves native `NativeBackend` non-mutating valid-draft preview with truthful no-hardware completion, exact audio/MIDI generation after transactional session load, playback, generated-media save/reload, plus bounded malformed-preview failure. This host exposes no usable default ALSA playback device, so audible native preview is an explicit environment skip rather than a success claim.
- [x] Debug hosted Chrome passes the complete generated audio export, non-mutating preview, exact MIDI export, and callback-continuity self-test with 21,920 callbacks. Firefox 150 under Xvfb passes the same production flow with 6,704 callbacks. Self-contained Chrome passes the explicit offline-dummy flow, including fallback-context preview.
- [x] Warning-denying native/Wasm checks, Trunk 0.21.14 UI/worklet build, Python syntax, debug native/hosted/self-contained package verification, and compiled click-marker checks pass. Worklet import and full forbidden-dependency scans are repeated in Stage 4.
- [x] Commit the cross-target composition/artifact milestone and synchronized plans/documentation.

### Stage 4 — Final end-to-end validation and closure

- [x] Run `cargo fmt --all -- --check` and `git diff --check`.
- [x] Run focused warning-denying tests for `shoop_session`, `shoop_app_api`, `shoop_backend`, `shoop_app`, `shoop_egui`, and `shoopdaloop_egui`, including native-driver features.
- [x] Run `RUSTFLAGS="-D warnings" cargo build --workspace --features shoop_engine/app_backend` and `SHOOP_ALLOW_MISSING_BACKENDS=1 RUSTFLAGS="-D warnings" cargo test --workspace --features shoop_engine/app_backend`.
- [x] Build first, then run `QT_QPA_PLATFORM=offscreen target/debug/shoopdaloop_dev.sh --self-test` to retain the QML behavior oracle.
- [x] Build/verify debug and release native, hosted WebAssembly, self-contained HTML, and AudioWorklet artifacts using the locked workflow commands; rerun Chrome/Firefox normal, minimum-size, lifecycle, settings, session/media, Web MIDI, offline, and direct-file regressions in addition to click workflows.
- [x] Exercise end to end on sync and mixed audio/MIDI main loops: defaults, alternate sound pattern, fractional BPM, 0/100% odd delay, fill existing loop, preview/no mutation, generate, play, export, save/load, and sample-rate-changing driver/session operations.
- [x] Record exact test counts, platform/browser/audio environment, explicit skips, limits, and residual browser policy in this plan. Reconcile all click rows and project status with concrete evidence.
- [x] Audit every other document in `plans/` for stale status, limitations, cross-references, or roadmap text; update affected documents without altering frozen historical contracts, then commit the validation/documentation milestone.

### Completion evidence (2026-08-10)

- Formatting, `git diff --check`, warning-denying native workspace build, warning-denying focused native-driver tests, and warning-denying Wasm checks pass. The focused suites contain 182 tests: `shoop_session` 21, `shoop_app_api` 10, `shoop_backend` 30, `shoop_app` 46, `shoop_egui` 52, and `shoopdaloop_egui` 23. The final complete workspace run contains 1,222 passing tests across 78 result sets.
- The unchanged QML oracle passes all 236 test cases under `QT_QPA_PLATFORM=offscreen`. Its existing engine-gone, changing-content, queue-pressure, and Carla diagnostic messages do not produce test failures.
- Trunk 0.21.14 builds the release UI and import-free release AudioWorklet with explicit LLVM `lld`. Native Linux x86_64, hosted Wasm zip, and self-contained HTML debug/release package verification finds all four embedded click markers. UI/worklet `cargo tree` scans contain none of the forbidden native/frontend dependencies; `WebAssembly.Module.imports` reports zero worklet imports.
- Chrome 147 release hosted click/session self-test passes at 900x600 with 11,524 callbacks after the final capability change; Chrome also passes 360x200, deny-first, restart lifecycle, saturation, output-only, settings rejection/recovery, Web MIDI track/control/hotplug/restart, direct-file, and explicit self-contained offline modes. Firefox 150.0.1 under Xvfb passes the final hosted production click/session self-test with 4,648 callbacks. No workflow reports an unexpected console error.
- This Linux host has no `/dev/snd/seq` and no usable default ALSA playback device. Native MIDI/backend tests therefore use their documented `SHOOP_ALLOW_MISSING_BACKENDS=1` environment skip, and audible native preview is not claimed. The runner now probes default-output availability and disables Preview with an explanation when none exists; bounded failure reporting remains unit-tested. Browser preview succeeds from the Preview gesture in Chrome/Firefox and uses a gesture-created fallback context in explicit offline mode. Browser autoplay/user-media policy remains authoritative: hosted full-duplex audio requires its enable gesture, while output-only and preview use their own user gestures.
- Shared generator tests prove the four sorted decoded assets, default/alternate patterns, fractional BPM, exact floor duration and starts, 0/50/100% odd delay, truncation, 44.1/48 kHz resampling, velocity 127, final-boundary clamping, invalid values, and the 4,096-click/10,000,000-frame limits. Dialog tests prove stable sync/main IDs, applicability, retained drafts, exact defaults, Fill math/disabled reasons, minimum sizing, stale-target closure, validation, and unavailable-preview disabling.
- Fake, native, and worklet application workflows prove all-channel replacement, opposite-media preservation, offset/preplay reset, stable IDs, no partial mutation on injected failure or conflicts, non-mutating/stale preview handling, exact audio/MIDI export, playback/callback continuity, deterministic save/load, and existing sample-rate-changing driver/session conversion behavior. The native runner additionally verifies generated stereo audio identity/non-silence, exact MIDI events at frames 0/4,800/4,800/9,599 with velocity 127, play state, and stable 9,600-frame save/reload through `NativeBackend`.

### Immutable-criterion audit

1. Context-entry applicability and exact stable sync/main routing are covered by loop/track/widget tests and browser production-dialog opening.
2. Audio/MIDI-specific controls, resizable/minimum layouts, live validation, and limits are covered by all dialog paint/action/validation tests.
3. Defaults are shared plain API values; catalog resolution, source selectors, no-runtime-resource fallback, and unavailable-preview disabling are tested and package-verified.
4. Checked fractional timing, floor duration, primary/secondary cycling, odd delay, and finite limits are generator-tested.
5. Embedded WAV decode, first-channel use, resampling, overlap/truncation, all-channel copy, and loop length are generator/application/browser-tested.
6. MIDI note-on/off ordering, velocity 127, all-channel copy, exact frame boundaries, and loop length are generator/application/browser-tested.
7. Fill uses click count, sample rate, and existing frame length; precision and every disabled reason are tested.
8. Preview/generation use the application actor and transaction path, never UI/render/audio callbacks; bounded queues and warning builds pass.
9. Native/browser preview adapters, non-mutation, replacement ordering, stale generations, and truthful success/failure/unsupported results are tested; platform policy is recorded above.
10. Capture/prepare/replace/remap preserves stable IDs and unrelated/opposite media; failures/conflicts leave capture unchanged across fake/native/worklet evidence.
11. Exact exports and deterministic session save/load retain generated audio/MIDI and loop length.
12. Embedded markers pass debug/release native, hosted, and self-contained package checks without an external resource directory.
13. Native dummy and Chrome/Firefox worklet product workflows generate, play/inspect, export, and preserve callback progress; physical native output is the explicit environment skip above.
14. Warning-denying native/Wasm builds/tests, QML oracle, browser matrix, package verification, worklet-import inspection, and dependency scans all pass.
15. User/runner documentation and every affected planning document are synchronized; milestone commits preserve the implementation ledger and the final audit commit leaves a clean tree.

Final acceptance evidence must include one authoritative native workflow and one production browser workflow where a click draft is previewed without session mutation, generated into a real target loop, observed with exact expected frame/MIDI timing and length, played through the existing backend, saved/loaded as ordinary media, and followed by continuing application/audio progress. The final source/package audit must prove that egui artifacts use the shared generated-media implementation and embedded click assets without a Qt/frontend or runtime resource-directory dependency.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
