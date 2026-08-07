# egui Lua Scripting and MIDI Controller Integration

## Status and relationship to the replacement project

**Status:** In progress

This milestone implements the `shoop_scripting` boundary described by `EGUI_REPLACEMENT_PROJECT.md` and expands the scripting, keyboard-control, and script-created MIDI-control rows in `EGUI_FEATURE_PARITY_MATRIX.md`. The QML application remains the compatibility oracle until this milestone is complete.

The parity target is the native desktop egui application. The existing browser product must continue to build and run, but browser Lua and Web MIDI are outside this milestone: `mlua` supports WebAssembly through `wasm32-unknown-emscripten`, while the product uses `wasm32-unknown-unknown`, and the browser composition currently has no Web MIDI service.

## Goals

- Run trusted bundled and user-provided Lua 5.4 scripts in the native egui application through `mlua`, without Qt, QML, CXX-Qt, or the `frontend` crate.
- Preserve the existing `shoop_control` methods, constants, selectors, callback payloads, built-in modules, and observable script lifecycle closely enough that existing user scripts continue to work unchanged.
- Make the existing `keyboard.lua` and `akai_apc_mini_mk1.lua` sources fully functional, including keyboard press/release behavior, timers, loop/global events, composition operations, controller LEDs/faders, MIDI input/output control ports, hotplug, and autoconnect.
- Route GUI, keyboard, Lua, and MIDI-controller operations through the same authoritative application policies and backend boundary.
- Provide native egui management for bundled, user, and session scripts, with persisted enablement and actionable runtime/MIDI status.

## Scope

Included:

- A frontend-independent `shoop_scripting` crate containing the Lua runtime, compatibility API binding, script lifecycle, callback/timer dispatch, and a target-neutral MIDI-control service contract.
- Every method and constant currently installed by the QML `SessionControlHandler`, plus `require` for the existing Lua libraries and the existing print functions.
- The missing application/backend behaviors required by that API and the bundled scripts, including explicit transitions, ringbuffer adoption parameters, repeat-sync, and composition append/create behavior.
- One isolated Lua state per script; start, stop, restart, enabled-at-startup, status, documentation extraction, and cleanup of callbacks, timers, and MIDI rules.
- Native physical MIDI endpoint discovery and connections through a non-Qt adapter, including endpoint hotplug/reconnect, anchored regular-expression matching, independent logical input/output ports, received-message delivery, bounded output queues, and actual output-rate limiting.
- Existing machine-wide `script_settings.1` compatibility, default-enabled `keyboard.lua`, user-script file selection, and activation of enabled source-bearing `ScriptDocument` entries in `.shoop` sessions after transactional load.
- Native egui script management and keyboard event forwarding.
- Shared use of the current files under `src/lua`; no fork of the bundled scripts is planned because this plan makes no breaking Lua API change.

Not included:

- The separate QML MIDI-rule learning/filter/action editor and its generic `midi_control_configuration.1` UI. Its persisted data remains preserved and capability-rejected until its own milestone.
- Browser Lua execution, Web MIDI, or changing the browser target/toolchain.
- Native audio-driver selection or the broader JACK/CPAL settings milestone. MIDI controller transport is an independent native service and must not require the dummy audio backend to become a real audio driver.
- A hardened hostile-code sandbox. Scripts are user-authorized local code; compatibility isolation and error containment are required, but filesystem/process security is not an acceptance gate.
- New breaking Lua names, argument shapes, selector rules, or callback payloads. Additive APIs may be proposed later, but are not required here.

## Immutable acceptance criteria

