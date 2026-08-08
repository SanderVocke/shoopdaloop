# egui Cross-Target Ports, Browser Lua, and omniLua Migration Plan

## Status and document role

Status: **Complete**. Stages 0–6 satisfy the frozen acceptance criteria with native/workspace/realtime, retained QML Lua, production Wasm, Chrome hosted/direct-file, Firefox hosted, settings, routing, keyboard, source-bearing session, dependency, and release-package evidence. The only full-QML-suite exception is the documented environment-unavailable CPAL case; all retained Lua-specific QML cases pass. Native real-driver selection was intentionally outside this historical milestone and is now completed separately by `EGUI_NATIVE_AUDIO_DRIVER_SWITCHING_PLAN.md`; that later work does not change this plan's frozen scope.

This is the completed implementation ledger for making application ports, host ports, connection management, and Lua scripting consistent in the egui product, including the production WebAssembly build and self-contained HTML artifact, while replacing `mlua` with omniLua throughout the entire workspace. It supersedes the earlier accepted browser-specific omissions recorded in `EGUI_FEATURE_PARITY_MATRIX.md` and records the evidence that closes them.

This plan depends on and must be maintained with:

- `EGUI_REPLACEMENT_PROJECT.md` for project architecture and coarse status;
- `EGUI_FEATURE_PARITY_MATRIX.md` for capability-level status and evidence;
- `docs/egui_lua_compatibility_contract.md`, `docs/settings_format_v1.md`, `docs/session_format_v1.md`, and the egui runner README for delivered contracts and user-facing limits.

## Investigation findings and known technical risk

- The application/backend API already calls track ports local/application ports and opposite endpoints external ports, but the backend snapshot repeats endpoint candidates per local port rather than modeling host inventory explicitly.
- At this milestone's investigation boundary, the native dummy backend exposed mutable virtual external endpoints and native real-driver composition was absent. The later native-driver milestone now adapts retained JACK and CPAL/midir client/system endpoints into the same normalized model.
- Before this milestone, the Web Audio path was the exception: every track owned physical `ExternalAudioPort`s, `process_audio_quantum` mapped every track directly to the microphone/destination, the browser connection snapshot had no candidates and reported management unavailable, and the worklet protocol had no connection commands or confirmed route state.
- Before this milestone, direct MIDI track ports were created in the browser backend, but a no-endpoint category did not visibly identify its local ports in the connection matrix.
- Before this milestone, Lua was excluded with target `cfg`s from `shoop_app`, `shoopdaloop_egui`, settings registration, session loading, and key dispatch. `shoop_scripting` also depended unconditionally on native `midir`.
- Stock `mlua 0.11` with vendored Lua 5.4 is a concrete blocker for the existing `wasm32-unknown-unknown` product target: a minimal probe fails in `lua-src` with `don't know how to build Lua for wasm32-unknown-unknown`. Stock `mlua` documents Emscripten rather than this target, and changing the eframe/Trunk product to Emscripten is not an acceptable shortcut.
- A minimal `omnilua 0.7.1` Lua 5.4 probe compiles for `wasm32-unknown-unknown` without a C toolchain, and its embedding API is intentionally similar to `mlua`. The user has selected omniLua as the replacement runtime. It is young and not drop-in compatible, so the first stage must prove the complete Shoop compatibility surface and record required API adaptations before behavioral implementation proceeds.
- The required replacement was workspace-wide, not egui-only. At investigation time, in addition to `shoop_scripting`, the retained `frontend` crate directly used `mlua` across its Lua engine, QVariant/Qt conversion layer, stored callback wrappers, session control handler, weak runtime ownership, and QML bridge. Those paths and their retained tests therefore had to move to omniLua.

If omniLua cannot pass the frozen Lua API, callback, conversion, bundled-script, retained QML/frontend, native, and Wasm gates without weakening behavior, implementation must stop at Stage 0 and report the exact incompatibilities and upstream or local adaptation needed. Do not silently change the Lua contract, retain `mlua` as a fallback, switch the whole browser product target, or ship separate native/browser script semantics.

## Goals

