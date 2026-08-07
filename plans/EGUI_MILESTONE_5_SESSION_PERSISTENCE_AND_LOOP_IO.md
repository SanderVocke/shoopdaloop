# egui Milestone 5: Session Persistence and Loop Media I/O

## Status and relationship to the replacement project

**Status:** Complete

This is the next major milestone in `EGUI_REPLACEMENT_PROJECT.md`. It expands the persistence and media rows in `EGUI_FEATURE_PARITY_MATRIX.md` and makes the existing tracks/loops slice usable across runs on native and WebAssembly targets.

The QML application remains a behavior-discovery reference for session-scoped features, but its archive and media formats are not compatibility targets. Relevant discovery sources include `Session.qml`, `LoopWidget.qml`, the loop channel/port/FX descriptors, `generate_session.js`, and the retained session/content-snapshot tests.

## Goal

Deliver complete, transactional session save/load and individual-loop audio/MIDI import/export through the pure-egui application. The implementation must preserve exact settled loop content, all session-scoped state represented by the format, cycle timing, external connection rules, and opaque Carla state while keeping playback running during save. The same authoritative workflows and archive/media codecs must work in native and browser builds.

## Scope

Included:

- A target-neutral `shoop_session` persistence/media crate.
- A fresh, versioned `.shoop` v1 archive and typed session document.
- Full current egui session round trips and format coverage for known deferred session features.
- Exact, compressed, arbitrary-channel session audio.
- Sample-accurate Shoop MIDI plus standard MIDI import/export.
- Individual-loop audio and MIDI load/save dialogs and channel mapping.
- Sample-rate conversion, warnings, task progress, cancellation, and errors.
- Native filesystem and browser upload/download/file-handle adapters.
- Backend and AudioWorklet snapshot/load transactions needed for non-blocking persistence.

Not included:

- Implementing the UI or runtime behavior of dry/wet track creation, Carla hosting, composites, scripting, MIDI control, buses, or settings that remain assigned to later milestones. Their typed persistent forms must nevertheless be representable, validated, and preserved. A session requiring a runtime feature that is not implemented must fail before replacing the running session; it must never be partially loaded or silently simplified.
- Importing or writing QML-era `.shl`, `session.1`, tar/JSON/FLAC archives, or JSON `.smf` files. Selecting one must produce a clear unsupported-format error without changing the current session.
- Persisting transient engine state such as current play/record mode, playhead position, queued transitions, meters, xruns, task/dialog state, browser permissions, or device handles.
- Persisting machine-wide device/application preferences in the session. A versioned settings document may later use the same versioning machinery, but session-local overrides and mappings belong in `.shoop`.
- Lossy audio export.

## Immutable acceptance criteria

