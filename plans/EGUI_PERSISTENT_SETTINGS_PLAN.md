# egui Persistent Settings Plan

## Status and relationship to the replacement project

**Status:** Planned

This plan adds application-wide persistent settings to the pure-egui application described by `EGUI_REPLACEMENT_PROJECT.md` and expands the settings rows in `EGUI_FEATURE_PARITY_MATRIX.md`. It is separate from `.shoop` session persistence: machine/user preferences must not be serialized into sessions.

The repository already has a small `shoop_settings` crate that reads the legacy QML `settings.1` document for Carla startup policy. The egui application will use a fresh document identity, namespace, and storage key. Importing, rewriting, or otherwise supporting the QML settings format is not part of this work; the retained QML path must continue to pass its existing regression tests.

Implementation and review will remain on the dedicated `shoopdaloop-settings` branch and draft PR #677.

## Goal

Deliver a cross-target, versioned settings service and generated egui settings dialog. Internal consumers explicitly register typed setting definitions during composition so keys, defaults, validation, help text, and effect timing remain close to the code that uses them. The first persisted preferences configure defaults already used by the Add Track dialog.

## Scope

Included:

- A fresh versioned egui settings document with explicit format/version checks and an ordered migration boundary.
- A typed, metadata-bearing registry and immutable settings snapshots.
- Native Linux, Windows, and macOS config-directory storage plus browser-origin `localStorage`.
- Atomic/error-observable persistence and safe default behavior for absent, malformed, or unsupported documents.
- A registry-driven egui settings dialog with Save, Cancel, reset, validation, and persistence status.
- Initial settings for the default audio-channel count and MIDI-enabled state of newly opened Add Track dialogs.
- Native/Wasm tests, browser persistence automation, documentation, and replacement-project ledger maintenance.

Not included:

- Reading or migrating QML `settings.1`, MIDI-control settings, script settings, or any existing QML settings file.
- Session-local settings or changes to `.shoop` v1.
- Native real-driver/device selection, MIDI control, scripting, dry/wet tracks, FX composition, or exposing Carla hosting mode before the egui runtime can use it.
- A plugin ABI, linker-discovered registration, arbitrary executable code in setting definitions, cloud synchronization, or secrets/credential storage.
- Persisting transient session, transport, dialog, task, meter, or backend state as application preferences.

## Immutable acceptance criteria

1. **Fresh independent format.** The egui app reads and writes only a document identified as `shoop-egui-settings`. It neither searches for nor imports the QML settings document, and its path/key cannot collide with the retained QML `settings.json`.
2. **Cross-platform persistence.** Linux, Windows, and macOS use the OS configuration directory resolved from one stable application identity; hosted and self-contained Wasm use an origin-scoped, stable `localStorage` key. A missing storage service is observable but does not prevent startup with defaults.
3. **Version checks and migration boundary.** The envelope carries explicit format, format major/minor, and document schema versions. Unsupported format, older versions without a registered migration, and future versions are rejected before values are applied or rewritten. Supported versions pass through ordered, pure migrations into the current typed document.
4. **Typed registration and access.** Consumers explicitly register stable typed keys, defaults, category/order, label/help, editor constraints, and effect timing during startup, then read values through typed keys from an immutable snapshot. Duplicate keys, incompatible definitions, invalid defaults, and type-mismatched reads fail deterministically in tests/startup rather than silently aliasing values.
5. **Definitions remain near consumers.** The Add Track setting keys, registration function, and reads live with the Add Track behavior. The composition root explicitly aggregates registration functions; there is no hidden global registry or linker-time discovery.
6. **Safe evolution.** Missing registered keys use their declared defaults. Unknown same-version keys are retained across saves so target-specific or temporarily absent registrations are not destroyed. Invalid known values produce a specific warning and use the registered default. Unsupported/malformed source data is not automatically overwritten.
7. **Transactional save behavior.** Save validates and durably writes the whole current document before publishing the new immutable snapshot. Native writes use a same-directory temporary file, flush, and atomic replacement; browser storage exceptions are reported. Failure leaves both active values and the prior persisted bytes unchanged. Cancel/close discards the draft.
8. **Usable settings dialog.** The main-menu **Settings** action opens a resizable, minimum-viewport-safe dialog generated from registry metadata. It supports appropriate controls for the registered value kinds, per-setting/all reset, Save/Cancel, descriptions, effect timing, and actionable load/save/version/storage diagnostics.
9. **Existing feature integration.** Fresh installs retain Add Track defaults of stereo audio and MIDI disabled. Saved default channel count and MIDI state initialize each newly opened Add Track dialog, without modifying existing tracks, an already-open Add Track draft, or session data.
10. **Architecture boundaries.** `shoop_settings` remains Qt- and egui-independent; `shoop_egui` remains backend/filesystem/browser-API-free; platform stores and save scheduling stay in `shoopdaloop_egui`. Settings I/O never runs in an audio callback and settings are not routed through the session-business `AppIntent` path.
11. **Regression and target coverage.** Codec/registry/storage/UI/integration tests cover first run, restart, defaults, unknown keys, invalid values, every version decision, migration ordering, failed saves, and stable-key routing. Native and Wasm builds, browser reload persistence, existing egui workflows, realtime guards, and the retained QML suite remain green.
12. **Delivery discipline.** Work is developed in the dedicated branch and pull request; the plan, parity matrix, replacement project, and any affected completed milestone references are updated in the same stages that change their claims.

