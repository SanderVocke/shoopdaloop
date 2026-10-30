# New engine backend: native host

`shoop_rt_host_native` runs the [engine](../shoop_rt_engine) natively. It opens a
driver, prepares the engine for the opened device, renders it on the driver's
real-time thread and hands the control endpoint to the caller inside an
in-process protocol client. It contains no engine or application semantics and
depends on no application crate; `scripts/check_rt_engine_boundary.py` enforces
that.

Drivers:

- **Dummy**: a thread renders a buffer every buffer period (`Automatic`), or only
  the frames requested through `DummyControl` (`Controlled`). Inputs are silent
  or come from a function of host port and absolute frame.
- **CPAL with midir** (feature `cpal`, on by default): the engine renders in the
  CPAL output stream's callback; an input stream feeds it through a lock-free
  bridge; midir ports exchange MIDI with the callback through bounded bridges.

## Use

```rust
// Discover (CPAL only): hosts, devices with their channels and defaults, MIDI ports.
let discovery = shoop_rt_host_native::discover(None)?;

// Configure: the driver, and the engine's pools and settings.
let config = NativeConfig {
    driver: DriverConfig::Cpal(CpalConfig {
        output_device: DeviceSelection::Named(discovery.outputs[0].name.clone()),
        buffer_frames: Some(256),
        midi_inputs: discovery.midi_inputs.clone(),
        ..CpalConfig::default()
    }),
    engine: EngineDescription { max_frames: 1024, ..EngineDescription::default() },
};

// Start: the host renders; the client owns the control endpoint.
let Started { host, client } = shoop_rt_host_native::start(&config)?;
for warning in host.warnings() { /* show it */ }

// Hand the client to the adapter, which connects and pumps it on its thread.
// host.status() reports driver state, xruns, load and the next frame.

// Stop: drop the adapter (and with it the client) first, then stop the host.
drop(client);
let report = host.stop();
```

The host fills in the device's part of the `EngineDescription`: sample rate, host
ports, Master's initial connections to the first outputs (which the engine
reports as preferred host ports) and a control lead of at least one buffer, since
the control endpoint runs concurrently with rendering. Opening other devices or
another sample rate or buffer size is a lifecycle operation: stop, then start
again.

Host ports, in the engine's numbering (audio inputs, audio outputs, MIDI inputs,
MIDI outputs):

| Driver | Audio | MIDI |
| --- | --- | --- |
| Dummy | `audio_in_N`, `audio_out_N` | `midi_in_N`, `midi_out_N` |
| CPAL | `<device>:capture_N`, `<device>:playback_N` | the midir port name |

Names are shortened to the protocol's 64 bytes by shortening the device part.
A named device that does not exist is refused. A missing default input or MIDI
port, or a MIDI output that fails to open, is left out with a warning; a MIDI
input that fails to connect stays silent with a warning.

## Threads and bridges

| Thread | Work |
| --- | --- |
| Caller | `start`, `stop`, discovery; the client and control endpoint |
| Driver: dummy thread or CPAL output callback | The render owner: one engine callback per block |
| CPAL input callback | Pushes captured frames into the duplex bridge |
| midir input callbacks | Stamp messages with the host clock and push them into a MIDI input bridge |
| MIDI output pump | Sends render output and MIDI recovery to midir ports |

- **Duplex**: a single-producer single-consumer ring of interleaved samples
  holding four largest callbacks. It starts with one buffer of silence, so input
  reaches the output one buffer later. A full ring drops the newest frames and
  counts an input overrun; a short one fills with silence and counts an input
  underrun once the input stream has started. Separate devices are not
  resampled; drift shows up as these xruns.
- **MIDI input**: up to 1024 messages of at most 64 bytes per port. The render
  callback places the messages of the last period one period later, keeping their
  spacing; older ones go to the start of the block. Messages that do not fit the
  engine's per-callback capacity wait for the next callback. A full bridge drops
  and counts; empty, oversized or invalid messages are refused and counted.
- **MIDI output**: after each engine callback, ordinary output and then the
  engine's MIDI recovery go into a 1024-message bridge per port; a full bridge
  drops and counts. The pump sends as soon as it sees them (within about half a
  millisecond), without placing them within the buffer.

## Callback contract

The render callback, the bridges and the dummy driver's loop do not allocate,
free, lock or wait. Pacing sleeps of the dummy thread lie between its cycles.
The callback measures its time against its period and reports it as load.

- **Buffers**: a requested buffer above the engine's prepared maximum, or one
  the device does not support, is refused at open. Without a requested buffer,
  a device whose smallest buffer exceeds the maximum is refused; larger callbacks
  that still arrive are rendered as several engine callbacks of at most the
  maximum, counted as split callbacks. The driver status reports the last
  callback's size.
- **Xruns**: backend underruns, duplex overruns and underruns, and dummy cycles
  that start more than a period late; each is counted in `Stats` and reported to
  the engine. Any other stream error marks the driver `Failed`.
- **Faults**: after a render fault the engine keeps the output silent; the
  status reports `RenderPhase::Faulted` and the host should be stopped.

## Stopping

`stop` (or dropping the host) keeps callbacks running until the render owner
has observed the dropped control endpoint, silenced its output and queued its
MIDI reset (all notes off and all sound off on every channel), at most a few
buffers or 50 ms. It then stops the streams or the dummy thread, lets the pump
send what is queued, closes MIDI ports, and runs the engine's stopped cleanup on
the calling thread. `StopReport` says whether the reset went out and returns the
cleaned-up engine. With the client still alive no reset is sent.

## Tests

```sh
cargo test -p shoop_rt_host_native
cargo test -p shoop_rt_host_native --release
```

The CPAL tests run the driver against a software CPAL host (`tests/support`),
whose streams are driven by the test or by threads at the buffer period; every
data callback runs inside a heap measurement. MIDI ports are test sources and
sinks. The dummy tests guard each driver cycle the same way through
`DummyConfig::wrap_cycle`. Real devices and midir ports are checked manually.
