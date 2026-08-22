Tracks
------

Adding a track
~~~~~~~~~~~~~~

Use **Add Track** to create a **Regular**, **Trigger**, or **Dry + Wet** track.
Trigger tracks have no audio or MIDI channels. The dialog configures the display
name, audio-channel counts, optional MIDI, and, for processed tracks, the
processor kind.

Native processor choices are External, Tiny Synth/FX, OxiSynth, and feature-dependent
Carla modes. Browser builds offer Tiny Synth/FX and OxiSynth. New Dry + Wet tracks use one
shared audio-channel count for their matched dry inputs and wet outputs. Tiny
Synth/FX additionally requires one MIDI input.

Track controls
~~~~~~~~~~~~~~

Input gain affects monitored and recorded input. Input mute disables monitoring
without discarding recording input. The top-bar exclusive-input toggle makes
enabling one track's input monitoring mute all other track inputs, which is
useful when switching recording tracks. Output gain and mute affect monitored
and played-back output. Stereo sides expose balance controls. Meters and MIDI
activity indicators summarize applicable ports.

A track title can be edited after creation. Its stable port-name base does not
change when the title changes.

Connections and processors
~~~~~~~~~~~~~~~~~~~~~~~~~~

Open **Connections...** from a track menu for a track-scoped port matrix, or use
the main-menu Connections action for all tracks. External dry/wet tracks expose
dry input/send and wet return/output ports. Hosted processors keep their
internal endpoints private while exposing applicable dry inputs, wet outputs,
and dry MIDI.

The all-tracks Connections dialog also exposes **Global FX Control MIDI In**.
CC 0–119, channel pressure, and pitch bend on all MIDI channels fan out to every
MIDI-capable FX processor without being recorded, replayed, or turned into
automation. Notes, poly pressure, program changes, channel-mode CC 120–127,
system messages, SysEx, malformed messages, and other event-like traffic are
filtered. Sleeping processors keep only the latest value for each supported
control and apply it when normal processing resumes; the controller does not
wake their DSP. A dense restore is bounded and may finish over several active
audio blocks.

Connecting one controller to both this port and a track MIDI input is additive:
absolute controls can be applied twice, relative encoders may behave incorrectly,
and only the regular track copy can be recorded. Some host MIDI APIs cannot open
the same hardware endpoint twice; in that case the failed connection remains
unconfirmed and is reported in the matrix.

Control interpretation remains processor-owned. Configure Carla parameter
mappings in Carla. Tiny Synth/FX and External chains respond only to controls
they already support; the global port does not add a mapping editor or fixed CC
assignments.

Processed-track controls show only capabilities advertised by the selected
processor. Tiny Synth/FX uses an embedded editor. Carla tracks expose lifecycle,
UI, recovery, state, and bounded process-log controls when available.

OxiSynth is an embedded SoundFont instrument. Its track shape is fixed at two
dry audio inputs, two wet audio outputs, and one MIDI input; the synth ignores
the dry audio samples. Choose one preset in its embedded editor. OxiSynth merges
all source MIDI channels into one instrument, ignores MIDI bank/program changes,
and saves the selected preset with the session.
