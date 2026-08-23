Loops
-----

Transitions
~~~~~~~~~~~

Primitive loops can play, record, stop, and grab. Dry/wet loops additionally
support playing recorded dry content through the processor and recording that
processed result into wet content. Synchronized actions wait for the sync or
target loop; immediate actions do not.

Selection applies a transition to multiple loops. Solo mode stops competing
loops in affected tracks. Auto-play determines whether a completed recording or
grab starts playback. The record-cycle control sets a fixed duration; zero is
unbounded.

Playback controls
~~~~~~~~~~~~~~~~~

Each loop has playback gain and, for stereo content, balance. The status area
shows current mode, pending transitions, selection, targeting, and loop
progress. A double click on the status area targets a loop. Touch mode can be
toggled in the Appearance settings. In the browser it defaults on when the device
has no hover capability; native builds default it off. In touch mode, the play,
record, and stop controls are always visible, hover-only action variants are
unavailable, and a stationary long touch opens the loop's context menu.

Grabbing
~~~~~~~~

Grab copies recent input from bounded always-on recording buffers. In
synchronized mode it ends at the most recently completed sync boundary. In
immediate mode it includes the current interval and records its remainder. A
targeted loop can provide alignment instead of the global sync loop.

Click-track generation
~~~~~~~~~~~~~~~~~~~~~~

Right-click a primitive audio or MIDI loop and choose **Generate click
track...**. The dialog supports primary/secondary audio sounds or MIDI notes,
fractional tempo, click count, odd-click delay, and fitting to an existing loop
length. Preview is non-mutating. Generated media is saved as ordinary loop
content.

Files
~~~~~

The main menu saves and loads versioned ``.shoop`` sessions. Loop context menus
import or export exact ``.shoop-audio``/``.shoop-midi``, float WAV, and standard
MIDI. Normal exports use the logical latency-compensated loop. Explicitly
labeled raw exports include retained latency margins; exact Shoop media also
preserve take provenance. Different sample rates require confirmation before
deterministic timing, provenance, and media conversion. Standard imports begin
with zero/unknown latency provenance unless a bounded manual offset is supplied.

Loop details
~~~~~~~~~~~~

Select one primitive loop and open the bottom **details** pane to inspect its
media. Audio channels appear as waveforms. MIDI channels appear as read-only
piano-roll lanes with note pitch, timing, duration, loop region, and playback
position. Mixed loops show both kinds of channel. Drag a lane horizontally to
pan and use its zoom control to change the visible time range.

The basic MIDI lane displays note messages only. Controller, pitch-bend,
pressure, program, and SysEx messages remain in the loop but are not drawn.
Inspecting details never changes loop content.

On-screen MIDI piano
~~~~~~~~~~~~~~~~~~~~

The bottom **piano** pane spans MIDI notes 0–127. Pointer presses send channel-1
notes to every monitored track with a MIDI input. Releases follow the tracks
that received the press, including after monitoring changes; pane closure and
focus loss release held notes.
