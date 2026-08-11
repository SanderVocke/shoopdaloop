Tracy profiling and capture
---------------------------

The native application exposes Rust tracing data to Tracy 0.13.1.

Modes
~~~~~

Disabled
  Omit tracing options. Gated helpers avoid Tracy calls.

Coarse
  Use ``--tracing`` for a live profiler or ``--tracing-capture`` for a file.
  This includes GUI/application spans, engine control/graph work, and bounded
  callback/session categories.

Engine detail
  Add ``--tracing-engine-detail`` to either mode for per-stage realtime zones.
  This increases callback overhead and capture size.

Run a live profile::

  cargo run -p shoopdaloop -- --tracing

Capture a file using ``TRACY_CAPTURE_TOOL`` or ``tracy-capture`` on ``PATH``::

  TRACY_CAPTURE_TOOL="$(command -v tracy-capture)" \
    cargo run -p shoopdaloop -- \
      --tracing-capture \
      --tracing-engine-detail

Quit normally so capture shutdown finalizes the numbered ``.tracy`` file below
``traces/``. The directory also contains ``manifest.tsv`` with a generic
``label`` column and ``tracy-capture.log``. Require a non-empty capture, a
successful manifest row, a saved-trace log entry, and a successful Tracy parser
check before analysis.

Trace structure
~~~~~~~~~~~~~~~

``frontend.egui.*``
  GUI initialization, updates, rendering, settings actions, and intent creation.

``frontend.app.*``
  Intent dispatch/handling/application, backend advancement, snapshot
  application/publication, and runtime lifecycle. ``intent_id`` correlates
  submission with actor-side handling.

``engine.control.*`` and ``engine.graph.*``
  Bounded commands, waits, topology construction, scheduling, and graph apply.

``engine.rt.*``
  Driver/callback/cycle/session hierarchy. Detail mode adds fixed port, channel,
  composite, MIDI, routing, and processor stages.

``worker.*`` and ``engine.plugin.*``
  Background application/graph/plugin work and native processor operations.

Tracing is diagnostic instrumentation, not a transparent realtime measurement.
Tracy may allocate or lock internally and can change callback timing or cause
xruns. Start with coarse mode, enable detail only when needed, and compare like
capture modes.

The repository Tracy skill at ``.agents/skills/tracy/SKILL.md`` documents the
ShoopDaLoop-specific query and interpretation workflow.
