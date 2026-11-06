# P14 Audio Driver Switching Implementation Plan

Restore P14 so a running native session can change audio driver, device, rate, or buffer configuration through a confirmed host-managed transaction. Preserve supported session content and application identities, recover from transition failures, and save startup preferences separately from runtime success.

Implementation base: `feat/backend-replacement`, inspected at [`0b681225`](https://github.com/SanderVocke/shoopdaloop/tree/0b6812259ffc00b9a524bb6095cfc2c6b2112847). Follow the repository [planning instructions](https://github.com/SanderVocke/shoopdaloop/blob/0b6812259ffc00b9a524bb6095cfc2c6b2112847/.agents/prompts/write_plan.md) and [P14 parity requirement](https://github.com/SanderVocke/shoopdaloop/blob/0b6812259ffc00b9a524bb6095cfc2c6b2112847/docs/native_session_recovery/feature-parity.md).

## Goals and scope

Support Dummy, CPAL device/host selections, and direct JACK wherever those drivers are built and available. Cover the embedded native application and independent native hosts with UI attachment over socket and shared-memory carriers. CPAL's JACK host and the direct JACK backend are separate configurations.

Preserve every currently supported session feature: primitive audio/MIDI/trigger tracks, direct-track composites, dry/wet roles, built-in FX and OxiSynth state and mappings, recorded-take settings, supported bus configurations, controls, sync relationships, selection, targeting, and routing intent. Leave loops stopped after a completed switch; live transport position, pending transitions, and rolling input history are transient.

Browser device switching, firmware device switching, JACK transport/timebase, automatic device-failure switching, Carla, bus FX, latency-editing restoration, session script ownership conversion, and P17 dependent composite modes are outside P14. Their existing capability restrictions and refusal behavior remain. Browser and firmware builds still receive regression checks.

## Immutable acceptance criteria

| ID | Required result |
| --- | --- |
| P14 A1 Confirmation | Discovery and preflight leave the running driver and session intact. Every applied change requires explicit confirmation identifying the resolved driver, device configuration, rate, buffer, interruption, and conversion. Cancellation changes neither runtime nor saved preferences. A changed target rate requires renewed consent before committing that target. |
| P14 A2 Recording and conflicts | Active, armed, scheduled, or composite-driven future recording, Replacing, and wet rerecord block the destructive transition without stopping capture. Reject conflicting persistence, structural, input-edit, or shutdown work explicitly; recheck at confirmation and before teardown. |
| P14 A3 Session preservation | Successful switching preserves supported structure, content, role order, controls, processor and recorded-take state, sync/composite relationships, selection, targeting, and public application identities. Backend handles may change. Loops resume stopped, client script VMs are not reinstalled or restarted, and previews and held notes receive defined cleanup. |
| P14 A4 Conversion | Equal-format switches preserve exact media and timing. Rate changes use the production quantum-aware dependency conversion contract for audio, exact MIDI, lengths, offsets, preplay, capture alignment, composite timing, and retained prefix/suffix media. Preserve sync multiples and frame-precise MIDI; reject overflow, unsupported topology, or insufficient resources before teardown where determinable. |
| P14 A5 Failure recovery | Pre-teardown failure leaves the prior runtime intact. Failure after teardown restores the prior driver and captured session at the original format, with stopped transport. Target startup, upload, processor restore, and mapping failures cannot publish success. Failed rollback reports both errors and an explicit fatal state, retaining bounded recovery data. Dummy fallback cannot masquerade as a successful switch. |
| P14 A6 Ownership and recovery | Once confirmed work is accepted, the host/service completes it or rolls it back despite UI loss. Reattachment reconciles one retained result without replay. Stale confirmations, generations, backend replies, and transfer completions cannot mutate the replacement runtime. Unconfirmed preparations expire or cancel on owner loss without changing running audio. |
| P14 A7 Preference persistence | Runtime confirmation and saving the startup configuration have distinct outcomes. Save failure leaves the new driver active, reports unsaved preferences, and supports retry without another device transition. Save-only settings do not switch live audio. Saved preferences apply to the intended host on its next start. |
| P14 A8 Safety and delivery | Callback-reachable paths remain allocation/deallocation-free, nonblocking, and free of device I/O. Preparation, capture, queues, rollback owners, and cleanup have finite budgets and truthful exhaustion results. Production-route tests, native driver/carrier equivalence, applicable final-head CI, documentation, and automated review satisfy the delivery gate below. |

## Design rules

- The service owns switch identity, admission, capture, progress, retained outcomes, and restoration mappings. The native composition root owns device lifecycle and injects an asynchronous controller interface; neither the service nor RT adapter imports native drivers.
- Preserve the running service, host session identity, attachment endpoints, and operation ledger while replacing the engine-side connection. Rebind the host graph controller and discard observations from retired engine generations.
- Poll bounded work and publish progress. Device opening/closing, waiting, conversion, media copies, and processor preparation run outside render callbacks and outside blocking service turns. Confirmed work must not depend on a surviving UI codec worker.
- Reuse the immutable capture and prepared replacement machinery. Add an identity-preserving restoration mode rather than feeding the session unchanged through ordinary load, which allocates fresh live identities.
- Share existing conversion algorithms through a dependency-allowed neutral preparation layer. Keep archive/document adapters on the codec side; do not add a `shoop_host`, `shoop_app`, or backend dependency on `shoop_session`, or weaken dependency audits to permit it.
- Retain the original unconverted capture until target activation and service publication succeed. Budget source media, converted media, upload scratch, processor reservations, and rollback owners together. Keep existing finite capacities unless an explicit design revision justifies a change.
- Resolve physical ports by identity. Retain missing-port intent and diagnostics when switching backends; never connect a similarly named replacement implicitly. Recreate JACK publication and preserve ordinary bus order and zero-bus sessions.
- Use strict target startup for switching. Preserve startup fallback behavior as its own policy, but publish actual active-driver metadata truthfully.
- Keep preference storage local to the machine owning the audio host. Specify the independent host's startup-profile store and configuration precedence explicitly; do not save its device preferences into the attaching client's local audio profile or exported connection profile.

## Implementation stages

Execute stages in order. Stages 2 and 3 depend on the interface established in Stage 1; Stage 4 requires both. Each stage ends with its focused checks passing and a milestone commit. The first implementation milestone combines Stages 1–5 because the protocol, injected controller, restoration mapping, shared conversion and composition-root changes require a coherent workspace build; subsequent qualification and delivery changes remain separate milestones.

### Stage 1 Establish the transaction and delivery branch

- [x] Refresh `feat/backend-replacement`, inspect current instructions and relevant baseline CI, and create `feat/p14-audio-driver-switching` from that branch. Save the executable plan as `PLAN_P14_AUDIO_DRIVER_SWITCHING.md` in the repository.
- [x] Map P14 A1 through A8 to the retained reference tests and production test locations. Record the supported driver/platform/carrier matrix, including absent build features and separate CPAL JACK/direct JACK cases.
- [x] Define typed discovery, preflight, confirm/cancel, progress, terminal result, and preference-save/retry contracts in `shoop_app_api` and `shoop_app_protocol`. Bound names/catalogs and version wire changes explicitly.
- [x] Add service operation ownership and an injected, polled host-controller interface. Define the freeze/capture/teardown/publication boundaries, concurrent-command refusal rules, unconfirmed preparation expiry, and stale-generation rules. Device lifecycle calls retain their worker owner until completion; OS calls cannot safely be forcibly cancelled. Slow lifecycle work stays visibly pending and blocks a second transition rather than claiming cleanup or rollback completed.
- [x] Specify the host-local startup-profile persistence path and launch precedence for embedded and independent hosts, including unwritable storage and command-line overrides.
- [x] Add fake-controller contract tests, commit, push, and open a draft PR targeting `feat/backend-replacement`.

Verification: protocol round trips and bounds, incompatible versions, no-controller capability refusal, stale/duplicate confirmations, cancellation, concurrent operations, and detach before versus after confirmed admission. Run dependency audits immediately to validate interface placement.

### Stage 2 Add native preflight and lifecycle control

- [x] Reuse native discovery/configuration code in `shoop_host/src/engine.rs` and `shoop_rt_host_native`; expose resolved profiles without activating rendering or disturbing the current driver.
- [x] Implement strict Dummy, CPAL, and direct JACK start/stop through the host controller. Keep actual device rate, buffers, instance name, feature availability, and port capacities authoritative.
- [x] Recheck the opened target against consent. If its rate changed, stop the uncommitted target and restore the source as necessary before presenting renewed confirmation.
- [x] Preserve bounded MIDI reset, preview cancellation, stream teardown, graph teardown, and off-callback resource cleanup. Make lifecycle requests and results generation-scoped.
- [x] Provide deterministic fault injection through existing mock/device test facilities.

Verification: valid/invalid profiles, same-driver device and buffer changes, missing devices/backends, server absence, changed resolved rates, cleanup failure, repeated start/stop, and actual Dummy transitions. Use mock CPAL and existing JACK tests during iteration; reserve the complete physical matrix for Stage 6.

### Stage 3 Prepare coherent capture and target conversion

- [x] Extend service persistence capture for switch-owned immutable images, independent of client bulk-transfer lifetime. Fence relevant mutations at the capture cut and reject active/armed/scheduled/future recording without altering it.
- [x] Factor the existing `ConversionPlan`/`SessionFormatPlan` algorithms and media conversion into the allowed shared preparation interface. Retain document adapters and reuse the same timing rules for file conversion and driver switching.
- [x] Prepare the entire target session against negotiated target rate, quantum, capacities, and processor catalog before destructive transition where possible. Retain original media and source mappings for rollback.
- [x] Cover role-aware channels, supported processor/take state, composites and sync dependencies, signed offsets, preplay, retained media windows, and exact MIDI state.
- [x] Charge aggregate memory and work budgets before allocations; return explicit errors and release candidates on cancellation or preparation failure.

Verification: same-format byte equality; independent audio/MIDI and timing oracles for 48 kHz, 44.1 kHz, and 32 kHz conversions; sync multiples and quantum rounding; mixed media and composite timing; unsupported-state refusal; overflow and memory exhaustion. Run existing persistence/conversion regressions after the extraction and keep dependency audits passing.

### Stage 4 Commit restoration and rollback

- [x] Implement the service-owned transition in both embedded and independent native composition roots. Close the old engine connection with bounded cleanup, start the target, attach the replacement adapter, and restore prepared content.
- [x] Add identity-preserving service restoration and complete backend mapping validation. Restore policies, supported processors, graph publication, routing intent, and stopped loop state before publishing a terminal runtime result.
- [x] Fence old engine replies, media reads, transfers, and pre-transition musical operations. Advance the appropriate persistence generation/epoch without replacing the application's session authority or losing accepted results.
- [x] On target or publication failure, reconstruct the original-format source driver/session from the retained cut. Publish explicit rollback success or fatal rollback failure with both causes.
- [x] Ensure UI detach, connection loss, and reattachment do not abort confirmed work, duplicate execution, or trigger fresh-session initialization.

Verification: fault injection at every phase, including partial uploads, processor failure, bad mapping, failure after target startup, rollback startup/restoration failure, and lost result delivery. Compare public IDs and complete session state before/after; repeat over typed local queues, socket, and shared memory. Check source and converted owners are released after terminal outcomes.

### Stage 5 Integrate settings and preference retry

- [x] Route existing discovery/request/confirmation actions through `shoop_app_client` and the production service. Advertise `AudioDriverSwitching` only when the complete host-managed flow is available.
- [x] Reuse the settings dialog and progress projection. Remove the independent-host shortcut in `shoopdaloop/src/main.rs` that treats a switch request as a local preference save.
- [x] Persist the successful active profile through the owning host's configured startup store. Carry a separate save result and retry identity; stale save completions cannot complete a later switch.
- [x] Reconcile runtime success and unsaved-preference state after UI loss. Keep retry available without retaining the originating UI draft as the sole recovery authority.
- [x] Preserve local Lua VM lifecycle and update consumers of unchanged public IDs without reinstalling scripts. Keep browser/firmware capability refusal truthful.

Verification: confirmation/cancel UX, progress and error states, save-only behavior, save failure then retry without additional start/stop calls, restart into the saved profile, independent-host preference destination, stale save results, and UI detach during persistence.

### Stage 6 Validate production behavior and complete delivery

- [ ] Port the retained reference switch scenarios to production-route tests: confirmation/resampling, dynamic buses, cancellation, recording and active-I/O conflicts, target failure, remap rollback, fatal rollback, changed-rate reconfirmation, and save retry.
- [ ] Run a reusable full-session scenario containing exact audio/MIDI, sync, composites, dry/wet processor state, supported controls, explicit routes, and zero/multiple buses. Switch rates and drivers, save/export, reload, and compare independent expected content and timing.
- [ ] Qualify Dummy, CPAL, and direct JACK transitions in both directions where available, plus CPAL host/device/buffer changes. Exercise embedded and split hosts, socket/shared-memory delivery, UI crash/reconnect, repeated switching, absent devices, and resource exhaustion.
- [ ] Run native real-output/MIDI tests with the required device/server present. Record software/mock, virtual-device, and physical-device evidence separately; a skipped backend test does not qualify it. Cover the repository's supported Linux/macOS/Windows configurations through CI and available device qualification.
- [ ] Run final workspace and relevant application/host/processor suites, packaged native smokes, browser regressions, and firmware compilation/protocol checks. Add selected ownership/concurrency Miri checks if the change introduces ownership or queue paths; preserve existing workloads and report timeouts as incomplete.
- [ ] Update the parity entry, native-host/application guides, and switch/persistence contract with supported configurations, interruption, rollback, budgets, and save-retry semantics. Keep generated results in ignored work/artifact paths and PR/CI artifacts.
- [ ] Push the final implementation head, make the PR ready for review, and inspect every applicable CI result. Resolve failures and repeat only affected local checks plus changed final-head CI.
- [ ] Check automated PR reviews and inline threads after meaningful pushes and again on the final head. Address feedback and iterate until approval; report unavailable review explicitly. Leave the PR unmerged and the worktree clean. Close P14 only when the acceptance map and final-head delivery checks pass.

Verification: every P14 acceptance criterion links to production coverage and applicable platform evidence on the final PR head. Preserve unrelated open parity rows and label any baseline failures separately without treating them as passing acceptance.

## Test strategy and required gates

During implementation, run narrow protocol, service, conversion, adapter, native-host, and UI tests for the stage being changed. Prefer deterministic failure injection and exact media/state oracles over repeatedly launching the full matrix. Run conversion/persistence and recovery regression groups at their integration milestones; run the complete applicable gates at Stage 6 and after any subsequent material change.

Use the branch's [validation guide](https://github.com/SanderVocke/shoopdaloop/blob/0b6812259ffc00b9a524bb6095cfc2c6b2112847/VALIDATION.md) and the configured development environment. Native gates include:

```sh
cargo fmt --all -- --check
RUSTFLAGS="-D warnings" cargo build --workspace
SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend,shoop_backend/native-fx --profile ci
python3 scripts/check_shoop_test_usage.py
python3 scripts/check_feature_parity.py
python3 scripts/check_dependency_boundaries.py --scope runtime --scope loops --scope engine
python3 -m unittest discover -s scripts/tests
```

Use cargo-nextest 0.9.116 as required by the test profile. Omit `SHOOP_ALLOW_MISSING_BACKENDS=1` for real driver qualification. Add the DSP scope/feature isolation checks when touching processor preparation. Use the documented packaged application, browser/worklet, firmware, and targeted Miri commands rather than replacing them with compile-only claims.

Before each Rust milestone commit, format and complete the warning-denying build; run the test-usage audit when Rust tests change. Before pushing behavior changes, run the relevant suites. Use CI for other platforms and inspect final-head results; avoid repeatedly running unrelated expensive suites during local iteration.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

This workflow includes branch creation, milestone commits and pushes, PR creation, CI repair, and automated-review follow-up through approval. It does not include autonomous PR merging.

## Implementation references

- [P14 backlog and dependency boundaries](https://github.com/SanderVocke/shoopdaloop/blob/0b6812259ffc00b9a524bb6095cfc2c6b2112847/docs/native_session_recovery/feature-parity.md)
- [Retained switching behavior and tests](https://github.com/SanderVocke/shoopdaloop/blob/0b6812259ffc00b9a524bb6095cfc2c6b2112847/src/rust/shoop_app_reference/src/lib.rs#L4068)
- [Existing settings controls and confirmation](https://github.com/SanderVocke/shoopdaloop/blob/0b6812259ffc00b9a524bb6095cfc2c6b2112847/src/rust/shoop_egui/src/settings_dialog.rs#L686)
- [Client refusal path](https://github.com/SanderVocke/shoopdaloop/blob/0b6812259ffc00b9a524bb6095cfc2c6b2112847/src/rust/shoop_app_client/src/runtime.rs#L1386)
- [RT adapter switching stubs](https://github.com/SanderVocke/shoopdaloop/blob/0b6812259ffc00b9a524bb6095cfc2c6b2112847/src/rust/shoop_rt_backend/src/backend.rs#L438)
- [Native composition root](https://github.com/SanderVocke/shoopdaloop/blob/0b6812259ffc00b9a524bb6095cfc2c6b2112847/src/rust/shoop_host/src/serve.rs#L221) and [embedded composition](https://github.com/SanderVocke/shoopdaloop/blob/0b6812259ffc00b9a524bb6095cfc2c6b2112847/src/rust/shoopdaloop/src/engine_backend.rs)
- [Session restoration mappings](https://github.com/SanderVocke/shoopdaloop/blob/0b6812259ffc00b9a524bb6095cfc2c6b2112847/src/rust/shoop_app/src/service/persistence/mapping.rs) and [conversion algorithm](https://github.com/SanderVocke/shoopdaloop/blob/0b6812259ffc00b9a524bb6095cfc2c6b2112847/src/rust/shoop_session/src/conversion/session.rs)
- [Persistence timing and resource contract](https://github.com/SanderVocke/shoopdaloop/blob/0b6812259ffc00b9a524bb6095cfc2c6b2112847/docs/native_session_recovery/persistence-contract.md) and [dependency audit](https://github.com/SanderVocke/shoopdaloop/blob/0b6812259ffc00b9a524bb6095cfc2c6b2112847/scripts/check_dependency_boundaries.py)