1. **Cross-target operation.** Native, hosted Wasm, and the self-contained Wasm artifact can save and load `.shoop` v1 sessions and perform the supported loop I/O workflows without Qt, QML, `frontend`, native filesystem assumptions, or synchronous work in an audio callback.
2. **Version checks and evolution.** Every archive and embedded media document carries an explicit format/schema version. Readers dispatch by version, reject unsupported older or future major versions before the current session changes, and provide an ordered migration boundary for future supported revisions. Malformed data and unsupported runtime topology are reported transactionally. QML-era archives are intentionally outside the format and receive a clear unsupported-format error.
3. **Complete session document.** The schema covers all known session-scoped application state listed below, including stable identities/references, ordering, global performance controls, topology, external connection/autoconnect names, audio/MIDI content metadata, composite/script/MIDI-control payloads, buses, FX chains, and byte-for-byte Carla state strings. Currently implemented state round-trips through the authoritative application; deferred feature fixtures round-trip through the codec without being dropped.
4. **Exact and compressed content.** Session audio stores IEEE-754 `f32` bits losslessly and uses lossless archive compression. MIDI bytes, start state, same-frame ordering, source-frame positions, channel duration, loop length, offsets, and cycle relationships round-trip exactly at the same sample rate. Archive writing supports ZIP64-size output and has no application-level fixed channel-count limit; practical allocation/resource failures are explicit.
5. **Arbitrary loop channel count.** Session and native loop-media formats use `u32` counts/indices and per-channel payloads rather than a codec aggregate-channel ceiling. Application/backend/protocol APIs remove the current `u8`/10-channel persistence limits. Browser device I/O may remain constrained by the negotiated Web Audio input/output shape, but it must not prevent storing, loading, playing through a documented deterministic mix, or exporting loops with more channels.
6. **MIDI I/O.** The exact Shoop MIDI format uses integer source-frame timestamps and explicit start-state/duration metadata; no floating-point seconds are used. Standard `.mid` import honors tempo maps, merges selected tracks in stable absolute-time order, and retains all supported MIDI/SysEx bytes. Standard export uses the highest practical standard timebase, reports its bounded timestamp quantization, and writes duration/end-of-track information.
7. **Audio I/O.** A loop can import into an explicit mapping of source channels to any direct/dry/wet destination channels and optionally adopt the imported duration. It can export any selected ordered channel set. Native builds expose every readable format supported by the selected native sound-file adapter; all targets support float WAV and the compressed, bit-exact Shoop loop-audio format. Format/channel limits are capability-advertised and never silently drop or reorder channels.
8. **Sample-rate conversion.** Loading a session or loop asset with an explicit source rate different from the running rate first presents a warning containing source rate, target rate, and affected media. Cancel leaves state untouched. Accept performs high-quality audio resampling and deterministic rational conversion of loop/channel lengths, offsets, preplay, exact MIDI event frames, and other sample-domain values. Same-frame MIDI ordering remains stable, converted events remain within the converted duration, and cycle scheduling does not gain a spurious extra cycle.
9. **Playback-safe save.** Saving a settled session while loops are playing does not stop, transition, restart, starve, or lock playback. Scalar state and every audio/MIDI channel come from one validated session/content generation; compression and output run outside realtime processing. A content mutation such as an active recording/load/clear is handled by an explicit wait/retry/cancel or typed rejection, never by mixing generations. The UI may be modal while the task runs.
10. **Transactional load/import.** Read, decompression, limits, hashes, version/schema/reference validation, capability checks, and resampling complete before commit. Backend topology/content are staged while the old session continues; one acknowledged control-boundary commit replaces it. Failure or cancellation at any earlier point leaves the old session fully usable. Loaded loops begin stopped rather than restoring transient transport state.
11. **Safe, observable tasks.** Save/load/import/export expose bounded progress, cancellation where safe, completion, and actionable errors in immutable snapshots. Native save uses temporary output plus atomic replacement. Browser save completes validation/compression before publishing a download, or uses a transactional writable file handle when available. Archive readers reject duplicate/unsafe paths, size/count overflows, hash mismatches, and decompression bombs.
12. **Architecture and regression safety.** `shoop_egui` remains presentation-only and backend/filesystem-free; `shoop_app_api` remains framework-independent; persistence does not add Qt/native/audio-driver dependencies to Wasm trees. Existing native/browser audio, connection, loop-control, warning-free workspace, realtime guard, and QML regression gates continue to pass.

## Session and media design rules

### Container and versioning

- Use a fresh `.shoop` extension for a ZIP64 archive with a required root `manifest.json`; do not reuse or sniff the QML-era `.shl` container.
- `manifest.json` identifies `format = "shoop-session"`, an integer `{ major: 1, minor: 0 }` container version, a typed document schema version, writer app version, source sample rate, resource counts, media entries, and hashes. Container and document versions are independent.
- Major versions are incompatible unless an explicit migration exists. Minor additions must be optional and defaultable. Keep version decoding separate from the current typed model so future supported revisions can add pure, ordered `vN -> vN+1` migrations without changing runtime consumers.
- Use deterministic entry names/order and normalized JSON so equivalent fixtures are reviewable and reproducible. Validate paths and declared sizes before allocating or decoding.
- Deflate each payload with CRC plus a manifest SHA-256. Audio is stored as per-channel little-endian `f32` bitstreams, allowing exact values and any channel count without FLAC's aggregate-channel ceiling. Content-addressed payloads may be shared, but references—not GUI traversal—define ownership.

### Exact MIDI representation

Define a versioned `shoop-midi` document usable both inside `.shoop` and as the default exact loop-MIDI export:

- source sample rate and duration in frames;
- ordered start-state messages separate from timeline events;
- timeline events as `{ frame: u64, order: u32, data: bytes }` relative to channel/loop time;
- explicit loop/channel identity and timing metadata when embedded in a session.

Engine-internal negative sentinel timestamps are converted only at the backend boundary; they are not part of the file format. Equal-frame events preserve `order`. Standard MIDI is a compatibility export with disclosed quantization, not the session's canonical timing representation.

### Persistent state inventory

The current schema must have typed sections for:

- session metadata and source sample rate;
- global performance controls: default record/grab, play-after-record, sync, solo, and fixed cycle count;
- stable track groups, sync/main distinction, order, names, width/layout hints, and track topology kind;
- loops: stable IDs, order, names, sync flag, source-frame length, gain/balance, primitive channels or composite definitions, and channel/content references;
- channels: audio/MIDI kind, direct/dry/wet/disabled mode, gain, source-frame data length, start offset, preplay, connected port IDs, recording metadata, and recording FX-state references;
- ports: stable IDs, names, data/direction/role/connectability, gain, mute/monitor/passthrough, internal links, external connection/autoconnect names, and ringbuffer duration;
- selected/targeted loop IDs and other session-scoped editing state, with stale references rejected;
- buses and global ports;
- FX chain identity/type/title/ports, exact opaque Carla `internal_state` strings, and the captured FX-state registry used by loop recordings;
- composite playlists/nesting and script-composite data;
- scripts, MIDI-control configuration/mappings, and session-local settings/overrides.

Unknown future major schemas are rejected. Known-but-not-yet-runnable sections are preserved by the codec but cause an explicit capability error if application load would need to instantiate them.

### Resampling and timing

- Perform conversion in `shoop_session`, never in widgets or the realtime callback.
- Derive all target lengths from checked integer rational arithmetic. Use one documented rounding rule per category: ceiling for enclosing durations/loop lengths, nearest with stable tie handling for event positions and signed offsets, and clamping only when required by the converted duration.
- Resample each audio channel to its declared converted length with a shared cross-target high-quality converter. Do not infer duration from the last MIDI event.
- Convert exact MIDI event frames, offsets, preplay, ringbuffer sample counts, composite delays, and every other sample-domain field. Preserve event order after timestamp collisions.
- The warning and conversion decision are application state, so native and browser presentation behave identically.

### Realtime and task ownership

- Capture through the backend at one control boundary rather than traversing GUI state. Native capture yields one owned immutable DTO generation; browser capture is generation-tagged and transferred in bounded chunks. Capture must not alter loop transport.
- Replace the browser waveform path's full-loop copy with revision-pinned bounded chunk reads. Use the same bounded request/response mechanism for audio and MIDI persistence snapshots.
- Parse/compress/write in a native worker. On Wasm it runs on the application/control side while the independently scheduled AudioWorklet continues; normal and storage-cap stress automation must prove callback continuity before accepting this simpler boundary. AudioWorklet messages transfer bounded chunks only; no archive, codec, resampler, filesystem, or unbounded allocation work runs in `process()`.
- Stage loads with explicit begin/chunk/finalize/commit/abort commands and generation IDs. Journal only committed topology; an interrupted browser worklet must not replay a half-loaded session.
- The application owner coordinates tasks and publishes task state. The composition root supplies platform file sources/sinks; `shoop_egui` only emits typed intents and paints dialogs/progress.

## Staged implementation plan

Dependencies are ordered: freeze the format and resource contract first; build pure codecs before backend/application integration; add platform adapters before final UI/e2e closure.

### Stage 1 — Complete discovery and freeze the format contract

- [x] Expand the coarse persistence/media matrix rows into independently testable session, archive, audio, MIDI, resampling, task, versioning, and browser entries, citing behavior-discovery sources and current Rust boundaries without adopting the QML file layout.
- [x] Inventory every scalar/sample-domain field and the desired cross-target sound/MIDI capabilities; define one canonical spelling and representation for every new-format field and Carla chain type.
- [x] Add a checked-in format specification and representative golden fixtures: minimal, arbitrary-channel, controls/connections, audio+MIDI, composite, dry/wet, Carla state registry, malformed, older-version, and future-version archives.
- [x] Spike the ZIP64, hash, audio codec, standard MIDI, and resampler dependencies under `wasm32-unknown-unknown`; choose pure-Rust/common code where required and record resource limits.
- [x] Update this plan if evidence changes implementation detail, without weakening the immutable criteria.

Verification:

- [x] Format/spec review maps every persistent-state inventory item to a typed field and every media reference to a validated archive entry.
- [x] A Wasm compiler probe proves the selected common codec/versioning stack has no filesystem, threads, C library, Qt, or frontend dependency.

### Stage 2 — Implement `shoop_session` codecs, validation, versioning, and resampling

- [x] Create `shoop_session` with typed v1 documents, version-dispatched archive reader/writer over byte/stream abstractions, deterministic encoding, hashes, limits, and typed errors.
- [x] Implement per-channel compressed `f32` payloads, exact `shoop-midi`, float WAV/Shoop loop-audio codecs, and standard MIDI tempo-map/timebase conversion. Keep optional native sound-file support behind a native adapter.
- [x] Implement complete version/reference/schema/capability validation and pure cross-target sample-rate conversion.
- [x] Add property/golden tests for bit-exact samples, MIDI ordering/start state/duration, arbitrary channel counts, deterministic archives, corruption/limit rejection, supported-minor defaults, older/future-major rejection, and rational timing edge cases.