1. **Native standalone runtime.** The native `shoopdaloop_egui` product runs Lua 5.4 through `mlua` with no dependency on Qt, QML, CXX-Qt, or `frontend`; each script has an independently stoppable Lua state.
2. **Public API compatibility.** Every existing registered `shoop_control` function and constant is covered by a compatibility test for argument validation, return shape/order, coordinate selectors, sync coordinates `{-1, 0}`, mode/global semantics, gain/fader conversion, and error behavior. Existing `shoop_control`, `shoop_coords`, `shoop_helpers`, `shoop_format`, and `shoop_midi` imports work unchanged.
3. **One application control path.** Lua mutations use the same application reducers/policies and backend operations as equivalent GUI intents. Lua must not hold widget, QObject, engine-session, or raw backend references. Calls made during one Lua invocation have deterministic ordering and read-your-writes behavior where the old synchronous API provides it.
4. **Complete bundled scripts.** The unchanged `keyboard.lua` and `akai_apc_mini_mk1.lua` files run successfully. Automated workflows cover every documented keyboard command and release-sensitive sampler behavior, plus APC grid actions, selection/targeting, record/grab/stop/dry/composition modes, global controls, faders/mutes, LED reset/update, timer use, and reconnect.
5. **MIDI control ports and autoconnect.** A script can create logical control inputs and outputs, discover compatible native endpoints, match full endpoint names with anchored regexes, connect all matches, reconnect after hotplug, receive exact MIDI bytes in order, and send to connected outputs. Output `msg_rate_limit_hz` is enforced as a real maximum rather than merely enabling a timer. Invalid regexes, endpoint failures, queue overflow, oversized messages, and send failures are visible without crashing the script host.
6. **Events and timers.** Loop events contain coordinates, type, mode, length, selected, and targeted state; global events and non-repeat keyboard press/release events retain their existing payloads/constants. One-shot timers use a monotonic clock, callbacks execute only on the control side, callback order is deterministic, and stopping a script cancels all of its subscriptions, timers, ports, and queued output.
7. **Lifecycle and settings.** On first run, both bundled scripts are discoverable and only `keyboard.lua` is enabled by default. Users can add, enable/disable, restart, stop, forget, and inspect documentation/status for scripts. Existing valid `script_settings.1` data is read without losing unrelated settings; malformed or unsupported settings are reported and not overwritten.
8. **Session behavior.** Enabled embedded session scripts round-trip source/name/identity through `.shoop`, are syntax-checked before session replacement, and start only after backend commit. A failed/cancelled load starts no staged script and leaves prior scripts/session active. Runtime failures after commit are isolated and reported as script errors.
9. **Realtime and boundedness.** Lua evaluation, callbacks, MIDI discovery, regex matching, logging, and output throttling never execute in an audio callback or AudioWorklet `process()`. Cross-thread/worklet messages and MIDI queues remain bounded with observable drops/backpressure; a slow or failing script cannot corrupt the application or stop other scripts, though trusted scripts may block their own control-side execution.
10. **Presentation and keyboard safety.** `shoop_egui` remains backend/Lua/filesystem-free and only emits typed script-management and key-event intents. Normal controls retain focus behavior; performance shortcuts are not fired while editing text, auto-repeat is ignored as in QML, and key releases needed to end sampler mode are not lost on focus changes.
11. **Browser preservation.** All existing `wasm32-unknown-unknown` application, worklet, packaging, and browser workflows continue to pass without linking `mlua` or native MIDI. Script-bearing sessions remain explicitly capability-rejected in the browser rather than silently dropping or pretending to run scripts.
12. **Regression and documentation.** Existing native/browser tracks, loops, connections, persistence, realtime guards, and retained QML tests remain green. User/developer documentation describes the egui script manager, compatibility API, trusted-code model, MIDI matching/rate behavior, diagnostics, session versus machine-wide scripts, and browser limitation.

## Design rules and constraints

