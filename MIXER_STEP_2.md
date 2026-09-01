# Mixer step 2: bus controls and sidebar implementation plan

## Goal

Implement the next increment of `MIXER_ARCHITECTURE.md`: render the fixed stereo **Master** bus as a bus block in the right sidebar immediately above the logo, and give it post-sum gain, stereo balance, mute, and channel-aware peak metering. The same controls must be available through the Lua control API and behave identically in native, dummy, Worker, and AudioWorklet execution.

This increment extends the completed routing sandbox without changing its bounded graph: track outputs may still route independently to Master channels and system sinks, Master outputs may route to system sinks, and no implicit route is introduced.

## Scope

### Included

- One right-sidebar bus block per application bus, ordered vertically; the fixed Master is the only runtime bus in this increment.
- A volume fader, stereo balance dial, mute button, bus name, and one post-processing peak indication per output channel.
- One bus-wide gain and mute value. Gain applies uniformly to all channels.
- Balance for buses with exactly two ordered channels, using Left/Right attenuation after summing.
- Realtime bus processing after the channel sums and before all Master output fan-out.
- Normalized backend, application, protocol, Worker, AudioWorklet, and fake-backend control and meter state.
- Authoritative control reconciliation and bounded failure/saturation behavior equivalent to existing track controls.
- Session persistence, migration, replacement, resampling, and driver-switch preservation of bus controls.
- Lua getters and setters for bus gain, fader position, balance, and mute.
- Documentation, deterministic tests, native and WebAssembly validation, a dedicated branch and PR, green CI, and resolution of all automated Codex review findings.

### Excluded

- Creating, removing, renaming, reordering, or resizing buses.
- Additional runtime buses beyond the fixed stereo Master.
- Bus-to-bus routing or any expansion of the mixer graph boundary.
- Track inputs, loops, sends, returns, monitoring, MIDI, or track internals as bus sources.
- Per-channel faders, per-channel mutes, route/send levels, solo, pre-fader sends, or implicit channel mapping.
- Plugin hosting, editable insert chains, latency compensation, recording/export sinks, or offline rendering.
- Input-side bus controls or an input/output split in the bus block.
- Lua APIs for meter telemetry; this increment exposes bus control, not metering automation.

## Immutable acceptance criteria

1. The main UI has a right-sidebar bus area immediately above the logo. It renders one vertically ordered block per `AppState` bus and renders exactly one block named `Master` in the current fixed-bus capability.
2. The Master block shows one volume fader, one mute button, one balance dial, and a two-channel peak meter without an input/output split. The sync-track section and logo remain usable and do not overlap the bus area at supported window sizes.
3. A new or migrated Master defaults to `0 dB`, centered balance (`0.0`), and unmuted. Adding controls creates no route and does not make a disconnected Master audible.
4. Bus gain is bounded to the same decibel range and uses the same fader curve as track output gain. It applies uniformly to every channel after summing and before every bus-output fan-out.
5. Balance is available only when a bus has exactly two ordered channels. It uses the existing no-boost stereo attenuation law: negative values attenuate Right, positive values attenuate Left, and center leaves both at unity before bus gain. Mono or multichannel buses receive uniform gain only and reject balance mutation.
6. Mute silences every bus output channel without changing gain, balance, routes, or direct track-to-system output. Unmuting restores the signal under the retained gain and balance values.
7. Each bus channel publishes a finite post-gain/post-balance/post-mute output peak in dB. Silence and mute report the established meter floor, peaks accumulate/reset with existing polling semantics, and meter animation cannot become routing or control authority.
8. Gain, balance, and mute changes are realtime parameter updates. They do not rebuild or swap the graph, allocate, lock, log, or perform unbounded work in the audio callback.
9. Backend snapshots are authoritative. UI and Lua requests may be represented optimistically while pending, but they settle to confirmed backend/worklet state or clear on rejection, stale identity, command saturation, backend replacement, or timeout without leaving a false control value.
10. Native, dummy/engine, fake, Worker, AudioWorklet, and worklet-client implementations expose the same bus control semantics, channel peaks, validation, command supersession, and failure behavior.
11. Lua API versioning and compatibility remain explicit. The `shoop_control` module supports `bus_get_gain`, `bus_get_gain_fader`, `bus_get_balance`, `bus_get_muted`, `bus_set_gain`, `bus_set_gain_fader`, `bus_set_balance`, and `bus_set_muted` with zero-based bus selectors; index `0` selects Master in this increment. Existing Lua APIs and scripts remain compatible.
12. Gain, balance, and mute round-trip exactly through session save/load and backend session replacement and survive compatible audio-driver switches and resampling. Version-9 sessions migrate to the default control values. Peaks are runtime telemetry and are not persisted.
13. Mixer routing behavior from step 1 remains intact: Master starts disconnected, direct track routes remain independent, track-to-Master routes remain explicit and additive, and Connections-dialog authority and persistence do not regress.
14. The completed work is committed on a dedicated branch, pushed, and represented by a non-draft unified PR containing the bounded mixer foundation and intended step-2 delta over current `master`, with no unrelated changes. Every required CI check is green on the exact final PR head, every automated Codex finding has an evidence-backed fix and reply, and Codex reports no major issues on that same head.

