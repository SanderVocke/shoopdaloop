# New engine backend: native host

`shoop_rt_host_native` runs the [engine](../shoop_rt_engine) natively. It opens a
driver, prepares the engine for the opened device, renders it on the driver's
real-time thread. `start` returns an in-process protocol client; `start_endpoint`
returns the independently owned endpoint/outbox for a host to attach any carrier.
Connection retirement retains that endpoint and host to keep music running;
explicit shutdown drops the endpoint before stopping the host for MIDI cleanup. It contains no engine or application semantics and
depends on no application crate; `scripts/check_rt_engine_boundary.py` enforces
that.

Drivers:

- **Dummy**: a thread renders a buffer every buffer period (`Automatic`), or only
  the frames requested through `DummyControl` (`Controlled`). Inputs are silent
  or come from a function of host port and absolute frame.
- **Native JACK** (feature `jack`, on by default): audio and MIDI share one JACK
  callback. MIDI offsets are preserved without CPAL or midir bridges.
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
the control endpoint runs concurrently with rendering. Requesting another device,
sample rate or buffer size requires stop/start. JACK server buffer-size changes
are handled in place, as described below.

Host ports, in the engine's numbering (audio inputs, audio outputs, MIDI inputs,
MIDI outputs):

| Driver | Audio | MIDI |
| --- | --- | --- |
| Dummy | `audio_in_N`, `audio_out_N` | `midi_in_N`, `midi_out_N` |
| CPAL | `<device>:capture_N`, `<device>:playback_N` | the midir port name |
| JACK | `jack:<configured audio port>` | `jack:<configured MIDI port>` |

Names are shortened to the protocol's 64 bytes by shortening the device part.
A named device that does not exist is refused. A missing default input or MIDI
port, or a MIDI output that fails to open, is left out with a warning; a MIDI
input that fails to connect stays silent with a warning.

## Native JACK configuration

Select JACK in the application settings and restart the host. Both application
startup and independent `shoop_host --config <file>` use the direct JACK driver.
An existing profile containing only `client_name` receives two audio inputs, two
audio outputs, one MIDI input and one MIDI output. A host configuration can set
all port names and optional external startup connections:

```json
{
  "driver": {
    "Jack": {
      "client_name": "ShoopDaLoop",
      "audio_inputs": ["audio_in_1", "audio_in_2"],
      "audio_outputs": ["audio_out_1", "audio_out_2"],
      "midi_inputs": ["midi_in_1"],
      "midi_outputs": ["midi_out_1"],
      "connections": [
        ["system:capture_1", "audio_in_1"],
        ["audio_out_1", "system:playback_1"]
      ]
    }
  }
}
```

Port lists in the settings UI are comma-separated; startup connection pairs use
`source -> destination`, separated by semicolons. An empty list registers no ports
of that kind/direction. Names are unique across the bank, nonempty, at most 59
UTF-8 bytes, and contain no colon, comma or NUL. Each kind/direction has at most
64 ports. The client-name hint has at most 63 bytes and no colon or NUL.

A short connection endpoint names a local port. A `client:port` endpoint names
an external JACK graph port. Required connection failures roll back startup;
missing libraries/server or other startup failures are diagnosed, with the
application's existing dummy fallback and saved preference preserved. Probing
and startup never autostart JACK. The actual client name may gain a JACK suffix
when another client uses the hint; runtime metadata reports the resolved name.

Engine port identities use `jack:<short name>`, independent of that suffix, so
session connection intent survives restart. Master initially uses the first
configured audio outputs; external playback connections are explicit startup
pairs or JACK patchbay connections. Track-to-host routing uses the existing
application connection UI. The port bank is fixed for the host lifetime; changing
it requires restart, rather than dynamically registering per-track JACK ports.

The server determines sample rate and callback size. A callback above the engine
maximum is rendered in parts without an extra duplex buffer. MIDI offsets are
rebased for each part and restored for JACK output; recovery and ordinary output
are merged in timestamp order, with each output MIDI buffer cleared once per JACK
cycle. Invalid/capacity-limited input and failed output writes are counted.
Buffer-size changes remain supported through splitting. Control lead stays at
least one current server period when the buffer size changes. A sample-rate change
marks the host failed and silences output until restart. Server shutdown reports
a device failure; cleanup is bounded and does not claim MIDI recovery delivery
to an unavailable server. Normal stop drops the endpoint, delivers recovery,
deactivates JACK, and cleans up the render owner off the callback thread.

JACK and CPAL are independently selectable at build time:

```sh
cargo build -p shoopdaloop -p shoop_host -p shoop_host_manager --no-default-features --features jack
cargo build -p shoopdaloop -p shoop_host -p shoop_host_manager --no-default-features --features cpal
cargo build -p shoopdaloop -p shoop_host -p shoop_host_manager --no-default-features
```

Native distribution defaults enable both. JACK is loaded at runtime; browser
production graphs contain neither native driver. CPAL's JACK host remains a
separate CPAL selection. Live driver switching and JACK transport/timebase or
session management are not provided by this driver.

## Threads and bridges

| Thread | Work |
| --- | --- |
| Caller | `start`, `stop`, discovery; the client and control endpoint |
| Driver: dummy thread, CPAL output or JACK process callback | Owns rendering and processes audio blocks |
| JACK monitor | Publishes server shutdown from the control side |
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
SHOOP_ALLOW_MISSING_BACKENDS=1 cargo test -p shoop_rt_host_native
python3 scripts/run_native_jack_tests.py
python3 scripts/run_native_jack_tests.py --profile release
```

The CPAL tests run the driver against a software CPAL host (`tests/support`),
whose streams are driven by the test or by threads at the buffer period; every
data callback runs inside a heap measurement. MIDI ports are test sources and
sinks. The dummy tests guard each driver cycle the same way through
`DummyConfig::wrap_cycle`. Real devices and midir ports are checked manually.

The strict JACK runner requires `jackd` with its dummy audio backend. It creates
an isolated named server, removes the missing-backend skip flag, verifies
readiness, and runs direct audio/MIDI, server-loss, buffer-change, host-process,
application-startup and service/session recovery scenarios. Logs are written to
`target/native-jack`; a missing server fails this acceptance. The Linux native
JACK CI job runs the same commands. The small `shoop_jack` notification adapter
isolates the JACK trait's unsafe shutdown declaration to an atomic store;
`shoop_rt_host_native` retains its `forbid(unsafe_code)` policy.
