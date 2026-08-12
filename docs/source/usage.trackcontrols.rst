Tracks
------

Adding a track
~~~~~~~~~~~~~~

Use **Add Track** to create either a **Regular** or **Dry + Wet** track. The
dialog configures the display name, audio-channel counts, optional MIDI, and,
for processed tracks, the processor kind.

Native processor choices are External, Tiny Synth/FX, and feature-dependent
Carla modes. Browser builds offer Tiny Synth/FX. Tiny Synth/FX requires matched
dry/wet audio counts and one MIDI input; External and Carla tracks allow
independent dry and wet counts.

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

Processed-track controls show only capabilities advertised by the selected
processor. Tiny Synth/FX uses an embedded editor. Carla tracks expose lifecycle,
UI, recovery, state, and bounded process-log controls when available.