- Give every backend an explicit, typed distinction between application-owned ports and host/driver-owned ports; an empty host inventory is a valid state, not an unsupported application-port state.
- Replace Web Audio's fixed per-track DSP mapping with real, mutable connections between application audio ports and browser microphone/destination channel endpoints.
- Keep audio and MIDI track ports available on every target. With no Web MIDI implementation, browser MIDI application ports remain visible while the MIDI host inventory is empty.
- Show Lua-created logical MIDI control ports in the global connection view, including the no-host-endpoint browser state, without changing script-owned autoconnect semantics into competing GUI-owned policy.
- Replace `mlua` with one pinned, audited omniLua Lua 5.4 implementation across `shoop_scripting`, the retained frontend/QML Lua bridge, tests, manifests, and lockfile.
- Run the existing Lua scripting engine and compatibility API in production `wasm32-unknown-unknown` builds. Bundle and expose the unchanged built-in scripts in both hosted and self-contained browser artifacts, with keyboard control enabled by default.
- Preserve native and retained QML behavior, realtime bounds, session/settings integrity, presentation isolation, browser audio lifecycle recovery, and existing artifact contracts.

## Scope

Included:

- `shoop_app_api`, `shoop_app`, `shoop_backend`, `shoop_scripting`, `shoop_audio_protocol`, `shoop_audio_worklet`, `shoop_egui`, `shoopdaloop_egui`, and the retained `frontend` Lua/QML bridge contracts and tests needed for this milestone;
- fake/dummy and Web Audio application/host port inventories and connection behavior;
- browser microphone/output endpoint lifecycle and mutable worklet routing;
- direct-track audio/MIDI ports and Lua-created logical MIDI control ports;
- project-wide omniLua adoption, removal of `mlua` and its C-runtime dependency chain, cross-target bundled Lua, keyboard events, timers/callbacks, source-bearing session scripts, browser settings, packaging, and automation;
- migration of browser sessions that predate explicit Web Audio route persistence;
- maintenance of all planning documents and relevant durable/user documentation.

Not included:

- Web MIDI endpoint discovery or message I/O;
- native egui JACK/CPAL driver selection and device settings;
- the generic MIDI-control rule editor deferred in the parity matrix;
- dry/wet, FX send/return, or composite editing topology;
- increasing the existing bounded Web Audio device-channel ceiling as an unrelated feature. Every channel admitted by that boundary must nevertheless have a truthful host endpoint;
- hostile-code sandbox hardening, C Lua modules, LuaJIT, or browser filesystem paths for machine-wide user scripts.

## Immutable acceptance criteria

These criteria may not be weakened or reinterpreted without explicit user approval.

### Port architecture and presentation

1. The shared contracts represent application ports, host ports, and confirmed links as distinct typed concepts. Host inventory may be empty while application audio/MIDI/control ports remain valid and visible.
2. Application-port identity includes ownership. Track scope contains exactly that track's ports; global scope contains sync, main-track, and active Lua-created control ports. Presentation does not infer ownership from names.
3. Direction and data-type compatibility are enforced once at the application/backend boundary. Exact stable endpoint identities, pending desired state, confirmed state, failures, and lifecycle churn remain observable.
4. A connection category with application ports but zero compatible host endpoints still displays the application-port names and an explicit no-host-endpoints state.
5. Fake/dummy connection contracts continue to pass. The architecture remains suitable for the retained JACK client/system-port and CPAL/midir virtual-port mappings without adding those native drivers to this milestone.

### Web Audio connections

6. Each browser destination channel admitted to DSP is published as a host audio input/sink endpoint. Each microphone channel actually present is published as a host audio output/source endpoint; output-only, denied, ended, retry, and changed-channel states remove or restore microphone endpoints truthfully.
7. Web Audio connection cells are mutable. Connect/disconnect is carried over the bounded UI/worklet protocol, changes the actual microphone/engine/destination route, and is reported as confirmed only from authoritative worklet state.
8. Fresh browser state preserves the currently usable audio behavior through ordinary initial connections equivalent to the existing mono/stereo mapping. Those links are visible, disconnectable, persistable, and are not silently recreated after an explicit disconnect.
9. Session capture/load round-trips explicit Web Audio routes, including explicit no-connection state. Sessions produced before this milestone receive a documented deterministic migration rather than becoming unexpectedly silent or making disconnection impossible to persist.
10. The browser never invents MIDI host endpoints. Direct MIDI track ports are creatable and visible with no candidates until a future Web MIDI service supplies real endpoints.

### omniLua replacement, browser Lua, and control ports

