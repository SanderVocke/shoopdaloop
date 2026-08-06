# Carla Subprocess Hosting Plan

## Goals and scope

Run each native Carla LV2 FX-chain instance in a project-controlled, supervised subprocess while preserving the existing in-process mode, session state, audio/MIDI behavior, external UI, and supported Windows/Linux/macOS application behavior.

This plan covers the native Carla Rack, Carla Patchbay, and Carla Patchbay 16x hosts. It does not make arbitrary LV2 plugins directly hostable, add browser/Wasm plugin hosting, or complete the broader pure-egui FX/settings/persistence roadmap. The current QML product is the integrated Carla behavior baseline; new lifecycle, transport, diagnostics, and settings semantics must live below the frontend so the pure-egui application can reuse them when its deferred FX milestone is implemented.

## How to use this document

This is a living implementation plan.

- The **Requirements** section is the immutable acceptance contract. Requirements may be checked off as evidence is obtained, but must not be added, removed, or reinterpreted without explicit user approval.
- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised only for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
- Record deliberate shortcuts under **Prototype debt**, durable choices under **Decisions**, and material progress under **Progress log**.
- Preserve the ordering principle: first build an integrated end-to-end subprocess prototype behind stable seams, then refine it to meet the full robustness and real-time requirements.
- Do not perform a broad IPC bake-off before the integrated prototype works. Begin with the simplest viable transport behind explicit interfaces and evaluate replacements later.
- A working prototype is not feature completion. Completion requires evidence for every requirement and resolution or explicit user acceptance of every prototype compromise.
- If a requirement appears infeasible or no defensible implementation path remains, stop and report the evidence, attempted paths, blocker, and input needed.

## Current-branch baseline

The original plan was written at `712d0f5a` on `origin/plan/carla-subprocess-ipc`. This refreshed plan accounts for the current branch through `4a50e1d0`:

- Carla hosting is now implemented in Rust by `CarlaLv2Host`. It discovers and instantiates Carla LV2 variants, owns fixed audio/atom buffers, processes audio/MIDI, serializes portable LV2 state as JSON, and hosts the external UI.
- The full native application backend creates an in-process Carla host and registers it in the engine session. The callback currently traverses title-keyed hosts, locks each host, formats/searches route names, stages MIDI, and processes Carla inline.
- Engine control has moved to sequenced bounded commands, pending object handles, state mirrors, asynchronous graph scheduling, and explicit real-time allocation/lock guards. The bridge must use these facilities rather than recreate the removed QObject backend/update-thread model.
- The QML `FXChain` remains the supported end-to-end Carla/session UI adapter. It wraps the Rust application-backend handle; process supervision, transport framing, and recoverable state must not be implemented in QML or CXX-Qt.
- A pure-egui architecture now exists through `shoop_app_api`, `shoop_app`, `shoop_backend`, and `shoop_egui`, but FX chains, dry/wet topology, native real drivers, settings, and persistence are deferred there. The subprocess core must remain frontend-independent, and the parity matrix must record newly discovered FX semantics without making this feature depend on completing the whole migration.
- User settings are still loaded and owned by `SettingsWindow.qml` using `settings.1`. A startup-owned Rust settings service does not yet exist.
- Existing dry/wet Carla QML tests, Carla engine tests, state/session tests, no-allocation tests, lock-guard tests, and Tracy instrumentation provide useful regression and performance surfaces.

None of these baseline facts satisfies the subprocess-specific requirements below.

## Requirements — immutable acceptance contract

### Configuration and compatibility

- [ ] **REQ-01:** Provide a persisted global setting that selects subprocess hosting for LV2-hosted Carla instances.
- [ ] **REQ-02:** When the setting is disabled, preserve supported in-process Carla behavior.
- [ ] **REQ-03:** The setting and its effective scope must be clear in the UI. If it cannot safely migrate already-running instances, the UI must state when it takes effect.
- [ ] **REQ-04:** Existing settings files must continue to load through a defined default or schema migration.

### Isolation and hosting

