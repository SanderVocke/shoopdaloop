# Native egui audio-driver and configuration switching plan

## Current evidence

- The native egui runner currently always creates `EngineBackend::new_dummy(48_000, 256)` in `shoopdaloop_egui`; hosted browser runs own a separate automatic Web Audio path.
- `shoop_engine::app_backend` already implements JACK, CPAL+midir, dummy, and deterministic JACK/CPAL test drivers, including native host/device/MIDI discovery. The retained QML frontend composes this API, but `shoop_backend` does not yet expose it to the egui application.
- `shoop_backend::Backend` already supports transactional session capture/replacement and stable backend-ID remapping. `shoop_app` uses those operations for session loading and loop-media replacement.
- `shoop_session::resample_session` already converts every session sample domain: audio, exact MIDI event timing, loop lengths, offsets, preplay, ring-buffer lengths, and cycle timing. Session loading already presents a sample-rate warning before invoking it.
- The egui settings service already provides typed defaults, drafts, validation, atomic native persistence, unknown-key preservation, and revision publication after a successful save. It does not yet register audio-driver values or offer dynamic driver/device editors.

## Goals and scope

Deliver native egui runtime switching between every production audio-driver family currently usable on the machine and between changed configurations of the active driver. Give each discovered driver its own settings surface, warn before every switch, identify an exact source/target sample-rate difference before confirmation, preserve the current session through the switch, and persist the successfully selected driver and its per-driver configuration for the next launch.

In scope:

- native `shoopdaloop_egui`, `shoop_backend`, `shoop_app`, `shoop_app_api`, `shoop_settings`, and backend-independent `shoop_egui` presentation;
- production JACK, CPAL+midir, and the always-available dummy/offline fallback, subject to compile-time support and runtime discovery;
- one persisted editable configuration per driver family; changing that configuration is a switchable same-driver variant;
- target-only composition of the existing native engine drivers, startup selection/fallback, discovery, switch lifecycle, warnings, rollback, settings persistence, and tests;
- best-effort restoration of external connections whose exact host endpoint IDs still exist after switching.

Out of scope:

- named collections of multiple saved profiles for one driver;
- browser driver selection: hosted Web Audio remains automatic and `?offline=1` remains the explicit browser dummy mode;
- new audio drivers, changes to JACK/CPAL audio algorithms, realtime resampling, session-format changes, Web MIDI, FX/Carla UI, or retained-QML feature work;
- treating `JackTest` or `CpalTest` as user-visible drivers; they remain deterministic test seams.

## Immutable acceptance criteria

1. The native Settings dialog has an **Audio** category listing each production driver family supported by the build and its current runtime availability. Every available family is selectable; unavailable families are disabled with an actionable reason, and test-only drivers are never shown.
2. Every listed driver has its own validated settings. CPAL exposes currently discovered host, input/output device, sample-rate/buffer/channel, capture-ring, and MIDI endpoint choices; JACK exposes only settings it actually supports; dummy exposes sample rate and buffer size.
3. The settings model retains one independent configuration per driver. Editing one driver does not overwrite another, and changing any effective field of the active driver enables a same-driver **Switch** action.
4. Pressing **Switch** never mutates runtime or persisted state immediately. It first shows a warning that audio processing and current transport activity will be interrupted, identifies the source and resolved target driver/configuration, and offers explicit Confirm and Cancel actions.
5. If source and resolved target sample rates differ, that same warning prominently names both exact rates and states that all loop audio, MIDI timing, lengths, offsets, preplay, ring-buffer durations, and cycle timing will be resampled. A target whose exact rate cannot be resolved cannot proceed to this confirmation.
6. Cancel leaves the active backend, loop/session contents, settings document, and preferred startup driver unchanged.
7. Confirmed switching is serialized by the application owner. Recording/replacing or another conflicting I/O/switch task is rejected or deferred before teardown; no partial topology or mixed old/new backend state is published.
8. A successful switch preserves application track/loop identity, topology, loop contents, names, controls, Lua state, and compatible external links. Loops resume stopped and stale or driver-specific host links become disconnected diagnostics rather than corrupting or rejecting otherwise valid content.
9. A sample-rate-changing switch uses the existing `shoop_session::resample_session` path, not a second resampler, and commits the converted session only after the user confirmed the exact rate pair.
10. If target preparation, conversion, replacement, or remapping fails, the prior driver/session remains usable or is restored from the captured transaction. The failure is visible and the preferred persisted startup driver is unchanged. If restoration itself fails, the application reports a fatal backend-unavailable state with both errors rather than claiming success.
11. After backend commit, the selected driver and all per-driver settings are durably saved through the existing settings manager before the operation is reported fully successful. A persistence failure clearly reports that the new runtime is active but not the next-start preference and offers a save retry without another backend switch.
12. On the next native launch, the persisted driver/configuration is attempted first. If it is no longer available, startup falls back deterministically to another usable production driver and reports the fallback without silently overwriting the preference.
13. Driver probing, switching, resampling, persistence, and UI rendering never run in an audio callback. Native driver dependencies remain target/feature gated, and production Wasm builds and browser behavior remain unchanged.
14. Deterministic dummy/test-driver tests prove same-rate, changed-rate, same-driver-variant, cross-driver, cancellation, failure/rollback, persistence/restart, stale endpoint, and conflicting-operation behavior; optional real JACK/CPAL smoke checks skip with an explicit environment reason when hardware/services are unavailable.

