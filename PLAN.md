# Per-object state mirrors and asynchronous engine control plan

## How to use this plan

- [ ] Before each implementation session, read `AGENTS.md`, `.agents/index.md`, and every instruction or information document they identify as relevant to the work being done.
- [ ] Treat **Requirements** and **Design principles** below as the agreed contract. The executor may change implementation details and phase boundaries as the code reveals better approaches, but must check with the user before changing a requirement or design principle.
- [ ] Keep this plan and `REMAINING_ISSUES.md` current as implementation discovers additional API cases, risks, or deferred work.
- [ ] Make each phase independently reviewable and leave the tree buildable and tested at its stated verification gate.
- [ ] After completing and verifying each phase, commit that phase and push the branch before beginning the next phase. Do not combine multiple completed phases into one commit or defer their push until the end.
- [ ] Do not add an interim bulk-snapshot polling fix. Move directly toward per-object mirrors and asynchronous handles, even if the migration itself spans several phases.

## Requirements

### State reads

- [ ] Restore per-object state publication for loops, audio channels, MIDI channels, audio ports, MIDI ports, and any other application-facing object whose ordinary state is polled.
- [ ] Make ordinary `get_state()`/polling reads immediate and nonblocking; they must read the latest per-object mirror and may return state that is slightly stale or whose fields came from different processing iterations.
- [ ] Do not provide implicit global read-after-write consistency for ordinary state reads.
- [ ] Do not fall back from a periodic frontend poll to a blocking engine query.
- [ ] Preserve names and other immutable/non-RT-owned metadata in the frontend handle rather than forcing them through atomic state.
- [ ] Implement consumer-resettable peak and event-count semantics without queueing reset commands to the audio thread.
- [ ] Implement data-dirty tracking with a published sequence and frontend-side acknowledgement rather than audio-thread query/reset commands.

### Commands and queue saturation

- [ ] Make normal state-changing operations fire-and-forget: enqueue the command and return without waiting for an audio cycle.
- [ ] Make the low-level nonblocking enqueue API return `Result<CommandSequence, SendError>`; never discard a queue error or silently lose a command.
- [ ] Define `CommandSequence` so successful enqueueing has a stable, testable ordering token and the process side can publish progress where an explicit fence is needed.
- [ ] On `SendError::Full`, make the caller emit a warning and use an explicit blocking-until-space retry path. Blocking for queue capacity must not also wait for command execution.
- [ ] Propagate `SendError::Disconnected` as a real failure rather than retrying forever.
- [ ] Preserve ownership of a rejected command, or structure the API so retry reconstructs it safely; full-queue handling must not drop move-only payloads.
- [ ] Ensure a parked engine can be pumped while a caller waits for queue capacity, so the blocking retry cannot deadlock before a driver starts.
- [ ] Distinguish topology-changing commands from ordinary control commands so gain, mute, position, mode, data reads, and similar operations do not unnecessarily arm graph rebuilding.

### Asynchronous object creation and lifecycle

- [ ] Make loop, channel, session-port, JACK-port association, and FX-chain internal-port creation return stable handles without waiting for the next audio cycle.
- [ ] Back each asynchronous handle with a shared `Arc<ObjectControl>`-style control block containing at least lifecycle (`Pending`, `Ready`, `Failed`, `Closed` or equivalent), resolved engine identity/index, creation command sequence, failure information, and the object's state mirror.
- [ ] Publish successful creation and its engine identity with release/acquire ordering; ordinary independent state fields may use relaxed ordering.
- [ ] Allow commands queued immediately after creation to reference the pending control block. Queue ordering must ensure they resolve the created identity after the creation command runs.
- [ ] Define and test behavior when creation fails, a dependent object fails, a pending handle is dropped, or a command targets a `Failed`/`Closed` object. Do not alias index `0` or another valid object as an error fallback.
- [ ] Use the same pending `Arc<ObjectControl>` approach for asynchronous JACK registration. JACK callback-side registrations must resolve the session index from the control block and safely skip/not expose a port while it is pending or failed.
- [ ] Reject or safely handle references between objects from different backend sessions.

### Complex getters and explicitly deferred efficiency work

- [ ] Keep complex data-returning getters functional without requiring an audio-cycle rendezvous. For now, use shared mutex-protected state/queues where atomics are not suitable.
- [ ] Include at least audio channel `get_data`, MIDI channel `get_all_midi_data`, dummy dequeue operations, and any similar getter found by the API audit.
- [ ] Do not optimize the full-buffer/event copying strategy in this task.
- [ ] Do not attempt a complete RT-safety cleanup of mutexes, allocations, boxed commands, or data preparation unless a minimal change is required to implement this plan.
- [ ] Record every deferred mutex, allocation, copying problem, remaining exceptional blocking query, and newly discovered related issue in `REMAINING_ISSUES.md` with detailed status.

