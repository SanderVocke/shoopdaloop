# Worklet backend and driver refactoring plan

## Status and execution contract

This document is an implementation plan for the production refactoring required to separate application behavior, remote worklet communication, audio-driver lifecycle, engine processing, and browser presentation. It establishes the architecture assumed by the WebAssembly testing project, including a production-supported Worker dummy driver and migration of browser offline mode.

During implementation:

- Keep this plan updated and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not change without explicit user approval.

## Goals

- Keep the application model dependent on one backend/domain interface while hiding worklet transport and concrete audio drivers below that boundary.
- Extract the remote worklet backend from browser application composition into a reusable, platform-neutral crate.
- Separate message transport state, driver lifecycle, remote engine state, browser presentation, and host MIDI responsibilities.
- Make backend snapshots authoritative while preserving immediate UI response through explicit application-owned desired state and widget interaction optimism.
- Separate engine processing from the scheduler or driver that decides when process quanta run.
- Support physical AudioWorklet processing and a production Worker-hosted dummy driver through the same application command/event protocol.
- Support deterministic explicit processing, cooperatively free-running processing, and realtime-paced dummy processing.
- Migrate browser offline mode to the Worker-hosted dummy backend.
- Provide stable lifecycle, readiness, restart, replay, failure, quiescence, and teardown contracts suitable for production and later Wasm test fixtures.
- Preserve existing native driver and Carla architecture unless a change is required by the shared backend contract.

## Scope

Included:

- A new platform-neutral remote-worklet backend client crate.
- Extraction of transport sequencing, journal replay, wire conversion, remote snapshot assembly, and chunked transfers.
- Restricted control handles for driver/transport attachment and lifecycle publication.
- Browser physical audio-driver and presentation separation.
- Browser host-MIDI injection behind an interface.
- Engine runtime and processing-scheduler separation.
- Reusable JavaScript host logic for raw worklet Wasm interaction.
- Browser Worker dummy driver with explicit, cooperative free-running, and realtime-paced modes.
- Production browser offline-mode migration.
- Typed asynchronous backend progress, mutation failure, and pending desired state.
- Multi-instance isolation and fixture-control seams required by the downstream testing project.
- Production documentation, compatibility checks, and migration tests.

Excluded:

- Forcing JACK, CPAL, midir, Carla, or native plugin processing through Worker or MessagePort architecture.
- Changing the bounded JSON application/worklet protocol to a binary encoding.
- Making one application runtime control multiple engines or worklets.
- Treating Worker scheduling as evidence of AudioWorklet realtime behavior.
- Sending production audio sample streams over the application command MessagePort.
- Implementing the complete Wasm-pack test-suite migration, JUnit orchestration, or smoke-suite reduction described by the separate testing plan.
- Adding test-only branches to application domain behavior.

## Current architecture and pressure points

The application core already receives a `Backend` implementation, which is the correct high-level boundary. The browser path currently concentrates several lower-level concerns in one browser module:

- the remote backend implementation,
- durable and ephemeral command transport,
- MessagePort ownership,
- physical Web Audio lifecycle,
- browser permission and DOM presentation,
- browser track-MIDI bridging,
- optimistic remote snapshots,
- smoke-test event hooks.

The physical browser path runs the engine behind an AudioWorklet and protocol boundary. Browser offline mode instead instantiates an elapsed-time dummy backend directly in the application realm. The two browser modes therefore exercise different ownership, communication, lifecycle, and scheduling paths.

Normal asynchronous backend progress is also represented in several places as error strings, and the application recognizes selected strings as a pending state. Remote command failures are not fully separated from transport or polling failure. The remote backend publishes some requested values as if they were committed before the worklet has acknowledged or reported them.

## Target architecture

### Component boundaries

