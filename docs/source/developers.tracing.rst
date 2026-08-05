Tracy profiling and QML test captures
--------------------------------------

ShoopDaLoop exposes its Rust tracing data to Tracy 0.13.1. Three modes are
available:

Disabled
  Omit all tracing options. Direct realtime helpers perform only relaxed atomic
  gate checks and never call Tracy.

Coarse
  Use ``--tracing`` for live profiling, or ``--tracing-capture`` for a file.
  This includes application/frontend spans, engine control and graph work, and
  bounded callback/engine/session/category zones.

Engine detail
  Add ``--tracing-engine-detail`` to either tracing mode. This adds static
  per-node port/channel zones, composite and MIDI sub-stages, external routing,
  plugin processing, and diagnostic publication. It deliberately increases
  callback overhead and trace volume. The option is rejected without a parent
  tracing mode.

Safety and overhead
~~~~~~~~~~~~~~~~~~~

Tracing is a debugging mode, not a transparent realtime measurement mode.
Direct Tracy calls in JACK, CPAL, dummy-driver, MIDI, and plugin callbacks may
allocate, use thread-local initialization, grow Tracy's C++ queues, or lock
inside Tracy. They may cause xruns and alter callback timing. Prewarming
engine-owned threads reduces first-use overhead where possible, but does not
prove that Tracy is bounded, allocation-free, or lock-free. Driver-owned
callback threads cannot always be entered before the host API activates them.
The client uses Tracy's portable timer fallback so non-invariant-TSC and
virtualized hosts can start safely; use the timer resolution recorded in each
trace when interpreting short zones.

Direct callback zones use static source locations and numeric values. Both the
global tracing-output gate and the engine-detail gate are checked before Tracy
is called. The smallest practical allocation-permitted scope encloses only
Tracy begin/end/value/frame operations; surrounding engine processing remains
under ``realtime_alloc_guard``. The allocation tests prove this Rust scoping,
not the behavior of Tracy's C++ internals. Existing explicitly named one-time
engine preparation scopes and the bounded recording reserve are separate
engine policies, not Tracy exceptions.

``--rt-alloc-guard`` is useful for diagnostic runs. A steady-state allocation
failure outside a named scope is a bug. Compare disabled and traced runs, but do
not interpret a traced callback duration or xrun count as the uninstrumented
performance of the engine.

Names and trace structure
~~~~~~~~~~~~~~~~~~~~~~~~~

Names are fixed by category; runtime fields do not become zone names. This
keeps cardinality bounded and avoids embedding MIDI/audio payloads, paths,
plugin state, or user labels in hot-zone names.

``app.*``
  Process startup, configuration, Qt initialization/event loop, crash handling,
  and shutdown.

``frontend.*``
  QML/Lua/file/session operations, control dispatch, rendering, object-state
  consumption, and refresh scheduling.

``engine.control.*``, ``engine.graph.*``, ``engine.composite.*``
  Non-realtime command queue, synchronous waits, graph construction/application,
  reclamation, and composite-plan work.

``engine.rt.*``
  Direct realtime hierarchy. A typical dummy callback nests
  ``engine.rt.driver`` → ``engine.rt.driver.dummy`` → ``engine.rt.callback`` →
  ``engine.rt.cycle`` → ``engine.rt.session``. Coarse session categories include
  ``loops``, ``fx``, and ``state_publication``; detail adds static
  ``ports.*``, ``channels.*``, ``composites.*``, ``midi.*``, and
  ``routing.external`` zones. Numeric zone values carry driver kind, frame count,
  or bounded arena index.

``worker.*`` and ``tool.*``
  Named background workers and packaging/support tools.

``engine.callback`` and ``frontend.refresh`` are frame marks for aligning audio
cycles with GUI state consumption. ``BackendWrapper/*`` plots publish callback
last/worst duration and budget overruns, cycles/frames, command depth and
sequence, schedule request/applied generation, stale/stuck cycles, sub-blocks,
trace drops, graph arms/applies, capture under/overruns, xruns, and DSP load.
Fixed ``engine.loop.*``, ``engine.composite.*``, ``engine.port.*``,
``engine.channel.*``, and ``engine.fx.*`` plots show the most recently consumed
object state; use the adjacent object update spans when several objects update
in the same refresh.

A useful investigation normally follows these links:

#. correlate a ``frontend.*`` control span with its ``engine.control.*`` command
   sequence and queue/wait outcome;
#. find the matching ``engine.graph.*`` arm/apply if topology changed;
#. inspect the next ``engine.rt.callback`` hierarchy and compare it with the
   callback budget, schedule-generation, stale/stuck, and sub-block plots;
#. follow ``engine.rt.state_publication`` to ``frontend.refresh.run`` and the
   object/health plots.

The deterministic engine stage profiler remains a separate facility. Explicit
tracing enables its port/channel/loop stage clocks and the frontend report uses
nanoseconds (latest cycle total, per-call average, and worst cycle). Tracy zones
do not replace its counters or semantics; agreement between profiler/state
reports and trace plots is a validation check.

QML capture files
~~~~~~~~~~~~~~~~~

Install the Tracy 0.13.1 ``tracy-capture`` executable and run, for example::

  target/debug/shoopdaloop_dev.sh \
    --no-crash-handling \
    --tracing-capture \
    --tracing-engine-detail \
    --tracing-capture-tool "$(command -v tracy-capture)" \
    --tracing-capture-output-dir traces/qml \
    --self-test

``--tracing-capture`` enables tracing automatically. The tool can alternatively
be selected with ``TRACY_CAPTURE_TOOL`` or found on ``PATH``. The output
directory defaults to ``traces``.

A self-test run writes one numbered ``.tracy`` file per loaded ``tst_*.qml``
test file, plus ``manifest.tsv`` and ``tracy-capture.log``. The capture remains
active while that file's QML engine is unloaded and is finalized before the
next file starts. Output is briefly gated during profiler rotation so a zone
cannot span two profiler connections. Capture startup, connection, graceful
SIGINT shutdown, bounded forced reap, non-empty output, and manifest outcome are
all checked explicitly.

Open captures with the matching Tracy 0.13.1 profiler. A non-interactive parse
check can use::

  tracy-csvexport capture.tracy > zones.csv
  tracy-csvexport -u -p capture.tracy > events-and-plots.csv

Also inspect ``tracy-capture.log`` and application output for ``Instrumentation
failure``. Environment failures such as unavailable ``/dev/snd/seq``, CPAL
hardware, JACK, Carla, or Mesa acceleration should be recorded separately from
trace-format or instrumentation failures.

CI capture is intentionally opt-in. Manually dispatch the ``Build and test``
workflow and enable ``qml_trace_capture_all_variants`` to capture every active
non-coverage package-test variant on Linux, macOS, and Windows. Each variant's
uploaded artifact contains captures, manifest, capture log, QML console output,
JUnit report, and workflow metadata. The input defaults to false, automatic
workflow events do not install or invoke the capture tool, failure artifacts use
``if: always()``, and artifacts are retained for 30 days. Coverage remains an
untraced negative control. Do not claim this CI path was exercised unless that
manual dispatch actually occurred.