11. omniLua is the only Lua runtime used anywhere in the workspace. `mlua`, `mlua-sys`, vendored `lua-src`/`luajit-src`, compatibility aliases that hide `mlua`, and target-specific fallback runtimes are absent from manifests, resolved dependency trees, lockfiles, Rust sources, tests, and packaged products.
12. The pinned omniLua Lua 5.4 configuration preserves the public Shoop Lua language/API contract across native egui, browser egui, and the retained QML frontend: the 61-function control surface, constants, conversions, sandbox runner, built-in modules, callback ownership/order, timers, errors, and script lifecycle remain behaviorally compatible.
13. The retained frontend's Lua engine, Qt/QVariant conversions, callback wrappers, session-control registration, weak runtime ownership, MIDI-control bridges, and QML-facing behavior use omniLua directly or through shared Shoop abstractions; no frontend-only `mlua` semantics remain.
14. `AppSnapshot.scripting.supported` is true in production browser builds. Bundled and source-bearing session scripts use the same omniLua-backed compatibility implementation as native egui.
15. The unchanged embedded `keyboard.lua` is available in hosted and self-contained artifacts, enabled on first run, and drives authoritative browser application/backend state from egui press/release events. It requires no source checkout, fetch, or physical MIDI endpoint.
16. The unchanged embedded APC script is available and can run with zero Web MIDI endpoints: its logical input/output control ports and zero-connection diagnostics are visible, while the script remains healthy and ready for future endpoint discovery.
17. Lua-created logical MIDI ports have stable application ownership and appear in the global connection view. Script regex autoconnect remains script-owned policy; the matrix reflects those confirmed links on native systems without creating a conflicting GUI desired-state owner.
18. Browser bundled-script settings are registered and persisted in `localStorage`; keyboard defaults enabled and APC defaults disabled. Native path-based user-script settings remain native-only and are not exposed as a nonfunctional browser file-path workflow.
19. Enabled browser session scripts are syntax-checked before transactional commit, start after commit, save back exactly, and fail script-locally. Browser session loading no longer rejects a document merely because it contains supported scripts.

### Realtime, packaging, and documentation

20. Lua runs only on the application owner/control side in egui and retains its existing control-thread ownership in the QML product. No Lua interpreter, script callback, unbounded allocation, lock, JSON handling, or connection topology mutation is introduced into an audio render callback.
21. Worklet command/event queues, topology changes, channel storage, and publication remain bounded and failure-visible. Existing callback-budget, no-allocation/no-lock, saturation/recovery, and sustained-audio evidence continues to pass.
22. Hosted archives and self-contained HTML contain the UI Wasm, worklet assets, omniLua runtime code, Shoop Lua libraries, and built-in scripts. The standalone artifact runs keyboard control directly from `file:` wherever the existing app itself is supported; physical audio remains subject to documented browser policy.
23. `plans/EGUI_WEB_PORTS_AND_WASM_LUA_PLAN.md`, `plans/EGUI_REPLACEMENT_PROJECT.md`, and `plans/EGUI_FEATURE_PARITY_MATRIX.md` remain synchronized at every stage. Relevant durable contracts, runner documentation, package checks, CI descriptions, and retained QML Lua documentation are updated in the same stage as behavior.

## Design rules and constraints

- Use **application port** and **host port** in new shared contracts. UI copy may retain “External port” where useful, but type and owner names must not conflate an app port with a device/system endpoint.
- Prefer normalized host inventory plus confirmed link pairs over duplicating independently mutable endpoint truth in every application-port candidate list. Presentation-specific matrices are derived read models.
- Use stable full host endpoint names/IDs for identity and session persistence; display splitting/grouping is presentation-only.
- Model application-port ownership explicitly, including non-track/script ownership. Do not assign fake `TrackId`s to control ports.
- Keep one ordered application owner for GUI, Lua, connection, and session intents. Backend/worklet owns physical route truth; scripts own only their declared autoconnect policy.
- Preserve the browser's current mapping as initial desired connections, not hidden DSP wiring. Separate desired policy from confirmed worklet truth so endpoint loss/retry is coherent.
- Prepare and apply graph/topology changes outside the render section, then atomically adopt bounded prepared state. Do not mutate graph vectors or allocate while processing a quantum.
- Keep `shoop_egui` backend-, engine-, Lua-, browser-API-, and filesystem-free. It renders plain owner/port/script snapshots and emits typed intents.
- Use `NullMidiService` (or an equivalent empty host service) on Wasm until Web MIDI exists. Split `midir` and native synchronization dependencies behind target-specific modules rather than disabling scripting.
- Use one exact, reviewed omniLua version and Lua 5.4 configuration throughout the workspace. Pin and audit the pre-1.0 dependency; upgrades require rerunning the complete native, Wasm, and retained QML compatibility evidence.
- Do not add an `mlua` compatibility shim, renamed dependency, fallback feature, or separate frontend/browser interpreter. Adapt Shoop's embedding and conversion code to omniLua's real API, centralizing only genuinely shared Shoop behavior.
- Keep built-in Lua sources single-sourced under `src/lua`; no browser forks or generated script rewrites.
- Preserve explicit browser gestures for audio. Lua/keyboard readiness must not depend on microphone permission or AudioContext startup.
- Keep session format migration deterministic and versioned. Never infer a future explicit disconnect from an empty legacy route list without format/provenance evidence.

