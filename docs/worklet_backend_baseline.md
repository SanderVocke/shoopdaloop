# Worklet backend refactoring baseline

This baseline records the production ownership and behavior immediately before the refactoring governed by `worklet_backend_refactoring_plan.md`. The source revision is `11cb0a02c572cddc7de4ad78e8594b1f8ea5b2c4`.

## Browser ownership

### Physical Web Audio

```text
browser composition (`shoopdaloop::Runtime`)
  ├─ BrowserMidiController / BrowserMidiHub
  ├─ CooperativeApplicationRuntime
  │    └─ WebAudioBackend (`dyn Backend`)
  │         ├─ authoritative and optimistic BackendSnapshot fields
  │         ├─ stable-ID/resource bookkeeping
  │         ├─ waveform, MIDI, session, and loop-content assemblies
  │         ├─ concrete BrowserMidiHub
  │         └─ Rc<RefCell<Transport>>
  └─ BrowserAudioController
       ├─ permission and DOM presentation
       ├─ AudioContext / MediaStream / source / AudioWorkletNode
       ├─ MessagePort and JavaScript callbacks
       └─ the same Rc<RefCell<Transport>>

AudioWorkletProcessor JavaScript
  └─ WorkletHost Wasm
       └─ EngineBackend::new_web_audio
            └─ Session::process called by the physical render callback
```

The application runtime sees only `dyn Backend`. Browser application composition nevertheless owns both lower-level halves and passes shared mutable transport state between them.

### Browser offline mode

```text
browser composition (`shoopdaloop::Runtime`)
  ├─ BrowserMidiController
  └─ CooperativeApplicationRuntime
       └─ EngineBackend::new_dummy(48_000, 256)
            ├─ engine Session in the application/main realm
            └─ elapsed-time scheduling from Backend::advance
```

Offline mode does not create an `AudioContext`, `MediaStream`, `AudioWorkletNode`, `Worker`, or production protocol connection. Each application tick converts elapsed nanoseconds to frames, processes at most eight configured quanta, discards excess elapsed debt, and increments one xrun.

## Current responsibility inventory and destinations

| Current responsibility | Current owner | Destination |
| --- | --- | --- |
| Backend/domain API | `shoop_backend::Backend` | Retained application boundary |
| Remote resource IDs and snapshots | `WebAudioBackend` | `shoop_worklet_client::RemoteWorkletBackend` |
| Wire/domain conversion | `browser_audio.rs` | `shoop_worklet_client` |
| Command journal, sequence, inbound queue, overflow and error slot | `Transport` | remote client transport core |
| Concrete `MessagePort` posting and callbacks | `Transport` / `BrowserAudioController` | browser message-endpoint adapter |
| Driver lifecycle and generations | `BrowserAudioController` plus `Transport` fields | `BrowserAudioDriver` through restricted control |
| Permission and DOM state | `BrowserAudioController` | browser presentation adapter |
| Web Audio graph and assets | `BrowserAudioController` | `BrowserAudioDriver` |
| Track Web MIDI drain/send | `WebAudioBackend` with concrete `BrowserMidiHub` | injected `HostMidiBridge` |
| Scripting MIDI service | browser composition | remains separately owned |
| Engine command host | `shoop_audio_worklet::WorkletHost` | reusable remote engine host |
| Raw Wasm memory and ABI | `audio_worklet.js` | reusable raw Wasm host bridge |
| Physical scheduling | `AudioWorkletProcessor.process` | physical worklet adapter |
| Dummy elapsed scheduling | `EngineBackend::advance` and `EngineBackendMode` | scheduler/driver adapter |
| Offline engine ownership | application realm | Worker dummy driver |
| Smoke lifecycle hooks | broad window events in `BrowserAudioController` | narrow diagnostics/fixture control |

Direct browser-driver access to transport currently includes driver-state reads and writes, error publication, port attachment/removal, inbound delivery, ephemeral sends, saturation inspection, and shutdown. No other browser module mutates `Transport`.