Acceptance criteria may not be weakened by treating a target-specific omission, unconfirmed optimistic value, non-persisted control, hidden meter, missing Lua operation, or stale CI/review result as an acceptable limitation.

## Design rules and constraints

- Preserve the post-track-output graph and explicit route rules in `MIXER_ARCHITECTURE.md`.
- Treat gain, balance, and mute as one built-in bus processing stage between each channel sum and its bus-owned output. Do not model these controls as routes, host links, or editable FX inserts.
- Keep the bus model channel-count-generic even though only stereo Master can be instantiated. Store meter values by ordered channel identity/count rather than hard-coding only Left/Right fields in backend contracts.
- Use typed bus identity for every command and intent. Display order, names, labels, engine indices, and host port IDs are not control identity.
- Define one normalized `BusControl` contract for gain in dB, balance, and mute, and lower it into backend-specific audio-port parameters. Do not duplicate target-specific control laws.
- Validate all floating-point input as finite before mutation. Clamp gain and balance once at the normalized boundary; never serialize or publish NaN or infinity.
- Keep gain and balance values when muted. Compute each channel's effective factor from the retained bus-wide state and apply mute independently.
- Meter the bus-owned output after all built-in bus processing and before sink fan-out so every destination observes the metered signal.
- Reuse or extract existing fader, balance-dial, optimistic-value, meter-ballistics, and mute presentation primitives where practical; do not create subtly different gain curves or interaction behavior.
- Keep backend/worklet snapshots authoritative and use the existing desired-control reconciliation and bounded mutation-failure model rather than creating a UI-only truth.
- Supersede repeated queued commands only for the same bus and control parameter. A gain command must not supersede balance/mute or another bus's command.
- Persist semantic bus controls explicitly. Keep bus output port transport fields canonical so capture/replacement cannot apply gain or mute twice.
- Bump serialized protocol, session, and Lua API versions when their contracts change, and update all fixtures and compatibility documentation in the same stage.
- Preserve structural sharing where practical, but do not suppress visible meter or control updates merely because mixer topology is unchanged.
- Add no callback allocation, blocking, graph construction, logging, or unbounded iteration. Extend realtime guards and no-allocation tests for active bus processing and control changes.
- Keep the fixed Master lifetime and no-add/remove UI policy from step 1.

## Implementation stages

Dependencies are linear unless stated otherwise. Each stage must leave its touched packages compiling and its focused tests passing before the next dependent stage begins.

The recorded step-1 baseline is `a31d54d59806a29c16e87855c6118bdd978fcb45` from PR #830. On 2026-09-01 the user explicitly directed PR #843 to become one unified mixer PR against `master` and PR #830 to close. The branch was ultimately rebased onto `5cabe0fa9970eab3536d7fdd7870a666a233e5cb`; the approved unified diff contains both the completed step-1 foundation and this step-2 increment.

### Stage 0 — Establish the step-2 baseline and branch

- [x] Confirm `MIXER_MASTER_GOAL.md` has no unchecked items and record the final step-1 commit used as the baseline.
- [x] Create `shoopdaloop-mixer-step-2` from the completed step-1 head before making behavior changes; keep the working tree clean and avoid mixing unrelated changes.
- [x] Initially open the work as a stacked PR, then follow the user-approved consolidation: retarget PR #843 to current `master`, close superseded PR #830, rebase onto current `master`, and verify the unified diff contains only the bounded mixer foundation and step-2 work.
- [x] Record the implemented defaults, gain range/fader curve, stereo attenuation law, post-processing meter point, Lua selector rules, and session migration policy in this plan if code investigation reveals any required refinement.

