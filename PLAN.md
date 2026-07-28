# CPAL + midir integration plan

## Goal

Integrate CPAL audio and midir MIDI as an application backend without changing the track/channel/loop model or the QML connection UI.

The main idea is to make CPAL + midir look like a small JACK-like routing environment to the rest of ShoopDaLoop. The application should continue to see driver ports, external ports, and connections. CPAL/midir-specific details should stay below the backend bindings layer.

## Design summary

Add a new backend type:

```text
--backend cpal
```

Starting this backend initializes:

- CPAL audio output stream
- optionally CPAL audio input stream
- optionally midir MIDI input connections
- optionally midir MIDI output connections
- a shared virtual external-port router

The router exposes physical CPAL/midir I/O as virtual external ports. Existing application-created ports can then connect to those virtual ports just as they connect to JACK ports.

Example virtual external port names:

```text
cpal:Built-in Audio:capture_1
cpal:Built-in Audio:capture_2
cpal:Built-in Audio:playback_1
cpal:Built-in Audio:playback_2

midir:Launchkey Mini:output
midir:FluidSynth:input
```

## Direction semantics

Use JACK-like direction semantics:

- physical audio capture/input channel = external `Output`
  - it outputs audio into Shoop input ports
- physical audio playback/output channel = external `Input`
  - it receives audio from Shoop output ports
- MIDI source/controller = external `Output`
  - it outputs MIDI into Shoop MIDI input ports
- MIDI sink/synth = external `Input`
  - it receives MIDI from Shoop MIDI output ports

This preserves the existing opposite-direction matching used by the GUI.

## Runtime routing

The CPAL/midir backend owns connection maps such as:

```text
Shoop audio input port  <- CPAL capture channel(s)
Shoop audio output port -> CPAL playback channel(s)

Shoop MIDI input port   <- midir input/source(s)
Shoop MIDI output port  -> midir output/sink(s)
```

During each CPAL output callback:

1. Drain CPAL input ring buffers.
2. Stage connected capture-channel audio into connected Shoop audio input ports.
3. Drain pending midir input messages into connected Shoop MIDI input ports.
4. Run `session.process(n_frames)`.
5. Sum/mix connected Shoop audio output ports into CPAL playback channels.
6. Send connected Shoop MIDI output events to connected midir outputs.

The CPAL output callback is the audio clock. midir input events are staged into the next audio cycle. This means MIDI timing is not sample-accurate like JACK MIDI; all pending midir events are effectively quantized to the next process buffer.

## CLI design

### Backend selection

```text
--backend jack|dummy|jack_test|cpal
```

### Audio device enumeration

```text
--list-audio-devices
--list-cpal-hosts
--cpal-host <name>
```

Example output:

```text
CPAL hosts:
- alsa
- jack
- pipewire

Audio output devices:
[0] Built-in Audio Analog Stereo
[1] USB Audio Interface

Audio input devices:
[0] Built-in Audio Analog Stereo
[1] USB Audio Interface
```

### CPAL startup options

```text
--cpal-output-device <name-or-index|default|none>
--cpal-input-device <name-or-index|default|none>
--cpal-sample-rate <hz|default>
--cpal-buffer-size <frames|default>
--cpal-input-channels <n|all>
--cpal-output-channels <n|all>
--cpal-capture-ring-frames <frames>
```

Recommended defaults:

```text
--cpal-output-device default
--cpal-input-device default
--cpal-sample-rate default
--cpal-buffer-size default
--cpal-input-channels all
--cpal-output-channels all
--cpal-capture-ring-frames 4096
```

Output-only mode should be possible:

```text
--backend cpal --cpal-input-device none
```

### MIDI enumeration

```text
--list-midi-devices
```

Example output:

```text
MIDI input ports:
[0] Launchkey Mini MK3 MIDI
[1] Midi Through Port-0

MIDI output ports:
[0] FluidSynth
[1] Midi Through Port-0
```

### midir startup options

```text
--midir-input <name-or-index|all|none>      # repeatable
--midir-output <name-or-index|all|none>     # repeatable
```

Possible defaults:

```text
--midir-input all
--midir-output all
```

or the safer alternative:

```text
--midir-input none
--midir-output none
```

Prefer `all` for usability if opening MIDI ports has no disruptive side effects and no routing happens until the user connects virtual ports in the GUI.

## Backend implementation plan

1. Add `AudioDriverType::Cpal`.
2. Add backend name mapping for `cpal`.
3. Add CLI parsing for CPAL/midir options and device-listing commands.
4. Add a CPAL/midir settings struct in the Rust engine app-backend interface.
5. Add a virtual external-port registry/router in the Rust engine app-backend interface.
6. Make `find_external_ports()` return CPAL/midir virtual external ports for the CPAL backend.
7. Make `connect_external_port()` and `disconnect_external_port()` update the router for CPAL backend ports.
8. Implement CPAL callback integration against the existing `BackendSession`.
9. Implement midir input/output integration into the same router.
10. Keep JACK and Dummy behavior unchanged.
11. Keep QML unchanged except where needed to expose the new backend name and CLI-provided settings.

## Testing plan

Add tests at multiple levels:

1. Router unit tests without hardware:
   - virtual external ports are listed with correct direction/type
   - connecting/disconnecting updates connection state
   - audio routing maps capture channels to Shoop input ports
   - audio routing maps Shoop output ports to playback channels
   - MIDI routing maps midir inputs to Shoop MIDI input ports
   - MIDI routing maps Shoop MIDI output ports to midir outputs

2. Simulated callback tests without hardware:
   - run a fake CPAL cycle through the router and session
   - verify loop recording from virtual capture input
   - verify loop playback reaches virtual playback output
   - verify MIDI input is staged and recorded
   - verify MIDI output is collected for sending

3. Existing hardware-dependent tests:
   - keep `shoop_engine` CPAL tests skip-if-no-device
   - keep midir virtual-port tests skip-if-unavailable

4. Application-level regression test:
   - start CPAL backend in a mock/simulated mode if possible
   - verify external ports are visible through the same QML/backend API as JACK external ports
   - verify connections can be made through existing connection controls

## Non-goals

- Do not make tracks, loops, or channels aware of CPAL/midir.
- Do not introduce a separate CPAL-specific routing UI.
- Do not require JACK-style free routing from CPAL itself; emulate the needed subset in the backend.
- Do not promise sample-accurate MIDI timing for midir.

## Open questions

- Should default MIDI startup be `all` or `none`?
- Should CPAL device selection use exact names, indices, or stable stored IDs where available?
- Should CPAL allow separate input and output devices with mismatched sample rates, or reject/resample?
- Should audio output routing sum multiple Shoop output ports into one playback channel, or reject multiple connections?
- Should virtual port names include host/API names for disambiguation?