```text
Application and UI
  ├─ application desired-state overlay
  ├─ widget interaction optimism
  └─ dyn Backend
       └─ RemoteWorkletBackend
            ├─ authoritative remote snapshot
            ├─ command journal and in-flight operations
            ├─ protocol conversion and transfer assembly
            ├─ HostMidiBridge
            └─ RemoteBackendControl
                 └─ MessageEndpoint
                      ├─ AudioWorklet MessagePort
                      └─ Worker MessagePort

Physical browser driver
  ├─ AudioContext and MediaStream
  ├─ AudioWorkletNode
  ├─ driver lifecycle
  └─ RemoteBackendControl

Worker dummy driver
  ├─ Worker lifecycle
  ├─ engine Wasm instance
  ├─ production application MessagePort
  ├─ production realtime-paced scheduler
  └─ optional fixture-control MessagePort

Remote engine host
  ├─ protocol command host
  ├─ engine runtime
  └─ explicit process-quantum entry
```

### Crate boundary

Add a `shoop_worklet_client` workspace crate. It depends on backend domain types and the audio protocol, but not on browser, DOM, Web Audio, Web MIDI, or UI crates.

It owns:

- `RemoteWorkletBackend`, implementing `Backend`;
- transport and replay state;
- sequence and generation validation;
- authoritative remote snapshots;
- requested operation state and typed outcomes;
- wire/backend conversions;
- waveform, MIDI, loop-content, and session-transfer assembly;
- a restricted `RemoteBackendControl` handle;
- runtime-independent message endpoint and host-MIDI interfaces.

Browser-specific adapters remain in the browser application composition crate unless later evidence justifies a dedicated web adapter crate.

### State model

Use three explicit layers:

1. **Authoritative backend state**: the latest state observed from the engine or worklet. `BackendSnapshot` never labels a merely queued mutation as committed.
2. **Application desired state**: accepted user intent waiting for backend confirmation. The effective application snapshot overlays desired values on authoritative values where optimism is allowed.
3. **Widget interaction state**: short-lived values used during direct manipulation until the application publishes its desired state.

State policy:

- Continuous and idempotent controls are optimistic and last-write-wins.
- Structural and transactional operations are explicitly provisional or pending.
- Metering, positions, callback counts, driver lifecycle, route confirmation, recording completion, and realtime diagnostics are authoritative only.
- Rejection removes the relevant desired state, rolls effective state back to the authoritative value, and publishes a typed error.

### Driver and connection state

Keep distinct state authorities for:

- driver lifecycle;
- MessagePort connection and generation;
- protocol initialization and replay;
- remote engine status.

Readiness milestones are ordered and observable:

1. driver created;
2. driver running or able to process;
3. MessagePort attached;
4. protocol initialized;
5. negotiated channel/device configuration accepted;
6. durable journal replay completed;
7. backend ready.

The application-facing backend must not claim full readiness before the required milestones complete.

### Protocol decisions

- Retain bounded JSON envelopes and existing production commands/events during this refactor.
- Retain stable expected IDs initially, but distinguish reserved/provisional resources from confirmed resources.
- Make sequence correlation, stale-generation rejection, replay completion, and delayed mutation failure explicit.
- Application commands use one production MessagePort and unchanged production envelopes.
- Tests and deterministic driver control use a second fixture-control MessagePort. Explicit stepping and fixture audio inspection do not enter the production application protocol.
- AudioWorklet sample buffers continue to cross the JavaScript/Wasm linear-memory boundary, not the application command port.

### Processing modes

The Worker dummy driver supports:

- **Explicit**: process only when fixture control requests an exact quantum.
- **Cooperative free-running**: process bounded batches as quickly as practical, yielding regularly so commands, shutdown, and diagnostics remain responsive.
- **Realtime-paced**: process according to sample rate and quantum duration, with bounded catch-up and observable discontinuity/xrun accounting.

Production browser offline mode uses realtime-paced processing. Explicit and cooperative free-running modes are available through fixture control and internal verification surfaces, not ordinary application commands.

### Multi-instance decision

Support multiple independent app/backend/worklet compositions in one process or browser page. Each has isolated engine state, application MessagePort, generations, scheduler, diagnostics, and teardown.

One application controlling multiple engines is not part of this project.

## Design rules and constraints