Verification:

- [x] Compare the branch merge base and diff against the recorded step-1 baseline.
- [x] Confirm the existing fixed Master route, persistence, and Connections-dialog focused tests pass before changing their contracts.

### Stage 1 — Add normalized bus control and meter contracts

- [x] Add typed backend bus control values for gain in dB, balance, and mute, plus bus-wide confirmed control state and ordered per-channel output peaks in `BackendBusState`.
- [x] Extend the backend trait with a typed `set_bus_control` operation and normalized mutation kind/detail data so stale IDs, invalid values, unsupported balance, and backend rejection are observable and bounded.
- [x] Add corresponding application `BusState` control/meter fields, a typed `BusAction`/`AppIntent`, control capability derived from channel count, and shared gain limits/validation.
- [x] Extend fake backend state and operation capture so application and scripting tests can prove exact bus identity and control values.
- [x] Define matching helpers for desired-versus-authoritative bus controls without conflating transient peaks with persistent control state.

Verification:

- [x] Add contract tests for defaults, finite/clamped gain and balance, mute retention, stale bus rejection, non-stereo balance rejection, ordered peak shape, and fake operation identity.
- [x] Run focused `shoop_backend`, `shoop_app_api`, and fake-backend tests.

### Stage 2 — Implement native and engine realtime bus processing

Depends on Stage 1.

- [x] Store normalized gain, balance, and mute state on the fixed Master in `EngineBackend` and `NativeRuntime`.
- [x] Lower bus state into effective gain/mute parameters on each bus output port: uniform gain for arbitrary channel counts and the shared Left/Right attenuation factors only for exactly two channels.
- [x] Apply control updates without graph changes and preserve routes, direct track fan-out, output identities, and retained values across mute/unmute.
- [x] Collect and publish each bus output port's post-processing peak with the same accumulation/reset and dB-floor rules used by existing audio meters.
- [x] Ensure session polling and native state mirrors observe the effective bus output without adding callback work beyond the bounded per-channel processing already in the prepared graph.
- [x] Extend no-allocation coverage to active summed bus processing and gain/balance/mute changes between callbacks.

Verification:

- [x] Use deterministic one- and two-track fixtures to prove unity center output, known dB attenuation, left/right balance extremes and intermediate values, mute/unmute restoration, additive summing before processing, post-processing peaks, and unaffected direct outputs.
- [x] Prove control changes leave the active graph/schedule identity unchanged.
- [x] Run focused engine/dummy, native app-backend, session, port-meter, and realtime no-allocation tests.

### Stage 3 — Carry controls and meters through Worker and AudioWorklet

Depends on Stages 1–2 and may proceed alongside Stage 4 after normalized types stabilize.

- [x] Bump `PROTOCOL_VERSION` and add wire bus control state, ordered channel peaks, a typed set-bus-control command, and normalized failure mapping.
- [x] Define journal supersession by `(bus identity, control kind)` and preserve independent gain, balance, mute, route, and unrelated bus commands under saturation.
- [x] Extend AudioWorklet command dispatch and snapshots to invoke the normalized backend operation and return authoritative bus controls and peaks.
- [x] Extend the worklet client to map bus state and failures in both ordinary polling and session replacement, submit controls, and clear stale desired commands across generations.
- [x] Update raw Wasm host contracts, serialization fixtures, snapshot fixtures, command capacities, and remote browser mappings.

Verification:

- [x] Add protocol round-trip and supersession tests for every bus control variant, invalid values, command saturation, and stale generations.
- [x] Run a Worker/AudioWorklet audio fixture proving gain, balance, mute, post-processing peaks, summing, direct fan-out independence, and restoration after unmute.
- [x] Run focused `shoop_audio_protocol`, `shoop_audio_worklet`, `shoop_worklet_client`, and Wasm remote-application tests for native harnesses and `wasm32-unknown-unknown`.

### Stage 4 — Reconcile application state and persist bus controls

Depends on Stages 1–3 for complete parity.