- Put Lua ownership, bindings, callbacks, timers, and MIDI-controller orchestration in `shoop_scripting`; keep `shoop_egui` presentation-only and `shoop_app_api` free of `mlua`, drivers, and filesystem types.
- Construct the non-`Send` Lua states on the native application actor thread rather than enabling `mlua`'s workspace-wide `send` feature. Refactor actor startup to create the model/runtime inside its owning thread and report initialization success through a startup handshake.
- Bind Lua to a per-invocation control view: queries read an application-owned compatibility snapshot, mutators update its shadow state and append typed ordered operations, and the application applies the completed batch through shared reducers. Do not use reentrant model borrows, synchronous actor round-trips, or unsafe back-pointers.
- Derive coordinate selectors at the compatibility boundary from stable application IDs and current track/row order. Stable IDs remain authoritative internally; positional coordinates remain only the legacy Lua representation.
- Produce loop/global events from committed application changes, not GUI repaint diffs. Queue events until the current Lua callback returns so callbacks are never recursively re-entered.
- Keep one Lua state per script so dropping it is the authoritative teardown. Store callbacks/timers/MIDI rules under the owning script ID; do not infer liveness from weak Lua pointers alone.
- Reuse the existing Lua source files as the single source of truth and embed bundled libraries/scripts in the native product for reliable packaging. The QML adapter may reuse the extracted generic runtime, but the new application must not depend on the frontend adapter.
- Abstract MIDI device access behind a fakeable control-port service. The native adapter may use `midir` directly and may create platform-visible virtual ports where supported; script-facing input/output direction and callback semantics must remain independent of host naming details.
- Poll or subscribe for endpoint changes off the realtime path. Compile each non-empty anchored regex once per rule revision. Output to multiple matches in deterministic endpoint order and retain bounded FIFO ordering through throttling.
- Treat scripts as trusted local extensions. Preserve the compatibility environment and restricted Shoop `require` behavior where practical, but do not delay the milestone for adversarial sandbox hardening. Syntax/runtime errors and panics at Rust boundaries must still be contained and observable.
- Preserve `script_settings.1` and `.shoop` v1 compatibility. Machine-wide path-based scripts and source-bearing session scripts are distinct; never write absolute machine paths into a session archive.

## Staged implementation plan

Dependencies are ordered: freeze compatibility first, establish the runtime and common control path, close application capability gaps, then add events/MIDI, persistence, presentation, and full-script evidence.

### Stage 1 — Freeze the Lua, lifecycle, keyboard, and MIDI compatibility contract

- [x] Expand the parity matrix into independently testable rows for runtime/module loading, every control API family, selectors/constants, script lifecycle, events/timers, keyboard routing, MIDI port lifecycle, autoconnect/hotplug, send throttling, settings, session scripts, diagnostics, and target support.
- [x] Convert the existing QML control-handler tests and generated docstrings into a framework-independent compatibility table, recording exact argument/return/event semantics and any current defects that should be fixed rather than preserved.
- [x] Record the bundled scripts' complete API/capability call graph; explicitly map composition, ringbuffer, direct-transition, repeat-sync, and MIDI operations to application/backend work.
- [x] Add a minimal native `mlua` actor-thread probe and confirm that `shoop_scripting` can remain excluded from the `wasm32-unknown-unknown` dependency graph without enabling `mlua/send` globally.

Verification:

- [x] Every installed legacy method/constant and every bundled-script call has a matrix row and an owning implementation stage.
- [x] The native probe executes a callback on the actor thread; browser dependency scans contain no Lua/native-MIDI packages.

### Stage 2 — Create the frontend-independent Lua runtime and script lifecycle

- [x] Add `shoop_scripting` with per-script Lua states, bundled-source/module loading, print/log bindings, compatibility execution environment, source naming, syntax compilation, execution, status/error records, and deterministic teardown.
- [x] Move or extract the generic `mlua` engine behavior from the frontend so runtime/module semantics have one tested implementation; retain a narrow QML adapter until QML retirement.
- [x] Add script IDs and immutable status/error summaries plus typed start/stop/restart/source-loaded commands to `shoop_app_api`, without exposing Lua values or file paths as runtime handles.
- [x] Refactor native application startup so all non-`Send` Lua state is constructed and destroyed on the application actor thread; retain cooperative/browser startup without scripting.

Verification:

- [x] Runtime tests cover isolation, bundled `require`, prints, syntax/runtime errors, restart, teardown, same-name scripts, and one failing script not affecting another.
- [x] Native warning-denying builds pass; `shoop_egui` and browser dependency trees remain Lua-free.

### Stage 3 — Implement the complete script control/query reducer

