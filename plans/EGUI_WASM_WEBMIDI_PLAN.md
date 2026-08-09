# egui Wasm Web MIDI Recording and Control Plan

## Status and document role

Status: **Complete**.

This is the implementation contract for adding browser Web MIDI to the production Wasm driver. It extends the completed browser port/Lua architecture so physical browser MIDI endpoints can serve both direct-track MIDI recording/playback/monitoring and Lua-created MIDI control ports, matching the functional paths already available through dummy, JACK, and CPAL+midir drivers within the timing and permission limits of Web MIDI.

This plan depends on the normalized port model and AudioWorklet protocol delivered by `EGUI_WEB_PORTS_AND_WASM_LUA_PLAN.md`. That completed plan remains a historical ledger; its frozen no-Web-MIDI scope must not be rewritten. Current capability and project status must instead be kept synchronized in:

- `plans/EGUI_FEATURE_PARITY_MATRIX.md`;
- `plans/EGUI_REPLACEMENT_PROJECT.md`.

## Investigation findings

- Browser direct tracks already create MIDI input/output application ports and MIDI loop channels. The hosted worklet currently uses `DummyMidiPort` for those ports but supplies no physical MIDI events or endpoints.
- `WebAudioBackend` and protocol v3 already carry normalized application ports, host ports, confirmed links, and `SetPortConnected`; only `webaudio:capture_N` and `webaudio:destination_N` host endpoints exist today.
- The AudioWorklet owns the engine and connection truth. Web MIDI APIs are available only on the browser main thread, so physical access must be main-thread-owned while bounded MIDI event transport and track routing cross the existing worklet boundary.
- Browser scripting currently receives `NullMidiService`. `CooperativeApplicationRuntime` already supports an injected `MidiControlService` on native builds, and the scripting service contract already covers endpoint discovery, logical input/output connections, bounded input draining, output sending, hotplug, and diagnostics.
- One browser MIDI access owner is required. A Web MIDI input exposes one event handler, so track routes and any number of script-control subscriptions must fan out from a shared hub instead of competing for the physical port.
- Web MIDI supplies stable opaque `MIDIPort.id` values and asynchronous permission/open/hotplug state. Names are display data, not identity. Browser MIDI inputs are host sources/application-facing outputs; browser MIDI outputs are host sinks/application-facing inputs.
- Track MIDI storage accepts nonempty messages up to four bytes. Script control accepts messages up to 256 bytes. Existing limits must remain explicit; invalid, oversized, saturated, disconnected, and permission-gated messages must be dropped or rejected visibly, never truncated.
- Like CPAL+midir, Web MIDI cannot provide sample-exact input timing against the audio clock. Pending input can be staged at frame zero of the next worklet quantum, and output can preserve event order while being delivered with documented browser/main-thread latency.
- Current browser automation has no Web MIDI fixture. Deterministic end-to-end evidence will require a browser-startup Web MIDI test double that exercises the production `navigator.requestMIDIAccess` adapter boundary without adding a fixture backend to product composition.

## Goals

- Discover and publish real browser MIDI inputs and outputs as stable normalized host ports in the production Wasm application.
- Route Web MIDI input into connected direct-track MIDI ports so loops can record, monitor, grab, save/load, and replay MIDI through connected browser outputs.
- Replace the Wasm null MIDI control service with a shared Web MIDI service so bundled and session Lua scripts can autoconnect, receive, and send physical controller messages.
- Keep track connections user-managed and script control connections owner-managed while showing one coherent host inventory and authoritative confirmed links.
- Preserve browser audio continuity, realtime safety, bounded transport, native/QML behavior, target dependency isolation, and existing session/settings/artifact contracts.

## Scope

Included:

- the `wasm32-unknown-unknown` production composition in `shoopdaloop_egui`;
- direct `web-sys` Web MIDI access, permission/status, endpoint discovery, hotplug, input callbacks, and output sends;
- shared browser MIDI hub/adapters for `WebAudioBackend` and `shoop_scripting::MidiControlService`;
- bounded worklet protocol and engine-backend transport for track MIDI input/output;
- normalized connection inventory, mutation, confirmation, reconnection, diagnostics, and existing route persistence;
- direct-track MIDI recording, monitoring, grabbing, activity state, playback, and session/media round trips through Web MIDI;
- Lua control input/output, regex autoconnect, rate limiting, APC Mini behavior, and audio-independent control readiness;
- deterministic browser automation, packaging checks, durable documentation, and synchronized project plans.