- [ ] **REQ-05:** In subprocess mode, each Carla FX-chain instance must have its own supervised child process so one instance can fail independently of the others.
- [ ] **REQ-06:** The child must run ShoopDaLoop's Carla LV2 loading, processing, state, and external-UI hosting implementation. The bridge itself must be implemented and controlled by this project.
- [ ] **REQ-07:** A Carla or hosted-plugin crash must not crash ShoopDaLoop or leave its audio callback blocked indefinitely.
- [ ] **REQ-08:** Normal Carla UI closure, intentional shutdown, session unload, and application exit must not be reported as crashes.
- [ ] **REQ-09:** Child processes must be shut down and reaped reliably, including during abnormal parent termination as far as each supported operating system permits. Orphan workers and stale IPC resources must be prevented or cleaned up.

### Audio and MIDI communication

- [ ] **REQ-10:** Audio and MIDI must cross the process boundary with bounded memory use and without per-block serialization or allocation in the final real-time path.
- [ ] **REQ-11:** The final parent audio-thread path must not use ordinary mutexes, perform control-protocol I/O, log, format strings, create or destroy processes, or wait without a bounded deadline.
- [ ] **REQ-12:** MIDI byte content and sample offsets within each processing block must be preserved, subject only to explicit and observable fixed-capacity overflow handling.
- [ ] **REQ-13:** A late, hung, disconnected, or crashed child must fail safely. The parent must produce a defined fallback for the affected wet output, avoid shared-memory races, and remain able to process later blocks.
- [ ] **REQ-14:** Communication overhead and added latency must be minimized. The final design must use bulk shared memory or an equivalently low-overhead mechanism for real-time audio and MIDI unless measurements demonstrate a better alternative.
- [ ] **REQ-15:** Real-time buffers, queues, slot counts, deadlines, and overflow policies must be explicit, bounded, observable, and tested.

### State preservation and recovery

- [ ] **REQ-16:** The parent must retain a last-known-good Carla state independently of the child process.
- [ ] **REQ-17:** Successful state restores and state saves must update the recoverable checkpoint without replacing a good checkpoint with a failed or partial result.
- [ ] **REQ-18:** Saving a session while a worker is crashed or unavailable must preserve the last-known-good state rather than silently replacing it with an empty or unavailable state.
- [ ] **REQ-19:** After a crash, the next appropriate FX-button click must start a new Carla process generation, instantiate the same chain type, restore the recoverable state, restore the desired active state, and open the Carla UI.
- [ ] **REQ-20:** If restart or state restoration fails, the chain must remain safely unavailable, preserve its checkpoint and diagnostics, and communicate the failure to the user.

### Diagnostics and UI

- [ ] **REQ-21:** Capture each Carla worker's stdout and stderr continuously into separate, bounded, per-instance buffers without allowing full pipes to deadlock the child.
- [ ] **REQ-22:** The user must be able to open, refresh or inspect, copy, and clear each stream's captured output from the UI. Truncation or dropped data must be disclosed.
- [ ] **REQ-23:** Preserve useful diagnostics across a worker restart, distinguishing process generations.
- [ ] **REQ-24:** Show one user-visible crash notification per unexpected process generation. It must identify the affected chain and provide access to its logs.
- [ ] **REQ-25:** The FX control must visibly distinguish running, starting/restarting, crashed/unavailable, bypassed, and UI-visible states where relevant.

### Cross-platform robustness

- [ ] **REQ-26:** Support Windows, Linux, and macOS as first-class targets with the same user-visible semantics.
- [ ] **REQ-27:** IPC naming, permissions, framing, version negotiation, process launch, parent-death handling, timeout behavior, and cleanup must be designed and tested for all three target operating systems.
- [ ] **REQ-28:** The packaged application must be able to locate and launch its worker implementation without relying on development-tree paths or shell wrappers.
- [ ] **REQ-29:** Carla external UI functionality that requires LV2 instance access must execute in the same child process as its Carla LV2 instance.

### Verification and maintainability

- [ ] **REQ-30:** Keep real-time and control communication behind explicit abstractions so concrete IPC implementations can be replaced or compared without changing Carla/session/frontend semantics.
- [ ] **REQ-31:** Provide automated coverage for protocol validation, audio/MIDI transfer, state preservation, logs, clean shutdown, crashes, hangs, deadline misses, restart, malformed input, and repeated process generations.
- [ ] **REQ-32:** Provide allocation-guard coverage for the final bridged real-time path.
- [ ] **REQ-33:** Measure the final transport against the in-process baseline across representative buffer sizes and Carla channel counts, including tail latency and deadline misses, on Windows, Linux, and macOS.
- [ ] **REQ-34:** Document the setting, safety behavior, recovery behavior, diagnostics UI, expected overhead, and platform limitations for users and developers.

