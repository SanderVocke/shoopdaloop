Tracy profiling and capture
---------------------------

The native application exposes Rust tracing data to Tracy 0.13.1.

Modes
~~~~~

Disabled
  Omit tracing options. Gated helpers avoid Tracy calls.

Coarse
  Use ``--tracing`` to capture in process. This includes GUI/application spans,
  engine control/graph work, and bounded callback/session categories. There is
  no live TCP profiler mode or external capture executable.

Engine detail
  Add ``--tracing-engine-detail`` for per-stage realtime zones. This increases
  callback overhead and capture size.

Capture a file::

  cargo run -p shoopdaloop -- \
    --tracing \
    --tracing-engine-detail

Quit normally so application and engine workers quiesce and capture shutdown
atomically publishes the numbered ``.tracy`` file below ``traces/``. Require a
non-empty capture, no corresponding ``.partial`` file, and a successful Tracy
parser check before analysis. Abort, fatal signals, forced termination, OOM,
and power loss cannot finalize an in-process trace.

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