Not included:

- a generic MIDI learn/rule editor;
- changing the engine's four-byte recorded-message limit or scripting's 256-byte control-message limit;
- sample-exact Web MIDI timing, MIDI clock synchronization, timestamp compensation, or a browser MIDI sequencer;
- changing native dummy, JACK, CPAL+midir, retained QML, or session-format semantics except shared regression-safe abstractions needed by this implementation;
- claiming Web MIDI support in browsers/origins that do not expose or permit the API;
- silently enabling SysEx or weakening browser permission requirements.

## Immutable acceptance criteria

These criteria may not be weakened or reinterpreted without explicit user approval.

1. A supported secure browser exposes an explicit Web MIDI enable/retry flow. Unsupported API, pending permission, denial, success, hotplug, port-open failure, and output-send failure remain distinguishable and user-visible; no permission request occurs silently at page load.
2. Every currently connected Web MIDI input and output is published exactly once with a stable direction-qualified identity derived from `MIDIPort.id`, its current display name, MIDI data type, and the correct host direction. Names, map order, and reconnect order are never used as identity.
3. Direct-track MIDI connection cells are user-manageable. Connect/disconnect affects actual event flow, pending state remains separate from confirmed state, and only authoritative backend/worklet observations confirm track links.
4. A message from a connected Web MIDI input reaches every connected direct-track MIDI input exactly once, is staged no later than the next available worklet quantum, and can be monitored, recorded, grabbed, saved, loaded, and replayed with byte/order fidelity allowed by the existing four-byte engine contract.
5. MIDI emitted by a direct track is delivered exactly once to every connected Web MIDI output, with stable equal-time ordering and without requiring an audio route. Disconnect, endpoint disappearance, mute/monitor policy, worklet restart, and session replacement cannot create stale or duplicate delivery.
6. Web MIDI track timing is documented as coarse rather than sample-exact: pending input is assigned to frame zero of the next processed quantum, and output preserves engine order but may incur worklet/main-thread/browser scheduling latency.
7. Wasm Lua scripts use the same `MidiControlService` semantics as native scripts: endpoint regex matching, input callbacks, output `send`, positive-rate pacing, `0` unthrottled mode, multi-endpoint fanout, hotplug reconnect, teardown, and per-rule diagnostics work with real Web MIDI endpoints. The unchanged APC Mini script completes a physical-I/O-equivalent workflow against deterministic browser endpoints.
8. MIDI control availability is independent of microphone permission and AudioContext startup. Track MIDI remains safely idle until its worklet clock exists, while keyboard Lua and Web MIDI control can operate without enabling browser audio.
9. Track and Lua-control views share one physical endpoint inventory without duplicate rows. Track links remain `UserManaged`; script links remain `OwnerManaged` and reflect the script's actual logical subscriptions without allowing the matrix to compete with regex policy.
10. Input fanout, worklet commands/events, per-port queues, and output batches have explicit finite capacities. Empty/oversized messages, queue saturation, stale generations, malformed events, and device loss are refused or dropped with counters/diagnostics; payloads are never truncated and render processing does not allocate, lock, await JavaScript, serialize, or call Web APIs.
11. Messages accepted by the existing scripting limit, including SysEx up to 256 bytes where the user grants the browser's required permission, are preserved exactly. Permission unavailable or denied and larger messages are reported explicitly. The four-byte engine recording limit remains unchanged and visible.
12. Web MIDI endpoint IDs and desired track routes round-trip through existing session persistence. Missing endpoints remain desired-but-unconfirmed as allowed by the current connection contract and reconnect when the same stable endpoint returns; legacy no-Web-MIDI browser sessions continue to load deterministically.
13. Hosted production Chrome/Chromium automation proves endpoint discovery, connection mutation, direct-track input record/playback output, control input/output, hotplug, denial/retry, bounded overflow recovery, worklet restart, session round trip, and continuing audio callbacks. Other browser/origin runs either pass the supported workflow or expose a documented unsupported/denied state without regressing the application.
14. Native warning-denying builds, workspace tests, realtime guards, native MIDI behavior, retained QML tests, browser audio/Lua/settings/session workflows, and hosted/self-contained packaging remain green. Wasm dependency and package scans contain no native MIDI/system backend leakage or alternate production fixture path.
15. `plans/EGUI_FEATURE_PARITY_MATRIX.md` and `plans/EGUI_REPLACEMENT_PROJECT.md` are updated in every stage that changes discovery, architecture, status, limitations, or evidence. Current README/usage/port/browser documentation no longer claims that browser Lua, Scripts UI, or Web MIDI is absent once the corresponding behavior is delivered.