- Preserve the existing `Backend` boundary at the application model; improve its semantics rather than exposing transport or driver types to the application.
- Keep native direct-driver implementations direct. Do not generalize native realtime paths merely to resemble the browser path.
- The protocol-data crate remains data-focused and does not own runtime transport state.
- The remote backend client has no `web_sys`, DOM, audio-device, or Web MIDI dependency.
- Browser presentation may observe driver state but may not mutate transport internals.
- Physical and dummy drivers interact with remote backend state only through the restricted control handle.
- Driver callbacks and MessagePort handlers must reject stale generations.
- Durable replay order is deterministic and documented. Ephemeral input, poll, transfer, and fixture-control messages are never journaled accidentally.
- Sequence IDs identify one command response. Unknown, duplicate, missing, and stale responses are observable failures.
- Continuous control coalescing must retain the newest desired value and must not let a stale acknowledgement clear or overwrite it.
- Structural mutations must have a documented confirmation or rollback path.
- Backend polling failure means the backend itself is unavailable; ordinary command rejection is a typed mutation outcome, not a generic poll failure.
- No asynchronous control flow depends on matching error-message text.
- Logical elapsed time drives deterministic backend polling. Wall-clock deadlines are confined to driver pacing, startup timeout, and bounded external waits.
- Worker loops yield and honor shutdown. No free-running mode may monopolize an event loop.
- Every owned Worker, port, stream, node, timer, callback, and Wasm host has an explicit teardown owner.
- Production application and worklet messages remain compatible during staged rollout unless a deliberate protocol version change is documented and tested.
- The physical AudioWorklet remains authoritative for browser realtime timing evidence.

## Immutable acceptance criteria

- `shoop_worklet_client` contains the platform-neutral remote `Backend` implementation and compiles without browser, DOM, native audio/MIDI, Carla, or native Tracy dependencies.
- The application model receives only backend/domain state and never handles MessagePort, Worker, AudioContext, or AudioWorklet objects.
- Physical Web Audio and Worker dummy drivers attach through the same restricted remote-backend control interface.
- Physical and dummy drivers use the same production application command/event envelopes over their production MessagePorts.
- Browser MIDI is injected through a host-service interface rather than owned concretely by the remote backend.
- `BackendSnapshot` contains authoritative observed state. Optimistic continuous controls are represented by application desired state and widget interaction state.
- Rapid continuous control changes are responsive, last-write-wins, survive stale snapshots, and converge to authoritative state after confirmation.
- Rejected optimistic mutations roll back and publish typed, correlated errors.
- Structural mutations are visibly provisional or pending until confirmed and have tested rejection/recovery behavior.
- Session capture, session replacement, and loop-content replacement use typed pending/progress states rather than error-string control flow.
- Driver, connection, protocol, replay, and backend readiness states are distinct and transition in the documented order.
- Response sequence and generation validation reject duplicates, unknown responses, stale messages, and superseded driver instances.
- Engine processing no longer depends on an internal dummy-versus-physical mode switch; scheduling is owned by driver/scheduler adapters.
- A reusable raw Wasm host bridge serves both the production AudioWorklet adapter and Worker dummy adapter.
- The Worker dummy supports explicit, cooperative free-running, and realtime-paced processing with bounded diagnostics and shutdown.
- Production browser offline mode uses the Worker dummy and no longer runs its engine directly in the application realm.
- Multiple independent app/backend/worklet compositions can coexist without state, message, timer, or teardown leakage.
- Native backend behavior, native driver switching, Carla support, session compatibility, and application-visible browser behavior remain intact.
- The complete native and browser validation gates pass with warning-denying builds and dependency isolation.

## Implementation stages

Stages are sequential unless explicitly noted. Each stage must preserve a buildable, testable repository and should be committed independently.

### Stage 0 — Baseline behavior and contracts