## Staged implementation plan

Dependencies are strict unless a documented finding warrants reordering: Stage 0 blocks the mandated runtime migration; Stage 1 defines port contracts needed by Stages 2 and 5; Stage 2 establishes physical Web Audio routing before end-to-end browser workflows; Stage 3 migrates shared scripting and enables browser Lua; Stage 4 migrates the retained frontend and removes `mlua`; Stage 5 combines script control-port publication with complete UI/settings behavior.

### Stage 0 — Freeze compatibility evidence and prove the omniLua migration path

- [x] Pin exact omniLua 0.7.1 with the reviewed lean Lua 5.4 configuration and audit its license, dependency tree, binary policy, panic/reentrancy behavior, garbage-collection/rooting model, callback ownership, and pre-1.0 upgrade policy in `docs/omnilua_runtime.md`.
- [x] Exercise syntax, sandbox runner, preloaded `require`, Rust callbacks, integer/table/multivalue adaptation, cloned callback handles, weak/owned runtime lifetime patterns, timers, error containment, all embedded libraries, and both bundled scripts through the migrated scripting/frontend tests. Browser execution remains a separate verification item below.
- [x] Inventory and compile the retained frontend-specific QVariant/Qt conversions, evaluation/execute behavior, stored arguments, Rust-to-Lua and Lua-to-Rust callbacks, engine ownership, MIDI control wrappers, and callback cleanup paths directly against omniLua.
- [x] Run the complete 21-test `shoop_scripting` compatibility suite against omniLua, including every `CONTROL_FUNCTION_NAMES` entry and callback retention/removal.
- [x] Record omniLua as the required runtime and document the supported Lua 5.4 standard-library/sandbox profile in `docs/egui_lua_compatibility_contract.md` and `docs/omnilua_runtime.md` without changing the frozen Shoop API.
- [x] omniLua met the immutable contract through local API adaptations; the blocked stop condition was not triggered and no fallback runtime was introduced.

Verification:

- [x] Native `shoop_scripting` (21 tests), frontend Rust tests (33 tests, including 11 Lua-engine tests), and all retained Lua-specific QML self-tests pass with omniLua.
- [x] The production Wasm artifact runs embedded omniLua `keyboard.lua`; Chrome CDP key events clear/move authoritative loop selection after the full audio/session workflow.
- [x] `cargo tree -p shoop_scripting --target wasm32-unknown-unknown` contains omniLua but no former C Lua runtime, `midir`, ALSA/CoreMIDI/WinMM, or Emscripten dependency.

### Stage 1 — Normalize application-port/host-port contracts and ownership

- [x] Refactor API/backend connection DTOs to publish separate typed application-port inventory, host-port inventory, and confirmed links with stable IDs, directions, data types, revisions, and failures.
- [x] Replace mandatory `track_id` ownership with an explicit owner model supporting sync/main tracks and Lua control ports; retain stable track mapping across backend replacement.
- [x] Derive immutable pending state and compatibility from normalized truth in `shoop_app`; route mutation with typed `HostPortId` identity and reject stale/incompatible pairs deterministically.
- [x] Update FakeBackend and EngineBackend dummy discovery/mutation contracts, endpoint churn, deferred confirmation, persistence capture, and rollback tests.
- [x] Update connection-dialog grouping and empty-endpoint rendering so app columns remain visible. Keep track scope isolated and global scope owner-complete.
- [x] Update preview fixtures and GUI tests for normalized track, churn, pending, no-host, and error states without adding implementation dependencies to `shoop_egui`; Lua control publication remains Stage 5.
- [x] Add `docs/egui_port_model.md` and update both project planning documents with the implemented contract/evidence from this stage.