## Design rules and constraints

- Own `MIDIAccess`, physical `MIDIPort` objects, callbacks, and permission lifecycle on the browser main thread. The AudioWorklet must never access browser globals or await a promise.
- Use one shared browser MIDI hub with separate track-driver and scripting adapters. Open each physical endpoint centrally and fan out through bounded logical subscriptions/routes.
- Use direct `web-sys` bindings rather than the `midir` Web backend: Shoop must own explicit permission timing, SysEx choice, stable endpoint lifecycle, diagnostics, queue bounds, and shared track/control fanout.
- Keep browser bindings in the Wasm composition/platform layer and target-neutral routing/queue state independently testable. Do not add Web APIs, scripting, or backend dependencies to `shoop_egui`.
- Derive host IDs from endpoint direction plus opaque browser ID through one shared canonical helper. Presentation names and script regexes use current complete display names only.
- Keep physical endpoint inventory authoritative on the main thread, track route truth authoritative in the worklet, and script subscription truth authoritative in `ScriptManager`. Merge these truths into one normalized read model without inventing confirmation.
- Journal endpoint configuration and desired track routes in generation-safe order so worklet startup/restart cannot apply a route before its endpoint exists or replay ephemeral MIDI messages.
- Never journal live MIDI events. Tag bounded input/output batches with worklet generation and reject stale delivery after restart or session replacement.
- Reuse driver-style `ExternalMidiPort` staging/output semantics for physical-mode track ports rather than expanding test-only dummy queue behavior into a browser driver contract.
- Preserve current message limits. Validate before queueing, preallocate render-side storage, cap per-quantum/per-poll work, count every refusal/drop, and prevent an output flood from starving audio or application commands.
- Do not claim sample-exact timestamps. Preserve byte and stable ordering contracts and document the same next-cycle timing class as CPAL+midir.
- Keep Web MIDI enablement independent of Web Audio enablement. Permission and SysEx copy must be explicit, and unsupported browsers/direct-file origins must remain usable for non-MIDI workflows.
- Browser test doubles may replace the platform API before application startup, but production composition must still execute the same Web MIDI adapter and must not contain a query-selected fake MIDI backend.

## Staged implementation plan

Dependencies are ordered: Stage 0 freezes the browser and bounded-transport contract; Stage 1 establishes the shared main-thread service used by both consumers; Stage 2 supplies worklet/engine track transport; Stage 3 integrates normalized routing, persistence, and presentation; Stage 4 proves complete browser workflows; Stage 5 closes all regression and documentation gates.

### Stage 0 — Freeze Web MIDI platform, timing, identity, and capacity contracts

- [x] Inventory the exact `web-sys` Web MIDI surfaces and browser policies used for access, SysEx permission, port maps/state changes, input data/timestamps, open/close, and output send; record supported and graceful-unsupported browser/origin behavior.
- [x] Define canonical endpoint IDs, display-name construction, direction conversion, permission/lifecycle states, next-quantum input timing, output ordering, generation handling, and reconnect semantics.
- [x] Set and document finite hub subscription, input queue, protocol batch, per-quantum, and pending-output capacities plus counters and failure behavior, consistent with the existing 4-byte track and 256-byte control limits.
- [x] Add a target-neutral fake platform/hub test seam and prove that browser automation can install deterministic `requestMIDIAccess` inputs/outputs before loading the unchanged production composition.
- [x] Add planned Web MIDI rows/status to `EGUI_FEATURE_PARITY_MATRIX.md` and mark this milestone planned/in progress in `EGUI_REPLACEMENT_PROJECT.md` without rewriting historical completed-plan criteria.

