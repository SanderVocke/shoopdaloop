# Carla Subprocess Hosting Plan

## How to use this document

This is a living implementation plan for running LV2-hosted Carla instances in isolated subprocesses.

- The **Requirements** section is the fixed contract. Executing agents may check requirements off as evidence is obtained, but must not add, remove, or reinterpret them without explicit user approval.
- The **Implementation steps** section is intentionally editable. Executing agents should update, split, reorder, replace, or expand those steps as implementation discoveries warrant.
- Keep this document current: check completed work, record verification evidence, document prototype shortcuts, and update the next actions after each meaningful iteration.
- Preserve the overall ordering principle: **first build an integrated end-to-end prototype, then refine it to satisfy the full robustness and performance requirements**.
- Do not perform a broad IPC mechanism bake-off before an integrated prototype works. Introduce abstract communication interfaces, begin with the simplest viable implementation, and evaluate or replace the concrete IPC underneath those interfaces later.
- A working prototype is not feature completion. The feature is complete only when every requirement has evidence and all temporary prototype compromises have either been removed or explicitly accepted by the user.
- If a requirement appears infeasible or needs to change, stop and ask the user rather than editing the requirement.

## Requirements — fixed contract

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

## Implementation steps — living and editable

### Phase 0: Maintain the plan and define seams

- [ ] Add a short status entry under **Progress log** whenever a phase materially advances.
- [ ] Record all deliberate prototype shortcuts under **Prototype debt** as soon as they are introduced.
- [ ] Define stable semantic interfaces before choosing an optimized transport:
  - [ ] Parent real-time block submission/completion interface.
  - [ ] Child real-time block receive/complete interface.
  - [ ] Parent control interface for lifecycle, visibility, state, and health.
  - [ ] Child control dispatcher interface.
  - [ ] Process supervisor/status interface.
  - [ ] Per-instance stdout/stderr log-buffer interface.
- [ ] Define transport-neutral data and status types with explicit bounds, errors, process generation, request IDs, and protocol version.
- [ ] Keep direct in-process hosting and subprocess hosting behind a common higher-level Carla backend API.

### Phase 1: Create the simplest subprocess vertical slice

The goal of this phase is integration, not final RT performance. Use the simplest viable channel implementation behind the abstractions. A framed local socket or similarly straightforward cross-platform mechanism may temporarily carry control, audio, and MIDI.

- [ ] Add an internal worker entry mode that starts without constructing the normal QML application.
- [ ] Prefer self-spawning the installed ShoopDaLoop executable in hidden worker mode unless prototyping proves a separate helper is materially safer.
- [ ] Launch one worker for one Carla chain and complete a versioned handshake.
- [ ] Instantiate the existing Carla LV2 host inside the worker.
- [ ] Send one bounded audio block to the worker, process it through Carla, and return the output.
- [ ] Send and receive bounded MIDI data with frame offsets.
- [ ] Exercise active state, show/hide external UI, state save, and state restore over the control abstraction.
- [ ] Prove that the Carla external UI and LV2 instance remain colocated in the worker.
- [ ] Add a fake worker/backend option for development and automated tests where Carla is unavailable.
- [ ] Add only enough functional tests to stabilize this vertical slice; defer broad stress and mechanism comparison.

### Phase 2: Integrate the prototype with sessions and FX controls

- [ ] Add a subprocess Carla backend variant while retaining the direct backend variant.
- [ ] Route an actual session FX chain through the prototype transport.
- [ ] Keep the prototype path isolated behind the transport interfaces so session code does not depend on socket framing or another concrete mechanism.
- [ ] Connect current active, visible, ready, and state-save/restore behavior to the subprocess backend.
- [ ] Add process-generation and high-level lifecycle states such as starting, running, crashed, restarting, stopped, and unavailable.
- [ ] Change the FX-button semantic API from a raw visibility inversion to a high-level toggle-or-recover operation.
- [ ] Verify manually that a real track can process audio/MIDI, open Carla, save state, close Carla, and unload cleanly through the prototype.

### Phase 3: Add supervision, logs, crash reporting, and prototype recovery