Verification:

- [x] API structural-sharing/identity tests, fake/dummy backend contracts, application pending/error tests, GUI interaction/paint tests at 360×200 and 900×600, and preview native check pass; the same target-neutral crates compile in the production Wasm check.
- [x] Source/dependency boundaries keep `shoop_egui` free of backend, engine, scripting, driver, and platform API dependencies.

### Stage 2 — Implement authoritative Web Audio host ports and mutable DSP routes

- [x] Introduce bounded engine/worklet host-channel routes and separate virtual track ports; remove unconditional per-track microphone staging and destination mixing.
- [x] Publish stable destination endpoints and lifecycle-driven microphone endpoints for every channel admitted by the Web Audio boundary.
- [x] Extend worklet protocol version 3 with bounded set-connection commands, same-link journal supersession, nonfatal rejection, and authoritative topology/confirmed-link publication.
- [x] Implement connect/disconnect routing for microphone-to-app-input and app-output-to-destination links, including mono fan-out/current N-channel mapping as explicit initial links.
- [x] Preserve explicit desired links through delayed startup/worklet replay; clear stale confirmation through authoritative host snapshots and restore journaled desired state after retry.
- [x] Add exact-route session capture/replacement and defaultable `connection_model_version` migration for pre-normalized browser sessions.
- [x] Make Web Audio report connection management available independently of whether microphone permission has produced input endpoints.
- [x] Update browser/audio/session contracts, runner README, parity rows, and project status for the delivered routing implementation; package/runtime browser evidence was intentionally deferred to the later integration stages.

Verification:

- [x] Engine/worklet tests prove disconnected input records silence, connected selected-channel input, disconnected output silence, connected mono/stereo/many-channel output, exact session routes/migration, and allocation-free route processing.
- [x] Protocol/worklet/application tests prove desired/confirmed ordering, journal replay/supersession, malformed/stale rejection, queue bounds, timeout, nonfatal failure, and session route round-trip.
- [x] Hosted Chrome and Firefox automation opens the real global connection dialog; Chrome observes normalized endpoint/link counts, disconnects/reconnects exact destination links, and proves silent/restored output while callbacks continue, with Firefox rerunning the complete workflow.
- [x] Chrome output-only, denial/retry, track-end/retry, processor-loss/retry, saturation/recovery, stress, offline dummy, settings-unavailable, and direct-file artifact regressions pass.

### Stage 3 — Migrate shared scripting to omniLua and enable the cooperative browser app

- [x] Replace `shoop_scripting`'s former runtime dependency and imports with pinned omniLua Lua 5.4, adapting the sandbox, Shoop modules, control API, conversions, callbacks, timers, lifecycle, logs, and diagnostics without a compatibility alias.
- [x] Isolate `NativeMidiService`/`midir` behind native target dependencies while compiling the shared service contract, fake service, and empty service on Wasm.
- [x] Remove Wasm rejection branches and target `cfg`s from `shoop_app` scripting state, reducers, events, script compositions, session staging/commit, and save capture.
- [x] Add startup-script support to the cooperative runtime and construct the Wasm application with the empty MIDI host service.
- [x] Keep scripting progress bounded per application tick and preserve callback non-reentrancy, queue limits, script-local failure, and audio-worklet independence.
- [x] Compile the existing source-bearing session transaction, rollback, machine/session separation, exact save/load, callback/timer, APC, and full keyboard paths into the Wasm application; native execution of the same paths remains green.
- [x] Update the Lua contract, session contract, runner README, parity matrix, and project architecture/status for the removed compiler/composition limitations.

Verification:

- [x] Existing native `shoop_scripting` (21), application (31), and bundled keyboard/APC tests pass on omniLua without accepted behavior differences.
- [x] Production Wasm compile plus Chrome hosted/direct-file execution prove embedded omniLua, keyboard operations, zero-MIDI-host APC startup/settings, and active source-bearing session load/exact save; shared native suites cover callback/timer/error edge cases compiled into that artifact.
- [x] egui Wasm dependency scans contain omniLua and `shoop_scripting` while excluding the former C Lua toolchain and native MIDI/system audio packages.

### Stage 4 — Migrate the retained frontend/QML runtime and eliminate `mlua`