- [ ] Record current browser physical-audio and browser offline ownership diagrams.
- [ ] Inventory remote backend commands, events, durable journal rules, ephemeral commands, transfer state, optimistic mutations, and error paths.
- [ ] Inventory all direct sharing between browser driver code and transport internals.
- [ ] Record current startup, restart, shutdown, permission, offline pacing, session transfer, track MIDI, and presentation behavior.
- [ ] Add or strengthen characterization tests for protocol ordering, journal replay, stable IDs, chunk transfers, rapid controls, driver restart, stale generations, and offline processing.
- [ ] Record current native, web build, package, and smoke gates and representative timing.
- [ ] Decide and document the initial public types that move into the new crate without changing behavior.

Verification:

- [ ] Characterization tests pass before extraction begins.
- [ ] Every current browser transport responsibility has a destination component in the target architecture.
- [ ] No known behavior is left dependent on an undocumented shared mutable field.

### Stage 1 — Create the platform-neutral remote client crate

- [ ] Add `shoop_worklet_client` to the workspace.
- [ ] Move wire/backend conversion helpers into the new crate.
- [ ] Move remote track/loop/composite resource bookkeeping and snapshot assembly.
- [ ] Move waveform, MIDI-detail, session-capture, session-replacement, and loop-content transfer assembly.
- [ ] Move the remote `Backend` implementation with behavior-preserving names and APIs.
- [ ] Keep a temporary narrow adapter to the existing browser transport while extraction is incomplete.
- [ ] Replace browser/UI type imports with domain types from backend and application API crates.
- [ ] Add dependency-tree checks for browser, native driver, Carla, and native Tracy isolation.

Verification:

- [ ] Native checks and existing browser builds pass without behavior changes.
- [ ] Remote backend characterization tests pass from the new crate.
- [ ] The new crate builds for native and `wasm32-unknown-unknown` with forbidden dependencies absent.

### Stage 2 — Extract transport core and restricted control handle

- [ ] Split pure transport state from concrete `web_sys::MessagePort` ownership.
- [ ] Introduce the runtime-independent message endpoint interface.
- [ ] Introduce `RemoteBackendControl` with attach, detach, receive, driver-state, and failure operations only.
- [ ] Remove external access to journal, sequence, inbound queue, error slot, and in-flight counters.
- [ ] Track individual in-flight sequence IDs rather than only a count.
- [ ] Validate response version, sequence, generation, duplication, and ordering.
- [ ] Define and implement durable replay completion.
- [ ] Separate transport failure from command rejection and remote engine failure.
- [ ] Add an in-memory endpoint adapter for native contract tests.

Verification:

- [ ] Browser physical audio still attaches and communicates through a temporary MessagePort adapter.
- [ ] Duplicate, unknown, stale-generation, malformed, and out-of-order response tests fail observably.
- [ ] Replay tests prove exact durable order and exclusion of ephemeral commands.
- [ ] No browser driver or presentation code can mutate transport internals directly.

### Stage 3 — Formalize readiness, quiescence, and logical polling

- [ ] Add distinct driver, connection, protocol, replay, and remote-engine state models.
- [ ] Implement the ordered readiness milestones.
- [ ] Delay application-facing ready state until initialization, negotiation, and replay complete.
- [ ] Replace remote wall-clock poll scheduling with elapsed time supplied through `Backend::advance`.
- [ ] Define remote quiescence in terms of pending commands, replay, transfer work, inbound messages, and driver activity.
- [ ] Implement meaningful remote idle/quiescence observation instead of a no-op.
- [ ] Retain bounded wall-clock startup and external wait timeouts outside deterministic backend progression.

Verification:

- [ ] Readiness cannot be observed early under delayed acknowledgement tests.
- [ ] Logical-time tests produce identical polling behavior independent of host speed.
- [ ] Idle waits complete only after all specified work is settled and fail on bounded timeout.
- [ ] Driver restart resets only the intended milestones and generations.

### Stage 4 — Introduce typed asynchronous progress and mutation outcomes