## Design rules and constraints

- Keep `shoop_egui` presentation-only. It may render plain driver/configuration state from `shoop_app_api` and emit typed actions, but it must not depend on `shoop_backend`, `shoop_engine`, CPAL, JACK, filesystem APIs, or device handles.
- Keep driver creation, probing, activation, shutdown, and native engine object lifetimes in `shoop_backend`. Reuse `shoop_engine::app_backend::{AudioDriver, BackendSession, AudioDriverSettings}` and discovery helpers instead of duplicating JACK/CPAL implementations.
- Model production driver kind/configuration, discovery results, resolved rate, switch request identity, progress, and errors as framework-independent typed contracts. Do not expose engine enums, raw handles, or free-form settings maps across the application boundary.
- Give browser and non-switchable backends explicit unsupported/default implementations so the common backend contract remains cross-target.
- Treat preflight and commit as one generation-checked transaction. If the resolved target rate changes after confirmation, abort and request a new confirmation; never resample against an unconfirmed rate.
- Preserve the old backend and a complete captured `BackendSessionData` until target replacement/remapping succeeds. Make rollback and fatal restoration failure explicit states.
- Reuse session capture/conversion/remapping helpers already exercised by session load. Extract shared helpers where needed rather than routing a driver switch through file encoding/decoding or restarting the whole application runtime.
- A driver switch intentionally stops current playback and queued transitions. It must not silently stop an active recording merely to make capture succeed.
- Host endpoint identities belong to a driver instance. Restore exact compatible IDs when present; missing endpoints are non-fatal disconnected state with diagnostics.
- Add optional stable audio keys to settings format v1 without changing `document_version` unless implementation evidence shows an existing representation must change. Preserve unknown values and existing recovery semantics.
- Keep configured values distinct from resolved runtime values, especially CPAL/JACK defaults. Warnings and status use resolved values; persistence retains the user's configured selectors.
- Saving ordinary per-driver drafts must not switch the backend. Only the explicit confirmed Switch flow changes `audio.selected_driver` after backend commit.
- Keep all switch orchestration off realtime callbacks and bounded/serialized with existing application commands. Heavy conversion may use the existing control/background task boundary, but completion returns through the application owner before publication.
- Native feature wiring must not pull JACK, CPAL, midir, LV2, or platform audio packages into `wasm32-unknown-unknown` dependency trees.

## Staged implementation

Dependencies are sequential unless a checkbox explicitly says otherwise. Complete and verify each stage before beginning its dependent stage.

### Stage 0 — Freeze contracts and test fixtures

- [x] Add plain API types for driver family, configured and resolved variants, discovered capabilities/endpoints, active state, preflight warning data, and generation-scoped switch task/status.
- [x] Extend `AppSnapshot` and typed intents without importing engine types; define explicit unsupported/browser defaults.
- [x] Extend the backend contract with discovery, preflight, commit/abort, and rollback semantics sufficient for an exact-rate confirmation before mutation.
- [x] Add deterministic fake catalogs/configurations and failure injection for unavailable drivers, changed negotiation, target failure, replacement failure, and rollback failure.
- [x] Document invariants for stopped-on-switch transport, exact-rate reconfirmation, ID remapping, and best-effort external-link restoration.

Verification:

- [x] `cargo test -p shoop_app_api -p shoop_backend` passes contract/serialization-free identity tests and fake switch-state tests (9 API and 16 backend tests on 2026-08-08).
- [x] Browser/default backend contract tests prove switching is unsupported without changing current Web Audio state.
- [x] Commit the contract and fixture milestone.

### Stage 1 — Native driver-capable backend and discovery

- [ ] Add a native-only `shoop_backend` feature/implementation that adapts the existing application-backend `AudioDriver`/`BackendSession` object model to the full normalized `Backend` topology, content, status, connection, capture, and replacement contract.
- [ ] Implement runtime discovery for production JACK, CPAL hosts/devices, midir endpoints, and dummy fallback. Filter test drivers from production catalogs and preserve actionable discovery/start errors.
- [ ] Implement configured-to-engine settings conversion and resolved-state reporting, including the actual sample rate and buffer size negotiated by defaults.
- [ ] Implement generation-safe driver preparation, teardown, replacement, and restoration while retaining captured session data until commit.
- [ ] Preserve exact compatible external links and publish missing endpoints as disconnected/failure observations rather than failing session restoration.
- [ ] Target-gate native driver dependencies in Cargo so the existing direct core `EngineBackend` remains available to browser/offline and focused dummy tests.

Verification:

- [ ] Run the shared backend topology/session/connection contract against native dummy and deterministic JACK/CPAL test adapters.
- [ ] Add native backend tests for discovery filtering, configured/resolved values, same-family restart, cross-family switch, endpoint churn, target failure, and rollback.
- [ ] `cargo test -p shoop_backend --features native-drivers` and `RUSTFLAGS="-D warnings" cargo check -p shoop_backend --features native-drivers` pass.
- [ ] `cargo tree -p shoopdaloop_egui --target wasm32-unknown-unknown` still excludes native driver packages.
- [ ] Commit the native backend milestone.

### Stage 2 — Per-driver settings and persistence contract

- [ ] Register stable native audio keys for preferred driver and independent JACK, CPAL+midir, and dummy configurations, with defaults and constraints matching engine semantics.
- [ ] Add any required settings editor/value support for driver/device choices without putting runtime device handles or transient discovery state in `settings.json`.
- [ ] Keep stored configured selectors separate from resolved runtime values and preserve unknown keys, recovery behavior, failed-save atomicity, and existing script-settings publication order.
- [ ] Update `docs/settings_format_v1.md` with the new optional keys, effects, defaults, and startup/fallback behavior; retain document version 1 if only optional keys are added.
- [ ] Add helpers that map a validated settings snapshot/draft to typed driver configuration and update only the preferred-driver key after a successful switch.

Verification:

- [ ] Settings tests cover defaults, independent driver values, invalid/stale device values, deterministic JSON, unknown-key preservation, failed save, retry, and restart loading.
- [ ] Existing settings and script reconciliation tests remain unchanged in behavior.
- [ ] `cargo test -p shoop_settings -p shoop_egui -p shoopdaloop_egui` passes focused settings tests.
- [ ] Commit the settings milestone.

### Stage 3 — Transactional application switch and resampling

- [ ] Add an application-owned switch state machine: validate idle/capture eligibility, preflight the configured target, publish exact warning data, handle cancel, and generation-check confirm.
- [ ] Capture the current backend session before teardown and retain application/Lua state and old backend metadata for rollback.
- [ ] For a changed rate, convert the captured data through the existing session-bundle mapping and `resample_session`; for an unchanged rate, bypass conversion byte/value-semantically.
- [ ] Commit target driver/session replacement, reuse backend-entity remapping to retain application IDs, stop transport state, invalidate waveform caches as needed, and refresh status/connection snapshots atomically.
- [ ] Abort and return to confirmation if commit-time rate resolution differs from the confirmed target.
- [ ] Implement failure restoration and explicit fatal-unavailable publication when both target commit and old-driver restoration fail.
- [ ] Exclude recording/replacing and conflicting session/media I/O without silently mutating either task.

Verification:

- [ ] Application actor tests cover cancel/no-op, same-rate cross-driver, same-driver changed settings, rate change, exact warning text/data, reconfirmation, active recording, conflicting I/O, target failure, remap failure, rollback, and rollback failure.
- [ ] Rate-change tests assert all audio samples and every integer-frame domain use `resample_session` results, preserve equal-frame MIDI order, and are not converted on cancellation/failure.
- [ ] Stable application IDs, controls, script state, stopped transport, compatible links, and stale-link diagnostics are asserted before/after commit.
- [ ] `cargo test -p shoop_app --features shoop_backend/native-drivers` passes, including threaded actor ordering tests.
- [ ] Commit the application transaction milestone.