- [x] Replace the workspace runtime dependency with pinned omniLua and migrate the frontend Lua engine, public conversion traits, QVariant/table conversions, multivalue handling, callback interfaces, and error mapping.
- [x] Adapt stored `Function`/`Table`/`Value` rooting, `Arc`/`Weak` runtime identity, engine installation/uninstallation, timed callbacks, MIDI-control callbacks, and dead-runtime cleanup to omniLua's ownership model without stale callable handles.
- [x] Keep shared sandbox/print/module behavior single-sourced where it was already shared; no egui-only or QML-only compatibility environment was introduced.
- [x] Preserve tested evaluate/execute/callback/conversion results and script error containment, including compile-before-expression fallback so failed chunk evaluation is never executed twice.
- [x] Remove the former runtime from all manifests and Rust sources, regenerate `Cargo.lock`, and remove its C-runtime dependency chain.
- [x] Update durable Lua documentation and planning rows to identify omniLua as the sole project runtime.

Verification:

- [x] Focused frontend Rust tests and every retained Lua-specific QML testcase pass on omniLua, including `tst_LuaEngine.qml`, `tst_LuaEngine_SessionControlHandler.qml`, and `tst_LuaScriptWithEngine.qml`. The full offscreen suite passed 235/236; the sole unrelated CPAL-port case was unavailable because CPAL test settings were absent.
- [x] Native egui scripting tests rerun after workspace dependency removal; the shared 21-test scripting suite and frontend Lua-engine tests use the same pinned omniLua semantics.
- [x] Workspace manifest, Rust-source, metadata, lockfile, resolved-tree, release archive, and packaged-Wasm string scans contain no former runtime or C-runtime dependency.

### Stage 5 — Publish control ports and complete browser settings/artifact UX

- [x] Give each Lua MIDI input/output registration a deterministic script/registration-owned application-port ID; remove it on stop/forget and restore the same ID on restart without pending-link leakage.
- [x] Merge script logical ports and raw-ID host MIDI observations into the global connection view. Mark regex-autoconnect cells owner-managed and preserve diagnostic/confirmed truth.
- [x] Ensure direct MIDI track ports and logical control ports remain named/visible with zero MIDI host endpoints; application and GUI tests cover stop/restart and global-versus-track scope.
- [x] Split script settings registration into cross-target bundled toggles and native-only user-path definitions/actions. Show a functional browser Scripts tab without a dead Add-file action.
- [x] Reconcile committed browser settings revisions into startup/runtime bundled scripts; failed `localStorage` saves cannot change the active revision or running scripts.
- [x] Ensure hosted and self-contained package generation embeds omniLua and all `include_str!` Lua sources, and rejects missing/stale assets, forbidden native dependencies, or any reintroduced `mlua` runtime.
- [x] Update all planning documents, port/settings/session/Lua contracts, runner README, artifact descriptions, and browser limitation copy in the same milestone.

Verification:

- [x] Application/GUI/settings tests prove global versus track ownership, control-port stop/restart stability, owner-managed native links, zero-endpoint categories, bundled/native registry separation, existing Save/Cancel/failure semantics, and keyboard/APC defaults.
- [x] Hosted/self-contained browser automation opens the registered Scripts category, verifies keyboard/APC saved runtime states, drives authoritative keyboard selection, observes two APC ports with zero MIDI hosts, and loads/activates/exactly resaves a source-bearing session script.
- [x] Release archive and standalone HTML inspections find the omniLua dependency in the resolved tree, built-in source/settings markers in packaged Wasm, no former C Lua runtime, and direct-file execution without checkout/network Lua dependencies.

### Stage 6 — Final end-to-end validation and documentation closure

- [x] Run formatting and warning-denying native/Wasm builds for all changed crates and artifacts.
- [x] Run focused API, backend, engine realtime, protocol, worklet, scripting, frontend/QML, application, presentation, runner, preview, session, settings, and package tests.
- [x] Run `cargo test --workspace --features shoop_engine/app_backend` and the retained QML self-test suite.
- [x] Build debug and release hosted and self-contained browser artifacts; run Chrome/Firefox normal, minimum-size, output-only, microphone, lifecycle, connection mutation, Lua keyboard, session-script, settings, stress, and direct-file workflows.
- [x] Recheck native egui dummy connection and omniLua workflows, including native MIDI virtual-port evidence where the host supports it and a documented environment skip otherwise.
- [x] Audit dependency trees and packaged files for target isolation, omniLua version/configuration consistency, absence of `mlua` and its C runtime, native MIDI leakage into Wasm, stale fixed-routing copy, and missing embedded scripts.
- [x] Reconcile every planned matrix row and the project coarse status with exact test/artifact evidence; leave no plan document claiming completion without evidence.