## Design rules and constraints

- Preserve `CarlaLv2Host` as the authoritative ShoopDaLoop Carla/LV2 implementation. In subprocess mode its instance, LV2 state calls, and external UI runtime belong exclusively to the worker.
- Keep chain identity stable and distinct from process generation. Every control request, block slot, observation, log record, and notification must carry enough identity to reject stale-worker activity.
- Keep three boundaries explicit: a high-level FX-chain backend, non-real-time lifecycle/control, and real-time block transport. QML, egui, session persistence, and dry/wet policy must not know socket/shared-memory details.
- Integrate lifecycle and observations with the current command/state-mirror architecture. Do not restore the removed per-object backend polling thread or put QObject ownership into engine/IPC crates.
- The engine session owns the parent real-time endpoint with single-writer callback access. Supervisors and frontends communicate through bounded commands and published snapshots, not an ordinary mutex shared with the callback.
- Keep worker launch, state transfer, UI commands, logs, timeout escalation, and process destruction off the audio thread.
- Keep the session document compatible: hosting mode is global application policy, not serialized per FX chain. Existing Carla state strings remain the checkpoint payload unless a versioned migration is demonstrably necessary.
- Default existing settings to in-process hosting. A changed setting applies only to subsequently created instances or after the clearly documented reload boundary; do not live-migrate a processing instance.
- The defined failure fallback is silence/drop for the affected wet audio/MIDI result while the independent dry path and other chains continue. Any later change requires a recorded decision and tests.
- Native desktop targets are in scope. Browser/Wasm builds must continue to exclude LV2/process hosting cleanly rather than receive stub success behavior.
- Prototype-only allocation, serialization, callback locking, or coarse routing is allowed only when recorded as debt and isolated behind the final interfaces.

## Implementation steps — living and editable

### Phase 0: Freeze the baseline and semantic seams

- [x] Inventory current Carla creation, destruction, active/visible state, state save/restore, dry/wet routing, MIDI gating, session load/unload, and UI-close behavior; link each behavior to an existing or new regression test.
- [x] Capture in-process baseline traces/benchmarks for 2- and 16-channel variants and representative block sizes before changing ownership.
- [x] Define transport-neutral chain IDs, process generations, request IDs, protocol versions, bounded errors, lifecycle states, and health/overflow counters.
- [x] Define explicit interfaces for:
  - [x] high-level FX lifecycle/state/UI operations;
  - [x] parent and worker real-time block submission/completion;
  - [x] worker control dispatch;
  - [x] process supervision and status publication;
  - [x] bounded per-generation stdout/stderr snapshots.
- [x] Decide the protocol/value crate boundary so worker-capable code does not depend on Qt, egui, `shoop_app`, or native audio-driver implementations.
- [x] Verify focused Carla engine/QML/session tests and record exact baseline commands/results under **Progress log**.

### Phase 1: Refactor in-process Carla behind the new seams

This phase changes ownership structure without adding IPC and must preserve behavior.

- [x] Split Carla instance operations from the parent-facing FX-chain handle so direct and subprocess implementations share high-level semantics.
- [x] Replace title-derived callback routing with stable chain/routing entries prepared when topology is applied; retain current names only at session/frontend boundaries.
- [ ] Route active/visible/state requests through bounded control operations and published state rather than requiring frontend code to lock a callback-owned host.
- [x] Give the session callback a stable processor entry and keep the direct implementation available for REQ-02.
- [ ] Add a fake processor/worker implementation that supports deterministic audio, MIDI, state, UI-state, delay, hang, disconnect, and crash scenarios without Carla installed.
- [ ] Verify unchanged direct Carla behavior, session round trips, dry/wet mode tests, graph rebuilds, shutdown-thread ownership, and unavailable-Carla behavior.

### Phase 2: Build the simplest subprocess vertical slice

The goal is end-to-end integration, not final transport performance. A framed local socket or equivalent cross-platform mechanism may temporarily carry both control and blocks.