- [x] Define framework-independent script query snapshots and ordered control operations for all loop, track, and global API methods; implement selector parsing/conversion once in `shoop_scripting`.
- [ ] Refactor equivalent GUI and script commands onto shared application reducers, including deterministic target/selection ordering, solo/sync/fixed-cycle policy, gain/fader conversion, and read-your-writes shadow updates.
- [ ] Extend the backend façade and engine mapping for API gaps: explicit cycle/alignment transitions, parameterized ringbuffer adoption, repeat-sync, and regular composition creation/append/parallel updates. Add browser proxy variants only where required to keep shared session/backend contracts coherent; browser script invocation remains unavailable.
- [x] Publish enough authoritative loop/track state for all legacy queries without asking widgets or copying media content.

Verification:

- [ ] The framework-independent port of the complete QML API test table passes against the application model and both Fake/engine backends.
- [ ] GUI and Lua forms of each shared action produce equivalent model/backend observations, including failure and stale-selector cases.
- [ ] Engine tests prove explicit transition timing, repeat-sync, ringbuffer adoption, and composition execution without realtime allocation/locking regressions.

### Stage 4 — Add application events, timers, and keyboard delivery

- [x] Add a committed application event stream with granular loop/global payloads and deterministic ordering; feed script subscriptions only after the originating control batch completes.
- [x] Implement monotonic one-shot timers in the script coordinator with script-owned cancellation and bounded callbacks per pump.
- [ ] Add target-neutral key/modifier/event values and a Qt-compatible constant mapping for legacy scripts; translate egui press/release input, suppress repeats, handle focus loss releases, and avoid firing shortcuts during text entry.
- [x] Keep script callback errors/status observable while allowing subsequent callbacks and other scripts to continue.

Verification:

- [ ] Tests cover all event kinds and payload fields, no duplicate/reentrant callbacks, timer ordering/cancellation, key constants/modifiers, repeat suppression, text-edit suppression, and focus-loss sampler release.
- [ ] `keyboard.lua` passes an automated command-by-command workflow using the real bundled source.

### Stage 5 — Implement native MIDI control ports, hotplug, and autoconnect

- [ ] Add a target-neutral MIDI-control service contract and deterministic fake covering endpoint identity/name/direction, discovery revisions, logical port open/close, input delivery, output submission, and diagnostics.
- [ ] Add the native adapter using supported platform MIDI facilities, with bounded callback-to-control queues and no dependency on GUI or audio callback timing.
- [ ] Implement script-owned auto-open input/output rules: compile full-name anchored regexes, open the logical port, connect every compatible match in stable order, emit opened/connected callbacks with the existing Lua port table, and reconnect on endpoint reappearance.
- [ ] Implement exact-byte input callbacks and bounded output broadcast with a monotonic per-rule rate limiter that preserves FIFO order; expose drop/refusal/send/connect diagnostics and cleanly close all connections on script stop/restart.
- [ ] Add platform-gated virtual-port integration tests where the host supports them, while retaining the fake service as the cross-platform authoritative contract.

Verification:

- [ ] Fake-service tests cover direction/type filtering, empty/invalid/partial/full regexes, multiple matches, hotplug, duplicate discovery, reconnect, failure, cleanup, queue saturation, SysEx/message limits, and exact rate/order behavior.
- [ ] Native virtual MIDI evidence sends real messages into a script and receives script output where CI/platform facilities exist; unavailable host facilities are explicit skips, not fake successes.

### Stage 6 — Integrate settings, bundled resources, and session scripts

- [ ] Extend `shoop_settings` with typed preservation-aware access to existing `script_settings.1`; discover embedded bundled scripts, default only `keyboard.lua` to enabled, and preserve unrelated/unknown service-owned settings fields.
- [ ] Add composition-root file adapters for adding/reloading user scripts and atomically persisting enablement, following the existing bytes/intent boundary rather than putting filesystem access in `shoop_egui`.
- [ ] Map source-bearing `.shoop` `ScriptDocument` entries into staged syntax-checked runtimes and activate them after successful session commit; stop replaced session scripts only at commit and preserve machine-wide scripts.
- [ ] Ensure save captures session-script source/identity/enabled state but never embeds path-based machine-wide scripts implicitly.
- [ ] Package the shared Lua sources in native debug/release artifacts and keep development overrides deterministic.

Verification:

- [ ] Settings tests cover first run, old valid files, malformed/unknown schema, atomic update, missing user files, relative bundled names, and preservation of MIDI/Carla/unknown fields.
- [ ] Session tests cover exact source round-trip, syntax rejection before mutation, post-commit activation, cancellation/rollback, machine/session lifecycle separation, and browser capability rejection.
- [ ] Packaged native artifacts start `keyboard.lua` without relying on the source checkout.

### Stage 7 — Deliver the egui script-management surface

- [ ] Add a main-menu entry and script manager showing name, bundled/user/session kind, enabled state, lifecycle status, latest error, callback/timer/MIDI listening state, and MIDI diagnostics.
- [ ] Add enable/disable, restart, stop, forget-user-script, add-user-script, and bundled-docstring help actions with stable script IDs and stale-result validation.
- [ ] Route raw performance key events from the eframe application through typed intents independently of widget paint, while preserving text-edit and focus rules.
- [ ] Present native MIDI unavailable/permission/open/connect/regex/overflow/send failures distinctly; on browser builds keep scripting absent or clearly unsupported rather than rendering controls that cannot work.

Verification:

- [ ] Backend-free egui tests cover manager actions, statuses/errors, stale IDs, help, unavailable MIDI, minimum/common viewports, and keyboard routing/focus behavior.
- [ ] Native composition tests cover first-run keyboard startup, adding/restarting/stopping a user script, settings persistence, and complete resource cleanup at application shutdown.

### Stage 8 — Prove the unchanged bundled controller script end to end

- [ ] Run the unchanged APC script against a deterministic 8×8 fake controller and authoritative application/engine session.
- [ ] Cover initial connection and delayed LED reset, every loop-button modifier family, sync-loop coordinates, global momentary/permanent toggles, stop/select/clear-all, N-cycle selection, track gain/balance/mutes, loop/global event-driven LED updates, composition append/parallel behavior, output throttling, disconnect/reconnect, and stop cleanup.
- [ ] Add a smaller platform-gated real virtual-MIDI smoke using the same script and MIDI adapter where host support exists.
- [ ] Compare unchanged-script observations with retained QML behavior and document only well-supported defect fixes, such as correctly enforcing the requested output rate.

Verification:

- [ ] Both bundled scripts execute from their production embedded sources with no compatibility shims in copied script files.
- [ ] Fake-controller expectations prove input actions changed authoritative application/backend state and exact output MIDI bytes were emitted in bounded order.

### Stage 9 — Final validation and project-ledger update

- [ ] Run formatting, warning-denying native/all-target builds, focused runtime/API/application/backend/GUI/settings/runner tests, realtime guards, browser compiler/package/workflow checks, and the retained QML self-test.
- [ ] Inspect native, browser, GUI-preview, and worklet dependency trees and packaged artifacts for boundary/resource violations.
- [ ] Update `EGUI_FEATURE_PARITY_MATRIX.md` with discovered rows and concrete evidence; update `EGUI_REPLACEMENT_PROJECT.md` architecture/status/roadmap and `shoopdaloop_egui` documentation.
- [ ] Update Lua developer/API and keyboard/MIDI-controller documentation, including compatibility, lifecycle, trusted-code model, autoconnect matching, rate limiting, diagnostics, settings/session ownership, and browser limitation.

Final gates:

- [ ] `cargo fmt --all --check`
- [ ] `RUSTFLAGS="-D warnings" cargo build --workspace --all-targets --features shoop_engine/app_backend`
- [ ] Focused tests for `shoop_scripting`, `shoop_app_api`, `shoop_app`, `shoop_backend`, `shoop_engine`, `shoop_settings`, `shoop_egui`, and `shoopdaloop_egui`
- [ ] `cargo test --workspace --features shoop_engine/app_backend` using the documented unavailable-device policy where necessary
- [ ] Existing `wasm32-unknown-unknown` UI/worklet checks and hosted/self-contained browser workflows
- [ ] Native packaged-resource startup plus fake-controller end-to-end workflows on supported desktop CI targets
- [ ] Retained QML self-test and source/dependency scans proving no Qt/frontend dependency entered the egui scripting path

## Execution contract

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