### Frontend, drivers, and correctness boundaries

- [ ] Remove all periodic frontend fallbacks from nonblocking polling to engine queries, including loop, channel, and port update paths.
- [ ] Keep driver/session scalar state that is already safely atomic on an immediate read path.
- [ ] Ensure periodic external connection-state polling does not use one blocking Qt/backend invocation per port. Enumerate/cache asynchronously in bulk and publish cached state to frontend objects.
- [ ] User-triggered external connect/disconnect may remain synchronous if it is outside periodic polling and remains acceptably rare.
- [ ] Make FX-chain port getters return stable owned ports rather than creating another session port on every getter call.
- [ ] Convert `adopt_ringbuffer_contents` to fire-and-forget. Report execution failure asynchronously through logging/status rather than blocking only to return `Result<()>`.
- [ ] Preserve explicit blocking/response behavior only for exceptional operations that genuinely require an exact response and cannot use the temporary mutex-backed design; inventory and document every such exception.

### Documentation and verification

- [ ] Create `REMAINING_ISSUES.md` early and update it during every phase rather than reconstructing deferred issues at the end.
- [ ] Add tests proving polling latency and track/object creation latency do not scale with audio buffer duration.
- [ ] Add tests proving commands are not silently lost under queue saturation and retain FIFO ordering through pending-object creation.
- [ ] Preserve existing behavior unless this contract intentionally changes it; investigate baseline failures separately from regressions introduced by this work.

## Design principles

- [ ] **Poll:** return the latest per-object mirror immediately; stale and cross-field/cross-object skew are acceptable for GUI state.
- [ ] **Command:** enqueue and return a sequence; do not wait for an audio cycle in normal control flow.
- [ ] **Creation:** return a stable pending handle; resolve it on the process side in queue order.
- [ ] **Complex shared data:** use a mutex-backed temporary implementation and document its RT/copying cost.
- [ ] **Exact response/barrier:** make it explicit, rare, and visibly blocking or asynchronous; never hide it in an ordinary getter.
- [ ] **Queue pressure:** return a typed error first; warning and blocking retry are an explicit caller policy, not silent behavior in the nonblocking primitive.
- [ ] **Error visibility:** failed creation, disconnected engines, and execution failures must be observable. Never substitute a plausible index or silently no-op without status/logging.
- [ ] **Ordering:** use the single FIFO command stream and shared control blocks to order creation and dependent commands; do not reintroduce a mutex around the RT-owned `Session`.
- [ ] **Atomic ordering:** use relaxed operations for independent GUI metrics and acquire/release only for lifecycle/identity or another documented synchronization invariant.
- [ ] **RT ownership:** only the engine/process side mutates engine objects. Frontend access is through atomics, pending controls, or the explicitly accepted temporary mutex-backed stores.
- [ ] **Topology:** graph scheduling is armed only by commands whose type can affect topology.
- [ ] **Migration:** do not spend work on a phase-1 snapshot workaround that will be deleted; migrate object families to their final mirrors and controls.
- [ ] **Adaptability:** concrete type layouts, module boundaries, atomics wrappers, retry mechanics, and exact phase splits may change when tests or code structure justify it, while the requirements above remain fixed unless the user approves a change.

## Phased implementation

### Phase 0 — Baseline, complete API inventory, and deferred-issue register

- [x] Re-read the project instructions and relevant build/test documents before changing code.
- [x] Capture the current branch, build state, targeted Rust test state, workspace test state, and frontend/QML self-test state; record known pre-existing failures so they are not confused with regressions.
- [x] Inventory the full application-facing `shoop_engine` API and every frontend call site, classifying each operation as:
  - [x] immediate mirrored scalar/state read;
  - [x] fire-and-forget control command;
  - [x] topology-changing command;
  - [x] asynchronous creation returning a pending handle;
  - [x] complex mutex-backed data read/write;
  - [x] explicit exceptional response/fence;
  - [x] driver/external-manager operation;
  - [x] stub, no-op, or unrelated correctness issue.