- [x] Add a worker entry mode selected before normal Qt/egui application construction and capable of running in installed layouts.
- [x] Compare self-spawning the installed executable with a dedicated packaged helper using concrete startup/packaging evidence, then record the choice under **Decisions**.
- [x] Launch one worker for one chain, complete a nonce-authenticated version/capability handshake, and reject malformed/incompatible peers.
- [x] Move creation and ownership of the existing `CarlaLv2Host` into the worker without duplicating Carla/LV2 behavior.
- [x] Send one bounded audio/MIDI block with frame offsets, process it through the fake worker and real Carla when available, and return bounded audio/MIDI output.
- [ ] Exercise active state, external UI show/hide/close, state save, state restore, and intentional shutdown over the control interface.
- [x] Prove that the Carla external UI and LV2 instance remain colocated in the worker.
- [x] Start draining stdout and stderr immediately at launch so the prototype cannot deadlock on full pipes.
- [x] Record all temporary callback blocking, serialization, capacities, and fallback behavior under **Prototype debt**.
- [ ] Verify the vertical slice in development and packaged-layout test fixtures on each available host platform.

### Phase 3: Integrate the prototype with the native engine and current application

- [x] Add direct/subprocess Carla backend selection at the Rust application-backend creation boundary; keep QML as an adapter over the same high-level handle.
- [x] Register only the parent real-time endpoint in the engine session in subprocess mode; never share a worker-owned `CarlaLv2Host` with the parent callback.
- [ ] Route an actual QML-created dry/wet Carla chain through the prototype while preserving current port descriptors, Rack/Patchbay/16x aliases, activation modes, MIDI gating, and session schema.
- [ ] Publish lifecycle, desired active/visible state, generation, availability, and failure through target-neutral snapshots/state mirrors.
- [ ] Replace raw visibility inversion with a high-level toggle-or-recover operation while retaining ordinary show/hide behavior for healthy workers.
- [ ] Update `EGUI_FEATURE_PARITY_MATRIX.md` with discovered subprocess/FX semantics and evidence. Do not add a second IPC implementation to `shoop_app` or block this stage on the deferred egui FX milestone.
- [ ] Manually and automatically verify audio/MIDI processing, UI open/close, state round trip, clean unload, and multiple independent chains through the prototype.

### Phase 4: Add supervision, logs, crash reporting, and recovery

- [ ] Add one supervisor per subprocess chain, owning launch, control connection, generation changes, expected-shutdown markers, timeout escalation, reaping, and cleanup.
- [ ] Drain stdout and stderr independently from process start through exit into separate fixed-capacity buffers with oldest-data eviction and dropped-byte counts.
- [ ] Distinguish startup failure, protocol failure, unexpected exit/signal/exception, unresponsive termination, normal UI closure, requested stop, session unload, and application shutdown.
- [ ] Retain generation-tagged status, summaries, recent diagnostics, and logs across restart without unbounded growth.
- [ ] Retain the last-known-good state in the parent; update it only after a confirmed successful restore or complete save.
- [ ] Return the checkpoint during session save when the worker is unavailable or a live save fails/times out.
- [ ] Implement toggle-or-recover: launch a new generation, handshake, instantiate the same chain type, restore the checkpoint, restore desired active state, then show the UI.
- [ ] Preserve the checkpoint/logs/failure details and remain safely unavailable if any recovery step fails.
- [ ] Publish one queued crash notification per unexpected generation and target-neutral log actions/data.
- [ ] Add the current QML FX status/log UI with separate streams, generation markers, copy, refresh/inspect, clear, truncation disclosure, and accessible crash/recovery states.
- [ ] Verify crash, abort, malformed protocol, pipe flood, hang, timeout escalation, repeated restart, save-while-down, and multi-chain independence with the fake worker.

### Phase 5: Move settings ownership to startup and persist hosting mode

- [x] Introduce an always-loaded Rust user-settings model/service independent of whether a settings window exists.
- [x] Define a typed global Carla hosting mode and load it before any session can create FX chains.
- [x] Migrate or default `settings.1` data without losing MIDI/script settings; use in-process as the default for old or absent settings.
- [x] Bind the current settings UI to the startup-owned value and state that changes affect newly created instances or require session/application reload.
- [ ] Ensure session load and concurrent chain creation cannot race settings initialization.
- [ ] Keep the setting available to future pure-egui settings/application composition without adding Qt types below the adapter.
- [ ] Verify first run, old valid files, migrated files, malformed files, save/reload, initialization ordering, enabled subprocess selection, and disabled direct selection.

### Phase 6: Specify and implement the final real-time transport

Start only after the integrated prototype demonstrates the complete lifecycle and user flow.

