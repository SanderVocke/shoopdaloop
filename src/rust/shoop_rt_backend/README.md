# New engine backend: the adapter

`shoop_rt_backend` implements the application's `Backend` trait (from
`shoop_backend`) over the [`shoop_rt_protocol`](../shoop_rt_protocol) client. It is
the only place where application concepts (tracks, ports, the Master bus, sessions)
meet engine terms (strips, loop slots, host ports, routes). It depends on no engine
or host crate: the composition root chooses a carrier and a clock and hands them over.

## Construction

```rust
// A client over any carrier, not yet connected.
let backend = RtBackend::connect(client, clock)?;
```

- `RtBackend::connect(client, clock)` sends `Hello`, takes capabilities, limits and
  the driver status from `Welcome`, requests a resynchronization and waits through
  the clock until the engine's state is mirrored.
- `RtBackend::begin(client, clock)` does the same without waiting; the backend
  refuses operations until `is_ready()`. Pumping it (`poll`, `advance` or `pump`)
  completes the handshake. Environments that cannot block use this.
- `EngineClock` tells the adapter how engine time passes: `advance(elapsed)` for
  `Backend::advance`, and `wait()` to let the engine progress while the adapter
  waits. `Sleep` (native) sleeps while a host renders on its own thread; `NoWait`
  returns at once. Tests use an offline clock that renders
  `shoop_rt_engine::testing::OfflineRender` when asked.
- `disconnect()` sends `Goodbye`; the engine keeps its state.

The engine must have a two-channel Master.

## Identities and the mirror

Every mutation returns at once. Application identities (tracks, loops, ports) are
counters that are never reused. Engine slots are reserved on the control side: a
track takes a strip slot and a loop takes a loop slot, each with a new generation.
Removing a track queues `RemoveStrip` and releases its slots immediately; the engine
handles requests in order, so a later reservation of the slot follows the removal.

Requests wait in a queue until the engine's credits allow sending them. Each request
carries its purpose; its outcome confirms the change in the mirror or becomes a
failure:

| Outcome of | Done | Refused |
| --- | --- | --- |
| Strip or loop creation | The track or loop appears in the snapshot | `mutation_failures` (`TrackCreation`, `LoopCreation`); the identity is dropped |
| Strip removal | The track, its loops, ports, links and routes disappear | `TrackRemoval` failure; the track stays |
| Controls, loop gain and balance, sync source | The confirmed value | `TrackControl`, `BusControl`, `LoopGain`, `LoopBalance`, `LoopControl` failures |
| Transitions, clear, MIDI injection, smoothing | (telemetry shows the effect) | `LoopControl` or `MidiInput` failures |
| Host connections | `confirmed_links` | `connections.failures` |
| Mixer routes | `mixer.confirmed_links` | `mixer.failures` |

Refusals the adapter can decide itself, such as an exhausted pool ("all 8 prepared
track slots of the engine are in use"), unknown identities, invalid values or
incompatible host ports, are returned as errors from the call.

`poll` pumps, then assembles the snapshot from confirmed values and the latest
telemetry; it never waits. Loop modes, positions, lengths and planned transitions
come from `Telemetry::Loop`; meters from strip, loop and bus telemetry, holding the
maximum since the previous snapshot (in dBFS, -200 for silence); status from engine
telemetry. The loop state describes the start of the last rendered callback, one
callback behind the legacy backends, which report the next frame to render. Delays
derived from a loop position and the sync loop's length can therefore come out one
cycle later when a request falls within one callback of a wrap.

`wait_idle` pumps and waits through the clock until no request is queued or
unanswered, no content read is in progress and telemetry newer than the last outcome
has arrived. With `NoWait` it returns after one pump.

On connection the resynchronization stream replaces the host port inventory and the
Master's controls and connections; engine strips the application does not know are
removed. `Disconnected` or a refused handshake marks the driver as failed in the
snapshot and every later operation is refused.

## Mapping

- **Tracks** are direct tracks with 0 to the engine's strip channels per direction and
  optional MIDI in and out; ports are named as by the legacy backends
  (`{base}_direct_in_1`, …, `{base}_direct_midi_in`). Gains in dB become linear
  engine gains; balances are clamped to [-1, 1]. The regular default playback mode is
  accepted.
- **Loops** belong to a track. `cycles_delay` is passed as the engine's `delay`: a
  delay of `n` applies at the `(n + 1)`-th pulse of the sync source, a nearer request
  supersedes, and a loop without a sync source that is not playing transitions at
  once. Loop gain is clamped to [0, 1], balance to [-1, 1].
- **Content reads** use chunked bulk reads. `loop_audio_data*` and `loop_midi_data`
  return `None` while a read is in progress and the content once every chunk arrived;
  an empty loop is answered at once and a recording loop returns `None`. A read
  belongs to one version of the content and restarts when the loop records or is
  cleared.
- **Host ports** come from the resynchronization; their names are the application's
  host port identities. An engine input (capture) is an application output port.
- **Master** has the fixed application identities of the legacy Master: bus, channel
  and output port IDs. Its outputs connect to host outputs; mixer routes connect one
  track output port to one Master channel.
- **The global FX control port** exists as in the legacy backends; its connections are
  accepted and confirmed without reaching the engine.
- **Smoothing** in milliseconds becomes `SetSmoothing` frames at the engine's sample
  rate; values above the engine's limit come back as a failure.
- **New session** (`replace_session`, `replace_session_async`) accepts a structure-only
  document of the core set: direct tracks with regular playback and no FX, at most the
  Master bus, one global FX control port, routes to Master. The whole document is
  checked first; one that needs anything else is rejected without changes. The
  replacement then removes every track, resets Master's controls and connections and
  creates the new structure. The asynchronous variant is ready when every request of
  the replacement has its outcome.

## Refused

The backend reports no optional capability except `MidiInjection` when the engine
announces it. Every other operation is refused with the capability's explanation:
session capture and loading with content, composite loops, grab, the replacing and
dry/wet modes, dry/wet and processor tracks, track and bus FX, latency compensation,
take alignment, loop content import, loop length and timing edits, buses other than
Master and audio driver switching. Track-less loops (`create_loop`) are refused as
well. Nothing is routed to another engine.

## Tests

```sh
cargo test -p shoop_rt_backend
```

- `tests/contracts.rs` runs `shoop_backend::contracts::backend_contracts` and
  `harness_contracts` against the adapter, the in-process carrier, the engine and the
  offline host. The timing harness renders one-frame callbacks at 1 kHz with the
  timing ramp on the capture input and declares an observation lag of one frame.
- `tests/adapter.rs` covers refusal messages, exhausted pools, engine refusals as
  mutation failures, asynchronous reads, removal of unknown engine structure at
  connection and disconnection.

`shoop_app`'s `tests/rt_backend.rs` runs the core workflow through the cooperative
application runtime on the same stack.