- [x] Search for all `query`, `send`, `send_inner`, `poll`, blocking Qt connection, session index, snapshot, peak reset, event reset, and data-dirty call sites; add every result to the inventory.
- [x] Create `REMAINING_ISSUES.md` with one entry per deferred item, including current behavior, why it is deferred, user-visible/RT impact, temporary implementation, affected API/object family, and a concrete future direction.
- [x] Seed `REMAINING_ISSUES.md` with at least:
  - [x] full audio-data copying in `AudioChannel::get_data`;
  - [x] MIDI event/vector allocation and copying in `MidiChannel::get_all_midi_data`;
  - [x] audio and MIDI dummy dequeue storage/copying;
  - [x] JACK callback registered-port mutex, MIDI mutexes, and callback allocations;
  - [x] CPAL callback mutexes, ring/deque locks, temporary vectors, and scratch resizing;
  - [x] arbitrary boxed command allocation/deallocation and allowed command-time RT allocation;
  - [x] graph describe/install response round trips and graph-build allocation status;
  - [x] large data loading/copying on or near the process path;
  - [x] FX/Carla mutex and state serialization behavior;
  - [x] diagnostics/profiling and explicit barriers still requiring responses;
  - [x] known stubs/no-ops such as disconnect, profiling/crash hooks, and state-tracking methods, except where this plan directly implements their required semantics.
- [x] Add or identify test hooks/counters that can detect a blocking query from a periodic GUI path.

### Phase 1 — Reliable command protocol and typed graph effects

- [x] Introduce `CommandSequence` and assign it exactly once for each successfully accepted command.
- [x] Change the low-level enqueue path to return `Result<CommandSequence, SendError>` and remove every ignored queue result.
- [x] Ensure failed enqueue preserves enough ownership to retry payload-bearing commands safely.
- [x] Publish the latest applied command sequence from the process side without locks.
- [x] Add an explicit wait-for-capacity/retry facility that:
  - [x] warns when the queue is full;
  - [x] releases the shared handle mutex while waiting;
  - [x] reclaims completed commands;
  - [x] pumps a parked engine;
  - [x] exits on disconnect/closure;
  - [x] does not wait for the retried command to execute.
- [x] Keep the first attempt nonblocking and place the warning/block/retry policy at application/frontend callers or a clearly named compatibility helper used by those callers.
- [x] Propagate send results through backend handle methods and frontend adapters instead of preserving misleading `Ok(())`/`void` behavior where signatures can be corrected.
- [x] Introduce typed control-vs-topology enqueue paths (or typed commands carrying this property), and audit every mutation's classification.
- [x] Retain an explicit sequence fence/wait primitive for tests and truly exact workflows; do not call it from ordinary state getters or frame-rate updates.
- [x] Test with a deliberately tiny queue:
  - [x] `Full` is returned and logged;
  - [x] retry eventually queues the same logical command once;
  - [x] move-owned payloads survive retry;
  - [x] FIFO order and sequence progression are correct;
  - [x] disconnected behavior terminates cleanly;
  - [x] parked-engine saturation makes progress;
  - [x] ordinary control commands do not arm graph rebuilds.

### Phase 2 — Shared lifecycle/control blocks and loop migration

- [ ] Introduce reusable lifecycle and identity primitives for pending handles, with typed identities where practical to prevent loop/channel/port mix-ups.
- [ ] Define lifecycle transition rules and error storage, including memory-ordering rationale and behavior of dependent commands.
- [ ] Introduce `LoopStateMirror` with atomic fields for mode, length, position, planned mode, and planned delay.
- [ ] Attach the loop mirror/control to the engine loop so the process owner publishes current values at appropriate mutation/cycle points without allocation.
- [ ] Convert `BackendSession::create_loop` to allocate a control block, queue creation, and return a pending `Loop` immediately after successful enqueue.
- [ ] Convert all loop commands to resolve the loop identity from the control block when executed.
- [ ] Convert loop `get_state`/`poll_state` to mirror reads only. Define pending/failed behavior without a blocking fallback.
- [ ] Convert loop setters, transition, clear, sync-source changes, and multi-loop transition to return/propagate queue results and avoid response waits.
- [ ] Convert `adopt_ringbuffer_contents` to enqueue-only and log/publish process-side failure asynchronously.
- [ ] Test:
  - [ ] creation returns while the engine is active with a deliberately long audio period;
  - [ ] a setter queued immediately after creation applies in order;
  - [ ] pending, ready, failed, and closed states are observable and never alias another loop;
  - [ ] sync/multi-loop commands work with pending controls and reject cross-session handles;
  - [ ] repeated state polling performs no query and remains bounded independently of buffer size;
  - [ ] accepted stale read-after-write behavior is explicit in tests.

### Phase 3 — Audio and MIDI channel migration