## Design rules and constraints

### Format and evolution

The checked-in v1 specification will define canonical UTF-8 JSON equivalent to:

```json
{
  "format": "shoop-egui-settings",
  "format_version": { "major": 1, "minor": 0 },
  "document_version": 1,
  "writer_version": "<ShoopDaLoop version>",
  "values": {
    "tracks.new.default_audio_channels": 2,
    "tracks.new.default_midi": false
  }
}
```

- Decode the small envelope before decoding a version-specific DTO. Keep stored DTOs separate from runtime values.
- Format major changes are incompatible unless explicitly supported. Minor additions must be optional/defaultable. A document schema change adds a concrete `Vn -> Vn+1` pure migration and tests; runtime consumers only receive the current model.
- Use stable dotted ASCII keys. Registration metadata is code, not duplicated into the file. Values are JSON primitives initially; editor/value kinds may be extended without allowing executable callbacks in documents.
- Preserve unknown entries for same-version round trips. Reject unsupported envelopes without opportunistic save/migration, and require an explicit user reset/recovery action before replacing such data.
- Keep this format independent from `.shoop` session versions and legacy QML schemas.

### Registry and call-site API

Implement the target-neutral contract in `shoop_settings`, using an API shaped around:

- `SettingKey<T>` constants for typed access;
- `SettingsRegistryBuilder::register(SettingDefinition<T>)` for startup registration;
- metadata for stable key, category/order, label, help, default, validation/editor constraints, and `Immediate`, `NextUse`, or `RestartRequired` effect timing;
- a finalized `SettingsRegistry` for validation, type erasure needed by the generic dialog, and deterministic ordering;
- a revisioned immutable `SettingsSnapshot` with `get(SettingKey<T>)` and diagnostics;
- a validated draft/change set returned by the settings presentation.

Registration is explicit: modules expose `register_settings(&mut SettingsRegistryBuilder)`, and the composition root calls those functions before loading values or constructing consumers. Do not use `inventory`, mutable statics, string lookups at ordinary read sites, or callbacks that bypass application/runtime ownership.

`shoop_egui` receives the registry's presentation-safe metadata plus immutable current/draft values and returns settings-specific UI actions. It must not read/write files or `localStorage`, and settings changes must remain separate from `shoop_app_api::AppIntent`, which owns session/business behavior.

### Storage and lifecycle

- Reuse the established `directories`, `serde`/`serde_json`, and `tempfile` dependencies rather than adopting an immature all-in-one settings/schema crate. Keep the migration dispatcher repository-owned and explicit.
- Native storage is `ProjectDirs::from("org", "ShoopDaLoop", "ShoopDaLoop egui").config_dir()/settings.json`, with the exact resolved path exposed for diagnostics and documented for Linux, macOS, and Windows. Test path selection through injected stores/paths rather than mutating a developer's real config directory.
- Browser storage uses a namespaced key such as `org.shoopdaloop.egui.settings` in `localStorage`. Treat unavailable/denied/quota storage as a typed storage failure. Browser-origin scoping and direct-file browser limitations must be documented.
- Keep the current legacy-QML helpers in `shoop_settings` isolated and regression-tested, but do not make them a fallback for egui.
- Startup order is: build/finalize registry, load/decode/migrate, validate/default, publish the initial snapshot, then construct runtime and widgets. Startup-only consumers therefore receive final values before creating backend resources.
- Dialog Save submits a full validated draft to the composition-owned manager. Native persistence runs outside audio/application actors; browser persistence uses the small synchronous storage operation. Publish a new snapshot only after success. Surface warnings and save state without preventing the main app from running.

