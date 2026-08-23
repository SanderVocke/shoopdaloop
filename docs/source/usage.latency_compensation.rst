Latency compensation
--------------------

ShoopDaLoop keeps live monitoring immediate. Compensation changes where recorded
media is selected and when dry loop media is dispatched to a processor; it does
not intentionally delay the current input-monitor path.

Open a track's **⋮** menu and choose **Latency compensation...**. Policies affect
future operations. Existing takes retain the observation and component choices
that were latched when they were made.

Components
~~~~~~~~~~

**External capture** is the measured input path, such as a JACK capture range.
**Processor / FX** is input-to-output algorithmic delay for External, Carla, or
Built-in Synth processing. **Cue / output** applies only when **Performance
followed Shoop cue** is enabled and a normalized application or connected host
output is selected. **Backend buffering** is a separately scoped hop and must
not be enabled when the provider's end-to-end path already includes it.
**Manual correction** is an explicit frame value.

Each component can be disabled, use its automatic observation, replace that
observation manually, or use automatic plus a signed trim. Positive values move
the selected captured media earlier on playback. A positive processor render
advance dispatches dry media earlier so delayed wet output lands on the intended
frame. A negative trim reduces the automatic value; invalid negative totals are
unresolved rather than wrapping. Values are bounded to 768,000 frames.

For a performer responding to a ShoopDaLoop output, a typical direct recording
uses ``input capture + selected cue output + manual correction``. A live wet
recording also includes the processor path. For a world-timed source that did
not follow the application cue, leave **Performance followed Shoop cue** off;
the selected output then contributes zero.

Ranges and capabilities
~~~~~~~~~~~~~~~~~~~~~~~

Exact, ranged, estimated, manual-only, and unknown values are labeled
separately. For a range, choose minimum, midpoint, or conservative maximum. The
panel shows frames and milliseconds at the active sample rate, selected policy
total, frozen take total, retained margins, and current-versus-frozen revision.

JACK publishes connected path ranges. Carla Rack and Patchbay use the versioned
adapter in the pinned runtime; an unpatched or unsupported runtime remains
unknown/manual. CPAL/midir and Web MIDI have coarse or manual timing where their
APIs provide no shared sample clock. Browser ``baseLatency``/``outputLatency``
are estimated when available and unknown otherwise. Built-in Synth reports the
validated phase-dependent 0..63 frame range rather than claiming fixed zero or
64 frames.

Dry/wet operations
~~~~~~~~~~~~~~~~~~

Ordinary playback applies only a take's frozen capture alignment.
**Playing dry through wet** additionally renders the dry take ahead by the
current processor recipe, including across callback and loop boundaries.
**Recording dry into wet** consumes that processor advance while rendering and
writes canonical wet media; later ordinary wet playback does not apply the same
delay again. Planned transitions prerender while public mode remains unchanged.
Immediate transitions visibly defer when there is not enough safe lead time.
Processor warm-up and tail work are separately bounded from declared latency.

Raw media and consolidation
~~~~~~~~~~~~~~~~~~~~~~~~~~~

Waveform and MIDI lanes show distinct raw and logical start markers. Normal WAV
and standard MIDI export uses the logical compensated loop. Explicit raw export
requires confirmation and includes retained pre/post material without mutating
the take. Exact Shoop media carry latency provenance. **Consolidate / bake**
renders the logical window into canonical media and clears compatible take
alignment; use it before destructive replacement when observations are
incompatible.

Warnings and troubleshooting
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

* **Unresolved/provider failure:** choose a manual replacement, disable the
  component, or repair the JACK/Carla/browser provider.
* **Path ambiguity:** select one application output or remove duplicate host
  routes; ShoopDaLoop does not guess.
* **Changed observation:** the active/existing take remains frozen. Compare the
  take and current revision, then consolidate or rerecord if desired.
* **Insufficient margin/finalization:** wait for postroll, reduce compensation,
  consolidate, or rerecord. Partial compensated content is not silently saved.
* **Deferred transition:** wait for safe media/render preroll; live monitoring
  remains immediate.

The diagnostics section contains bounded counters and fixed-history plots for
capture alignment, render advance, and active postroll. See
``../latency_diagnostics.md`` and ``../latency_design_evidence.md`` for provider
validation and physical test procedures.