- [ ] Convert prototype measurements into a versioned transport contract covering maximum channels/frames, audio layout, MIDI event/byte capacities, slot count, ownership states, sequence/generation rules, deadlines, fallback, counters, and memory ordering.
- [ ] Implement fixed-layout shared-memory block transfer, or document measured evidence for an equivalent lower-overhead bounded mechanism.
- [ ] Use multiple ownership-tracked slots so timeout never permits parent reuse while a stale worker may still read/write a slot.
- [ ] Allocate, map, initialize, and pre-fault all storage before processing; validate layout, atomic support, alignment, capacities, nonce, and protocol compatibility at startup.
- [ ] Keep control/state/log traffic off the real-time transport.
- [ ] Implement a measured bounded notification/deadline strategy and wet-silence/MIDI-drop fallback without unbounded callback waits.
- [ ] Prevent stale generations from publishing completions; reclaim abandoned slots only under an explicit safe ownership transition.
- [ ] Keep the simple transport as a reference/test implementation until optimized transport equivalence is proven.
- [ ] Verify boundaries, overflow, timeout/reuse races, stale generations, disconnects, hangs, and recovery under stress and model-checking where practical.

### Phase 7: Remove real-time integration debt

- [ ] Remove callback-time host-map cloning, title cloning, name formatting, linear port searches, temporary vectors, and host-map reconstruction from Carla routing.
- [ ] Remove ordinary mutex acquisition from the bridged callback path and give it single-owner access to the parent real-time endpoint.
- [ ] Preallocate bounded MIDI staging and preserve byte content/frame offsets in both directions with observable overflow.
- [ ] Ensure UI, checkpointing, logging, supervision, graph scheduling, and process teardown cannot hold a resource needed by the callback.
- [ ] Add allocation-guard and lock-guard coverage around the full session-to-worker-to-session path, including failure fallback.
- [ ] Add Tracy zones/plots for submission, wait, completion, fallback reason, queue/slot occupancy, worker processing, deadline misses, and generation without formatting/logging from realtime.
- [ ] Verify no-allocation/no-unapproved-lock tests, bounded callback completion during worker failure, and unchanged non-Carla engine paths.

### Phase 8: Evaluate and finalize concrete IPC choices

- [ ] Define representative microbenchmarks and end-to-end benchmarks from the working implementation.
- [ ] Compare the retained simple transport and optimized transport under identical interfaces and workloads.
- [ ] Audit candidate mapping/socket/notification libraries for maintenance, licensing, permissions, timeout/crash behavior, and Windows/Linux/macOS support.
- [ ] Record selected and rejected mechanisms with measurements under **Decisions**.
- [ ] Replace only concrete transport pieces; do not leak mechanism behavior into engine, session, Carla, or frontend semantics.
- [ ] Remove unused dependencies/implementations after equivalent coverage exists, unless a retained reference transport has clear test value.

### Phase 9: Cross-platform lifecycle and packaging hardening

- [ ] Implement secure per-user endpoint/shared-memory naming with random instance identity and restrictive permissions.
- [ ] Apply framing limits, nonce validation, protocol negotiation, malformed-input rejection, and safe cleanup on all target platforms.
- [ ] Implement platform-appropriate parent-death handling and document unavoidable abnormal-termination limits.
- [ ] Ensure forced termination/reaping occurs outside realtime and cannot race a new generation.
- [ ] Package and locate the worker without shell wrappers or development paths; verify inherited environment and dynamic-library/LV2 search requirements.
- [ ] Verify paths/usernames containing spaces and non-ASCII characters.
- [ ] Verify application shutdown with hidden, visible, busy, hung, starting, crashed, and multiply restarted workers.
- [ ] Run development, installed, portable, and platform-specific package smoke tests on Windows, Linux, and macOS.

### Phase 10: Final state/UI refinement, validation, and documentation