### Initial settings

Define these next-use preferences beside the Add Track dialog code:

- `tracks.new.default_audio_channels: u32`, default `2`, validated as a supported `DirectTrackSpec` channel count without reintroducing the removed persistence channel ceiling;
- `tracks.new.default_midi: bool`, default `false`.

Read them only when a new Add Track dialog is opened. Saving them must not mutate session-global controls, existing track topology, or a draft the user is already editing.

## Staged implementation plan

Dependencies are ordered: freeze the contract and storage identity first; implement/test the target-neutral core before platform adapters; integrate lifecycle before presentation and end-to-end closure.

### Stage 0 — Plan delivery and review setup

- [x] Create the dedicated `shoopdaloop-settings` branch from current `origin/master`.
- [x] Commit this plan, push the branch, and open draft PR #677 before implementation.
- [ ] Record review-driven implementation-detail changes in this plan without weakening its goals or immutable criteria.

Verification:

- [x] Draft PR #677 contains only planning/documentation changes before implementation starts and targets `master`.

### Stage 1 — Freeze the settings contract and dependency boundary

- [x] Add a checked-in egui settings v1 specification covering the envelope, key/value rules, version decisions, migration dispatch, unknown/invalid values, recovery, storage identities/locations, and non-compatibility with QML settings.
- [x] Inventory settings/session boundaries and expand the parity matrix into independently testable format, registry, storage, dialog, and Add Track integration rows.
- [x] Define target-neutral registry, snapshot, diagnostic, draft, and storage-result contracts; document which pieces remain in `shoop_settings`, `shoop_egui`, and `shoopdaloop_egui`.
- [x] Confirm the selected existing dependency stack under `wasm32-unknown-unknown`; no additional target-gating is needed before the core implementation.

Verification:

- [x] Format review maps every acceptance rule to a document rule or typed API behavior.
- [x] `cargo check -p shoop_settings --target wasm32-unknown-unknown` proves the existing settings dependency boundary compiles without Qt, egui, backend, engine, or native-window dependencies.

### Stage 2 — Implement the registry, codec, validation, and migration dispatcher

- [x] Add typed keys/definitions, deterministic explicit registration, metadata/value erasure, finalized-registry validation, immutable snapshots, and typed getters to `shoop_settings`.
- [x] Implement current v1 DTO encoding/decoding, envelope-first checks, ordered migration dispatch, unknown-key retention, registered-default resolution, and typed diagnostics.
- [x] Keep legacy QML/Carla APIs behaviorally isolated; add guards proving the fresh egui codec rejects legacy QML documents while the legacy decoder continues requiring `settings.1`.
- [x] Add fixture/property-style tests for duplicate/type errors, invalid defaults/constraints, deterministic output, missing/unknown/invalid values, all version branches, and migration order/failure. Store-level no-clobber recovery evidence remains in Stage 3.

Verification:

- [x] `cargo test -p shoop_settings` passes 7 target-neutral core tests; `--features legacy` passes all 18 core/native/legacy tests with current, unsupported-old/future, malformed, QML, invalid/unknown-key, deterministic, typed-access, storage, and migration-dispatch coverage.
- [x] `cargo check -p shoop_settings --target wasm32-unknown-unknown` and warning-denying native checks pass; native store and legacy dependencies are opt-in features and absent from `shoop_egui` trees.

### Stage 3 — Add platform stores and composition-owned lifecycle

- [x] Implement the injectable native file store with stable `ProjectDirs` identity, directory creation, same-directory temporary output, flush/atomic replacement, and actionable path-aware errors.
- [x] Implement the composition-root browser `localStorage` adapter with the stable key and typed unavailable/security/quota failures.
- [x] Add the settings manager to `shoopdaloop_egui`: explicit registration aggregation, startup load before runtime/widget construction, revisioned snapshot publication, asynchronous native saves, browser saves, and no publish on failure.
- [x] Add explicit reset/recovery for malformed or unsupported data without silently overwriting it, and expose storage location/key plus load/save diagnostics to presentation.

Verification:

- [x] Native injected-path tests cover first run, atomic replace, parent creation, failed read/write/commit, unchanged prior bytes, no temporary-file leak, restart, stale draft, recovery, and unknown-key retention.
- [x] Hosted Chrome automation covers absent storage, successful save/reload, unavailable storage, injected set failure without active-value publication, invalid known values, and retained rejected future-version text.
- [x] Composition and GUI tests prove startup consumers see loaded values and failed saves retain the prior snapshot/revision.

