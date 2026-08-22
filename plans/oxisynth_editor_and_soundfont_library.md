# OxiSynth editor and SoundFont library implementation plan

## Goal

Add a native and browser OxiSynth editor in two deliverable phases. The MVP makes the embedded TimGM6mb instrument inspectable, directly configurable, automatically synchronized with externally received MIDI, and persistable. The second phase adds arbitrary SF2 import, managed assets, portable sessions, missing-asset recovery, and expanded sound controls without changing the realtime guarantees of the existing processor.

## Status

- [x] Planning and architecture audit complete.
- [x] Stage 1 API/state proof implemented and verified.
- [x] Phase 1 direct controls, authoritative native/browser snapshots, configuration codec, session v5 persistence, and initial egui editor implemented.
- [ ] Phase 1 lifecycle hardening, comprehensive tests, screenshots, and integration gate in progress.
- [ ] Phase 1 implementation in progress.
- [ ] Phase 2 implementation pending.
- [ ] Final end-to-end validation pending.

## Scope

### Phase 1 — embedded-SoundFont MVP

- Add an OxiSynth editor containing the embedded SoundFont identity, a selected MIDI-channel control, searchable preset selection with bank/program identity, MIDI activity, audition, and panic.
- Configure OxiSynth through its direct Rust API. Editor actions must not manufacture MIDI messages or enter recorded MIDI.
- Maintain a bounded, lock-free snapshot of the relevant global and 16-channel synth configuration. Incoming bank select, program change, controllers, pitch bend, pressure, and reset messages update the synth and the published snapshot so external changes appear in the UI.
- Persist and restore the reconstructible baseline configuration while excluding active voices, effect delay buffers/tails, transient audition notes, and SoundFont bytes.
- Preserve old stateless OxiSynth sessions by migrating them to documented defaults.

### Phase 2 — arbitrary SF2 and expanded editor

- Add a content-addressed SoundFont library shared by native and browser application models, with import, metadata/preset discovery, deduplication, validation, removal rules, and explicit replacement.
- Support application-managed assets and portable session-embedded SF2 assets. Do not persist opaque runtime `SoundFontId` values or silently substitute a missing asset.
- Add missing-asset resolution, load/error status, master gain and output metering, chorus/reverb controls, preset favorites/recent choices, and a compact overview of all 16 channel assignments.
- Reuse parsed immutable SoundFonts across tracks if the OxiSynth ownership model permits it without shared mutation or realtime synchronization; otherwise deduplicate stored bytes and document per-track parsing/memory cost.

### Out of scope

- Sample-accurate serialization of active voices, envelopes, filters, oscillators, render phase, or chorus/reverb delay contents.
- SF2 authoring, zone/sample editing, merging/stacking multiple SoundFonts in one track, SF3, microtuning UI, a full per-channel mixer, or generator/NRPN editing.
- Automatically recording editor changes as MIDI, rewriting existing MIDI loops, or treating transient performance controls as persisted defaults.
- Referencing unrestricted native filesystem paths from sessions; portable and browser behavior must use managed or embedded bytes.

## Immutable acceptance criteria

1. Native and browser OxiSynth tracks expose the same editor and reconstructible state semantics.
2. Selecting a preset in the editor invokes a typed OxiSynth control that ultimately uses the direct Rust API with channel plus exact SoundFont, bank, and program identity; it emits no MIDI event and creates no recorded MIDI data.
3. Valid external MIDI continues to retain its in-block timing and audible behavior. Any message that changes a represented channel property is reflected in the next bounded backend snapshot and UI refresh, including bank/program selection while the editor is open.
4. Snapshot publication and editor control delivery perform no allocation, blocking I/O, logging, locking, or panicking on the audio callback/AudioWorklet render path. SoundFont parsing and state decoding occur before realtime publication.
5. The persisted MVP state restores the embedded SoundFont configuration, all 16 channel program assignments, and every explicitly persisted baseline field. It never claims to restore live voices or effects tails, and legacy OxiSynth sessions load with the prior defaults.
6. Phase 2 identifies every arbitrary SoundFont by a stable content digest, validates its bytes before publication, and enumerates the actual presets present in that file without assuming General MIDI names or dense banks.
7. A portable saved session contains each referenced user SoundFont at most once, verifies declared size and digest on load, rejects unsafe/oversized/malformed assets transactionally, and produces the same assignments on native and browser runtimes.
8. A missing or changed SoundFont never falls back silently. The decoded session is retained as an inactive recovery candidate, including every unresolved track and expected digest, while the current backend session keeps running; the user can locate/import the exact digest or explicitly choose a replacement before transactional activation.
9. Session replacement, driver switching, track duplication/removal, activation, and save/load preserve configuration and asset ownership without stale UI state, leaked resources, stuck notes, or interruption of an already-running session on failure.
10. Existing Direct, External, Carla, Tiny Synth/FX, legacy session, realtime-allocation, protocol, and package-size behavior remains compatible unless an intentional version/budget update is documented and tested.