- [ ] Add a per-instance supervisor that owns the child lifecycle and control connection.
- [ ] Drain child stdout and stderr independently from process start until exit.
- [ ] Implement bounded per-stream buffers with oldest-data eviction, dropped-byte counts, and generation markers.
- [ ] Distinguish expected shutdown from crash, signal/exception termination, startup failure, protocol failure, and unresponsive termination.
- [ ] Expose process status, crash generation, crash summary, and log snapshots to the frontend update layer.
- [ ] Add a per-instance log window with separate stdout/stderr views, copy, refresh, and clear actions.
- [ ] Add a queued crash notification with an action to open the affected logs.
- [ ] Retain a last-known-good state in the parent.
- [ ] Implement the FX-click recovery sequence: launch, handshake, instantiate, restore checkpoint, restore desired active state, then show UI.
- [ ] Preserve logs, checkpoint, and failure details if restart fails.
- [ ] Add targeted prototype tests for crash detection, pipe flooding, restart, and state recovery without yet attempting exhaustive stress coverage.

### Phase 4: Add the persisted global setting

- [ ] Move settings ownership into an always-loaded application settings model rather than tying it to whether the settings window is open.
- [ ] Add the subprocess-hosting setting and a safe default.
- [ ] Add or update schemas and migration logic so existing settings files load correctly.
- [ ] Ensure Carla backend creation cannot race ahead of settings loading.
- [ ] Bind new Carla instances to the selected hosting mode.
- [ ] State in the UI whether changing the setting affects existing instances or requires session/application reload.
- [ ] Add settings persistence, migration, and initialization-order tests.
- [ ] Verify that disabling the setting still exercises the in-process implementation.

### Phase 5: Specify and implement the final real-time transport

Start this phase only after the integrated prototype demonstrates the complete subprocess lifecycle and user flow.

- [ ] Convert prototype observations into a concrete real-time transport contract, including:
  - [ ] Maximum channels and frames.
  - [ ] Audio layout.
  - [ ] MIDI capacities and overflow policy.
  - [ ] Number and lifecycle of block slots.
  - [ ] Sequence and generation handling.
  - [ ] Notification and deadline semantics.
  - [ ] Fallback output policy.
  - [ ] Counters and diagnostics.
- [ ] Implement a versioned fixed-layout shared-memory block transport beneath the existing abstractions.
- [ ] Use multiple ownership-tracked slots so a timed-out block cannot race with reuse by the parent.
- [ ] Pre-fault or initialize mappings and allocate all storage before real-time processing begins.
- [ ] Keep control messages, process supervision, logs, and state transfer off the real-time transport.
- [ ] Implement bounded spin-then-wait or another measured deadline strategy without unbounded audio-thread blocking.
- [ ] Ensure the worker can recover/free abandoned slots and that stale generations cannot publish output into a restarted generation.
- [ ] Make startup fail clearly on protocol, layout, atomic, alignment, or capacity incompatibility.
- [ ] Retain the simple prototype transport as a test/reference implementation until the optimized transport is proven.

### Phase 6: Remove remaining real-time integration overhead

- [ ] Precompute Carla routing entries when the session graph/topology changes.
- [ ] Resolve audio and MIDI port indices outside the audio callback.
- [ ] Remove per-cycle title cloning, name formatting, port searching, temporary vectors, and host-map reconstruction.
- [ ] Give the subprocess real-time client single-owner access from the session process path rather than sharing it through an ordinary mutex.
- [ ] Preallocate MIDI staging and overflow reporting.
- [ ] Ensure UI, state, logs, and supervisor activity cannot hold a lock needed by the parent audio thread.
- [ ] Add allocation-guard and bounded-deadline tests around the full session-to-worker-to-session path.

### Phase 7: Evaluate and refine concrete IPC mechanisms

This is the deliberate IPC evaluation point. It occurs after the feature works end to end and its actual communication patterns are known.