- [ ] Add audio-channel and MIDI-channel control blocks and mirrors, including parent loop control plus resolved session/local identities needed by engine operations.
- [ ] Convert `Loop::add_audio_channel` and `Loop::add_midi_channel` to return pending stable handles without waiting for indices.
- [ ] Make channel creation commands resolve a pending parent loop in FIFO order and publish `Failed` if parent/channel creation fails.
- [ ] Publish all ordinary audio-channel state atomically: mode, gain, output peak, length, start offset, played-back sample, preplay count, and data sequence.
- [ ] Publish all ordinary MIDI-channel state atomically: mode, triggered-event count, active-note count, length, start offset, played-back sample, preplay count, and data sequence.
- [ ] Implement float atomics through bit representation or another justified lock-free representation.
- [ ] Implement output peak as process-side atomic max plus frontend `swap(0)` consumption; remove peak-reset commands.
- [ ] Implement event-count consumption with atomic accumulation plus frontend `swap(0)` where the API represents events since the previous poll. Keep gauges such as active notes as ordinary loads.
- [ ] Implement data-dirty using a process-published monotonically changing sequence and frontend-side acknowledgement; make clear/reset tracking methods functional without an engine query.
- [ ] Convert channel connect/disconnect, settings, clear, and data-load operations to queued commands that resolve pending channel/port controls.
- [ ] Implement existing disconnect methods if needed for the command API migration; otherwise document their no-op correctness gap in `REMAINING_ISSUES.md` with exact status.
- [ ] Introduce the temporary mutex-backed audio and MIDI content stores needed by `get_data` and `get_all_midi_data`; keep copying behavior simple and document all process-thread locking/allocation consequences.
- [ ] Remove snapshot reads and periodic query fallback for channels.
- [ ] Test pending-parent creation, immediate post-creation configuration, mirror state, peak/event consumption, data-dirty acknowledgement, mutex-backed data round trips, queue saturation, and no-query polling.

### Phase 4 — Audio/MIDI ports, driver registration, and JACK pending controls

- [ ] Add audio-port and MIDI-port controls/mirrors with atomic gain/mute/passthrough/ring-size, peak, event/note, and lifecycle/index state as applicable.
- [ ] Convert driver-port/session-port creation to pending handles and queue engine insertion without waiting for an arena index.
- [ ] Change all channel/port and port/port connection commands to resolve controls at execution time.
- [ ] Replace index-derived compatibility assumptions where pending creation makes them invalid; use stable control/port identity without aliasing.
- [ ] Change JACK registered-port records to hold the same object control rather than a copied session index.
- [ ] Register JACK resources without waiting for engine insertion; make callbacks acquire the resolved index only when ready and safely ignore pending/failed/closed entries.
- [ ] Define cleanup for JACK registration failure, engine creation failure, and either side becoming ready before the other.
- [ ] Apply the pending-control design consistently to audio and MIDI JACK ports.
- [ ] Convert port `get_state`/`poll_state` to mirror-only reads and eliminate reset commands.
- [ ] Implement audio input/output peak consumption and MIDI event-count consumption with atomics.
- [ ] Move dummy output/dequeue data to the temporary mutex-backed shared queues and document their RT/copying costs.
- [ ] Keep decoupled ports that do not belong to the session on an appropriate stable-ID path; do not force them into an arena-index lifecycle if they do not need it.
- [ ] Test:
  - [ ] pending ports can be connected/configured before readiness;
  - [ ] JACK callbacks never use an unresolved/wrong index;
  - [ ] failure and close races are safe;
  - [ ] peaks/event counts consume correctly;
  - [ ] dummy queue/dequeue behavior remains correct;
  - [ ] state polling and port construction do not wait for an audio cycle;
  - [ ] real-JACK integration tests pass when the environment provides JACK.

### Phase 5 — External connection publication and frontend polling

- [ ] Trace the complete frontend polling fan-out for loops, channels, ports, driver state, and external connections.
- [ ] Remove `None => get_state()` and equivalent blocking fallback behavior from all periodic update paths.
- [ ] Define a nonblocking pending-state policy for GUI objects before the first mirrored state is available (retain defaults/last value, expose readiness, or skip the update without querying).
- [ ] Replace per-port blocking Qt/backend connection polling with one asynchronous bulk enumeration per backend/driver at a controlled cadence.
- [ ] Cache connection enumeration on the owning backend thread and publish immutable/cached results for immediate frontend reads.
- [ ] Ensure periodic frontend reads never synchronously enter JACK, the audio thread, or a backend thread through `BLOCKING_QUEUED_CONNECTION`.
- [ ] Keep user-triggered connect/disconnect behavior separate from periodic polling and refresh the cache after mutations.
- [ ] Add query counters/timing assertions and integration tests showing polling cost does not grow by one audio cycle per object or with configured buffer size.

