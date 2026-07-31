# Engine application API inventory

This inventory is the Phase 0 classification for the per-object state mirror migration. It covers the public application API in `app_backend`, the engine boundary primitives it uses, and periodic frontend callers. Update it when a method changes category or a new application-facing method is added.

## Baseline

Captured on branch `per-object-state-mirrors` at `c9f0c0d6`.

| Gate | Command | Result |
|---|---|---|
| Targeted engine suite | `cargo test -p shoop_engine --features app_backend` | Expected environmental failure: all tests passed except the three `midir_driver` tests, because `/dev/snd/seq` is unavailable. |
| Workspace Rust suite | `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo test --workspace --features shoop_engine/app_backend` | Pass. The three virtual-MIDI tests reported unavailable ALSA and were accepted by the documented environment override. |
| Frontend/QML suite | `SHOOP_ALLOW_MISSING_BACKENDS=1 target/debug/shoopdaloop_dev.sh --self-test` | Pass: 188 passed, 0 failed, 1 skipped (`CpalPorts`, no configured device). |
| JACK application tests | Included in both Rust invocations | Pass: 4 passed against the available JACK backend. |

The target suite's only failures are environmental and match the known sandbox limitation. The full suite with the explicit missing-backend override establishes the behavioral baseline for this branch.

## Migration status

### Phase 1

- Engine commands carry monotonic `CommandSequence` values and publish the newest applied sequence through `Stats`.
- The low-level enqueue API returns `Result<CommandSequence, SendError>` and reports `Disconnected` after engine destruction.
- Application enqueueing reserves capacity before moving payloads. A full queue is warned about, waited on without holding the shared handle mutex, and retried; parked engines are pumped to make progress.
- Application mutations are classified through `send_control`/`send_topology` and `query_control`/`query_topology`.
- Backend methods return queue results where practical; compatibility adapters explicitly report failures.
- An explicit command fence is available for tests and exact workflows.
- Verification: `SHOOP_ALLOW_MISSING_BACKENDS=1 RUSTFLAGS='-D warnings' cargo test -p shoop_engine --features app_backend` passed, including 542 library tests, 4 live-JACK tests, command saturation/sequence tests, and all integration/no-allocation tests. One earlier parallel library run hit the separately documented intermittent Carla/JUCE teardown crash; its immediate rerun and the final phase gate passed.

### Phase 2

- `ObjectControl<I, M>` provides typed identity, backend-session identity, lifecycle, creation sequence, failure text, and an `Arc` state mirror.
- Lifecycle starts `Pending` with an invalid index. Creation stores the typed index, then publishes `Ready` with release ordering; readers acquire lifecycle before loading identity. `Failed` publishes error text before the release store. `Closed` makes identity unavailable. Dependent commands only resolve `Ready` identities, so pending/failed/closed targets cannot alias index zero.
- A pending handle dropped before any dependent command exists cancels creation through a weak command reference. A dependent queued command intentionally keeps the control alive until FIFO work drains. Dropping a ready handle does not yet remove the engine object because the application API has no loop-removal lifecycle; that remains a separate ownership/removal design concern.
- `LoopStateMirror` publishes mode, length, position, next mode, and next delay through independent relaxed atomics. `Loop::get_state` and `poll_state` read only this mirror; `Pending` polls return `None` and direct reads return an immediate error.
- Loop creation returns after queue admission and all loop commands resolve the pending control in FIFO order. Loop relationships reject backend-session mismatches.
- Ringbuffer adoption is now enqueue-only and reports execution failure from the process side.
- Verification: the serialized warning-clean engine gate passes 550 library tests plus all engine integration, live-JACK, and no-allocation tests. Parallel Carla/JUCE teardown remains intermittently unsafe as documented.

## Boundary primitives

| Primitive | Current role | Target category |
|---|---|---|
| `SharedSession::send` | Queues any mutation, silently ignores queue failure, arms graph scheduler unconditionally | Nonblocking typed enqueue returning `Result<CommandSequence, SendError>`; topology effect explicit |
| `SharedSession::send_inner` | Queues without graph scheduling, silently ignores queue failure | Internal non-topology enqueue with the same lossless result contract |
| `SharedSession::query` | Queues and waits; also used for creation and mutations needing an error | Exceptional explicit response only |
| `SharedSession::query_inner` | Scheduler responses and parked-engine direct access | Exceptional scheduler/diagnostic response only |
| `SharedSession::poll` | Reads bulk snapshots subject to global `queued_at_cycle` trust | Remove after per-object mirror migration |
| `SharedSession::drain_queue` | Blocking control barrier used by driver tests/control | Keep as an explicit barrier, not an ordinary API read |
| `EngineHandle::send` | Bounded nonblocking queue; returns `Result<(), SendError>` but loses rejected command ownership through mapping | Sequenced, lossless nonblocking enqueue |
| `EngineHandle::send_for_result` / `wait_for_result` | Exceptional command-response path | Retain narrowly and inventory every caller |
| `StateSnapshot` publication | Bulk loop/channel/port GUI state | Remove after all object mirrors are live |