Verification:

- [x] Native unit tests cover identity, direction, lifecycle transitions, fanout, queue boundaries, message validation, and hotplug using the target-neutral core.
- [x] A warning-denying Wasm probe compiles the selected direct `web-sys` API, and a hosted Chrome probe reaches deterministic granted/denied access through the production adapter seam.

### Stage 1 — Implement the shared main-thread Web MIDI hub and scripting service

- [x] Add one Wasm-owned Web MIDI hub that performs explicit asynchronous access, maintains stable endpoint objects/state, installs one input handler per physical source, opens/closes outputs, handles state changes, and publishes revisioned snapshots and diagnostics.
- [x] Implement bounded logical input subscriptions and output handles so physical input fans out safely and shared output use preserves send order without duplicate browser handlers.
- [x] Implement a Wasm `MidiControlService` adapter over the hub, including asynchronous-open failure propagation, exact input draining, drop accounting, output sending, teardown, and hotplug recovery.
- [x] Make cooperative runtime MIDI-service injection target-neutral and construct browser scripting with the hub adapter instead of `NullMidiService`; preserve startup scripts and settings reconciliation.
- [x] Add an explicit Web MIDI enable/retry/status surface in the composition root, independent of audio controls, with clear unsupported/denied/SysEx state and no Web API dependency in `shoop_egui`.
- [x] Update the parity matrix and project document with the delivered service boundary and current evidence.

Verification:

- [x] Focused hub, scripting, application, and composition tests prove regex matching, exact receive/send, rate limiting, multi-match fanout, stop/restart cleanup, hotplug, permission/open/send failures, bounded drops, and audio-independent APC startup.
- [x] Hosted Chrome automation enables deterministic Web MIDI, turns on the unchanged APC script, injects controller input that changes authoritative application state, and observes exact/rate-limited outbound controller bytes before Web Audio is enabled.

### Stage 2 — Add bounded direct-track MIDI transport to the worklet driver

- [x] Bump the versioned audio protocol and add bounded endpoint-configuration, host-input batch, and host-output batch contracts with stable endpoint/application IDs, frame offsets/order, generation validation, validation errors, and drop counters.
- [x] Refactor physical-mode track MIDI ports to driver-style external MIDI staging/output and add backend APIs that route one host input to all confirmed app inputs, collect each cycle's app output, and fan it out to confirmed host sinks using preallocated bounded storage.
- [x] Extend `WorkletHost` and the worklet shim/control path so main-thread input is staged outside render, render consumes/emits without allocation or locks, and bounded output is drained outside render for main-thread Web MIDI sends.
- [x] Have the browser backend journal endpoint inventory before route mutations, submit live input only ephemerally, drain/send output with a bounded per-tick budget, reject stale generations, and recover coherently after processor loss/retry.
- [x] Publish MIDI host endpoints, confirmed links, activity, refusals, and overflow diagnostics through snapshots without disturbing Web Audio host endpoints/routes.
- [x] Update the parity matrix and project architecture/status for functional worklet MIDI transport.

Verification:

- [x] Protocol tests cover round trips, byte limits, batch limits, ordering, malformed/stale input, journal rules, saturation, and protocol-version rejection.
- [x] Engine/worklet tests prove connected input monitoring and recording, disconnected silence, one-to-many input fanout, loop playback to one/many outputs, mute behavior, equal-time ordering, hotplug route loss/restore, and no duplicate delivery.
- [x] Realtime allocation/lock guards pass for MIDI-active and saturated render quantums; output overflow is counted and audio callback progress continues.

### Stage 3 — Integrate normalized routing, persistence, lifecycle, and user-facing diagnostics