- [x] Extend `BusModel` and immutable `AppState` snapshots with authoritative controls, ordered peaks, and stereo capability while preserving stable `BusId`/channel identities.
- [x] Handle UI and scripting bus intents through one application path, validate availability/capability, submit the backend mutation, apply bounded optimistic state, and reconcile on confirmation, failure, timeout, driver replacement, or bus disappearance.
- [x] Extend `BackendSessionBus` and the session document with explicit semantic gain, balance, and mute values; keep peak telemetry out of persistence and avoid double-applying output-port gain/mute fields.
- [x] Bump `SESSION_DOCUMENT_VERSION`, migrate version 9 and earlier accepted documents to `0 dB`/center/unmuted, and reject malformed, non-finite, out-of-range, unsupported, or noncanonical Master control shapes before backend mutation.
- [x] Capture and restore controls transactionally after bus/output creation and before exposing replacement completion; preserve them across same-rate replacement, resampling, compatible driver switching, and browser transfer.
- [x] Keep route and external-link capture/replacement unchanged and prove failed replacement rolls back controls with the rest of the staged session.

Verification:

- [x] Add application tests for pending-to-confirmed, rejection, saturation, timeout, stale bus, backend recreation, structural sharing, and meter updates without topology changes.
- [x] Round-trip default and non-default controls through deterministic archives, migration, same-rate/resampled replacement, failed-load rollback, driver switching, and browser session transfer.
- [x] Run focused `shoop_session`, `shoop_app`, fake-backend, native replacement, and remote-application tests.

### Stage 5 — Add the right-sidebar bus block column

Depends on Stage 4.

- [x] Add a reusable `BusControls`/bus-block widget keyed by `BusId`, with the bus name, channel-aware animated output meter, mute button, volume fader, and stereo-only balance dial.
- [x] Reuse the existing gain range, fader curve, double-click/default behavior, dial interaction, meter ballistics, optimistic values, disabled/error styling, touch-safe scrolling, and repaint policy used by track controls where applicable.
- [x] Place the ordered bus column directly above the fixed logo area in the 150-pixel right sidebar. Keep the logo pinned, keep the sync-track section separate, and add bounded vertical layout/scrolling so synthetic multiple-bus and short-window cases do not overlap or clip controls.
- [x] Emit exact typed bus intents, retain responsive drag state only while pending, and settle presentation from authoritative `AppState` snapshots.
- [x] Hide the balance control for non-stereo synthetic bus states while retaining one uniform gain/mute control and one meter indication per channel.
- [x] Prune widget-local state when a bus disappears and avoid deriving widget identity from bus name or list index.

Verification:

- [x] Add egui interaction tests for block order, right-sidebar placement above the logo, sync/logo non-overlap, fader/dial/mute intents, optimistic reconciliation, mute styling, animated dual peaks, stereo-only balance, stable widget identity, short windows, and a synthetic multi-bus scroll case.
- [x] Verify the Master block has no input/output split and no add/remove/rename/processor controls.
- [x] Run focused `shoop_egui` bus-widget and `AppWidget` layout tests plus native headless and browser rendering smokes where available.

### Stage 6 — Expose bus control through Lua

Depends on Stage 4 and may proceed alongside Stage 5.

- [x] Extend `ControlSnapshot` with ordered bus identities, indices, channel count/capability, gain, balance, and mute; update it from the same application snapshot used by the UI.
- [x] Add zero-based bus selectors with the existing scalar/list/`nil` conventions. In this increment selector `0` resolves to Master; missing indices select no bus, and invalid selector types are errors.
- [x] Add `bus_get_gain`, `bus_get_gain_fader`, `bus_get_balance`, `bus_get_muted`, `bus_set_gain`, `bus_set_gain_fader`, `bus_set_balance`, and `bus_set_muted` to `shoop_control`.
- [x] Use the shared linear-gain/dB/fader conversions and balance clamp. Reject balance for selected non-stereo buses before queuing an operation.
- [x] Route `ControlOperation` bus mutations through the application bus-intent handler and shadow accepted values within a script pump without bypassing backend authority.
- [x] Bump the Lua API minor version, preserve older minor compatibility, update function inventories and the compatibility contract, and leave existing bundled scripts unchanged.

Verification:

- [x] Add Lua shape, selector-order, getter, setter, clamping/conversion, mute, non-stereo rejection, invalid argument, optimistic shadow, and control-operation dispatch tests.
- [x] Add native and browser scripting integration tests proving Lua changes the same Master state/audio as the UI and that backend rejection reconciles the published state.
- [x] Run focused `shoop_scripting`, `shoop_app`, script-resource, native runtime, and Wasm scripting suites.

### Stage 7 — Documentation and end-to-end validation

Depends on all implementation stages.

- [x] Update `MIXER_ARCHITECTURE.md` to document this completed post-sum control increment while retaining the first sandbox section as historical scope.
- [x] Update `docs/port_model.md`, `docs/session_format_v1.md`, `docs/lua_compatibility_contract.md`, `docs/lua_dialog_api.md`, and relevant developer/user-facing UI documentation with processing order, defaults, meter point, persistence/version migration, Lua signatures, sidebar placement, and target parity.
- [x] Run an end-to-end dummy/native scenario: route two deterministic tracks to Master, retain one direct route, apply gain and balance, observe post-processing peaks, mute/unmute, modify controls from Lua and UI, save/reload, switch drivers, and verify exact controls, routing, direct audio, and Master audio after every transition.
- [x] Run the equivalent Worker/AudioWorklet scenario and verify authoritative snapshots and audio before/after every command and session replacement.
- [x] Verify new, version-9-migrated, disconnected, resampled, and malformed sessions; verify controls never create routes and direct routing remains unchanged.
- [x] Run `cargo fmt --all -- --check`.
- [x] Run `RUSTFLAGS="-D warnings" cargo build --workspace` in the environment selected by `.agents/info/build.md`.
- [x] Run `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend --profile ci`.
- [x] Run `python3 scripts/check_shoop_test_usage.py` because Rust tests will change.
- [x] Run `python3 scripts/check_tracing_coverage.py --require-closed`.
- [x] Build `shoopdaloop` and `shoop_audio_worklet` for `wasm32-unknown-unknown`, run the complete Node Wasm suite, and run packaged-browser smokes when browser executables are available.
- [x] Audit every immutable acceptance criterion against concrete code, test, audio fixture, document, and command evidence; leave no unchecked item or unsupported inference.

### Stage 8 — Push, PR, CI, and automated review closure

Depends on Stage 7 local gates.

- [x] Ensure each completed stage or meaningful milestone has a focused commit, the working tree is clean, and the unified branch contains no unrelated changes.
- [x] Push `shoopdaloop-mixer-step-2` and open non-draft PR #843 with the goal, architecture constraints, processing semantics, session/protocol/Lua version changes, and exact local verification commands/results in its description.
- [x] Follow the user-approved consolidation: retarget PR #843 to `master`, close superseded PR #830, rebase onto current `master`, resolve conflicts without weakening acceptance criteria, rerun affected local gates, and verify the final merge-base diff again.
- [x] Monitor `gh pr checks` for the exact current head SHA. For every failure, inspect the run attempt, matrix job, logs, and artifacts with the procedures in `.agents/info/ci-debug.md`; reproduce deterministic failures locally and use `.agents/info/ci-repro.md` before classifying a timing failure as a flake.
- [x] Fix every real CI defect, rerun relevant local suites, commit and push the fix, and restart the exact-head CI audit. Do not rely on green checks from an earlier SHA or a rerun that omits required jobs.
- [x] Enumerate every root automated Codex inline finding, assess it against the architecture and code, implement every valid fix with focused regression coverage, and reply to every finding with the fixing commit and evidence. If a finding is invalid, reply with concrete code/test evidence rather than silently dismissing it.
- [x] Request a fresh Codex review after each review-fix batch and continue until the review is completed on the exact final head with no unresolved findings and an explicit no-major-issues result.
- [x] Perform the final PR audit: local and remote heads match; working tree is clean; PR is open, non-draft, and merge-clean; every required check is completed successfully or legitimately skipped; all Codex findings have replies; latest Codex review covers the final SHA; every plan checkbox and immutable acceptance criterion has concrete evidence.

Verification:

- [x] Record the final branch SHA, PR URL, all-green check rollup, Codex review result, finding/reply count, and prompt-to-artifact acceptance audit before declaring step 2 complete.

## Final acceptance evidence audit

The implementation audit covers the final review-fix and audit commits after rebasing the unified branch onto `5cabe0fa`. PR #843 is the authoritative exact-head record for the full SHA, check rollup, and review result.

