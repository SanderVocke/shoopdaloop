# Remaining engine/control issues

This document tracks work intentionally deferred from the per-object state mirror and asynchronous control migration. Every entry records the current status, temporary behavior, impact, and a future direction. Update entries when implementation changes their status, and add newly discovered mutexes, allocations, copying, exceptional waits, or correctness gaps.

## Status vocabulary

- **Deferred**: explicitly outside the current migration's efficiency/RT scope.
- **Temporary implementation**: required for behavior in the current migration, but deliberately not the final efficient design.
- **Open correctness gap**: behavior is stubbed, incomplete, or silently ignores failure and needs separate work unless the migration directly depends on it.
- **Exceptional response**: a blocking/response path remains intentionally; it must not be used by periodic UI polling.

## Complex data getters and copying

### Audio channel full-data retrieval

- **Status:** Temporary implementation.
- **API:** `AudioChannel::get_data` and save/display-data callers.
- **Current behavior:** Application-backed channels opt into a shared `Mutex<Vec<f32>>` mirror. Recording writes into it from `AudioChannel::finalize_process`, and mutations replace it. `get_data` clones it immediately without an engine command or cycle wait.
- **Impact:** The process thread can contend on the mutex. Mirror growth may allocate in `Session::channel_finalize` (explicitly covered by `realtime_allow_alloc_once!`), and every getter copies the full recording while holding the lock. The ordinary non-application core path leaves complex mirroring disabled so established allocation-free processing remains intact.
- **Why deferred:** Efficient immutable chunk snapshots, ownership swaps, or RCU storage require a separate storage redesign.
- **Future direction:** Publish an `Arc` snapshot of immutable/chunked sample storage and perform contiguous copying/serialization on a worker thread.

### MIDI channel full-data retrieval

- **Status:** Temporary implementation.
- **API:** `MidiChannel::get_all_midi_data` and MIDI file save/display callers.
- **Current behavior:** Application-backed channels opt into a shared `Mutex<Vec<MidiEvent>>` mirror. Content changes rebuild a complete vector, clone each payload on the process thread, and replace the mirrored value. The getter immediately clones the mirrored vector without an engine command.
- **Impact:** Process-side locking and allocation remain possible, and every publication/read copies all events and byte payloads. Application processing relies on the existing exceptional command/process allocation permission; ordinary core channels leave complex mirroring disabled.
- **Why deferred:** A zero/low-copy immutable MIDI snapshot format is outside the current task.
- **Future direction:** Publish immutable storage generations and materialize API/file formats on a worker.

### Dummy audio dequeue

- **Status:** Temporary implementation.
- **API:** `AudioPort::dummy_dequeue_data`.
- **Current behavior:** Application driver ports share an `Arc<Mutex<Vec<f32>>>` capture store with `ExternalAudioPort`; dequeue drains it immediately without an engine query.
- **Impact:** The process path locks the store and may grow/drain its vector under the explicit realtime allocation exception. The test/control thread can block processing while dequeuing.
- **Future direction:** Use a bounded SPSC capture ring with ownership transfer or preallocated output buffers.

### Dummy MIDI dequeue

- **Status:** Temporary implementation.
- **API:** `MidiPort::dummy_dequeue_data`.
- **Current behavior:** Application driver ports share an `Arc<Mutex<Vec<MidiEvent>>>`; process-side capture clones each payload into it and dequeue drains it immediately.
- **Impact:** Process-side locking, per-event allocation/copying, and frontend/process contention remain. This path is intentionally exceptional and not used by normal JACK output delivery.
- **Future direction:** Use a bounded SPSC event ring with fixed-size payload storage and worker-side conversion.

## Process callback mutexes and allocations

### JACK registered-port registry

- **Status:** Deferred except for pending-control identity changes required by the migration.
- **Current behavior:** JACK callback processing locks `Arc<Mutex<Vec<JackRegisteredPort>>>` and walks registrations. Session-backed registration records now retain the shared audio/MIDI `ObjectControl` and resolve its index only when `Ready`; pending/failed/closed controls are safely ignored.
- **Migration behavior:** Pending-control identity is implemented, but the registry mutex itself remains.
- **Impact:** Registration/control activity can block the JACK callback; poisoned-lock recovery also occurs on the RT path.
- **Future direction:** Publish immutable/RCU registration arrays or use lock-free registration handoff outside the callback.

### JACK MIDI buffers and decoupled queues

- **Status:** Deferred.
- **Current behavior:** Decoupled MIDI paths use mutex-protected `Vec<MidiEvent>` and callback paths construct/clone MIDI vectors and byte data.
- **Impact:** Callback mutex contention and allocation can cause xruns under load.
- **Future direction:** Preallocated bounded SPSC MIDI rings with fixed payload limits and callback-local scratch.

### CPAL capture and connection state

- **Status:** Deferred except that periodic frontend connection reads must become cached/nonblocking.
- **Current behavior:** CPAL callbacks lock a `VecDeque` capture ring, external connection state, MIDI input/output collections, and decoupled-port vectors.
- **Impact:** Any holder can delay audio callbacks; connection management and callback processing share locks.
- **Future direction:** Split control-owned configuration from immutable callback snapshots and replace capture with lock-free SPSC rings.