### Stage 4 — Audio settings UI, warning, and persistence orchestration

- [ ] Add the native **Audio** settings category driven only by plain catalog/snapshot state: one section per discovered driver, appropriate controls per family, availability/error text, configured and resolved summaries, and reset behavior.
- [ ] Refresh host/device/MIDI choices when the dialog opens and when a dependent selector changes, retaining a missing saved selector visibly long enough for correction rather than silently replacing it.
- [ ] Enable **Switch** only for a valid driver/configuration that differs effectively from the active resolved/configured pair; ordinary Save continues to persist drafts without switching.
- [ ] Add the mandatory confirmation popup with interruption warning and source/target details. Add the prominent exact-rate resampling warning only when rates differ.
- [ ] Wire Confirm/Cancel to generation-scoped intents and render preflight, switching, resampling, restoring, failed, active-but-not-persisted, and completed states without blocking egui.
- [ ] In the composition root, retain the confirmed settings draft by request ID; after backend commit, persist the preferred driver/configuration through `SettingsManager`, report success only after durable save, and offer persistence retry without dispatching another switch.

Verification:

- [ ] Backend-free egui interaction/paint tests cover every driver editor, unavailable state, same-driver variant detection, validation, cancellation, same-rate warning, changed-rate warning with exact Hz values, progress/failure states, and minimum/common viewports.
- [ ] Runner tests prove switch completion triggers one settings save, failed persistence does not repeat the backend switch, retry saves the already-active configuration, and stale request/revision results are ignored.
- [ ] Browser presentation tests show no native Audio category or switching action and preserve existing Web Audio permission/status UI.
- [ ] Commit the UI and orchestration milestone.

### Stage 5 — Startup selection, fallback, and lifecycle hardening

- [ ] Construct the native runtime from persisted preferred driver/configuration instead of hard-coded dummy values.
- [ ] Attempt the preference first, then use a documented deterministic fallback order among currently usable production drivers; retain the failed preference and publish an actionable startup diagnostic.
- [ ] Ensure startup and shutdown release probe/candidate/old driver resources exactly once, including partial construction and fatal restoration paths.
- [ ] Add optional real JACK and CPAL smoke workflows that use current-system discovery and explicitly skip when services/devices are absent.
- [ ] Update `EGUI_REPLACEMENT_PROJECT.md` and `EGUI_FEATURE_PARITY_MATRIX.md` only after implementation evidence supports marking native driver/device management complete.

Verification:

- [ ] Restart tests load each persisted family/configuration, cover unavailable saved devices and fallback, and prove fallback does not overwrite the stored preference.
- [ ] Repeated test-driver switching and shutdown tests show no leaked driver threads, duplicate clients/streams, stale callbacks, or post-drop actor activity.
- [ ] A manual native smoke pass switches between every driver available on the validation host, including one same-driver settings change and one sample-rate-changing path where supported; environment skips are recorded precisely.
- [ ] Commit the startup/lifecycle milestone.

### Stage 6 — Final end-to-end validation

- [ ] Run `cargo fmt --all -- --check` and `git diff --check`.
- [ ] Run focused native settings/backend/application/egui/runner tests with warning denial.
- [ ] Run `RUSTFLAGS="-D warnings" cargo build --workspace --features shoop_engine/app_backend`.
- [ ] Run `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo test --workspace --features shoop_engine/app_backend`.
- [ ] Build first, then run the retained frontend/QML self-test suite with `target/debug/shoopdaloop_dev.sh --self-test`.
- [ ] Run the locked production Wasm check for `shoopdaloop_egui`, the preview Wasm check, the AudioWorklet build, and dependency-tree forbidden-package scans used by `.github/workflows/build_and_test_egui.yml`.
- [ ] Exercise end to end: create audio and MIDI loop content; switch same rate; switch a same-driver variant; cancel a changed-rate switch; confirm a changed-rate switch; verify scaled content/timing and stopped transport; save/restart; verify preferred startup configuration and links/diagnostics.
- [ ] Record exact driver/device environment evidence, skipped real-driver checks, test counts, and any residual limitations in the plan and parity matrix.
- [ ] Commit the completed validation/documentation milestone.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