- [ ] Define typed pending/ready progress for session capture, session replacement, and loop-content replacement.
- [ ] Remove application string matching for pending backend operations.
- [ ] Define typed delayed mutation failures with stable operation keys and messages.
- [ ] Deliver ordinary command rejection through backend snapshots or a dedicated typed outcome surface rather than failing the entire backend poll.
- [ ] Define immediate submission failure separately from delayed remote rejection.
- [ ] Add explicit completion and cancellation behavior for transfers interrupted by detach, restart, replacement, or shutdown.
- [ ] Preserve native synchronous behavior through immediate ready/applied results.

Verification:

- [ ] No application control flow recognizes pending work through error text.
- [ ] Native and remote backends satisfy the same typed operation contracts.
- [ ] Transfer cancellation and rejection leave no retained bytes, stale generations, or false completion.
- [ ] A rejected command does not mark the entire connection backend unavailable.

### Stage 5 — Move optimism into explicit application desired state

- [ ] Inventory all optimistic control and structural mutations in UI, application model, remote backend, and native backend paths.
- [ ] Add application desired-state records keyed by logical control or operation.
- [ ] Publish effective control values by overlaying desired state on authoritative backend state.
- [ ] Retain widget-level interaction optimism for direct manipulation and stale application publication.
- [ ] Remove remote-backend mutation of authoritative snapshots for continuous controls.
- [ ] Implement last-write-wins behavior for rapid gain, balance, mute, monitoring, loop-control, and Tiny Synth/FX changes.
- [ ] Clear desired values only when authoritative state confirms the latest desired value.
- [ ] Roll back and notify on typed rejection without allowing stale outcomes to affect newer desired values.
- [ ] Represent structural creation, removal, connection, driver switch, processor creation, and session replacement as explicit provisional/pending state where applicable.
- [ ] Keep meters, positions, driver state, route confirmation, recording completion, and realtime diagnostics authoritative.

Verification:

- [ ] Dial and continuous-control tests remain visually immediate under delayed backend snapshots.
- [ ] Rapid changes converge to the newest value under reordered delays and stale snapshots.
- [ ] Rejection rolls back exactly the affected desired value and retains newer values.
- [ ] Native and remote backends produce the same application-visible optimistic behavior.
- [ ] Authoritative backend snapshots never contain values merely because submission succeeded.

### Stage 6 — Extract host MIDI from the remote backend

- [ ] Define the platform-neutral host-MIDI bridge interface.
- [ ] Move browser endpoint discovery, track-input draining, and output sending behind the interface.
- [ ] Add null and deterministic in-memory implementations.
- [ ] Inject the bridge when constructing `RemoteWorkletBackend`.
- [ ] Keep scripting MIDI service ownership separate unless concrete evidence supports unification.
- [ ] Preserve canonical endpoint identity, direction, limits, drops, refusal counters, and hotplug semantics.

Verification:

- [ ] The remote client crate has no concrete browser MIDI dependency.
- [ ] Browser MIDI behavior and diagnostics remain unchanged.
- [ ] Null and deterministic bridges permit remote backend construction outside a browser.
- [ ] Track and scripting MIDI continue to share physical endpoint truth without duplicated identities.

### Stage 7 — Split physical browser driver from presentation

- [ ] Refactor physical Web Audio ownership into `BrowserAudioDriver`.
- [ ] Limit it to permissions, AudioContext, MediaStream, module loading, AudioWorkletNode, MessagePort attachment, lifecycle, and cleanup.
- [ ] Refactor DOM buttons, labels, status attributes, and user-facing permission presentation into a separate presentation adapter.
- [ ] Replace shared transport mutation with `RemoteBackendControl` operations.
- [ ] Replace broad test window events with narrow production diagnostics needed by packaged smoke verification.
- [ ] Ensure driver callbacks carry and validate generations.
- [ ] Make graph shutdown idempotent across denial, failure, retry, stream end, context close, application drop, and superseded startup.
- [ ] Keep application composition responsible only for constructing backend, driver, presentation, MIDI, and application runtime.

Verification:

- [ ] Physical microphone and output-only startup still work.
- [ ] Permission denial, retry, upgrade, suspend, resume, track end, processor failure, and shutdown retain truthful states.
- [ ] Presentation removal or failure cannot corrupt transport or driver state.
- [ ] No DOM type enters the remote backend or physical driver core state.

### Stage 8 — Separate engine processing from scheduling

- [ ] Identify engine fields and branches used only to distinguish dummy elapsed-time mode from physical callback mode.
- [ ] Extract a mode-independent engine runtime with explicit process-quantum entry.
- [ ] Move elapsed-time accumulation, bounded catch-up, and dummy xrun accounting into a local elapsed scheduler/driver.
- [ ] Keep physical AudioWorklet scheduling outside the engine runtime.
- [ ] Provide a local dummy backend wrapper for native and transitional uses.
- [ ] Remove internal dummy-versus-physical mode switches from engine domain behavior.
- [ ] Preserve sample rate, quantum limits, routing, storage preparation, MIDI timing, callback counts, and realtime allocation constraints.

Verification:

- [ ] Existing engine behavior tests pass against the mode-independent runtime.
- [ ] Local elapsed dummy behavior matches the recorded baseline.
- [ ] Explicit quantum processing matches physical-mode behavior for equivalent inputs.
- [ ] Realtime no-allocation and lock-safety tests remain valid.

### Stage 9 — Extract the reusable raw Wasm host bridge

- [ ] Separate raw module instantiation, UTF-8 command transfer, response decoding, memory-view refresh, and process invocation from `AudioWorkletProcessor` registration.
- [ ] Keep the worklet Wasm import-free.
- [ ] Make the physical AudioWorklet adapter use the extracted host bridge.
- [ ] Preserve memory-growth detection, render-growth diagnostics, pointer validation, capacity checks, and fatal-error handling.
- [ ] Define an adapter-neutral host lifecycle with create, command, process, diagnostics, and destroy operations.
- [ ] Add host-bridge contract tests using the actual worklet Wasm artifact.

Verification:

- [ ] Production AudioWorklet behavior is unchanged after adopting the shared bridge.
- [ ] The actual worklet artifact has no unexpected imports.
- [ ] Command and process ABI tests cover malformed data, capacity, memory growth, traps, and shutdown.

### Stage 10 — Implement the production Worker dummy driver

- [ ] Add a Worker adapter using the reusable raw Wasm host bridge.
- [ ] Give it one production application MessagePort and an optional second fixture-control MessagePort.
- [ ] Instantiate isolated engine and scheduler state per Worker.
- [ ] Implement explicit process-quantum control through fixture control.
- [ ] Implement cooperative free-running processing with bounded batches and mandatory event-loop yields.
- [ ] Implement realtime-paced processing with bounded catch-up, discontinuity/xrun reporting, pause, resume, and stop.
- [ ] Add readiness, state, diagnostics, and shutdown messages on the appropriate control surface without changing application envelopes.
- [ ] Add bounded fixture audio staging and output capture outside the production application protocol.
- [ ] Ensure production configuration cannot accidentally enable fixture-only control without an explicitly supplied fixture port.
- [ ] Add Worker asset loading for hosted and self-contained builds.

Verification:

- [ ] All three processing modes pass lifecycle and processing tests.
- [ ] Production application commands are byte-compatible between AudioWorklet and Worker dummy paths.
- [ ] Fixture control cannot be received on the application port.
- [ ] Free-running modes remain responsive to commands and shutdown under load.
- [ ] Worker failures, traps, and termination become typed driver/backend failures.

### Stage 11 — Migrate production browser offline mode

- [ ] Replace direct application-realm `EngineBackend::new_dummy` construction with `RemoteWorkletBackend` plus Worker dummy driver.
- [ ] Use realtime-paced mode for ordinary offline operation.
- [ ] Preserve offline defaults, sample rate, quantum, external-port presentation, scripting, MIDI injection, preview behavior, session behavior, and status messaging.
- [ ] Preserve explicit user selection of offline mode and ensure it requests no physical audio or microphone permission.
- [ ] Route offline startup, readiness, failure, restart, and shutdown through the same remote lifecycle contracts.
- [ ] Remove transitional direct browser dummy code after equivalence evidence passes.