### CPAL scratch and temporary allocations

- **Status:** Deferred.
- **Current behavior:** Callback paths resize scratch vectors and construct temporary audio/MIDI vectors.
- **Impact:** Allocation or resizing can introduce unbounded callback latency.
- **Future direction:** Size all scratch at stream activation and reject/reconfigure outside the callback when device shape changes.

### Plugin/Carla locks

- **Status:** Deferred.
- **API:** FX visibility, activity, state polling, state serialization/restoration, and processing host ownership.
- **Current behavior:** Carla host operations are protected by mutexes; some GUI operations synchronously enter the plugin host.
- **Impact:** Plugin/UI calls can block one another and plugin behavior determines latency.
- **Future direction:** Dedicated plugin-control thread with asynchronous commands and immutable state publication, while keeping process data RT-owned.

## Command execution and payload preparation

### Boxed closure lifecycle

- **Status:** Deferred.
- **Current behavior:** Every engine command is a boxed dynamic `FnMut`. Allocation occurs on the producer; executed boxes return through a second queue so deallocation occurs off the process thread.
- **Impact:** Producer allocation remains, command variants are not statically classifiable, and exceptional execution is wrapped by an allocation permission.
- **Future direction:** A typed, preallocated command enum/pool whose graph effect and payload ownership are intrinsic.

### Exceptional command-time allocations

- **Status:** Deferred.
- **Current behavior:** `Engine::apply_commands` permits exceptional allocation around arbitrary command execution. Creation, topology description, loading, and response getters may allocate.
- **Impact:** Topology/data operations can allocate on the process thread and cause xruns even though steady-state tests are allocation-free.
- **Future direction:** Prepare all structures off-thread, reserve stable IDs/capacity, and queue ownership swaps or fixed-size operations only.

### Audio load copying

- **Status:** Deferred.
- **API:** `AudioChannel::load_data`.
- **Current behavior:** Input is copied to an owned vector on the caller, then channel storage copies it again during command execution.
- **Impact:** Large loads perform process-thread memory work and may need storage growth.
- **Future direction:** Build immutable/chunked storage off-thread and atomically install ownership.

### MIDI load preparation and installation

- **Status:** Deferred.
- **API:** `MidiChannel::load_all_midi_data`.
- **Current behavior:** Much of the event conversion is prepared on the caller, but storage replacement and possible internal work occur in the command.
- **Impact:** Large sessions can spend significant process time installing MIDI contents.
- **Future direction:** Prepare a complete storage object off-thread and queue a cheap swap.

### Ringbuffer adoption failure reporting

- **Status:** Fire-and-forget migration implemented; structured completion deferred.
- **API:** `Loop::adopt_ringbuffer_contents`.
- **Current behavior:** Queues the operation and returns its command sequence; process-side execution failure is logged.
- **Impact:** Callers no longer receive synchronous success. There is no structured completion object yet.
- **Future direction:** Optional asynchronous operation status/receiver if UI workflows need completion feedback.

## Graph scheduling and exact responses

### Topology description and schedule installation round trips

- **Status:** Exceptional response retained for now.
- **Current behavior:** The graph scheduler worker queues a topology description response, builds off-thread, then queues schedule installation and waits for the displaced schedule so it is freed off the process thread.
- **Impact:** Two process-thread rendezvous occur per graph rebuild. They are off the GUI thread but still consume command capacity and can delay topology convergence.
- **Why retained:** Correct ownership, coalescing, and off-RT destruction are already encoded in this path; replacing it is not required for nonblocking GUI state/creation.
- **Future direction:** Versioned topology publication plus asynchronous prepared-schedule install/acknowledgement.

### Explicit fences and driver settling

- **Status:** Exceptional response/barrier retained.
- **API:** command-sequence fence, `AudioDriver::wait_process`, queue drain, and graph flush.
- **Current behavior:** Tests and exact dummy-driver workflows wait until queued control and graph work settle.
- **Impact:** These APIs intentionally block and must not enter periodic frontend paths.
- **Future direction:** Keep explicit; add asynchronous completion variants only where application workflows need them.

### Plugin configuration publication

- **Status:** Immediate publication implemented; late driver reconfiguration remains deferred.
- **Current behavior:** `SharedSession` publishes driver sample rate and buffer size atomically when a driver is attached; Carla creation reads those values without an engine query. Defaults are 48 kHz/256 before attachment.
- **Impact:** Attaching a not-yet-started driver and configuring it later does not currently republish its eventual values, so chains created in that ordering use defaults.
- **Future direction:** Have every driver configuration/start event update a driver-owned immutable configuration generation and make plugin reconfiguration explicit.

## External connection management

### Cached external/JACK enumeration