## Design rules and constraints

1. **Separate configuration from performance.** Persist a versioned `OxiSynthConfiguration`; publish an `OxiSynthSnapshot` that also reports current MIDI-observable values. Active voices, temporary audition state, MIDI activity decay, and effect tails are runtime-only.
2. **Use one mutation path.** Typed editor controls and translated MIDI events must update the same processor-owned canonical state at the exact point the corresponding OxiSynth call succeeds. Never optimistically publish a value rejected by OxiSynth.
3. **Make origin explicit.** For fields that can differ from the saved baseline, snapshots distinguish baseline configuration from current MIDI-overridden value so saving does not accidentally make sustain, pitch bend, or an incidental program change permanent.
4. **Keep realtime state fixed-size.** Use arrays for 16 channels and fixed-size controller/value representations. Publish through the repository's existing bounded command/snapshot mechanisms or an equivalent wait-free handoff; do not clone preset names, asset bytes, or variable collections in the callback.
5. **Resolve display data off-thread.** Snapshots carry stable numeric/digest identities and generation counters. The control/application layer joins those identities with immutable SoundFont metadata for names and search results.
6. **Do not persist runtime IDs.** Map content-digest asset IDs to per-instance OxiSynth `SoundFontId` values during construction. Exact preset identity is asset digest plus bank and program.
7. **Apply state transactionally.** Parse assets and validate/decode configuration on the control path, construct a replacement processor, apply configuration in deterministic order, and publish only after every required preset and parameter succeeds.
8. **Define reset semantics.** System reset and deactivation clear voices and restore the documented runtime state. Specify separately whether MIDI system reset restores the persisted baseline or OxiSynth defaults, then use that rule identically on native and browser paths.
9. **Bound asset risk.** Extend archive limits for SF2 assets deliberately, stream/hash where practical, reject undeclared entries and digest mismatches, and account for compressed and uncompressed sizes before allocation.
10. **Keep format evolution explicit.** Version the OxiSynth state payload independently where useful and increment the session/protocol versions where their closed representations change. Older readers must fail clearly rather than misinterpret asset-bearing sessions.
11. **Preserve deterministic rendering.** UI controls are asynchronous control-plane changes with acknowledged snapshots; MIDI remains sample-offset performance input. Tests must define ordering when a control and MIDI program change reach the same processing quantum.
12. **Treat phase boundaries as shippable.** Phase 1 must not depend on arbitrary-file infrastructure. Phase 2 extends the asset identity already used for the embedded font instead of replacing the MVP state model.
13. **Prepare browser assets outside the AudioWorklet.** Parse, validate, and construct arbitrary SoundFont-owned state in a browser worker/main-thread preparation service that can transfer a prepared immutable asset or construction recipe without blocking the live-rendering AudioWorklet message/render thread. The final worklet handoff is bounded and must not parse SF2 bytes.
14. **Reclaim processors off the realtime thread.** Replacement and removal return displaced OxiSynth processors through a bounded deferred-drop queue to the native control thread or browser host. Audio-thread graph mutations must never destroy SoundFonts or large processor buffers.

## State model to prove before implementation

The implementation should start with a small API/realtime proof that classifies fields rather than serializing the private OxiSynth object:

- Persisted global baseline: SoundFont asset identity and optional global settings introduced by the current phase.
- Persisted channel baseline: exact preset identity for each of 16 channels; add only controls with explicit product semantics.
- Snapshot current state: SoundFont/bank/program, relevant CC values, pitch bend, pitch-wheel sensitivity, channel pressure, and a monotonically increasing revision/MIDI activity indication.
- Runtime-only state: active voices, note/key pressure arrays unless needed for diagnostics, effects buffers, audition notes, render position, and raw OxiSynth runtime IDs.
- Phase 2 asset metadata: digest, original filename, SF2 INFO metadata, byte length, and sorted preset descriptors; favorites reference digest/bank/program and remain user preferences rather than required session state.

If OxiSynth lacks a getter required for a represented field, maintain a processor-owned mirror updated only alongside successful direct calls and MIDI translation. Add focused conformance tests against OxiSynth reset and bank-selection behavior before relying on the mirror.

## Implementation stages

### Stage 1 — state semantics and OxiSynth API proof

- [x] Inventory OxiSynth 0.1.0 direct getters/setters, MIDI reset/bank/program rules, preset enumeration, effect activation support, and allocation behavior for each proposed control.
- [x] Specify the exact phase-1 persisted fields, current snapshot fields, defaults, validation ranges, system-reset behavior, and same-quantum control/MIDI ordering in a short state contract.
- [x] Prototype fixed-size configuration/snapshot types and prove that direct preset selection plus externally received bank/program MIDI converge on the same reported state for all 16 channels.
- [x] Decide whether inactive tracks continue consuming configuration-changing MIDI for UI accuracy, documenting the choice consistently with existing bypass semantics.
- [x] Confirm how preset descriptors are enumerated and sorted from the embedded SF2 without retaining or copying them on the render path.

**Verification:** focused engine tests cover exact direct selection, bank MSB/LSB plus program changes, channel 10, invalid presets/channels, reset, inactive behavior, and zero allocations after warm-up; native and wasm test builds exercise the same state contract.

### Stage 2 — engine control and realtime snapshot publication

- [ ] Extend the engine OxiSynth wrapper with typed configuration controls, deterministic apply/restore, preset metadata extraction, audition note lifecycle, panic, and a fixed-size canonical state mirror where the crate cannot be queried safely.
- [ ] Route UI commands to the processor without MIDI conversion and add acknowledgements/errors so rejected changes do not remain optimistically visible.
- [ ] Update MIDI processing so every successfully applied represented MIDI event updates canonical state at the same sample offset; coalesce publication to at most one fixed-size snapshot per processing quantum/revision.
- [ ] Publish snapshots through a bounded lock-free mechanism consumable by the control thread/worklet host, including overflow/coalescing counters and last-known-good behavior.
- [ ] Reset audition and active-voice state safely on panic, bypass, replacement, and removal without changing the persisted baseline unintentionally.
- [ ] Return displaced and removed processors through bounded deferred-reclamation queues and drain/drop them outside native callbacks and AudioWorklet rendering.

**Verification:** engine/session route tests cover direct-control versus MIDI ordering, snapshot coherence, high-rate controller/program traffic, queue saturation, activation/removal/replacement with deferred destruction, no stuck audition notes, and warmed-up no-allocation/no-lock rendering.

### Stage 3 — shared API, backend, and native/browser protocol

- [ ] Add OxiSynth editor descriptors, preset identities, configuration/snapshot types, typed controls, and track actions to the shared application API; advertise embedded UI and persistent state capability.
- [ ] Extend backend track state/control conversion so snapshots are sourced from the engine rather than an application-side optimistic copy.
- [ ] Add versioned wire controls and OxiSynth snapshot/configuration representations, with bounded journal supersession for continuous controls and non-supersedable audition/panic operations.
- [ ] Implement equivalent native, worklet-client, and AudioWorklet mappings, including authoritative snapshot round trips after external Web MIDI and editor controls.
- [ ] Ensure stale revisions cannot overwrite newer UI state after command acknowledgement, worklet restart, session replacement, or driver switching.

**Verification:** API and protocol round-trip/exhaustiveness tests, backend conformance tests, native dummy-driver tests, and wasm/worklet host tests demonstrate identical controls, revisions, external-MIDI reflection, failures, and snapshots.

### Stage 4 — phase-1 session persistence and migration