### Stage 4 — Deliver the registry-driven settings dialog and initial consumers

- [x] Enable main-menu **Settings** and add a resizable dialog whose categories, controls, defaults, descriptions, ordering, validation, and effect labels come from registry metadata.
- [x] Implement isolated draft editing, per-setting/all reset, Save/Cancel/close behavior, saving-disabled state, and load/save/version/storage diagnostics at minimum/common viewports.
- [x] Define/register the two Add Track defaults beside their consumer and initialize each new dialog draft from the current typed snapshot.
- [x] Change `AppWidget` responses to keep settings UI actions separate from session/business intents and keep preview/test hosts supplied by plain settings fixtures.

Verification:

- [x] Focused GUI/runner tests cover menu opening, generated bool/integer controls, stable-key edits, reset, Save routing, validation, stale revisions, diagnostics, and 360×200/900×600 paint.
- [x] Integration tests prove defaults are stereo/MIDI-off, persisted values affect the next Add Track dialog after save/restart, and an already-open draft is unchanged.
- [x] Warning-denying native and `wasm32-unknown-unknown` checks pass for `shoop_settings`, `shoop_egui`, `shoopdaloop_egui`, and `shoop_egui_preview` without adding platform I/O to `shoop_egui`.

### Stage 5 — Add restart, browser, failure, and architecture evidence

- [x] Add native restart round trips using an injected temporary settings path and composition-owned settings manager.
- [x] Extend product browser automation to edit/save settings, reload the hosted artifact, verify the next Add Track defaults, and exercise unavailable/invalid/future-version/failed-save storage without console exceptions.
- [x] Verify the self-contained direct-file Chrome settings workflow and document origin/direct-file persistence limits without claiming cross-origin portability.
- [x] Inspect dependency trees and source paths to prove settings I/O remains outside presentation, application-session logic, backend, engine processing, and audio callbacks; native store and legacy code are feature-isolated from standalone `shoop_egui`.

Verification:

- [x] Native tests and hosted/direct-file Chrome workflows demonstrate real persisted bytes/text, reload, typed reads, and Add Track consumption without session mutation.
- [x] Native/browser failure injection demonstrates no active-value publication or persisted-byte loss on failed save, rejected version, invalid known value, or unavailable store.
- [x] `cargo tree` scans show `shoop_egui` contains no backend, engine, filesystem, browser, eframe, or Qt/frontend dependency; the product browser tree remains Qt/frontend/native-driver-free.

### Stage 6 — Final validation and documentation/ledger closure

- [ ] Update `EGUI_FEATURE_PARITY_MATRIX.md` with discovered settings rows, implementation status, intentional QML-format difference, and concrete evidence.
- [ ] Update `EGUI_REPLACEMENT_PROJECT.md` coarse status, architecture, settings ownership, roadmap, and remaining driver/FX/MIDI/script settings scope.
- [ ] Review `EGUI_MILESTONE_5_SESSION_PERSISTENCE_AND_LOOP_IO.md` and keep its historical completion record accurate while clarifying the boundary between delivered app-global settings and still-deferred session/runtime capabilities wherever its wording became stale.
- [ ] Update user/developer documentation for the dialog, initial preferences, native locations, browser key/origin behavior, format/version recovery, and deliberate lack of QML settings migration.
- [ ] Run formatting, warning-denying native/Wasm builds, focused/full Rust tests, product browser workflows, realtime guards, and retained QML self-tests; record exact evidence in this plan and the ledgers.
- [ ] Commit each completed stage or meaningful milestone, keep the pull request current, and obtain green authoritative cross-platform CI before merge.

Final gates:

- [ ] `cargo fmt --all --check`
- [ ] `RUSTFLAGS="-D warnings" cargo build --workspace --all-targets --features shoop_engine/app_backend`
- [ ] `cargo test --workspace --features shoop_engine/app_backend`
- [ ] Affected standalone/product `wasm32-unknown-unknown` checks and production Trunk/worklet packaging checks.
- [ ] Hosted browser settings save/reload workflow plus the applicable self-contained browser check.
- [ ] `target/debug/shoopdaloop_dev.sh --self-test` after the required build.
- [ ] The eight-cell egui CI matrix passes on Linux, Windows, macOS ARM, and WebAssembly in debug/release.
- [ ] Source/dependency scans and all plan/docs claims match the delivered architecture and evidence.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