### Phase 6 — FX chains and remaining creation sites

- [ ] Inventory every FX-chain engine object and internal port created from getters or setup code.
- [ ] Create each FX-chain port once, retain its pending stable handle as chain-owned state, and return clones of that handle from getters.
- [ ] Ensure repeated getters do not mutate session topology or create duplicate ports.
- [ ] Use pending controls for FX ports and queue their insertion/topology changes without waiting for an arena index.
- [ ] Keep suitable FX scalar state immediate (atomic or existing non-audio ownership); retain/document mutex use for Carla/plugin state and serialization.
- [ ] Audit any remaining blocking construction query, including driver/plugin sample-rate and buffer-size lookups. Replace ordinary creation waits or document why a truly exact response remains necessary.
- [ ] Test stable port identity, no duplicate topology, pending configuration, unavailable FX behavior, and existing FX integration behavior.

### Phase 7 — Remove obsolete snapshot/query polling infrastructure

- [ ] Confirm through repository-wide search that no ordinary object state getter or periodic frontend path uses `SharedSession::query` or `StateSnapshot`.
- [ ] Remove `queued_at_cycle` and the global snapshot trust/read-after-write mechanism.
- [ ] Remove per-object state fields and snapshot publication plumbing that are no longer used; retain only unrelated mechanisms with demonstrated consumers.
- [ ] Remove peak-reset commands and stale snapshot fallback comments/tests.
- [ ] Narrow `query`/response APIs to named exceptional uses so adding a new blocking call requires an intentional choice.
- [ ] Audit every remaining `query`, wait, mutex, allocation permission, and blocking Qt connection; either justify it as in-scope exceptional behavior or add/update its detailed `REMAINING_ISSUES.md` entry.
- [ ] Audit every application-facing command to confirm its enqueue error is handled and every topology classification is correct.
- [ ] Audit all lifecycle controls for leaked pending objects, stale registrations, invalid indices, and cross-session references.

### Phase 8 — Final verification and documentation

- [ ] Run formatting and warning-clean build steps required by the current project instructions.
- [ ] Run targeted `shoop_engine` tests while iterating, then the documented full Rust workspace test command.
- [ ] Build the application and run the documented frontend/QML self-test suite, comparing with the captured baseline.
- [ ] Run real-JACK tests when JACK is available; clearly report an environmental skip rather than treating it as a pass.
- [ ] Exercise several buffer sizes, including deliberately large periods, and verify:
  - [ ] frame-rate state polling remains responsive;
  - [ ] creating loops/tracks/channels/ports returns promptly with pending handles;
  - [ ] immediate follow-up commands eventually apply in FIFO order;
  - [ ] no periodic path performs an audio-cycle query;
  - [ ] queue-full warning/retry behavior is visible and lossless.
- [ ] Add stress tests for many objects, rapid command bursts, concurrent frontend producers, creation failure, shutdown with pending work, and repeated handle cloning/dropping.
- [ ] Review `REMAINING_ISSUES.md` against the final repository-wide audit and ensure each deferred item states what remains, current risk, temporary workaround, and suggested future solution.
- [ ] Update architecture documentation to describe the final contract: mirrored stale reads, sequenced fire-and-forget commands, pending stable handles, temporary mutex-backed complex data, and explicit exceptional barriers.
- [ ] Report any behavior changes, remaining known failures, environment-limited tests, and deferred RT/copying work to the user.

## Completion criteria

- [ ] Ordinary loop/channel/port state reads are per-object mirror reads and cannot wait for an audio cycle.
- [ ] Periodic GUI polling contains no fallback to a blocking engine/backend/Qt round trip.
- [ ] Normal state-changing methods queue work, return/propagate a command sequence or typed enqueue failure, and never silently drop commands.
- [ ] Queue saturation is covered by warning plus explicit blocking-for-space retry and lossless tests.
- [ ] Loops, channels, ports, JACK associations, and FX internal ports use pending stable controls and do not block creation on engine indices.
- [ ] Complex data getters use the agreed temporary mutex-backed implementation and their efficiency/RT limitations are documented.
- [ ] Peaks, event counts, and data-dirty state use lock-free consume/acknowledgement semantics without reset commands.
- [ ] Graph rebuilding is armed only for topology-affecting commands.
- [ ] FX port getters are stable and idempotent.
- [ ] `REMAINING_ISSUES.md` is complete for known mutexes, allocations, copying, exceptional waits, stubs, and discovered deferred work.
- [ ] Automated tests and buffer-size exercises demonstrate that UI polling and object creation latency no longer scale with audio-cycle duration.