- [ ] Define a typed, versioned OxiSynth configuration document rather than an opaque dump of the `Synth`; exclude SoundFont bytes and runtime state.
- [ ] Increment the session document version and migrate version-4 stateless OxiSynth tracks to the embedded asset digest and documented 16-channel defaults.
- [ ] Validate channel counts, numeric ranges, exact embedded asset identity, presets, and absence of forbidden runtime fields before backend mutation.
- [ ] Serialize the persisted baseline, not incidental current MIDI overrides; document and test the user action or policy that promotes current programs to the baseline if provided.
- [ ] Integrate configuration with track duplication, recorded-take FX state policy, driver switching, transactional replacement, and deterministic archive output.

**Verification:** archive migration/validation tests cover versions 1–4, malformed and future configurations, deterministic encoding, save-after-external-program-change semantics, native/browser round trips, and failure atomicity.

### Stage 5 — phase-1 editor

- [ ] Add a reusable OxiSynth egui editor and connect it to track visibility using the capability-driven editor descriptor.
- [ ] Show embedded SoundFont identity/status, selected MIDI channel, searchable preset names with bank/program numbers, current-versus-baseline indication, and incoming MIDI activity.
- [ ] Add direct preset selection, previous/next navigation, audition, and panic; audition must be visibly transient and must never enter recorded MIDI.
- [ ] Reconcile widgets from authoritative revisioned snapshots while preserving in-progress search/channel UI state and avoiding feedback loops when external MIDI changes a value.
- [ ] Add accessible labels, keyboard navigation, compact/narrow layout behavior, empty/error states, and deterministic widget tests.
- [ ] Correct OxiSynth user documentation to describe the actual fixed two-ignored-dry/two-wet/one-MIDI topology and the new persistence semantics.

**Verification:** egui action/layout tests cover search, sparse banks, channel switching, external updates while open, acknowledgement failure, audition release, panic, narrow layouts, and two editors with independent local state; run the native app and browser smoke path and capture screenshots of the perceptible UI change.

### Stage 6 — phase-1 integration gate

- [ ] Exercise notes and external bank/program changes from the production MIDI adapters on native and browser paths while observing immediate editor reconciliation.
- [ ] Save, close, reload, switch driver/runtime, duplicate, bypass, reactivate, and delete an OxiSynth track; compare the persisted baseline and authoritative snapshot at each boundary.
- [ ] Confirm session and recorded MIDI contain no events synthesized by UI preset selection or audition.
- [ ] Profile control bursts and snapshot publication with the realtime allocation/lock guards and Tracy coverage where applicable.

**Verification:** run focused OxiSynth, backend, application, session, protocol, worklet, and egui suites followed by repository formatting, warning-denying builds, tracing inventory, complete native tests, wasm builds/tests, browser smoke checks where available, and native/browser release artifact-size checks.

### Stage 7 — phase-2 content-addressed SoundFont library

- [ ] Define a stable SHA-256 asset ID, immutable metadata/preset catalog, managed-byte ownership, load status/error model, and reference counting independent of track/runtime IDs.
- [ ] Implement bounded SF2 import and validation off the audio thread for native file selection and browser file input; reject malformed/unsupported content without disturbing running tracks.
- [ ] Deduplicate identical bytes, normalize display metadata safely, sort sparse presets deterministically, and cache immutable catalogs for UI search.
- [ ] Add application library operations for import, list, inspect, remove-if-unreferenced, and explicit replacement; durably persist content-addressed bytes together with catalog metadata in native and browser storage.
- [ ] Make byte/catalog installation and removal atomic so restarts never advertise a digest whose payload is absent, and garbage-collect only unreferenced payloads after the catalog update commits.
- [ ] Resolve asset digests to newly assigned per-synth `SoundFontId` values and construct configured replacement processors transactionally.
- [ ] Add native background jobs and a browser worker/main-thread preparation protocol so hashing, parsing, validation, and heavy construction never execute in the live AudioWorklet command or render callback; hand off only bounded prepared data and defer old-processor destruction back to the host.
- [ ] Measure parse time and memory across multiple tracks; introduce safe immutable parsed-font sharing only if supported and beneficial.

