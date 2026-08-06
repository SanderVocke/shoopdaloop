# Browser Audio and Worklet Contract

This document freezes the implemented Milestone 3 browser-audio contract. The typed source of truth is `shoop_audio_protocol`; values below describe protocol version 1.

## Ownership and clocks

Hosted secure browser runs create exactly one `AudioContext`. Its graph is:

```text
default microphone MediaStreamAudioSourceNode
    -> Shoop AudioWorkletNode
    -> AudioContext default destination
```

The dedicated `shoop_audio_worklet.wasm` instance privately owns `EngineBackend`, `Session`, all tracks, loops, channels, recording storage, and the engine sample clock. Only `AudioWorkletProcessor.process()` calls `process_audio_quantum`. UI animation updates call the browser proxy's no-op `advance`; elapsed UI time therefore cannot advance or catch up the physical-audio session.

The UI Wasm instance owns browser lifecycle objects and a proxy snapshot. It never accesses the worklet session or audio samples. Audio samples move only through Web Audio planar buffers and private worklet Wasm memory.

## Lifecycle and generations

The observable lifecycle is `AwaitingGesture`, `RequestingPermission`, `Starting`, `Running`, `Suspended`, `Denied`, `Unsupported`, `Failed`, and `Stopped`.

The enable button synchronously creates/resumes `AudioContext` and invokes `getUserMedia` before awaiting either promise. Startup requests the default microphone with echo cancellation, noise suppression, and automatic gain control disabled as optional preferences. Browser negotiation is authoritative.

Each start/retry increments a generation. Message, processor-error, context-state, and track-ended callbacks ignore old generations. Before replacement or shutdown, microphone tracks are stopped, node handlers are detached, ports and nodes are disconnected, and the context is closed. A retry replays the bounded command journal into one new worklet host; it never runs a second engine concurrently.

`file:` is not treated as microphone-capable. `?offline=1` explicitly selects the dummy engine. Without that selection, the UI reports the secure-context limitation.

## Protocol bounds and ordering

- Version: `1`; mismatch is a visible error.
- Main-side mutation journal: 256 commands. Superseded gain, balance, mute, monitoring, loop-gain, sync-source, clear, and final transport values coalesce by stable entity ID and control kind.
- Commands awaiting worklet response: 256.
- Main-side event queue: 256 events.
- Maximum encoded command: 16 KiB.
- Waveform payload: at most 512 samples per chunk.
- Status poll cadence: no faster than 50 ms.
- Stable IDs: assigned by the application-side proxy and verified against worklet creation results.
- Ordering: every posted command carries a strictly increasing sequence. Duplicate, stale, skipped, malformed, and out-of-order commands return a typed error and do not silently retarget an object.
- Backpressure: submission at capacity fails synchronously and increments `command_overflows`. Nothing waits for queue space.
- Main-thread behavior: all sends use `MessagePort.postMessage`; no operation waits for an acknowledgement or worklet result.
- Waveforms: requested by loop ID and revision, then transferred in ordered `(channel, offset)` chunks with total length and final-chunk markers. Unknown or cancelled revisions are ignored.

### Control-side processing design revision

Protocol commands are applied from the worklet's `MessagePort` task, never reentrantly from `process()`. AudioWorkletGlobalScope runs message and render tasks serially, so this cannot race the session. This is an evidence-backed revision to the plan's preliminary “enqueue then drain commands in process” preference: topology construction and JSON work allocate, and moving either into `process()` would violate the immutable render-path allocation rule. The main side bounds outstanding tasks, while the browser schedules message tasks between render calls. The render function only copies prepared samples, calls Rust DSP, copies output, and checks memory identity.

Track/loop topology and fixed recording storage are fully constructed in the control task before the next render call. Schedule installation pre-sizes session scratch and external port buffers. Wasm memory growth is allowed during control tasks, after which JavaScript rebuilds its typed views. A memory-buffer identity change during `process()` terminates the processor visibly.

## Render contract

- Maximum accepted callback quantum: 2048 frames.
- Actual callback length is passed to the engine and reported; common Chrome evidence is 128 frames.
- Sample rate is `AudioContext.sampleRate`; Shoop performs no device-rate conversion.
- Input and output support the current mono/stereo direct-track scope.
- JavaScript creates channel views once and uses indexed copies without per-quantum `subarray`, arrays, messages, or promises.
- Rust work buffers, session scratch, schedules, port buffers, copy-command queues, and recording chunks are prepared before rendering.
- The render call takes no mutex, sleeps, waits, joins, filesystem/DOM operation, or `postMessage` path.
- Render failure, shape overflow, or in-callback Wasm memory growth returns `false` and emits a visible failure message on the exceptional path.

## Routing and mixing

Every direct track receives the default capture channels deterministically:

- Mono track: capture channel 1.
- Stereo track with mono capture: capture channel 1 duplicated.
- Stereo track with stereo capture: capture left/right.
- More capture channels are ignored for this milestone.

Track input monitoring defaults off and uses the existing monitor control. Mono track output is mixed equally to both destination channels. Stereo output maps left/right. Track outputs sum, then the destination mix clips to `[-1, 1]`. The browser handles conversion at microphone and destination boundaries.

There is no Web MIDI. MIDI-shaped application data remains inert in the worklet and diagnostics report Web MIDI unavailable.

## Recording storage

Each physical audio channel receives prepared storage for ten seconds at the actual context sample rate, for both recording and prerecord ownership. Growth beyond that hard capacity returns `StorageExhausted`, leaves existing content valid, increments `storage_exhaustions`, and does not allocate more chunks in the render callback. Clearing recycles prepared chunks.

## State and diagnostics

A main-side 50 ms poll asks the worklet to serialize a bounded snapshot outside the render callback. Plain API and DOM diagnostics expose:

- lifecycle/context state;
- actual sample rate and latest render quantum;
- callback and processed-frame counters;
- input/output activity peaks;
- xrun, command-overflow, and storage-exhaustion counters;
- engine/application revision;
- self-test state and Web MIDI absence.

Waveforms are not included in periodic snapshots. They use the revisioned chunk protocol.

## Portability inventory

| Existing path | Browser physical-audio classification |
|---|---|
| Native application actor thread and waits | Native-only; unavailable in worklet |
| Elapsed-time dummy `advance` | Explicit offline/native only; browser proxy no-op |
| Full `app_backend` workers and content snapshot runtime | Excluded from worklet |
| JACK/CPAL/Midir/LV2 drivers | Excluded from browser dependency graphs |
| `ExternalAudioPort` output capture mutex | Capture disabled; render uses borrowed current output and never locks |
| Session schedule construction | Control-side before rendering new topology |
| Session process schedule and scratch | Worklet-safe after schedule installation pre-sizing |
| Chunked audio growth | Replaced by hard-bounded preallocated storage |
| Direct session waveform copy | Control-side only, revisioned and chunked over MessagePort |
| Engine state mirrors | Read by control-side status poll; atomic publication remains render-safe |
| JavaScript DOM/media promises | Main-side controller only |
| JavaScript planar copies | Worklet render path, fixed views and indexed loops |

## Artifact boundaries

`shoopdaloop_egui` links UI/application/browser-controller code. `shoop_audio_worklet` links only protocol, backend core, engine DSP, and target-neutral dependencies. The worklet has no imports and no eframe, DOM, frontend, Qt, JACK, CPAL, Midir, LV2, X11, or Wayland dependency. Trunk's pre-build hook builds and copies the raw dedicated worklet Wasm; generated artifacts remain ignored.
