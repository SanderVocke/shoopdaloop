# egui Cross-Target Ports, Browser Lua, and omniLua Migration Plan

## Status and document role

Status: **In progress**. Stage 0 native/Wasm compile gates and the workspace runtime migration are underway; browser execution and retained QML end-to-end gates remain open.

This is the implementation ledger for making application ports, host ports, connection management, and Lua scripting consistent in the egui product, including the production WebAssembly build and self-contained HTML artifact, while replacing `mlua` with omniLua throughout the entire workspace. It supersedes the earlier accepted browser-specific omissions recorded in `EGUI_FEATURE_PARITY_MATRIX.md`; it does not claim that those omissions or the runtime migration have already been completed.

This plan depends on and must be maintained with:

- `EGUI_REPLACEMENT_PROJECT.md` for project architecture and coarse status;
- `EGUI_FEATURE_PARITY_MATRIX.md` for capability-level status and evidence;
- `docs/egui_lua_compatibility_contract.md`, `docs/settings_format_v1.md`, `docs/session_format_v1.md`, and the egui runner README for delivered contracts and user-facing limits.

## Investigation findings and known technical risk

- The application/backend API already calls track ports local/application ports and opposite endpoints external ports, but the backend snapshot repeats endpoint candidates per local port rather than modeling host inventory explicitly.
- The native dummy backend exposes mutable virtual external endpoints. The retained JACK and CPAL/midir paths already have analogous client/app-port and system/device-endpoint concepts, although native real-driver composition is not yet part of the egui runner.
- The Web Audio path is the exception: every track owns physical `ExternalAudioPort`s, `process_audio_quantum` maps every track directly to the microphone/destination, the browser connection snapshot has no candidates and reports management unavailable, and the worklet protocol has no connection commands or confirmed route state.
- Direct MIDI track ports are already created in the browser backend, but a no-endpoint category currently does not visibly identify its local ports in the connection matrix.
- Lua is excluded with target `cfg`s from `shoop_app`, `shoopdaloop_egui`, settings registration, session loading, and key dispatch. `shoop_scripting` also depends unconditionally on native `midir`.
- Stock `mlua 0.11` with vendored Lua 5.4 is a concrete blocker for the existing `wasm32-unknown-unknown` product target: a minimal probe fails in `lua-src` with `don't know how to build Lua for wasm32-unknown-unknown`. Stock `mlua` documents Emscripten rather than this target, and changing the eframe/Trunk product to Emscripten is not an acceptable shortcut.
- A minimal `omnilua 0.7.1` Lua 5.4 probe compiles for `wasm32-unknown-unknown` without a C toolchain, and its embedding API is intentionally similar to `mlua`. The user has selected omniLua as the replacement runtime. It is young and not drop-in compatible, so the first stage must prove the complete Shoop compatibility surface and record required API adaptations before behavioral implementation proceeds.
- Replacement is workspace-wide, not egui-only. In addition to `shoop_scripting`, the retained `frontend` crate directly uses `mlua` across its Lua engine, QVariant/Qt conversion layer, stored callback wrappers, session control handler, weak runtime ownership, and QML bridge. Those paths and their retained tests must move to omniLua before the migration is complete.

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
- [ ] If omniLua cannot meet the immutable contract, stop with exact failing sources/tests, the missing omniLua capability, and the upstream or local adaptation required; do not select another runtime without explicit user approval.

Verification:

- [x] Native `shoop_scripting` (21 tests), frontend Rust tests (33 tests, including 11 Lua-engine tests), and all retained Lua-specific QML self-tests pass with omniLua.
- [ ] A Wasm test artifact constructs isolated omniLua states, runs the embedded keyboard script, receives key callbacks, and emits expected typed control operations.
- [x] `cargo tree -p shoop_scripting --target wasm32-unknown-unknown` contains omniLua but no former C Lua runtime, `midir`, ALSA/CoreMIDI/WinMM, or Emscripten dependency.

### Stage 1 — Normalize application-port/host-port contracts and ownership

- [ ] Refactor API/backend connection DTOs to publish separate typed application-port inventory, host-port inventory, and confirmed links with stable IDs, directions, data types, revisions, and failures.
- [ ] Replace mandatory `track_id` ownership with an explicit owner model supporting sync/main tracks and Lua control ports; retain stable mapping across backend replacement and script lifecycle.
- [ ] Derive immutable connection-view candidates, pending state, and compatibility from normalized truth in `shoop_app`; route mutation by owning service and reject stale/incompatible pairs deterministically.
- [ ] Update FakeBackend and EngineBackend dummy discovery/mutation contracts, endpoint churn, deferred confirmation, persistence capture, and rollback tests.
- [ ] Update connection-dialog grouping and empty-endpoint rendering so local/app columns remain visible. Keep track scope isolated and global scope owner-complete.
- [ ] Update preview fixtures for track, control, no-host, churn, pending, managed-policy, and error states without adding implementation dependencies to `shoop_egui`.
- [ ] Update both project planning documents with the implemented contract/evidence from this stage.

Verification:

- [ ] API structural-sharing/identity tests, fake/dummy backend contracts, application pending/error tests, GUI interaction/paint tests at 360×200 and 900×600, and preview Wasm check pass.
- [ ] Source/dependency scans confirm `shoop_egui` still has no backend, engine, scripting, driver, or platform API dependency.

### Stage 2 — Implement authoritative Web Audio host ports and mutable DSP routes