Verification:

- [x] `cargo test -p shoop_session` passes, including same-rate exact round trips and 48 kHz ↔ 44.1/32/96 kHz conversions.
- [x] `cargo check -p shoop_session --target wasm32-unknown-unknown` passes.
- [x] Current-version fixtures decode to the expected typed documents; QML-era, unsupported older/future major, and unsafe archives are rejected before payload allocation.

### Stage 3 — Add coherent backend snapshot and transactional replacement APIs

- [x] Replace full-loop waveform reads with revisioned bounded ranges and add typed one-generation session snapshots containing topology, exact arbitrary-channel audio/MIDI, timing metadata, and Carla-state hooks.
- [x] Extend Fake and engine backends with fully staged replacement/commit and stable application↔backend ID mapping; add explicit browser begin/chunk/commit/abort generations; remove fixed application/protocol loop-channel limits.
- [x] Keep playback content independent from retained capture DTOs and reject active record/replace save requests until content settles.
- [x] Version and extend `shoop_audio_protocol`/`shoop_audio_worklet` with bounded session audio/MIDI transfer chunks and staged replacement. Separate two-channel physical device limits from `u32` loop content counts.
- [x] Make replacement a validated control-boundary swap, retaining the old session on failure and starting every loaded loop stopped.

Verification:

- [x] Shared Fake/engine contracts prove one-generation exact arbitrary-channel audio/MIDI/timing payloads, stable mappings, rollback on invalid replacement, and one-step commit; unavailable Carla topology is rejected explicitly.
- [x] Native realtime lock/no-allocation guards remain green; native compression is worker-owned, and the playing native integration test retains mode and advancing frames through save.
- [x] Worklet tests prove bounded messages, continued callback progress during snapshot/load staging, range-based waveform requests, no allocation in `process()`, stale/incomplete rejection, abort/retry, and no replay of incomplete loads.

### Stage 4 — Implement application-owned session and loop-I/O workflows

- [x] Add framework-independent I/O intents, capability descriptions, warning/confirmation/mapping states, task IDs/progress/errors, and completion notifications to `shoop_app_api` without embedding paths or large media bytes in `AppSnapshot`.
- [x] Add the `shoop_app` persistence coordinator for save/load and loop audio/MIDI import/export. Capture authoritative state, keep codecs/resampling off `process()`, use a native encoding worker, and feed results through ordered application messages.
- [x] Serialize all current model fields, including controls, identities/order, selection/target, connection rules, loop gain/balance, and exact content; validate/preserve deferred typed payloads at the codec boundary and capability-reject uninstantiable topology.
- [x] Apply sample-rate confirmation before mutation and disclose standard-MIDI export quantization; support cancel/retry, stale task rejection, and explicit active-content rejection.
- [x] Keep the old model/backend mapping until load commit succeeds, then publish the new immutable model and reset transient transport while retaining monotonic runtime diagnostics.

Verification:

- [x] Native actor/cooperative/browser tests cover save while playing, active mutation rejection, cancellation, mismatch warning, resampling, stale task IDs, queue saturation, unsupported topology, and rollback.
- [x] Current runnable egui state round-trips through application/backend mapping; deferred-state codec fixtures retain every field and Carla string byte-for-byte.
- [x] Loop import mapping covers fewer/equal/more source channels, duplicate mappings, length adoption, empty-file rejection, and arbitrary channel counts; direct destinations are runnable while typed dry/wet topology is capability-rejected until its owning milestone.

### Stage 5 — Add native and browser file services

- [x] Add composition-root file source/sink adapters: native worker path I/O with temporary-file/atomic replacement and browser asynchronous `File` upload/download handles with the `rfd` fallback behavior.
- [x] Keep picker handles, paths, browser objects, and byte streams outside `shoop_egui` and the persistent application document.
- [x] Support hosted and self-contained artifacts without assuming a secure origin for ordinary upload/download.
- [x] Keep browser compression/resampling on the control side while the dedicated AudioWorklet continues independently; expose bounded staged progress/cancellation and verify the design under storage-cap stress.
- [x] Add deterministic automation hooks that pass real produced session/audio/MIDI bytes back through authoritative import paths rather than fixture-only flags.

Verification:

- [x] Native tests prove atomic replacement, cancellation/no-choice behavior, temporary cleanup, failing destinations, and no partial target on failure.
- [x] Hosted Chrome/Firefox and self-contained Chrome automation route exact real-produced session/audio/MIDI bytes through the same application import/export boundary used by file adapters.
- [x] Browser callback counters and non-zero output advance throughout a storage-cap save/load; Chrome reports zero protocol overflow, render-budget diagnostics, discontinuities, console exceptions, and media-track leaks.

### Stage 6 — Deliver the egui session and loop I/O surfaces

- [x] Enable main-menu **Save session…** and **Load session…** actions with native/browser-neutral picker requests.
- [x] Add loop context actions for exact/float audio import/export and exact/standard MIDI import/export, including ordered channel selection, explicit mapping, length adoption, role/format labels, and quantization/resampling warnings.
- [x] Add task progress/cancel/error presentation while leaving the non-modal live controls available.
- [x] Present unsupported file/version/topology, malformed/resource-limit input, active mutation, sample-rate conversion, platform failure, and standard-MIDI quantization as distinct messages.
- [x] Add backend-free typed routing/paint tests at minimum/common viewports and validate stable task/entity IDs before applying dialog results.

Verification:

- [x] `cargo test -p shoop_app_api -p shoop_egui -p shoop_app -p shoop_backend -p shoop_audio_protocol -p shoop_audio_worklet -p shoopdaloop_egui` passes (17 app, 7 API, 10 backend, 2 protocol, 4 worklet, 27 GUI, and 4 runner tests at the recorded focused gate).
- [x] `cargo check -p shoop_egui --target wasm32-unknown-unknown` and product/preview Wasm checks pass; debug Trunk/worklet packaging succeeds.
- [x] GUI/API/application tests prove menu/dialog task-scoped routing, ordered selections/mappings, stale task rejection, and stale-loop validation.

### Stage 7 — Integrated I/O and performance evidence

- [x] Add native/application integrated workflows covering arbitrary-channel audio+MIDI session state, save/load/play, loop exact/WAV/MIDI export/import, connections, controls, selection/target, and different-rate conversion.
- [x] Extend browser automation with authoritative session plus loop-audio/MIDI byte round trips under running Web Audio, including non-zero playback and callback progress after load.
- [x] Add explicit QML-era rejection/no-mutation tests and retain the broader QML suite only as the unchanged-product regression gate.
- [x] Inspect dependency trees and Wasm imports; verify persistence did not enter `shoop_egui` or `process()`.
- [x] Exercise a ten-second-per-channel storage-cap browser save/load, recording transfer/archive bounds, callback continuity, cancellation behavior, and zero overflow/discontinuity/budget diagnostics.

Verification:

- [x] Same-rate session media is bit-identical, MIDI timing/order/start state is exact, Carla strings compare byte-for-byte, and resampled fixtures meet documented length/timing bounds.
- [x] Playing native/browser sessions retain playing mode and advancing frames/callbacks/non-zero output through save; Chrome storage-cap stress reports zero xrun/render discontinuity diagnostics.
- [x] Corrupt, malicious, unsupported-version/capability, stale/incomplete transfer, and injected replacement failures leave the prior session usable.

### Stage 8 — Final validation and project-ledger update

- [ ] Run formatting and warning-denying builds for native and all affected Wasm packages.
- [ ] Run the full Rust workspace, product browser workflows, native egui workflow/paint tests, realtime guards, and retained QML self-test.
- [ ] Update `EGUI_FEATURE_PARITY_MATRIX.md` with discovered rows, implementation status, intentional format differences, and concrete evidence.
- [ ] Update `EGUI_REPLACEMENT_PROJECT.md` coarse status, target architecture (`shoop_session` now real), roadmap, browser/native persistence notes, and remaining deferred runtime capabilities.
- [ ] Document `.shoop` v1, the intentional lack of QML archive compatibility, MIDI timing choices, supported media formats per target, resampling semantics, browser file behavior, resource limits, and recovery from errors.

Final gates:

- [ ] `cargo fmt --all --check`
- [ ] `RUSTFLAGS="-D warnings" cargo build --workspace --all-targets --features shoop_engine/app_backend`
- [ ] `cargo test --workspace --features shoop_engine/app_backend`
- [ ] All standalone/product `wasm32-unknown-unknown` checks and production Trunk/worklet packaging checks.
- [ ] Hosted Chrome and Firefox plus self-contained Chrome session/loop-I/O workflows.
- [ ] `target/debug/shoopdaloop_dev.sh --self-test` after the required build.
- [ ] Source/dependency scans confirm the documented architecture and no stale disabled Session I/O/loop-I/O menu entries remain.

## Execution contract

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