- **Status:** Temporary polling implementation.
- **Current behavior:** Session-backed ports register cache requests and return the latest connection map immediately. At most once per 100 ms, a worker takes one backend lock and bulk-enumerates every registered request, then replaces the cache. The engine-update thread forwards cached maps to `PortGui`; GUI getters no longer make blocking queued calls. Decoupled MIDI ports use the same immediate-cache policy but currently refresh per decoupled port rather than joining the session bulk.
- **Impact:** Enumeration no longer blocks frame polling, but worker threads still synchronously lock JACK/CPAL dummy connection managers. Decoupled ports can cause more than one enumeration per cadence, and cached values may lag mutations by roughly one refresh interval.
- **Future direction:** Use JACK graph callbacks/backend events to publish one driver-owned immutable connection graph shared by session and decoupled ports.

### User-triggered external connect/disconnect

- **Status:** Allowed exceptional synchronous operation.
- **Current behavior:** Calls JACK/external manager synchronously and mostly ignores backend errors.
- **Impact:** A user action can hitch; failures may be underreported.
- **Future direction:** Asynchronous driver command with explicit completion/error and cache refresh.

### Decoupled MIDI send saturation

- **Status:** Open error-visibility issue.
- **API:** `DecoupledMidiPort::send_midi` and some callback-side queue pushes.
- **Current behavior:** Some bounded queue push results are ignored; other decoupled paths count drops.
- **Impact:** MIDI messages can be silently lost under saturation.
- **Future direction:** Align with typed queue errors/drop counters and define whether UI MIDI control retries or reports loss.

## Object ownership and removal

### Ready handle drop does not remove engine objects

- **Status:** Deferred ownership/lifecycle design.
- **Current behavior:** Dropping an unreferenced pending loop/channel/port cancels creation through the command's weak control reference. JACK registrations intentionally retain their port control, so a registered pending port remains alive until driver teardown. Once ready, dropping the final ordinary frontend handle does not remove its session object; queued dependent commands intentionally keep controls alive until FIFO work drains.
- **Impact:** Dynamic object removal can leave unreachable session objects, and a JACK registration whose engine insertion never runs remains pending (but callback-safe) until the driver registry is destroyed.
- **Future direction:** Add explicit close/remove commands, publish `Closed`, detach topology safely, and define whether final-handle drop requests removal or ownership remains session-level.

## Diagnostics and incomplete API behavior

### Backend session state

- **Status:** Open correctness gap.
- **API:** `BackendSession::get_state`.
- **Current behavior:** Returns a null driver pointer and zero buffer counts regardless of live state.
- **Impact:** Callers cannot rely on most reported fields.
- **Future direction:** Publish meaningful driver/buffer counters from existing driver and engine atomics.

### Profiling report

- **Status:** Open correctness gap.
- **API:** `BackendSession::get_profiling_report`.
- **Current behavior:** Always returns `ProfilingReport::default()`.
- **Impact:** Frontend profiling UI can appear functional while reporting no backend data.
- **Future direction:** Publish immutable latest/worst profiling snapshots outside the process thread.

### Crash hooks

- **Status:** Open correctness gap.
- **API:** `segfault_on_process_thread`, `abort_on_process_thread`.
- **Current behavior:** No-op.
- **Impact:** Diagnostic/testing behavior promised by the API is absent.
- **Future direction:** Explicit diagnostic commands guarded for test/development builds.


### Ignored operation errors

- **Status:** Open audit item.
- **Current behavior:** Many process-side session calls use `let _ = ...`, while fire-and-forget methods return success or nothing.
- **Impact:** Invalid references or backend failures can become silent no-ops.
- **Migration direction:** Queue admission is always reported synchronously; creation failure enters lifecycle `Failed`; other execution failures are logged/published asynchronously.
- **Future direction:** Per-object operation error sequence/status or optional completion receivers for user-visible operations.

## Test/runtime stability

### Carla/JUCE parallel teardown assertion and crash

- **Status:** Intermittent test-runtime issue; not attributed to the command protocol change.
- **Evidence:** One Phase 1 targeted run reached the known Carla/JUCE `numScopedInitInstances`/message-manager teardown assertions and exited with `SIGSEGV`; an immediate identical `--lib` rerun passed all 542 tests. The baseline also printed the assertions without crashing.
- **Impact:** A single parallel test run is not reliable evidence of a regression or a clean gate around Carla host teardown.
- **Verification policy:** Rerun the same gate, preserve the output, and use a serialized Carla-focused run if the crash repeats. Do not hide deterministic failures under this entry.
- **Future direction:** Serialize global Carla/JUCE initialization tests or provide one process-wide test host lifecycle.

## Baseline environmental limitations

### ALSA sequencer unavailable

- **Status:** Environment limitation, not a product failure.
- **Evidence:** `/dev/snd/seq` is unavailable; the three `midir_driver` tests fail without the explicit missing-backend override and pass/skip policy with `SHOOP_ALLOW_MISSING_BACKENDS=1`.
- **Verification policy:** Run the workspace suite with the override in this sandbox, and run without it in an environment with virtual MIDI/JACK before final release where possible.

### CPAL device unavailable

- **Status:** Environment limitation.
- **Evidence:** QML baseline passes 188 tests and skips `CpalPorts::test_virtual_playback_ports_are_app_connectable` because CPAL settings/device are unavailable.
- **Verification policy:** Keep mock CPAL Rust coverage and report the QML skip explicitly; run device integration when available.
