Latency compensation
--------------------

ShoopDaLoop keeps live monitoring immediate. Compensation changes which part of
a completed recording is used for playback; it does not delay the live input
path.

Open a track's **⋮** menu and choose **Latency compensation**. The dialog's
**Recording alignment** section controls one signed effective recording offset:

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
zero offset on every affected channel; record a new take when compensation is
needed. Stop the loop before correcting a completed take's signed alignment with
its alignment control in the same dialog. The common control applies one delta
to every channel so dry/wet differences remain intact. Processed takes also show
a processor-alignment correction; it applies one atomic delta to Wet channels
only and preserves differences within each channel group. Alignment, start-offset,
and length edits must fit the raw media retained with every channel in the take;
otherwise they are rejected without changing any channel. This also applies when
an import updates a loop's length while retaining other channels. Waveform and MIDI
details show these aligned raw-media coordinates, while timeline edits are
translated back to the stored media layout. Successful alignment edits reload
these coordinates, and a browser-backend rejection reloads the authoritative
timeline instead of leaving the attempted edit displayed.

Positive offsets retain and select post-record media. Negative offsets retain
pre-record media. Recording starts only after the required bounded storage has
been prepared and the required preroll has actually been captured. Content
remains unsettled while required postroll is being captured; save, export, a new
recording, playback that would outrun the retained media, and waveform/details
reads wait or report that the content is still changing. If storage
preparation, preroll capture, or finalization cannot complete, the operation
fails without publishing a partially corrected take.

Normal playback, WAV/Shoop audio export, and standard/exact MIDI export use the
logical compensated loop window. There is no latency-specific raw-margin export
or bake/recovery operation.

Dry/wet operations
~~~~~~~~~~~~~~~~~~

**Processor latency** is independent from Recording alignment and has its own
**Automatic**, **Manual**, and **Automatic + trim** selector. Its effective value
is non-negative. Unsupported detector paths, including Carla, use an automatic
baseline of zero; ShoopDaLoop does not inspect Carla plugins or graphs. Enter the
known processor delay with Manual, or add it as a positive trim to the zero
baseline.

For an ordinary simultaneous dry/wet recording, Direct and Dry channels use the
signed recording offset ``R`` while Wet channels use ``R + P``, where ``P`` is
the effective processor latency. Storage is prepared separately for those
channel windows, so a take may require dry preroll and wet postroll at the same
time. Live input remains immediate; recording compensation changes retained
media and per-channel annotations rather than delaying monitoring.

While playing dry through wet or recording dry into wet, dry media is dispatched
``P`` frames early so delayed processor output reaches its intended frame.
Recording dry into wet writes canonical wet timing. Normal wet playback and
logical export use each Wet channel's stored capture alignment and never apply
``P`` a second time.

Pending and error text in the dialog is actionable. For an unavailable recording
automatic value, select Manual. For an invalid processor result, enter a
non-negative Manual value or reduce the trim. For a retention failure, reduce the
offset, processor latency, or recording length and retry. Wait for postroll to
finish before saving or exporting.