- [x] Feed the hub's canonical inventory into `WebAudioBackend` before and after AudioWorklet startup, deduplicate it with script-observed endpoints, and preserve correct `UserManaged` track versus `OwnerManaged` control policy.
- [x] Complete track connect/disconnect/pending/failure behavior across permission delay, endpoint disappearance, worklet restart, and output-only/microphone audio modes; confirmations must continue to come from worklet route truth.
- [x] Preserve desired Web MIDI route IDs through session capture/load, explicit disconnect, missing-endpoint load, sample-rate replacement, and worklet generation replacement without changing the session schema unnecessarily.
- [x] Surface permission, endpoint, queue-drop, oversized-message, stale-generation, and send/open errors through bounded existing status/notification/script diagnostic models.
- [x] Update `docs/egui_port_model.md`, browser runner documentation, root/user MIDI documentation, package/browser support copy, and every stale no-Web-MIDI statement affected by delivered behavior.
- [x] Reconcile `EGUI_FEATURE_PARITY_MATRIX.md` and `EGUI_REPLACEMENT_PROJECT.md` in the same commit with exact focused evidence and remaining browser limitations.

Verification:

- [x] Application/GUI tests prove one host row per physical endpoint, compatible cells, exact track intents, owner-managed control cells, pending versus confirmed truth, hotplug, errors, and global/track scopes.
- [x] Session/application/worklet tests prove exact route and recorded-MIDI round trips, explicit no-link persistence, legacy no-Web-MIDI loading, missing endpoint behavior, and stable reconnection.
- [x] Documentation and source scans find no current claim that the browser Scripts tab, browser Lua control, or implemented Web MIDI support is absent.

### Stage 4 — Prove production browser Web MIDI workflows and failure recovery

- [x] Extend Chrome/Chromium automation with a deterministic Web MIDI platform double that grants/denies access, hotplugs stable inputs/outputs, injects timestamped bytes, records sends, and exposes no product-only fixture backend.
- [x] Run a production hosted workflow that enables Web MIDI and audio, creates a MIDI track, connects exact endpoints in the real connection dialog, records injected note traffic, stops, plays, and observes exact ordered output while callbacks continue.
- [x] In the same production composition, enable APC, prove owner-managed input/output autoconnect and authoritative controller actions/LED output, and verify track and control consumers can share one physical endpoint without lost or duplicate events.
- [x] Cover disconnect/reconnect, endpoint removal/reappearance, permission denial/retry, malformed/oversized input, queue saturation/recovery, processor loss/retry, output-only audio, session save/load, and clean teardown.
- [x] Exercise self-contained/direct-file behavior where the browser permits it and assert a truthful unsupported/denied state otherwise; run Firefox/current secondary-browser regression with no unsupported support claim.
- [x] Update both required planning documents with browser evidence, environment skips, and supported-browser/origin limits.

Verification:

- [x] Hosted production Chrome/Chromium passes the complete track-record/playback plus Lua-control workflow with nonzero continuing audio callback counts and zero unexpected exceptions.
- [x] Unsupported/denied browser modes retain keyboard, Lua, session, settings, connection, output-only, and offline workflows without invented endpoints or fatal driver state.

### Stage 5 — Final end-to-end validation and closure

- [x] Run `cargo fmt --all -- --check`, `git diff --check`, and warning-denying native, production Wasm UI, and AudioWorklet builds.
- [x] Run focused hub/protocol/backend/worklet/scripting/application/presentation/composition/session/settings/package tests, then `cargo test --workspace --features shoop_engine/app_backend` and the retained QML self-test suite.
- [x] Re-run existing Chrome/Firefox browser audio, route, Lua, settings, session/media, lifecycle, stress, direct-file, and offline workflows in addition to the Web MIDI matrix.
- [x] Build debug/release hosted and self-contained artifacts; verify archive/standalone contents, worklet import policy, protocol version consistency, direct Web MIDI bindings, and absence of native `midir`, ALSA/CoreMIDI/WinMM, JACK, CPAL, Qt/frontend, or test-fixture leakage from Wasm products.
- [x] Where physical Web MIDI hardware is available, perform a manual input-record/playback and controller smoke; otherwise record the environment skip while retaining deterministic browser evidence as the acceptance gate.
- [x] Reconcile every new matrix row and the project coarse status with exact commands/artifacts, update this plan's checkboxes/evidence, and leave no stale current limitation or unsupported browser claim.

