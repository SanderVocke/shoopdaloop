Rust tracing coverage and baseline
==================================

Coverage inventory
------------------

``docs/tracing_coverage.csv`` is the machine-checkable inventory for production
Rust modules. Its classifications are:

``instrumented_direct``
  The module contains gated spans/events at its meaningful runtime boundary.

``instrumented_indirect``
  The module's work is visible inside the named orchestration/category caller in
  ``coverage_or_rationale``. A separate zone would duplicate a leaf operation
  or add noise to a hot loop.

``planned_direct`` / ``planned_indirect``
  Temporary implementation classifications. The closed inventory contains none;
  ``--require-closed`` rejects either value.

``excluded``
  Build-time/generated declaration code or logging glue for which runtime
  instrumentation is inapplicable or unsafe. Every exclusion has a rationale.

The inventory intentionally includes build scripts, proc macros, binding
generators, and CXX-Qt bridge declarations so their exclusion remains explicit.
Cargo integration tests under ``tests/`` are validation code and are not counted
as production modules. The self-test runner under the frontend source tree is
runtime code and is counted.

Validate completeness while implementing with::

  python3 scripts/check_tracing_coverage.py

The final coverage audit additionally rejects unresolved planned rows::

  python3 scripts/check_tracing_coverage.py --require-closed

Baseline at ``484f36d4``
------------------------

The baseline was recorded on 2026-08-03 before engine tracing was added. It is
not a performance budget; it records what the existing trace can and cannot
explain.

Build and allocation tests
~~~~~~~~~~~~~~~~~~~~~~~~~~

- ``RUSTFLAGS="-D warnings" cargo build`` passed.
- ``cargo test -p shoop_engine --test no_alloc`` passed 19 tests in 0.01 s.
  The minimal-feature test build emitted one pre-existing ``dead_code`` warning
  for ``EngineHandle::connected_flag``; the warning-free application build did
  not.

Targeted tracing-disabled QML run
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

Command::

  target/debug/shoopdaloop_dev.sh \
    --no-crash-handling \
    --self-test \
    --test-files-pattern src/qml/test/tst_TwoLoops.qml

Result: 6 passed, 0 failed, 0 skipped, exit 0. A single host sample took 6.531 s
and reached 849104 KiB maximum RSS. No ``tracy-capture`` process was started.
These host figures are diagnostic samples only and are not stable thresholds.

Targeted captured QML run
~~~~~~~~~~~~~~~~~~~~~~~~~

Command::

  target/debug/shoopdaloop_dev.sh \
    --no-crash-handling \
    --tracing-capture \
    --tracing-capture-tool "$(command -v tracy-capture)" \
    --tracing-capture-output-dir /tmp/shoop-tracing-plan-baseline \
    --self-test \
    --test-files-pattern src/qml/test/tst_TwoLoops.qml

Result: 6 passed, 0 failed, 0 skipped, exit 0. Tracy 0.13.1 produced a
9,566,591-byte capture. Its SHA-256 is
``a618c7c44fc671c206f48b3b7341a81c8cd8e27e438c9578bf3519b92f2f95fb``.
The local baseline capture and logs are retained under
``/tmp/shoop-tracing-plan-baseline`` and
``/tmp/shoop-tracing-plan-baseline-run.log`` for before/after comparison during
this implementation.

``tracy-csvexport`` parsed the capture successfully. It contained only three
named CPU-zone aggregates:

- ``reload_qml{path=src/qml/test/tst_TwoLoops.qml}``
- ``load_qml{path=src/qml/test/tst_TwoLoops.qml}``
- ``unload_qml``

It also contained frontend frame/plot samples, but no engine callback, command,
graph, session, loop, channel, port, composite, or FX CPU zones. There were no
Tracy instrumentation failures. Mesa reported unavailable AMD/DRI acceleration;
the offscreen QML test and capture still passed.

A rotation run with ``src/qml/test/tst_*Loops.qml`` passed all 35 tests and
produced exactly two manifest rows and two parseable captures in
``/tmp/shoop-tracing-plan-baseline-rotation``: 44,381,462-byte
``0001-tst_ThreeLoops.tracy`` and 9,104,295-byte
``0002-tst_TwoLoops.tracy``. Both outcomes were ``passed``; no capture process
remained and no Tracy instrumentation failure was reported.

The absence of engine zones means a tracing-enabled engine timing baseline
cannot be extracted from the existing trace. Existing deterministic engine
profiling and the 19 allocation tests are therefore the behavioral baseline;
the final comparison must demonstrate newly visible engine timing without
changing their results.

Thread and callback entrypoints
-------------------------------

The initial thread/callback audit identified these production boundaries. This
list is used to assign names and lifecycle spans; driver-owned callback threads
may only be named where the owning API permits it.

- ``common::tracing_capture``: external ``tracy-capture`` child process start,
  connection wait, stop, and reap.
- ``crashhandling::client``: client worker and crash-server child process;
  signal/exception and ``atexit`` handlers remain uninstrumented.
- ``crashhandling::server``: per-connection worker threads.
- Frontend: async task, audio-waveform render, click-track generation, and dummy
  process-helper worker threads; Qt queued callbacks and the main event thread.
- ``shoop_engine::graph_scheduler``: named graph-schedule worker.
- ``shoop_engine::app_backend``: connection-cache workers, composite
  acknowledgement/reclamation workers, dummy driver thread, JACK process
  callback, CPAL input/output callbacks, and backend lifecycle callbacks.
- ``shoop_engine::engine``: standalone/test driver threads plus ``process``,
  ``run_cycle``, and ``pump`` callback-boundary entrypoints.
- ``shoop_engine::cpal_mock``: deterministic input/output callback threads.
- ``shoop_engine::midir_driver``: external MIDI input callback and audio-cycle
  staging.
- ``shoop_engine::lv2_carla``: plugin runtime worker and process boundary.
- ``shoop_engine::session`` and processing modules: session callback, scheduled
  port/channel/loop/composite/FX stages, and state publication.

Realtime allocation-guard boundaries currently enter at
``Engine::process``, ``Engine::run_cycle``, and ``Engine::pump`` through
``realtime_alloc_guard::forbid_alloc_if_enabled``. Direct Tracy calls added
inside those regions must use the documented tracing gate and narrow
``allow_alloc`` exception; the rest of each region remains guarded.