Final evidence demonstrates one production browser run in which microphone and destination channels appear as host ports, a user-visible destination disconnect changes actual audio to silence, reconnect restores non-zero output, callbacks continue, omniLua-backed `keyboard.lua` controls authoritative selection, and MIDI track/control application ports remain visible with zero MIDI host endpoints. Hosted Chrome and Firefox and self-contained Chrome workflows exercise the production artifact, subject only to documented browser audio policy. The retained QML Lua product runs on omniLua, and source, lockfile, resolved-tree, archive, standalone, and packaged-Wasm scans prove workspace-wide absence of `mlua` and its C runtime dependency chain.

Stage 6 closure evidence (2026-08-08):

- `cargo fmt --all -- --check`, `git diff --check`, warning-denying native checks for all changed Rust crates, warning-denying production `wasm32-unknown-unknown` checking, and the release AudioWorklet build pass.
- `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo test --workspace --features shoop_engine/app_backend` passes with 1,415 tests passed, zero failed, and four ignored. This includes the backend realtime-allocation guard, protocol/worklet routing, scripting, application, GUI, runner, preview, settings, session, and documentation tests.
- The retained offscreen QML suite passes 235/236. Its only failure is `CpalPorts::test_virtual_playback_ports_are_app_connectable`, for which this host has no CPAL test configuration; every retained Lua-specific QML case and all 33 frontend Rust tests pass on omniLua.
- Native dummy/application and omniLua tests pass. The two physical virtual-MIDI probes skip with explicit evidence because `/dev/snd/seq` is absent, while fake/native-policy APC connection contracts remain covered by the workspace suite.
- Debug and release hosted/self-contained builds succeed. Chrome hosted, 360×200, output-only, denied/retry, track-ended/retry, processor-loss/retry, saturation/recovery, sustained stress, offline dummy, unavailable-storage, settings, microphone direct-file, and standalone workflows pass; Firefox hosted under Xvfb passes the complete audio/route/Lua/session workflow.
- The final release archive passes `unzip -t`; the standalone HTML is self-contained. Required marker scans find both unchanged bundled scripts, browser settings/session/Lua-control and Web Audio route markers. Dependency/source/package scans find pinned omniLua 0.7.1 and no `mlua`, `mlua-sys`, `lua-src`, `luajit-src`, native MIDI/system-audio dependency leakage, or stale fixed browser routing.

## Completion audit: acceptance criterion to artifact

This checklist maps every immutable acceptance criterion to implementation and direct verification surfaces. Stage checkboxes above describe the work sequence; this table is the final prompt-to-artifact audit.