Final acceptance evidence must show one production hosted browser run where a stable Web MIDI input/output pair appears once in the connection view; exact input bytes are both recorded by a connected direct-track loop and consumed by an owner-managed Lua controller; loop playback and controller feedback reach the expected output in order; hotplug/restart/session replacement recover without duplicates; and Web Audio callbacks continue through the workflow. Native and retained QML MIDI paths must remain unchanged and green.

## Completion evidence

Completed on 2026-08-09.

- Warning-denying native and Wasm checks, Trunk 0.21.14 debug/release builds, `cargo fmt --all -- --check`, `git diff --check`, JavaScript syntax, Python syntax, worklet no-import inspection, and forbidden Wasm dependency scans passed.
- Focused protocol, worklet, backend, scripting, application, composition, session, settings, and package suites passed. Allocation guards cover active and saturated Web MIDI render cycles; backend tests cover input/output fanout, disconnect/reconnect, recording, monitoring, grabbing, ordered playback, saturation, missing endpoints, desired-route persistence, and session replacement.
- `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo test --workspace --features shoop_engine/app_backend --no-fail-fast` passed 1,204 tests with no failed test summary. The retained offscreen QML self-test passed 236/236 with no skips.
- Release hosted Chrome passed Web MIDI control before audio, explicit SysEx request, denial/retry, generation-safe asynchronous port-open failure removal and recovery, direct-track record/save/load/ordered playback, unchanged APC control and LED output, shared endpoint fanout, output-send failure visibility, malformed/oversized/saturated input diagnostics, hot-unplug/replug, output-only processor restart, route recovery, and continuing callbacks. The same workflow passed from the release self-contained HTML. Existing hosted/self-contained audio, output-only, settings, session/media, lifecycle, saturation, stress, and direct-file workflows remained green.
- Firefox 150.0.1 retained the ordinary production Web Audio/session/Lua workflow with no MIDI hosts and a truthful unrequested Web MIDI state. Chrome/Chromium remains the production functional Web MIDI verification browser; other browser/origin support remains conditional on API and permission availability as documented in `docs/web_midi_contract.md`.
- Debug/release hosted ZIPs and self-contained HTML artifacts passed `package_artifacts.py verify`; package checks found the Web MIDI UI/adapter, embedded UI/worklet Wasm and worklet script, consistent protocol v4, and no native MIDI/audio/Qt/frontend or fixture backend leakage.
- No physical Web MIDI hardware was available in the validation environment, so the optional manual hardware smoke was skipped. The deterministic production-adapter Chrome workflows are the acceptance gate required by this plan.

## Requirement-to-artifact completion audit

The final audit below maps every immutable acceptance criterion to implementation and direct behavioral evidence. Checked stage boxes and package markers are bookkeeping and structural evidence only; they are not used as substitutes for the listed unit, integration, realtime, and production-browser behavior.