## Backend session and driver

| API | Current behavior | Classification / target |
|---|---|---|
| `BackendSession::new/create` | Creates parked engine and control handle | Setup; may allocate and block only on local work |
| `set_audio_driver` | Attaches driver, flushes graph, activates backend | Exceptional setup barrier; not periodic |
| `get_state` | Returns mostly placeholder values | Immediate metadata/stub; correctness gap tracked |
| `create_loop` | Blocking query for arena index | Asynchronous creation with pending `Loop` control |
| `create_fx_chain` | Host setup on caller; some queried configuration and queued insertion | Mixed setup/asynchronous creation; exact configuration query audited in Phase 6 |
| `get_profiling_report` | Stub | Diagnostic correctness issue, deferred |
| `segfault_on_process_thread`, `abort_on_process_thread` | No-op stubs | Diagnostic command correctness issue, deferred |
| `AudioDriver::new/start` | Driver setup | Exceptional setup, outside periodic object state |
| `get_sample_rate`, `get_buffer_size`, `active`, `get_state` | Driver-owned values/atomics | Immediate driver state |
| `wait_process` | Explicitly drains queue and flushes graph | Explicit barrier for tests/exact workflows |
| dummy mode/request/wait methods | Driver test control; some explicitly wait | Explicit test-driver operations, not GUI polling |
| dummy external mock-port mutation | Driver manager mutation | Driver-side command/setup; retain explicit semantics |
| `find_external_ports` | Synchronous manager/JACK enumeration | Replace periodic use with backend-level asynchronous bulk cache |

## Loops

| API | Current behavior | Classification / target |
|---|---|---|
| `add_audio_channel`, `add_midi_channel` | Blocking query for session and local indices | Asynchronous creation with pending channel control |
| `transition` | Fire-and-forget, queue error ignored | Sequenced control command |
| `get_state`, `poll_state` | Query or globally trusted bulk snapshot | Immediate `LoopStateMirror` read only |
| `set_length`, `set_position`, `clear` | Fire-and-forget, queue error ignored | Sequenced control commands |
| `set_sync_source` | Fire-and-forget topology-relevant relationship | Sequenced topology command using controls |
| `adopt_ringbuffer_contents` | Blocking query solely to return operation error | Fire-and-forget control command; asynchronous error logging/status |
| `transition_multiple_loops` | Calls one command per loop | Sequenced multi-object control; preserve order and report saturation |

## Audio channels

| API | Current behavior | Classification / target |
|---|---|---|
| `connect_input`, `connect_output` | Fire-and-forget by indices | Sequenced topology commands resolving pending controls |
| `disconnect` | No-op | Topology command correctness gap to implement or retain in remaining issues |
| `load_data` | Copies on caller, queues process-side load | Sequenced control with owned payload; deferred RT copy details |
| `get_data` | Blocking query and complete process-thread copy | Temporary mutex-backed complex getter; copying optimization deferred |
| `get_state`, `poll_state` | Query/snapshot; both reset peak through commands or direct mutation | Immediate atomic mirror with consumer-reset peak |
| gain/mode/offset/preplay setters | Fire-and-forget, queue errors ignored | Sequenced control commands |
| `clear_data_dirty` | No-op | Frontend acknowledgement of mirrored data sequence |
| `clear` | Fire-and-forget | Sequenced control command |

## MIDI channels

| API | Current behavior | Classification / target |
|---|---|---|
| `get_all_midi_data` | Blocking query; allocates/copies vectors and messages on process thread | Temporary mutex-backed complex getter; allocation/copy optimization deferred |
| `load_all_midi_data` | Prepares some vectors on caller, queues storage update | Sequenced control with owned payload; remaining process costs documented |
| `connect_input`, `connect_output` | Fire-and-forget by indices | Sequenced topology commands resolving pending controls |
| `disconnect` | No-op | Topology command correctness gap to implement or retain in remaining issues |
| `get_state`, `poll_state` | Query/snapshot | Immediate atomic mirror with consumer-reset event count and state gauges |
| mode/offset/preplay setters and `clear` | Fire-and-forget, queue errors ignored | Sequenced control commands |
| `clear_data_dirty` | No-op | Frontend acknowledgement of mirrored data sequence |
| `reset_state_tracking` | No-op | Command or local acknowledgement depending engine semantics; correctness gap tracked until implemented |

## Audio ports