**Verification:** asset tests cover duplicate content/different filenames, hash mismatch, malformed/truncated/oversized SF2, sparse/non-GM presets, Unicode metadata, removal with live references, concurrent imports, atomic-write interruption, native and browser restart/reload with byte revalidation, native/browser parity, failed replacement preserving audio, and a browser live-rendering load/replacement test that proves the AudioWorklet does no SF2 parsing or large destruction.

### Stage 8 — phase-2 portable session assets and recovery

- [ ] Extend the session bundle/manifest with declared SoundFont asset records and content-addressed archive paths, storing each referenced payload once.
- [ ] Increment the session format, add decode limits and aggregate accounting appropriate for SF2 files, verify size/digest before parsing, reject undeclared/duplicate/unsafe entries, and preserve deterministic output.
- [ ] Save portable sessions with user SF2 bytes while continuing to identify the built-in font without duplicating its payload.
- [ ] Split loading into decoded candidate, asset resolution, validated processor construction, and activation. Retain an inactive candidate document plus unresolved digests/errors for recovery while the current backend session keeps running, and publish the replacement only after every required asset and processor succeeds.
- [ ] Represent unresolved assets and affected tracks explicitly in the candidate-session application model; add cancel, locate/import-exact-digest, retry, and explicit-replacement flows, and never substitute the built-in asset automatically.
- [ ] Define export/privacy/licensing messaging that makes embedding a user-provided file explicit without asserting redistribution rights.

**Verification:** cross-runtime portable-session tests cover one asset shared by tracks, multiple assets, missing payloads, wrong digest/size, archive bombs/limits, retained candidate diagnostics, cancel/retry recovery while the old backend keeps producing audio, explicit replacement and preset remapping failures, deterministic archives, older readers/versions, and transactional activation or rollback.

### Stage 9 — phase-2 editor expansion

- [ ] Add SoundFont manage/import/change controls with loading, ready, missing, invalid, and replacement states.
- [ ] Add a compact 16-channel assignment overview that navigates the existing per-channel editor without becoming a full mixer.
- [ ] Add master gain, stereo output metering, and supported chorus/reverb parameters through direct Rust controls; verify true enable/bypass support or expose only truthful parameter semantics.
- [ ] Add previous/next, favorites, and recent presets keyed by digest/bank/program; keep favorites/recent data out of required session reconstruction.
- [ ] Preserve search and selected channel across catalog refreshes, and display an explicit unavailable assignment when a replacement SF2 lacks the configured preset.
- [ ] Update user, session-format, browser-storage, asset portability, license, and package-size documentation.

**Verification:** egui and application tests cover large catalogs, sparse banks, asset switching, load progress/errors, favorites/recent invalidation, 16-channel overview updates from MIDI, meters, effect controls, missing assets, accessibility, and native/browser screenshots.

### Stage 10 — final end-to-end validation

- [ ] Import representative small, large, sparse, percussion, non-GM, duplicate, and malformed SF2 files on native and browser builds.
- [ ] Configure several channels through the UI, override them with external MIDI, verify authoritative UI reflection, save a portable session, and reload it on the other runtime with matching baseline assignments and audible output.
- [ ] Exercise missing-file recovery and explicit replacement while another session is producing audio; prove failure atomicity and absence of silent fallback.
- [ ] Stress rapid UI changes, dense MIDI, repeated import/replacement, driver switching, session replacement, and teardown under realtime tracing and allocation guards.
- [ ] Inspect release packages and session archives for asset deduplication, declared hashes/sizes, notices, absence of unintended filesystem paths, and documented size budgets.
- [ ] Run the complete supported feature/target matrix and record unavailable hardware/browser coverage with the closest production-adapter test rather than silently omitting it.

**Verification:** `cargo fmt --all -- --check`; `RUSTFLAGS="-D warnings" cargo build --workspace`; `python3 scripts/check_shoop_test_usage.py` when Rust tests change; `python3 scripts/check_tracing_coverage.py --require-closed`; `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend --profile ci`; target-correct `wasm32-unknown-unknown` builds/tests for `shoopdaloop` and `shoop_audio_worklet`; documented browser smoke commands; focused realtime no-allocation/lock tests; and native/browser release artifact and portable-session inspections.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