| Criterion | Concrete implementation artifact | Direct verification evidence | Audit result |
|---:|---|---|---|
| 1 | `browser_midi.rs`: `BrowserMidiState`, gesture-only `request_access`, generation-scoped asynchronous `open` observation/removal with stale-failure diagnostics, stable open/pending-handle reuse and one-shot closed-handle reopen across connection-state events, send-error reporting, and unsupported/denied/retry/SysEx button copy | Hub lifecycle test; Chrome granted, `WEB_MIDI_DENY_FIRST=1`, generation-current `WEB_MIDI_OPEN_FAIL=1` inventory removal/reopen, superseded-open non-mutation diagnostics, exact bounded open/close counts and closed-connection recovery under state notifications, and forced send-failure paths; Firefox truthful unrequested state | Complete |
| 2 | `HubInner::refresh`, `endpoint_id`, `endpoint_name`, and `BrowserMidiCore`'s ID-keyed map publish `webmidi:source|sink:<MIDIPort.id>` once with current names/directions | `lifecycle_and_hotplug_publish_revisioned_stable_endpoints`; Chrome inventory/hotplug workflow asserts exactly two canonical endpoints and two MIDI host rows | Complete |
| 3 | `WebAudioBackend` sends route mutations through protocol v4; `EngineBackend` alone mutates worklet route truth and publishes confirmed links separately from desired state | Real rendered Web MIDI connection-cell pointer click emits the exact canonical route intent; backend disconnect/reconnect and missing-endpoint tests; worklet route tests; Chrome waits for authoritative confirmations after mutation, load, hotplug, and restart | Complete |
| 4 | Hub track fanout, `PushMidiInput`, `stage_web_midi_input`, and `ExternalMidiPort` stage frame-zero input into every confirmed direct-track input | `web_midi_input_fans_out_once_to_every_connected_track`, record/monitor/grab backend test, worklet test, and Chrome exact note record/save/load/playback workflow | Complete |
| 5 | Render-side ordered collection plus bounded main-thread `DrainMidiOutput` fanout sends each application event to every confirmed sink without requiring audio routes | Backend one-to-many output, explicit disconnect/reconnect, mute, missing-device, and session-replacement assertions; Chrome ordered note pair, hotplug, output-only restart, and no-duplicate route assertions | Complete |
| 6 | `docs/web_midi_contract.md` specifies next-quantum frame zero and ordered but main-thread/browser-latent output, with no sample-accuracy claim | Documentation/source inspection plus frame-zero protocol validation in `WorkletHost` tests | Complete |
| 7 | `WebMidiControlService` implements the shared `MidiControlService`; `ScriptManager` remains authoritative for regex, pacing, fanout, reconnect, teardown, and diagnostics | Existing scripting hotplug/multi-match/rate/zero-rate/teardown tests; Chrome open failure removes connection eligibility until refresh; unchanged APC production input, authoritative solo action, and LED output before and after recovery/hotplug | Complete |
| 8 | Browser `Runtime` injects the Web MIDI service independently of `BrowserAudioController`; worklet track pumping discards safely until running | Chrome reaches `control-ready-without-audio`, receives APC output, and retains an awaiting-audio driver before audio enablement | Complete |
| 9 | `midi_endpoint_host_id` reuses canonical Web MIDI IDs; application merge publishes one host inventory while track ports remain `UserManaged` and Lua ports `OwnerManaged` | `web_midi_track_and_control_views_share_canonical_host_rows`; Chrome asserts two host rows, two Lua ports, and simultaneous track/control confirmed links | Complete |
| 10 | Documented caps: 256 subscriptions, 1,024-message hub queues, 128-event protocol batches, 256 events per staged track/quantum, and 1,024 pending outputs; malformed/stale/refused/drop paths count rather than truncate | Hub boundary tests, protocol malformed/stale/batch tests, backend saturation/refusal and worklet no-allocation tests, workspace realtime guards, and Chrome 1,100-message saturation with callback recovery | Complete |
| 11 | Hub accepts control payloads through 256 bytes while engine input validates the unchanged four-byte `MidiStorageElem` contract; access requests `sysex: true` explicitly | `track_and_control_limits_refuse_without_truncation`, scripting message-limit tests, worklet oversized rejection, and Chrome explicit SysEx request plus malformed/257-byte counters | Complete |
| 12 | Existing session port connection lists persist canonical IDs; `desired_web_midi_connections` survives missing hosts and staged replacement without a schema change | Backend capture/missing/load/reappear test; production Chrome save/load verifies exact routes and recorded bytes before playback, then hotplug/restart reconfirmation | Complete |
| 13 | `browser_smoke.mjs` installs a pre-start `navigator.requestMIDIAccess` platform double while production still executes the direct `web-sys` adapter; no fixture backend exists in product composition | Production opens the global connection dialog; a rendered Web MIDI cell pointer test proves its exact canonical mutation intent, while hosted release Chrome covers that mutation through record/load/playback, APC I/O, denial/retry, open/send failure, overflow, hotplug, restart, and callback continuity; self-contained release repeats the full workflow | Complete |
| 14 | Native paths remain target-gated; Wasm manifests use direct `web-sys`; package verification and dependency scans reject missing MIDI markers and native backend leakage | Warning-denying native/Wasm/worklet builds; 1,204-test workspace; 236/236 QML; Chrome ordinary/output-only/settings/session/lifecycle/stress/direct-file regressions; Firefox regression; debug/release package, no-import, and forbidden-tree scans | Complete |
| 15 | Updated `README.md`, `src/rust/shoopdaloop_egui/README.md`, `docs/egui_port_model.md`, `docs/egui_lua_compatibility_contract.md`, `docs/source/usage.midicontrol.rst`, this contract, parity matrix, and replacement project | Current-doc absence scan finds no delivered browser Lua/Scripts/Web MIDI capability described as absent; `WMIDI-*` rows and project coarse status are `Complete` | Complete |

