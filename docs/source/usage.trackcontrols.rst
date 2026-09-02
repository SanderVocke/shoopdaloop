Tracks
------

Adding a track
~~~~~~~~~~~~~~

Use **Add Track** to create a **Regular**, **Trigger**, or **Dry + Wet** track.
Trigger tracks have no audio or MIDI channels. The dialog configures the display
name, audio-channel counts, optional MIDI, and, for processed tracks, the
processor kind and default playback mode. The **make default** option stores the
complete draft for the next track. Dry + Wet tracks may use regular wet playback
or replay their dry recording through live effects as their default; other track
types always use regular playback.

Native processor choices are External, **Built-in FX**, **Built-in Synth**, and
feature-dependent Carla modes. Browser builds offer both built-ins. Built-in FX
accepts matching mono, stereo, or higher dry/wet audio counts and requires one
MIDI input. Built-in Synth has two dry inputs, two wet outputs, and one MIDI
input.

Track controls
~~~~~~~~~~~~~~

Input gain affects monitored and recorded input. Input mute disables monitoring
without discarding recording input. The top-bar exclusive-input toggle makes
enabling one track's input monitoring mute all other track inputs, which is
useful when switching recording tracks.

The separate top-bar **Auto-arm track inputs** toggle defaults on. While script
composites run, it enables input monitoring for each track one sync cycle before
a child loop records or replaces and keeps it enabled through that capture.
Simultaneous recordings may monitor multiple tracks regardless of the
exclusive-input setting. Afterward, auto-arm remutes only tracks that it enabled;
tracks monitored beforehand remain monitored. This cycle-ahead application
control is intentionally not sample-exact.

Output gain and mute affect monitored and played-back output. Stereo sides
expose balance controls. Meters and MIDI activity indicators summarize
applicable ports.

A track title can be edited after creation. Its stable port-name base does not
change when the title changes. A Dry + Wet track's options menu can change its
default playback mode without affecting playback already in progress.

The default loop action resolves that preference whenever it would start a
primitive loop's playback. Explicit Play, Play Dry Through Wet, and script event
modes remain explicit. Regular composites have only ordinary playback: each
scheduled child uses its owning track's current default when triggered. Script
composites retain an explicit mode on every scheduled event.

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
mappings in Carla. External chains respond only to controls they already support.
Built-in Synth provides MIDI Learn for its reverb-send and chorus-send controls.
Built-in FX provides the same learn/assign/remove workflow for every continuous
rack control. It has no default mappings; toggles and type selectors are not
MIDI targets. Notes and unsupported messages on its MIDI input are ignored.

Processed-track controls show only capabilities advertised by the selected
processor. Both built-ins use embedded editors. Carla tracks expose lifecycle,
UI, recovery, state, and bounded process-log controls when available.

Built-in FX is powered by `FunDSP <https://github.com/SamiPerttu/fundsp>`_. Its
fixed order is Compressor, Drive, three-band EQ, Chorus, Modulation, then
Reverb. Drive offers Saturation, Overdrive, Distortion, and Fuzz; Modulation
offers Tremolo, Flanger, and Phaser; Reverb offers Room, Hall, and Plate. Each
stage has a toggle and a compact set of continuous controls in the embedded
editor. Mono is processed as mono, exactly two channels use linked/decorrelated
stereo behavior, and larger channel counts remain isolated. A disabled stage
does not run effect DSP; disabling or changing a stateful stage discards its old
tail. Rack values and learned CC assignments are saved with the session, while
tails, LFO phase, and editor visibility are transient.

Built-in Synth is powered by OxiSynth and the embedded SoundFont. Its track shape
is fixed at two dry audio inputs, two wet audio outputs, and one MIDI input; the
synth ignores the dry audio samples. Choose one preset in its embedded editor.
Preset-authored reverb and chorus sends remain active, while the two send controls
add up to the standard CC 91/93 modulation range and can learn any exact source
channel/CC pair. Only modulation (CC 1), expression (CC 11), sustain (CC 64),
pitch bend, and supported non-CC note/pressure messages reach OxiSynth itself;
other CC, bank, and program messages are filtered after MIDI Learn observes them.
All accepted source channels feed one logical instrument. The selected preset,
sends, and assignments are saved with the session.
