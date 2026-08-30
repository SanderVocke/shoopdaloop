Perfetto profiling and capture
------------------------------

ShoopDaLoop emits standard Perfetto ``.pftrace`` data through the private
``shoop_tracing`` facade on native, Window, Engine Worker, and AudioWorklet
realms.

Modes
~~~~~

Disabled
  Omit tracing options. Gated realtime helpers do not call a backend.

Coarse
  Use ``--tracing`` or **Settings > Developer**. This includes GUI/application
  spans, engine control/graph work, and bounded callback/session categories.

Engine detail
  Add ``--tracing-engine-detail`` for per-stage realtime records. This increases
  callback overhead and capture size.

Capture natively::

  cargo run -p shoopdaloop -- \
    --tracing \
    --tracing-engine-detail

A normal Save or application shutdown atomically publishes a numbered
``.pftrace`` below ``traces/``. Discard writes no file, and sequential captures
are supported. Use the pinned ``scripts/trace_processor`` wrapper for queries.

Hosted Chromium exposes the same controls and downloads application-owned trace
bytes. One capture combines Window with the active Engine Worker or
AudioWorklet. Multirealm audio tracing requires cross-origin isolation and
``SharedArrayBuffer``; serve COOP ``same-origin`` and COEP ``require-corp``.
Unsupported deployments remain functional and report why tracing is unavailable.

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

Counter tracks retain integer counts, identifiers, occupancy, and reason codes;
fractional loads/ratios use floating counters. Structured logs preserve level,
target, message, and typed fields as Perfetto arguments.

AudioWorklet timestamps are exact logical sample frames, not callback CPU
entry/exit measurements. Browser collection retains at most 262,144 complete
records per realm (12 MiB of raw records); later records are discarded and
reported in producer health rather than growing tab memory indefinitely. Always
inspect clock calibration, producer drops, discontinuities, and health data when
interpreting a browser trace.

Tracing is diagnostic instrumentation, not a transparent realtime measurement.
Start with coarse mode, compare equivalent workloads/modes, and use native CPU
tracks for callback-duration analysis.

The repository Perfetto skill at ``.agents/skills/perfetto/SKILL.md`` documents
capture, CI artifact, query, clock, and interpretation workflows.
