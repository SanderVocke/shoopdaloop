# New engine backend: browser host (worklet half)

`shoop_rt_host_worklet` runs the new engine inside the browser's AudioWorklet. It
is a raw Wasm module for `wasm32-unknown-unknown` with no Wasm imports, like the
legacy worklet, and contains no engine or application semantics: it owns the
[`shoop_rt_engine`](../shoop_rt_engine) owners, serves the engine's control
endpoint through the protocol's `RecordServer` and copies audio, records and MIDI
between module memory and the processor script. It depends only on the engine,
the protocol and `rt_runtime`'s buffer types; `scripts/check_rt_engine_boundary.py`
enforces that it reaches no application crate.

The processor script is `src/rust/shoopdaloop/rt_audio_worklet.js`; it registers
`shoop-rt-audio-processor`. The main-thread half (the protocol client over the
node's `MessagePort`, Web MIDI forwarding and browser backend selection) is built
against the interface below.

## Build and artifact

```sh
cargo build --target wasm32-unknown-unknown -p shoop_rt_host_worklet --release
python3 src/rust/shoopdaloop/build_worklet.py --profile release --output-dir <dir>
```

`build_worklet.py` (Trunk's pre-build hook) builds the legacy module and this one
in separate Cargo runs and copies `shoop_rt_host_worklet.wasm` next to
`shoop_audio_worklet.wasm` in `generated/`, so it is served as
`./generated/shoop_rt_host_worklet.wasm`. Serving the processor script and
packaging both files into the hosted and single-file applications belong to
browser backend selection.

`src/rust/shoopdaloop/rt_worklet_contract.mjs <wasm>` is the committed artifact
check that CI runs on the built module: no imports, the expected exports, and the
processor script driven over a simulated `MessagePort` (preparation, a split Hello
answered by a Welcome, silence without strips, MIDI counters, no memory growth
after preparation, stop and refused configurations).

## Lifecycle

1. The main thread compiles the module and creates the node:

   ```js
   new AudioWorkletNode(context, 'shoop-rt-audio-processor', {
     numberOfInputs: 1,
     numberOfOutputs: 1,
     channelCount: audioInputs,
     channelCountMode: 'explicit',
     outputChannelCount: [audioOutputs],
     processorOptions: { wasmModule, config },
   });
   ```

   `wasmModule` is the compiled `WebAssembly.Module`; `config` is the
   configuration record as an `ArrayBuffer` or typed array.
2. The processor instantiates the module with an empty import object, writes the
   record into module memory and prepares the engine in its constructor, before
   the first render quantum. Preparation allocates every buffer and pool; nothing
   allocates afterwards.
3. The main thread connects a `ProtocolClient` over its record port (Hello,
   Welcome with capabilities and limits) and works through the protocol.
4. On shutdown the client sends Goodbye; `{kind: 'stop'}` then ends processing and
   frees the module's memory.

## Messages

All messages are plain objects with a `kind`. Byte payloads may be an
`ArrayBuffer` or a typed array; the processor posts `ArrayBuffer`s and transfers
them.

| Direction | Message | Meaning |
| --- | --- | --- |
| main → worklet | `{kind: 'records', bytes}` | A batch of client protocol records |
| main → worklet | `{kind: 'midi', bytes}` | A batch of MIDI-in lane events |
| main → worklet | `{kind: 'status'}` | Ask for the diagnostic counters |
| main → worklet | `{kind: 'stop'}` | Stop processing and release the module |
| worklet → main | `{kind: 'records', bytes}` | Engine records of one quantum |
| worklet → main | `{kind: 'midi', bytes}` | MIDI-out lane events of one quantum |
| worklet → main | `{kind: 'status', counters}` | `discontinuities`, `overruns`, `midiInDropped`, `midiInLate`, `midiOutDropped`, `framingErrors` |
| worklet → main | `{kind: 'failed', stage, code, message}` | `stage` is `instantiate`, `prepare` or `process`; processing has ended |

The processor queues posted bytes and copies them into the module's inbound
buffers as space allows, so a batch may be split anywhere; it posts at most one
`records` and one `midi` message per quantum.

### Record framing

A record batch is whole protocol records back to back. Each record is the
protocol codec's: a little-endian `u16` total length (2 to `MAX_RECORD_BYTES`),
then the message. `framing::record_len` and `framing::records` split batches; a
main-thread `RecordPort` can append posted records to one outgoing batch per
main-thread turn and split received batches into a queue for `take`.

Per quantum the module delivers at most `records_per_quantum` client records to
the control endpoint before rendering and polls it again after rendering, so
results and telemetry that rendering produces leave in the same quantum. The
outgoing batch is bounded by `records_out_bytes`; what does not fit stays in the
engine outbox for later quanta (telemetry keeps only the latest records). A
length prefix outside the codec's bounds discards the inbound stream, counts a
framing error and ends the connection with `Disconnected(ProtocolError)`; a new
Hello reconnects.

### MIDI lanes

MIDI travels apart from protocol records. An event is:

| Bytes | Field |
| --- | --- |
| 8 | Frame on the AudioWorklet `currentFrame` timeline, little-endian `u64` |
| 1 | Host MIDI port index: an input index on the MIDI-in lane, an output index on the MIDI-out lane |
| 1 | Message length, 1 to 255 |
| n | The MIDI message |

`lane::MidiEvent::encode`, `lane::decode` and `lane::events` implement it. The
main thread converts Web MIDI timestamps to frames, for example through
`AudioContext.getOutputTimestamp()`, and may add a fixed delay so events arrive
before their quantum.

- Events due in the current quantum go into the engine's host MIDI input at their
  offset. Late events go to the quantum's first frame and count as late.
- Events for later quanta stay in the lane, which holds `midi_in_bytes`.
- Events for unknown ports, invalid messages and events beyond a port's
  per-quantum capacity (`midi_events`, `midi_bytes`) are dropped and counted.
- A zero length discards the lane's contents and counts a framing error.

After rendering, every event of the engine's host MIDI outputs, followed by its
MIDI recovery output, is posted with its frame on the `currentFrame` timeline.
Events beyond `midi_out_bytes` per quantum are dropped and counted.

## Configuration record

`WorkletConfig::encode` writes it and `WorkletConfig::decode` reads it. Every
number is a little-endian `u32`:

1. version (1), sample rate, largest quantum (`max_frames`), strip channels,
   Master channels, smoothing frames, smoothing limit, ramp frames, control lead
   frames, resource pages, page bytes, MIDI events and bytes per port, pending
   transitions, reliable credits, bulk credits, telemetry interval frames;
2. `records_in_bytes`, `records_out_bytes`, `midi_in_bytes`, `midi_out_bytes`,
   `records_per_quantum`;
3. counted lists, each a count followed by its items: pool segments (strips,
   loops); initial Master connections (Master channel, host audio output index);
   then the names of the host audio inputs, audio outputs, MIDI inputs and MIDI
   outputs, each name a `u8` length and up to 64 UTF-8 bytes.

The engine fields are `EngineDescription`'s. Host audio ports are the node's
input and output channels; host MIDI ports are the Web MIDI ports known when the
node is created. The control lead can be zero, because control and rendering
alternate on the audio thread. `WorkletConfig::new` uses 16 KiB record buffers,
4 KiB MIDI lanes and 8 records per quantum. Record buffers hold at least one
largest record, MIDI lanes at least one largest event.

## Status

The engine's telemetry reports the driver as `WebAudio` with the largest quantum
as its buffer size, `Starting` until the first quantum and `Running` afterwards.
Xruns count quanta whose `currentFrame` does not continue the previous one, and
quanta whose measured processing time exceeded their period. The processor
measures load where the scope has `performance`, and passes it with the next
quantum. If rendering faults, output is silent, MIDI resets go out on the MIDI
lane and the connection ends with `Disconnected(Fault)`; the processor keeps
running so these reach the main thread.

## Exports

The processor script uses these; pointers are byte offsets into `memory`.

| Export | Use |
| --- | --- |
| `shoop_rt_worklet_config_buffer(len) -> ptr` | Space for the configuration record; null if empty, above 64 KiB or prepared |
| `shoop_rt_worklet_prepare() -> code` | 0 prepared, 1 already prepared, 2 no record, 3 invalid record, 4 refused by the engine |
| `shoop_rt_worklet_release()` | Drop the host; the module can be prepared again |
| `shoop_rt_worklet_max_frames()`, `_audio_inputs()`, `_audio_outputs()` | Prepared shape |
| `shoop_rt_worklet_audio_input_ptr()`, `_audio_output_ptr()` | Planar `f32` channels, `max_frames` apart |
| `shoop_rt_worklet_buffer_ptr(id)`, `_capacity(id)`, `_len(id)` | Buffers 0 records in, 1 records out, 2 MIDI in, 3 MIDI out; an inbound length is where the next bytes go, an outbound length is the last quantum's bytes |
| `shoop_rt_worklet_commit(id, len) -> bool` | Accept bytes written at an inbound buffer's length |
| `shoop_rt_worklet_process(frames, input_channels, frame, load) -> code` | 0 rendered, 1 not prepared, 2 invalid quantum (nothing ran), 3 faulted; `frame` is `currentFrame`, `load` the previous quantum's share of its period or negative |
| `shoop_rt_worklet_counter(id) -> u32` | 0 discontinuities, 1 overruns, 2 MIDI-in dropped, 3 MIDI-in late, 4 MIDI-out dropped, 5 framing errors |

The crate is `#![deny(unsafe_code)]`; the exports module allows it only for
`no_mangle` and dereferences no pointers. The module holds one host per instance
in a module-global cell.

## Formats without the module

The `exports` feature (default) compiles the exports. A main-thread crate can
depend on this crate with `default-features = false` to use `WorkletConfig`,
`framing` and `lane` without adding the exports to its own Wasm module.

## Tests

```sh
cargo test -p shoop_rt_host_worklet
python3 scripts/run_wasm_tests.py --runtime node --package shoop_rt_host_worklet
node src/rust/shoopdaloop/rt_worklet_contract.mjs <shoop_rt_host_worklet.wasm>
```

The integration tests drive `WorkletHost` the way the processor script does: a
`ProtocolClient` on the main thread exchanges `postMessage` batches with a
processor stand-in that copies bytes into the module's buffers as space allows.
They create a strip and a loop, record and play it back, and check telemetry,
xruns and load, timestamped MIDI in and out, bounded delivery per quantum,
framing errors and reconnection. Every module call is heap-guarded. A separate
test exercises the exported functions.