| API | Current behavior | Classification / target |
|---|---|---|
| `new_driver_port` | Blocking query for session index, then synchronous driver/JACK registration | Asynchronous session creation and pending JACK association through one control |
| connectability and `direction` | Handle metadata | Immediate immutable read |
| `get_state`, `poll_state` | Query/snapshot and queued peak resets | Immediate atomic mirror with consumer-reset input/output peaks |
| gain/mute/passthrough/ring-size setters | Fire-and-forget, queue errors ignored | Sequenced control commands |
| `connect_internal` | Fire-and-forget by indices | Sequenced topology command resolving controls |
| `dummy_queue_data`, `dummy_request_data` | Fire-and-forget | Sequenced driver-test control |
| `dummy_dequeue_data` | Blocking query returning copied vector | Temporary mutex-backed complex dequeue |
| `get_connections_state` | Synchronous JACK/external manager query per port | Immediate read from asynchronously refreshed backend connection cache |
| external connect/disconnect | Synchronous JACK/external manager operation | Rare user-triggered driver action; allowed to remain synchronous |

## MIDI ports

| API | Current behavior | Classification / target |
|---|---|---|
| `new_driver_port` | Blocking query for session index, then synchronous driver/JACK registration | Asynchronous session creation and pending JACK association through one control |
| connectability and `direction` | Handle metadata | Immediate immutable read |
| `get_state`, `poll_state` | Query/snapshot | Immediate atomic mirror with consumer-reset event counts |
| mute/passthrough/ring-size setters | Fire-and-forget, queue errors ignored | Sequenced control commands |
| `connect_internal` | Fire-and-forget by indices | Sequenced topology command resolving controls |
| dummy clear/queue/request methods | Fire-and-forget | Sequenced driver-test controls |
| `dummy_dequeue_data` | Blocking query and event/data allocation | Temporary mutex-backed complex dequeue |
| `get_connections_state` | Synchronous JACK/external manager query per port | Immediate read from asynchronous backend connection cache |
| external connect/disconnect | Synchronous JACK/external manager operation | Rare user-triggered driver action; allowed to remain synchronous |

## Decoupled MIDI ports

| API | Current behavior | Classification / target |
|---|---|---|
| `new_driver_port` | Stable monotonic `PortId`; driver registration | Keep stable-ID path; not a session pending object |
| `maybe_next_message`, `name` | Lock-free queue/immutable metadata | Immediate read |
| `send_midi` | Pushes to bounded queue, currently ignores result | Driver-side enqueue; loss/error visibility should be audited |
| connection-state and external connect/disconnect | External manager operations | Cache periodic reads; rare user actions may remain synchronous |

## FX chains

| API | Current behavior | Classification / target |
|---|---|---|
| `available` | Frontend-owned enum check | Immediate read |
| `set_visible` | Mutex/Carla call | Plugin-owned control; document blocking/RT limitations |
| `set_active` | Frontend mutex plus queued test-chain command or Carla call | Sequenced control for engine chain; plugin call remains documented |
| `get_state` | Mutex and optional Carla query | Immediate frontend-owned state with documented plugin mutex |
| state string save/restore | Synchronous plugin serialization | Exceptional complex response/setup |
| audio/MIDI port getters | Create a new session port through a blocking query on every call | Stable chain-owned pending ports created once |

## Periodic frontend paths

| Caller | Current behavior | Target |
|---|---|---|
| `AnyBackendChannel::poll_state` | Falls back to blocking `get_state` | Mirror-only result; never query |
| `AnyBackendPort::poll_state` | Falls back to blocking `get_state` | Mirror-only result; never query |
| loop backend update | Falls back to blocking loop `get_state` | Mirror-only result; pending retains/defaults/skips |
| channel backend update | Calls `AnyBackendChannel::poll_state` | Mirror-only after adapter conversion |
| port backend update | Calls `AnyBackendPort::poll_state` | Mirror-only after adapter conversion |
| port GUI connection updates | Several blocking queued Qt invocations | Backend-level asynchronous bulk enumeration and cached publication |
| other blocking queued invocations | File I/O, explicit GUI actions, FX/plugin and channel data workflows | Audit by purpose; periodic connection/state calls are in scope, explicit task/file operations remain exceptional |

## Remaining explicit response callers

- Graph scheduler topology description and schedule installation.
- Graph-current assertions and exact ordering checks in tests.
- Driver `wait_process` queue drain and graph flush.
- Plugin configuration/state serialization where the plugin host owns the data.
- Any complex getter not yet converted to the temporary mutex-backed design.

Each remaining caller must be re-audited in Phase 7. Ordinary state and object construction are not valid reasons to remain on this list.

## Verification hooks

- An active engine that is deliberately not cycled makes any accidental `query` take the response timeout; mirror and pending-handle tests can prove they return promptly without relying only on wall-clock behavior in normal cycles.
- `EngineHandle::stats().commands_applied` and the command queue's pending count expose command progress for ordering tests.
- `CommandSequence` and the applied-sequence atomic added in Phase 1 will provide exact FIFO/fence evidence.
- Repository-wide searches for `query`, snapshot fields, `None => get_state`, and blocking Qt connection types are required gates in Phase 7.
- Buffer-size integration tests provide end-to-end evidence that API latency no longer tracks cycle duration.
