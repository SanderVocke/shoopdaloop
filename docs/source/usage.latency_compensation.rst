Latency compensation
--------------------

ShoopDaLoop keeps live monitoring immediate. Compensation changes which part of
a completed recording is used for playback; it does not delay the live input
path.

Open a track's **⋮** menu to edit **Recording alignment**. A track has one signed
effective recording offset:

* **Automatic** uses an exact backend value when one is available.
* **Manual** uses the entered signed frame value.
* **Automatic + trim** adds the entered signed trim to an available automatic
  value.

JACK is the only automatic provider. ShoopDaLoop accepts a JACK value only when
all relevant connected track inputs report the same exact capture latency. CPAL, dummy,
Carla, built-in synth, and browser tracks use the manual path rather than an
estimate. If an automatic value is unavailable, the menu asks for a manual
value instead of assuming zero. Changing an unarmed JACK input route refreshes
the automatic value, and loops added later inherit the current track values.

The value is latched when recording, replacement, or rendering starts. Changing
the track setting later affects the next operation and never moves an existing
take. Once a recording transition is armed, cancel it before changing the track
offset or input routes. Replacement and retrospective grab are retained only at
zero offset; record a new take when compensation is needed. Stop the loop before correcting a completed take's signed alignment with
its alignment control in the same menu. The correction must fit the raw media
retained with every channel in the take; otherwise it is rejected without
changing any channel.

Positive offsets retain and select post-record media. Negative offsets retain
pre-record media. Recording starts only after the required bounded storage has
been prepared and the required preroll has actually been captured. Content
remains unsettled while required postroll is being captured; save, export, and a
new recording wait or report that the content is still changing. If storage
preparation, preroll capture, or finalization cannot complete, the operation
fails without publishing a partially corrected take.

Normal playback, WAV/Shoop audio export, and standard/exact MIDI export use the
logical compensated loop window. There is no latency-specific raw-margin export
or bake/recovery operation.

Dry/wet operations
~~~~~~~~~~~~~~~~~~

**Processor** in the track menu is a separate non-negative render-advance value.
It is used only while playing dry through wet or recording dry into wet. Dry
media is dispatched early so delayed processor output reaches its intended
frame. Recording dry into wet writes canonical wet timing, so ordinary wet
playback does not apply the processor value again. Live monitoring remains on
the immediate path.

Pending and error text in the menu is actionable. For an unavailable automatic
value, select **Manual**. For a retention failure, reduce the offset or recording
length and retry. Wait for postroll to finish before saving or exporting.