## Protocol and replay inventory

Production envelopes are bounded JSON at protocol version 12. A command receives one monotonically increasing sequence number. The worklet requires exact sequence order beginning at one after each host construction.

### Journaled by the current remote backend

- MIDI endpoint configuration.
- Track create/remove and loop addition.
- Composite create/configure/transition/play-after-record/remove.
- Track controls and FX controls.
- Direct track MIDI injection.
- Loop gain, balance, grab, sync source, transition, clear, and length.
- Application-to-host connection changes.

The journal coalesces device/MIDI configuration, same-parameter track controls, selected continuous FX controls, loop gain/balance/sync/transition/clear/length, and identical connection keys. On attach, device channels are sent first, retained MIDI endpoint configuration second, and all other journal entries in retained insertion order. Sequence numbering restarts at one. Replay has no explicit completion state.

Direct MIDI injection, loop grabs, transitions, clears, and structural create/remove commands are retained today even though some are ephemeral intent or cannot safely be replayed indefinitely. The refactoring must classify them explicitly rather than preserve accidental retention.

### Ephemeral today

- Device channel negotiation (derived at attachment).
- Physical host MIDI batches.
- Poll and MIDI-output drain.
- Waveform and MIDI-detail chunk requests.
- Session capture begin/read.
- Session replacement begin/write/commit.
- Loop-content replacement begin/write/commit.
- Shutdown and test saturation polls.

`AbortSessionTransfer` exists on the wire but the browser client does not currently send it.

### Events and failure paths

The worklet publishes acknowledgement, generic error, connection mutation failure, MIDI output, snapshot, waveform, MIDI detail, transfer progress/completion/abort, and stopped events.

- Malformed or wrong-version events fail the transport and driver.
- Stale-generation callbacks are silently ignored.
- Any accepted event decrements a scalar in-flight count; sequence identity, unknown replies, duplicates, and response ordering are not validated by the client.
- A generic remote command error fails `Backend::poll`, and application code marks the backend unavailable.
- Connection rejection has a typed snapshot surface and does not fail the backend.
- Session capture/replacement and loop-content replacement report normal pending progress using selected error-message strings. Application I/O control flow matches those strings.
- Detach/restart/shutdown does not have complete typed cancellation for retained transfer assemblies.

## State and optimism baseline

`WebAudioBackend` mutates its local `BackendSnapshot` immediately after accepted submission for track and loop creation/removal, ports, composites, track controls, Tiny Synth/FX controls, loop gain, and loop balance. Later wire snapshots overwrite or extend this state. The application separately owns provisional connection state, but it has no general desired-state overlay for controls.

Authoritative-only values already include physical callback count, processed frames, meters, xruns and render diagnostics. The refactoring must also make route confirmation, lifecycle, positions, recording completion, and all committed topology/state authoritative while retaining explicit application/widget optimism.

## Lifecycle baseline

- Initial physical state: `AwaitingGesture`.
- Enable: create/resume `AudioContext`; microphone mode also requests `getUserMedia`.
- Startup: load worklet script and Wasm, create/connect graph, install callbacks, attach port, negotiate channels, replay journal, then publish context-derived running/suspended state.
- The current running state can be visible before negotiation acknowledgements or replay responses arrive.
- A 15-second wall-clock presentation timeout covers starting/suspended startup.
- Retry tears down the existing graph and increments the driver generation.
- Stale callbacks compare the captured generation before mutating driver state; stale transport events are ignored.
- Permission denial remains retryable. Processor error, stream end, malformed event, context failure, and explicit smoke failure tear down or fail the graph.
- Shutdown clears handlers, stops tracks, disconnects nodes, closes the port/context, and preserves denial/unsupported/failed terminal state.
- `WebAudioBackend::wait_idle` is a no-op.