| Criterion | Concrete implementation and verification evidence |
| --- | --- |
| 1–2. Sidebar blocks and controls | `src/rust/shoop_egui/src/bus_controls.rs` and `app_widget.rs`; `block_renders_channel_aware_meter_and_stereo_only_balance`, `mute_button_emits_the_typed_bus_action`, `fader_and_dial_emit_changes_and_reconcile_to_authoritative_values`, `bus_blocks_stack_immediately_above_logo_without_overlapping_sync`, and `unified_application_paints_at_minimum_and_common_sizes`. |
| 3. Neutral disconnected defaults | `BackendBusState`, fixed-Master initialization, version-9 migration, and session defaults; `engine_master_bus_is_disconnected_and_sums_explicit_fanout_routes`, `native_dummy_exposes_a_disconnected_stereo_master`, and `version_nine_buses_migrate_default_controls_and_invalid_controls_are_rejected`. |
| 4–6. Gain, stereo balance, and mute DSP | Normalized `BackendBusControl`, shared dB/fader law, engine/native post-sum processing, and atomic stereo native parameter command; `bus_control_contract_normalizes_and_fake_backend_tracks_exact_identity`, `engine_master_controls_process_post_sum_without_rebuilding_the_graph`, `paired_audio_port_parameters_use_one_control_command`, and `master_bus_sums_two_tracks_fans_out_and_disconnects_in_the_worklet`. |
| 7. Post-processing channel peaks | Backend/native/worklet mixer snapshots, `BusState::output_peaks_db`, and `BusControls` meter painting; DSP/worklet tests verify loud, attenuated, muted-floor, reset, and ordered-channel peaks. |
| 8. Realtime safety | Preallocated graph routes and port parameters in `shoop_engine`, the one-command native stereo update, realtime guards, and `installed_audio_fan_in_is_allocation_free`; graph revision assertions prove controls do not rebuild topology. |
| 9. Backend authority and bounded reconciliation | `ApplicationModel::desired_bus_controls`, authoritative snapshot overlay/settling, response-capable timeout aging, compensating timeout commands, transactional submission, rejected-command replay cleanup, mutation failures, stale-identity cleanup, and bounded transport reservation; `bus_controls_reconcile_confirmation_failure_timeout_stale_ids_and_peaks`, awaiting-gesture/route/host timeout and rejection tests, failed-submission restoration, saturation tests, and session-replay preflight tests. |
| 10. Target parity | `shoop_backend`, native adapter, fake backend, `shoop_audio_protocol`, `shoop_audio_worklet`, `shoop_worklet_client`, detached fixed-Master seed, and raw host contract; native, complete Node Wasm, Chromium, Firefox packaged smoke, worklet DSP, protocol, replacement, early-control replay, and reconnect tests. |
| 11. Lua API 1.5 | `src/rust/shoop_scripting/src/control.rs`, `shoop_app_api::LUA_API_VERSION`, compatibility docs, control-surface/argument/read-your-writes tests, and remote application Lua control coverage for all eight getter/setter names and zero-based Master selection. |
| 12. Persistence and migration | Session document version 10 fields and version-9 migration in `shoop_session`; archive validation plus cooperative/native/browser save/load, resampling, replacement, and driver-switch tests retain controls while excluding peaks. |
| 13. Routing regression safety | Atomic internal graph schedules, explicit typed mixer routes, Connections facets, host-link replacement, and direct-route independence; engine/worklet fan-in/fan-out/disconnect tests, dialog graph tests, session route tests, and no-allocation coverage. |
| 14. Delivery closure | Dedicated `shoopdaloop-mixer-step-2` branch and unified non-draft PR #843 against `master`; focused commits, exact-head required CI, eleven root Codex findings with twelve evidence-backed replies, and final explicit Codex no-major-issues review are recorded on the PR. |

Local final implementation gates: formatting, warning-denied workspace build, test-attribute policy, closed tracing inventory, full native nextest, both Wasm package builds, all 17 Node Wasm package suites, and ten consecutive focused Chromium reproductions of the corrected remote click-content race. CI defect evidence includes the downloaded coverage trace/log and Chromium JUnit/log artifacts; both timing assumptions received bounded waits and focused regressions rather than flake classification.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