| Criteria | Concrete implementation artifacts | Direct verification evidence |
|---|---|---|
| 1 | `shoop_app_api::{ApplicationPortState, HostPortState, ConfirmedConnectionState, PendingConnectionState, HostPortId}` and normalized backend/application snapshots | API identity/structural-sharing tests plus fake/dummy normalized inventory contracts |
| 2 | `ApplicationPortOwner::{Track, LuaControl}` and track/global scope filtering in `shoop_app`/`shoop_egui` | Owned-port application tests, global/track scope GUI tests, and Lua-control lifecycle coverage |
| 3 | `ConnectionPolicy`, typed compatibility validation, pending/confirmed/error publication, and stable host IDs | `actor_publishes_owned_ports_and_serializes_connection_churn_and_failure`; timeout/churn/failure tests; backend connection contracts |
| 4 | `shoop_egui/src/connection_dialog.rs` derives categories from application inventory independently of host rows | `empty_host_inventory_keeps_application_ports_visible_and_safe`; 360×200 and 900×600 paint/interaction coverage |
| 5 | `shoop_backend` normalized FakeBackend and EngineBackend dummy discovery/mutation | `connection_contract`; `fake_connection_control_covers_churn_external_change_and_deferred_failure`; workspace backend suite |
| 6 | `browser_port_descriptors`, `ConfigureDeviceChannels`, and lifecycle-driven capture/destination inventory in `shoopdaloop_egui`/AudioWorklet | Chrome output-only, denied/retry, ended/retry, hosted/direct-file microphone, and Firefox topology diagnostics |
| 7 | Protocol-v3 `SetPortConnected`, browser journal, worklet route application, and worklet-confirmed snapshots | `normalized_routes_mutate_authoritatively_without_stopping_audio`; production disconnect-to-silence/reconnect-to-non-zero browser workflow |
| 8 | Explicit default route construction and exact desired/confirmed route publication | Backend/worklet mono/stereo route tests and fresh production browser confirmed-link evidence |
| 9 | `connection_model_version`, exact route capture/replacement, and legacy browser migration in `shoop_session`/`shoop_app` | `web_audio_session_replacement_preserves_user_route_changes_over_defaults`; session migration/round-trip tests; production browser save/load workflow |
| 10 | Browser `NullMidiService`; MIDI application ports remain in normalized inventory | Global-dialog browser diagnostics and `empty_host_inventory_keeps_application_ports_visible_and_safe` |
| 11 | Workspace pin `omnilua = 0.7.1`; migrated manifests/sources and pure-Rust dependency graph | Cargo metadata, lockfile, native/Wasm trees, Rust-source, archive, standalone-payload, and packaged-Wasm forbidden-runtime scans |
| 12 | Shared `shoop_scripting` omniLua control/sandbox/module implementation and unchanged embedded sources | 21 scripting tests covering all 61 functions, callbacks, timers, errors, keyboard/APC; native and production-browser execution |
| 13 | Retained `frontend` Lua engine, conversions, callback wrappers, session control, and MIDI bridges import omniLua directly | 33 frontend Rust tests and all retained Lua-specific QML testcases; source scan contains no former runtime imports |
| 14 | Cooperative browser `shoop_app` scripting owner and `NullMidiService`; browser snapshot publishes scripting support | Production Wasm check plus Chrome/Firefox hosted and Chrome direct-file source-bearing/bundled script workflows |
| 15 | `KEYBOARD_SCRIPT` embeds `src/lua/builtins/keyboard.lua` with `include_str!`; browser startup/settings enable it by default | Package markers and real focused Chrome key press/release driving authoritative selection |
| 16 | Unchanged embedded APC source, empty browser MIDI service, and published registration-owned logical ports | APC-on browser settings workflow reports a healthy script, two logical ports, and zero MIDI hosts |
| 17 | `script_connection_port_id`, `ApplicationPortOwner::LuaControl`, and owner-managed policy | `lua_control_ports_are_owner_managed_stable_and_visible_without_midi_hosts`; native APC confirmed-link policy tests |
| 18 | Cross-target bundled settings keys; target-gated native user paths/actions; browser `localStorage` adapter | Settings registry/UI tests and hosted/direct-file save/reload/failure/reconciliation workflows |
| 19 | `validate_session_scripts`, staged `replace_session_scripts`, exact source capture, and application transactional load | `session_scripts_stage_before_commit_round_trip_and_preserve_machine_scripts`; production browser active/exact-resave workflow |
| 20 | Lua is owned by the native actor/cooperative application runtime; render paths contain only bounded route processing | `assert_no_alloc` backend/worklet render tests, workspace realtime guards, and source ownership audit |
| 21 | Protocol capacities, bounded command/event journals, preallocated route/channel storage, generation checks, and visible rejection | Protocol saturation/supersession tests, worklet no-allocation tests, Chrome saturation/recovery and sustained-stress workflows |
| 22 | Trunk hosted bundle, `build_single_file_app.py`, and `package_artifacts.py` include UI/worklet Wasm, worklet JS, omniLua, libraries, and scripts | Debug/release builds; `unzip -t`; decoded standalone payload scans; direct-file keyboard/microphone/settings/session workflows |
| 23 | This plan, `EGUI_REPLACEMENT_PROJECT.md`, `EGUI_FEATURE_PARITY_MATRIX.md`, durable port/Lua/session/settings docs, and runner README | Final cross-document status/row scan, stale-limitation scan, formatting, and `git diff --check` |

Gate-level command evidence is the Stage 6 closure list immediately above: formatting/static checks; warning-denying native, Wasm UI, and Wasm worklet builds; the 1,415-test workspace command; retained QML self-tests with the isolated CPAL environment exception; production browser mode matrix; native MIDI environment probes; and final dependency/package scans. No acceptance criterion depends solely on a checkbox or aggregate green status.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