Presentation is coupled to lifecycle: the controller mutates button visibility/text, permission labels, and `runtime_status` diagnostic attributes while owning the graph.

## Session, MIDI, and transfer behavior

- Session and loop-content JSON are capped at 256 MiB and transferred in 2 KiB chunks.
- Session capture pipelines at most eight reads while command occupancy remains below half capacity.
- Replacement writes while occupancy remains below half capacity and then sends one commit.
- Waveforms use 512-sample chunks; MIDI details use 16-event chunks and restart on content revision change.
- Browser host MIDI endpoint identity and direction come from one `BrowserMidiHub`. Track input batches are capped at 128 and assigned frame zero. Output events are sent through the same hub. Refusal/drop counters contribute to command-overflow diagnostics.
- Scripting MIDI uses a separate service façade backed by the same controller-owned physical endpoint truth.

## Native and browser compatibility gates

The authoritative CI definitions are in `.github/workflows/build_and_test.yml`.

Native gates include warning-denying builds, complete Linux workspace nextest with `shoop_engine/app_backend`, per-platform application suites, driver/Carla checks, formatting, tracing inventory, and packaged runtime checks.

Web gates include Trunk debug/release builds, hosted and self-contained packaging, native execution of browser-independent package tests, `wasm32-unknown-unknown` application/worklet compilation, forbidden-dependency tree checks, worklet import inspection, hosted/self-contained Chrome workflows, extended denial/lifecycle/saturation/stress workflows, and Firefox physical AudioWorklet smoke.

Warm local characterization timings on the baseline host:

| Command | Result | Elapsed |
| --- | --- | ---: |
| `cargo test -p shoop_audio_protocol` | 5 passed | 0.271 s |
| `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo test -p shoop_backend` | 37 passed | 0.310 s |
| `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo test -p shoop_audio_worklet` | 13 passed | 0.192 s |

The first local build required a writable Cargo git cache and compiled dependencies; its three parallel package invocations completed in 57–74 seconds. Browser executables, Trunk, the Wasm Rust target, and cargo-nextest were not present in that local shell, so CI remains the baseline evidence for those gates.

## Characterization coverage before extraction

- Exact production JSON bytes: `production_envelopes_have_stable_json_bytes`.
- Protocol sequence/stable IDs and journal coalescing: `shoop_audio_protocol` tests.
- Worklet order, duplicate/stale/malformed command rejection and capacity: `shoop_audio_worklet` tests.
- Bounded waveform/MIDI/session/loop-content transfers: protocol and worklet tests.
- Dummy elapsed pacing and xruns: `shoop_backend` elapsed-time tests.
- Physical-equivalent explicit quantum, routing, MIDI, Tiny Synth/FX, no-allocation, and session behavior: backend/worklet tests.
- Physical lifecycle, restart, permission, output-only, saturation, track end, and real callback behavior: packaged browser smoke matrix.

Client-side response correlation, readiness, quiescence, typed transfer progress, stale-generation observability, desired-state convergence, Worker lifecycle, and multi-instance teardown are intentionally identified coverage gaps and are mandatory destinations in later stages.

## Initial public extraction surface

The initial `shoop_worklet_client` surface will contain:

- `RemoteWorkletBackend` implementing `Backend`;
- cloneable `RemoteBackendControl` exposing attach, detach, receive, driver-state, and failure operations only;
- runtime-independent `MessageEndpoint`;
- `HostMidiBridge` plus null and deterministic implementations;
- typed driver/connection/protocol/replay/engine readiness state;
- typed operation progress and correlated mutation outcomes;
- remote client diagnostics needed for readiness, quiescence, and teardown verification.

Backend IDs, snapshots, session/content types, processor descriptors, and audio-driver domain types remain defined in domain crates. Wire envelopes remain defined in `shoop_audio_protocol`. Browser DOM, Web Audio, Worker, MessagePort, and Web MIDI adapters remain in browser composition.