- [ ] Define representative microbenchmarks and end-to-end benchmarks from the working implementation.
- [ ] Measure the current simple transport and the shared-memory implementation rather than relying only on theoretical comparisons.
- [ ] Evaluate mature mapping, local-socket, and process-shared notification libraries against the requirements.
- [ ] Audit candidate libraries for Windows, Linux, macOS, timeout behavior, crash behavior, maintenance risk, permissions, cleanup, and real-time suitability.
- [ ] Compare at least the retained prototype mechanism and the optimized candidate under the same abstraction.
- [ ] Record the selected implementation and rejected alternatives under **Decisions** with measured evidence.
- [ ] Replace only the transport implementation if a better mechanism is selected; do not leak mechanism-specific behavior into Carla, session, or UI layers.
- [ ] Remove unused transport dependencies and prototype implementations after equivalent test coverage exists, unless retaining one provides clear testing value.

### Phase 8: Cross-platform lifecycle hardening

- [ ] Implement secure per-user endpoint and shared-memory naming with random instance identity.
- [ ] Add framing limits, authentication/nonce validation, version negotiation, and malformed-message rejection.
- [ ] Implement parent-death and cleanup behavior appropriate to each platform.
- [ ] Ensure force termination is always performed outside the audio thread and cannot race with a new process generation.
- [ ] Test worker startup from development builds and installed/portable packages.
- [ ] Verify inherited environment and dynamic-library search behavior needed by LV2 and Carla.
- [ ] Verify paths containing spaces and non-ASCII characters.
- [ ] Verify clean application shutdown with hidden, visible, busy, hung, starting, and crashed workers.

### Phase 9: State and UI refinement

- [ ] Define when fresh checkpoints are requested without compromising the real-time path.
- [ ] Seed the parent checkpoint from loaded session state before recovery can be needed.
- [ ] Return the retained checkpoint if a live save request times out or the child is unavailable.
- [ ] Ensure restore operations cannot overlap processing in an unsupported way.
- [ ] Deduplicate crash notifications by process generation.
- [ ] Make recent stderr available in the notification without duplicating unbounded log data.
- [ ] Refine FX colors/tooltips and process-status text for all states.
- [ ] Handle multiple simultaneous Carla crashes without losing notifications or opening an uncontrolled dialog storm.
- [ ] Add accessibility and keyboard behavior for the logs and crash UI.

### Phase 10: Complete verification, performance work, and documentation

- [ ] Add protocol/layout unit tests and compile-time/runtime layout assertions.
- [ ] Add fake-worker integration tests for normal processing and every defined failure mode.
- [ ] Add tests for malformed, oversized, stale-generation, duplicated, and out-of-order messages.
- [ ] Add MIDI overflow and audio-capacity boundary tests.
- [ ] Add stdout/stderr flood, truncation, binary/non-UTF-8, and clear-buffer tests.
- [ ] Add repeated crash/restart and parent/child shutdown soak tests.
- [ ] Add worker hang and crash-during-block tests proving bounded callback behavior.
- [ ] Add real Carla smoke tests that skip clearly when Carla is unavailable.
- [ ] Run end-to-end allocation-guard coverage.
- [ ] Benchmark representative 2-channel and 16-channel chains at buffer sizes such as 32, 64, 128, 256, 512, and 1024 frames.
- [ ] Record median, high-percentile, worst observed transport overhead, CPU cost, deadline misses, and any added latency.
- [ ] Compare against in-process hosting on Windows, Linux, and macOS.
- [ ] Tune slot counts, wait strategy, deadlines, and buffer capacities from measurements.
- [ ] Exercise packaged builds on all three operating systems.
- [ ] Update user and developer documentation.
- [ ] Remove all unresolved entries from **Prototype debt**, or obtain explicit user acceptance for any retained compromise.
- [ ] Attach verification evidence to every requirement and check all satisfied requirements.

## Prototype debt

Record shortcuts here immediately; remove each entry when resolved.

- [ ] No prototype debt recorded yet.

## Decisions

Record durable choices and evidence here. Do not use this section to change requirements.

- [ ] No implementation decisions recorded yet.

## Progress log

Append concise dated entries containing completed work, verification, discoveries, and the next intended action.

- [ ] No progress recorded yet.