Verification:

- [ ] Browser offline behavior matches the Stage 0 baseline at the application boundary.
- [ ] Offline mode runs engine code only in the Worker realm.
- [ ] Offline mode uses the same production protocol client and journal replay as physical Web Audio.
- [ ] Offline mode starts and stops without AudioContext, MediaStream, or physical permission access.
- [ ] Hosted and self-contained offline artifacts both load Worker and worklet assets successfully.

### Stage 12 — Harden restart, replay, and multi-instance isolation

- [ ] Test and harden physical-driver restart with retained durable state.
- [ ] Test and harden Worker dummy restart in every processing mode.
- [ ] Define behavior for restart during each transfer and structural mutation phase.
- [ ] Verify stable/provisional ID handling across rejected creation and replay.
- [ ] Ensure stale messages and callbacks cannot mutate a replacement instance.
- [ ] Run multiple independent application/backend/worklet compositions concurrently.
- [ ] Verify independent ports, sequences, generations, MIDI bridges, engine state, timers, diagnostics, assets, and teardown.
- [ ] Add leak detection for Workers, MessagePorts, AudioContexts, MediaStreams, nodes, callbacks, timers, and Wasm hosts.

Verification:

- [ ] Restart tests converge without duplicate resources or replaying ephemeral input.
- [ ] Multi-instance tests show no cross-instance messages or state.
- [ ] Repeated create/destroy cycles leave no observable owned resources.
- [ ] Failure of one instance does not stop or corrupt another.

### Stage 13 — Remove transitional architecture and close documentation

- [ ] Remove temporary browser transport adapters and old shared mutable transport access.
- [ ] Remove old browser direct-dummy mode and obsolete mode branches.
- [ ] Remove error-string pending detection and obsolete test event hooks.
- [ ] Remove duplicate conversion, host bridge, lifecycle, and presentation logic.
- [ ] Update architecture, browser audio, offline mode, port model, lifecycle, and troubleshooting documentation.
- [ ] Update dependency-isolation and tracing inventory metadata.
- [ ] Review public names so physical Web Audio terminology is not used for generic remote-worklet components.
- [ ] Confirm the downstream Wasm testing plan's prerequisite assumptions against the completed architecture and revise its implementation details where evidence warrants.

Verification:

- [ ] Repository searches find no obsolete pending strings, direct browser dummy construction, or external transport-field mutation.
- [ ] Documentation names one authority and owner for every driver, transport, remote state, scheduler, and presentation responsibility.
- [ ] The downstream testing project can construct Node.js and browser dummy-worklet fixtures without additional production seams.

### Stage 14 — Final end-to-end validation

- [ ] Run formatting checks.
- [ ] Run warning-denying native workspace builds.
- [ ] Build application and worklet packages for `wasm32-unknown-unknown` in debug and release.
- [ ] Run the complete native Rust suite with required features and backend policy.
- [ ] Run remote client, transport, mutation, desired-state, engine, raw host bridge, physical driver, Worker driver, offline, restart, and multi-instance suites.
- [ ] Run native driver switching and Carla verification.
- [ ] Verify hosted and self-contained physical Web Audio artifacts.
- [ ] Verify hosted and self-contained Worker offline artifacts.
- [ ] Verify microphone, output-only, permission denial/retry, actual AudioWorklet callbacks, offline realtime pacing, explicit dummy processing, cooperative free-running processing, Web MIDI, session replacement, and clean shutdown.
- [ ] Inspect application, remote client, physical driver, Worker, and worklet dependency trees.
- [ ] Run tracing inventory checks and realtime allocation/lock gates.
- [ ] Re-run restart and multi-instance stress from a clean checkout.
- [ ] Compare application-visible behavior and runtime diagnostics with the Stage 0 baseline and document deliberate differences.

Final evidence must demonstrate every immutable acceptance criterion, account for any deliberate compatibility change, and show that no transitional implementation remains.