- [ ] Define a bounded checkpoint refresh policy and seed checkpoints from loaded session state before recovery can be needed.
- [ ] Ensure restores cannot overlap processing in an unsupported way and failed/partial operations cannot replace a good checkpoint.
- [ ] Refine status colors/tooltips/text, notification deduplication, recent-stderr access, multiple simultaneous crashes, keyboard behavior, and accessibility.
- [ ] Add protocol/layout assertions and tests for malformed, oversized, stale, duplicated, and out-of-order messages.
- [ ] Add audio/MIDI capacity, offset, overflow, stdout/stderr binary/non-UTF-8/flood/truncation/clear, repeated crash/restart, shutdown soak, and parent/child failure tests.
- [ ] Add real Carla smoke tests that skip with an explicit reason only when Carla is unavailable.
- [ ] Benchmark direct and subprocess modes for 2- and 16-channel chains at 32, 64, 128, 256, 512, and 1024 frames on Windows, Linux, and macOS.
- [ ] Record median, high-percentile, and worst observed overhead, CPU cost, deadline misses, fallback counts, and added latency; tune capacities/deadlines from evidence.
- [ ] Run final allocation/lock guards, engine tests, current QML self-tests, session/dry-wet Carla tests, package tests, and relevant pure-egui regression tests.
- [ ] Document user settings, scope/reload behavior, failure fallback, recovery, logs, overhead, supported targets, and platform limitations; document protocol/lifecycle design for developers.
- [ ] Update the egui project/parity documents so future FX/settings work consumes the shared semantics and does not regress subprocess behavior.
- [ ] Resolve every **Prototype debt** entry or obtain explicit user acceptance.
- [ ] Attach evidence to every requirement and check all satisfied requirements.

## Prototype debt

Record shortcuts immediately and remove them when resolved.

- [ ] The prototype serializes and allocates per-block audio/MIDI JSON over TCP rather than using the final shared-memory transport.
- [ ] The callback still reaches the processor through the current shared Carla mutex, and control and realtime traffic share one stream.
- [ ] The prototype has one in-flight block and relies on socket timeouts whose scheduling precision has not been validated as a realtime deadline.
- [ ] A transport error marks the parent endpoint unavailable but does not yet run the supervised generation-aware recovery sequence.
- [ ] Logs are bounded and drained, but generation history and frontend inspection/copy/clear actions are not yet exposed.
- [ ] Parent-death handling, stale resource cleanup, and packaged cross-platform launch evidence are not yet implemented.

## Decisions

Record durable choices and evidence here. Do not use this section to change requirements.

- [x] Preserve current session FX-chain documents and state payload semantics; hosting mode is global application policy rather than a per-session field. Evidence: current sessions serialize chain type, ports, and `internal_state`, while the requested mode is explicitly global.
- [x] Default old/absent settings to in-process mode. This preserves current behavior and gives `settings.1` a defined compatibility path.
- [x] Treat the QML application as the current integrated Carla acceptance surface, but keep all new semantics frontend-independent. Evidence: current QML supports Carla dry/wet/session workflows; pure egui explicitly defers FX/settings/persistence.
- [x] Self-spawn the installed ShoopDaLoop executable in hidden worker mode for the prototype. Evidence: the existing package has one authoritative executable and dependency closure; a Linux QML integration test launched workers from that executable without development shell wrappers. Revisit only if packaged cross-platform evidence shows a blocker.
- [x] Use nonce-authenticated framed loopback TCP with bounded JSON payloads as the prototype control/block transport. It is explicitly retained behind `shoop_plugin_protocol` and recorded as debt, not accepted as the final realtime mechanism.
- [ ] Final shared-memory/notification implementation is undecided pending Phases 6 and 8 measurements.

## Progress log

Append concise dated entries with completed work, verification, discoveries, and the next action.

- [x] **2026-08-06 — Plan import and refresh:** imported `PLAN.md` from `origin/plan/carla-subprocess-ipc` (`712d0f5a`) and reconciled it with the current Rust Carla host, sequenced engine controls/state mirrors, realtime guards/Tracy surfaces, QML adapter, pure-egui migration boundaries, dry/wet regression coverage, and still-window-owned settings. No implementation requirement is claimed complete.
- [x] **2026-08-06 — Baseline and vertical slice:** recorded behavior/tests and 2-/16-channel measurements in `CARLA_SUBPROCESS_BASELINE.md`; added the frontend-neutral versioned protocol, bounded framing and validation, a common processor seam, fake processor, topology-time route resolution, self-spawned pre-Qt worker mode, one-process real-Carla audio/MIDI/state lifecycle, bounded stdout/stderr drains, and Linux integration coverage. Added startup-owned typed settings with old-file defaulting and a QML restart-scope control; a subprocess-selected Patchbay 16x QML test passed. The larger pre-existing dry/wet Carla QML file still fails root creation and remains an uncovered gate. Next: complete supervision/generation publication and replace prototype realtime debt.