- [ ] Introduce bounded engine/worklet host-channel objects and separate virtual track ports; remove unconditional per-track microphone staging and destination mixing.
- [ ] Publish stable destination endpoints and lifecycle-driven microphone endpoints for every channel admitted by the Web Audio boundary.
- [ ] Extend the versioned worklet protocol with prepared set-connection commands and authoritative topology/confirmed-link publication; update size, ordering, journal replay, stale-generation, saturation, and malformed-message tests.
- [ ] Implement connect/disconnect routing for microphone-to-app-input and app-output-to-destination links, including mono fan-out/current N-channel mapping as explicit initial links.
- [ ] Preserve explicit desired links across suspend/resume and same-generation endpoint churn; clear physical confirmation on loss and restore only still-desired links after retry.
- [ ] Add explicit-route session capture/replacement and a versioned migration for pre-milestone browser sessions.
- [ ] Make Web Audio report connection management available independently of whether microphone permission has produced input endpoints.
- [ ] Update browser/audio/session contracts, runner README, package checks, parity rows, and project status for the delivered routing behavior.

Verification:

- [ ] Engine tests prove disconnected input records silence, connected input records the selected channel, disconnected outputs are silent, connected mono/stereo/many-track output follows the visible route set, and route changes allocate/lock nothing in processing.
- [ ] Protocol/worklet tests prove desired/confirmed ordering, endpoint churn, journal replay, malformed/stale rejection, queue bounds, and session route round-trip.
- [ ] Hosted Chrome and Firefox automation opens the real connection dialog, observes output/microphone endpoints, disconnects/reconnects exact cells, and proves corresponding non-zero/silent recording and output while callback progress continues.
- [ ] Output-only, denial/retry, track-end/retry, processor-loss/retry, saturation/recovery, stress, offline dummy, and direct-file artifact regressions pass.

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
- [ ] Wasm compiler/tests prove omniLua scripting support, all embedded source syntax, keyboard operations, callbacks/timers, script-local errors, zero-endpoint APC startup, and source-bearing session transactionality.
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
- [ ] Workspace manifest, Rust-source, metadata, lockfile, and resolved-tree scans contain no former runtime or C-runtime dependency; packaged-library scans remain open for Stages 5–6.

### Stage 5 — Publish control ports and complete browser settings/artifact UX

- [ ] Give each Lua MIDI input/output registration a stable script-owned application-port ID and lifecycle; remove it on script stop/restart/forget without leaking pending links.
- [ ] Merge script logical ports and host MIDI endpoint observations into the global connection view. Mark regex-autoconnect cells as policy-managed and preserve diagnostics/confirmed truth.
- [ ] Ensure direct MIDI track ports and logical control ports paint their names with zero Web MIDI endpoints; add/stop/restart scripts while the dialog is open to prove live churn.
- [ ] Split script settings registration into cross-target bundled toggles and native-only user-path definitions/actions. Show a functional browser Scripts tab without a dead file-path picker.
- [ ] Reconcile committed browser settings revisions into startup/runtime scripts; failed `localStorage` saves must not change running scripts.
- [ ] Ensure hosted and self-contained package generation embeds omniLua and all `include_str!` Lua sources, and rejects missing/stale assets, forbidden native dependencies, or any reintroduced `mlua` runtime.
- [ ] Update all planning documents, settings/session/Lua contracts, runner README, artifact descriptions, and browser limitation copy in the same milestone commit.

Verification:

- [ ] Application/GUI tests prove global versus track ownership, active control-port churn, policy-managed native links, zero-endpoint browser categories, settings Save/Cancel/failure behavior, and keyboard default enablement.
- [ ] Hosted and self-contained browser automation opens the Scripts tab, observes embedded keyboard/APC entries, drives an authoritative keyboard workflow, observes APC logical MIDI ports with zero candidates, reloads settings, and round-trips an enabled session script.
- [ ] Archive and standalone HTML inspections find omniLua runtime code, built-in source markers, no former C Lua runtime, and no checkout-relative or network fetch dependency for Lua.

### Stage 6 — Final end-to-end validation and documentation closure

- [ ] Run formatting and warning-denying native/Wasm builds for all changed crates and artifacts.
- [ ] Run focused API, backend, engine realtime, protocol, worklet, scripting, frontend/QML, application, presentation, runner, preview, session, settings, and package tests.
- [ ] Run `cargo test --workspace --features shoop_engine/app_backend` and the retained QML self-test suite.
- [ ] Build debug and release hosted and self-contained browser artifacts; run Chrome/Firefox normal, minimum-size, output-only, microphone, lifecycle, connection mutation, Lua keyboard, session-script, settings, stress, and direct-file workflows.
- [ ] Recheck native egui dummy connection and omniLua workflows, including native MIDI virtual-port evidence where the host supports it and a documented environment skip otherwise.
- [ ] Audit dependency trees and packaged files for target isolation, omniLua version/configuration consistency, absence of `mlua` and its C runtime, native MIDI leakage into Wasm, stale fixed-routing copy, and missing embedded scripts.
- [ ] Reconcile every planned matrix row and the project coarse status with exact test/artifact evidence; leave no plan document claiming completion without evidence.

Final evidence must demonstrate one production browser run in which microphone and destination channels appear as host ports, a user-visible connection changes actual audio flow, audio callbacks continue, omniLua-backed `keyboard.lua` controls authoritative state, MIDI track/control app ports remain visible with zero MIDI host endpoints, and the same behavior is present in the self-contained artifact subject only to documented browser audio policy. It must also demonstrate the retained QML Lua product on omniLua and a workspace-wide absence of `mlua` and its C runtime dependency chain.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