All staged implementation and verification items are also covered:

| Stage | Complete artifact/verification coverage | Audit result |
|---:|---|---|
| 0 | `docs/web_midi_contract.md`, canonical helpers/states/capacities, target-neutral hub core, pre-start browser API double, `WMIDI-*`/project rows, six core tests, warning-denying Wasm API build, and granted/denied Chrome probes cover every contract-freeze and verification item. | Complete |
| 1 | The shared hub owns access/handlers/open/close/hotplug, reuses open/pending handles and reopens closed handles without state-event churn, and provides bounded subscriptions and ordered sends; `WebMidiControlService`, target-neutral cooperative injection, independent UI, plan updates, scripting/application tests, generation-safe open-failure propagation/recovery, and pre-audio APC Chrome evidence cover every service item. | Complete |
| 2 | Protocol v4 endpoint/input/output contracts, `ExternalMidiPort`, worklet staging/draining, endpoint-first journal replay, ephemeral generation filtering, normalized snapshots/activity/diagnostics, protocol/backend/worklet fanout/order/mute/hotplug tests, and no-allocation saturation evidence cover every transport item. | Complete |
| 3 | Canonical deduplicated inventory, authoritative pending/confirmed policy, explicit disconnect and missing-device desired state, save/load and replacement preservation, bounded error surfaces, all named documentation, real rendered-cell pointer mutation, application/session/worklet tests, and stale-claim scans cover every routing/persistence/presentation item. | Complete |
| 4 | The production-adapter Chrome double grants/denies, fails/reopens, hotplugs, injects and records; hosted and standalone workflows cover exact track record/save/load/ordered output, shared APC I/O, disconnect/device loss, malformed/oversized/saturation recovery, processor restart/output-only mode, teardown, callbacks, and the Firefox conditional-support regression. Both planning documents record the limits/evidence. | Complete |
| 5 | Formatting/diff/warning gates, focused/full workspace and QML suites, retained Chrome/Firefox matrices, debug/release hosted/standalone packages, protocol/import/dependency/fixture scans, the documented physical-hardware skip, synchronized plans, checked boxes, and committed milestones cover every closure item. | Complete |

Gate audit:

- `cargo fmt --all -- --check`, `git diff --check`, JavaScript/Python syntax checks, warning-denying native/Wasm/worklet builds, and Trunk 0.21.14 debug/release builds passed on the final implementation.
- Focused suites passed before the full `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo test --workspace --features shoop_engine/app_backend --no-fail-fast` run; that full run reported 1,204 passed and no failed summary.
- `QT_QPA_PLATFORM=offscreen ... shoopdaloop_dev.sh --self-test` reported 236 passed, zero failed, and zero skipped.
- `package_artifacts.py verify` passed both debug and release ZIP/standalone pairs. Its marker checks were supplemented by actual hosted and self-contained Chrome execution, worklet import inspection, protocol tests, and target dependency-tree scans.
- The initial implementation/browser milestone is commit `b1dca267`, documentation closure is `cd202912`, and the first durable audit is `dedfca6e`; generation-safe open-failure propagation and state-event lifecycle reconciliation are in hardening commit `b30b2993`; exact rendered Web MIDI cell interaction is commit `6f49f10b`. Pre-existing unrelated `src/rust/shoop_egui` working-tree edits are excluded from every milestone commit.

No criterion is missing or represented only by a proxy signal. The only environment-dependent action is the explicitly conditional physical-hardware smoke, whose unavailable-hardware skip and deterministic production-adapter acceptance substitute are recorded above.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
