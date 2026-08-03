Tracy profiling and QML test captures
--------------------------------------

ShoopDaLoop can expose its Rust tracing data to Tracy 0.13.1. Start the
application with ``--tracing`` and connect the Tracy profiler for a live
session. This enables coarse engine callback zones as well as non-realtime
application spans. Add ``--tracing-engine-detail`` for more intrusive per-node
engine zones; that option requires ``--tracing`` or ``--tracing-capture``.

Tracing is a debugging mode, not a transparent realtime measurement mode.
Direct Tracy calls in audio/MIDI callbacks may allocate or lock inside Tracy and
may cause xruns or alter callback timing. With all tracing options omitted, the
realtime helper returns after atomic gate checks without calling Tracy.

To capture QML self-tests to files, install the Tracy 0.13.1
``tracy-capture`` executable and run, for example::

  target/debug/shoopdaloop_dev.sh \
    --tracing-capture \
    --tracing-capture-tool "$(command -v tracy-capture)" \
    --tracing-capture-output-dir traces/qml \
    --self-test

``--tracing-capture`` enables tracing automatically. The tool can alternatively
be selected with the ``TRACY_CAPTURE_TOOL`` environment variable or found on
``PATH``. The output directory defaults to ``traces``.

A self-test run writes one numbered ``.tracy`` file per loaded ``tst_*.qml``
test file, plus ``manifest.tsv`` and ``tracy-capture.log``. The capture remains
active while that file's QML engine is unloaded and is finalized before the
next file starts. Open a capture with the matching Tracy 0.13.1 profiler.

CI capture is intentionally opt-in. Manually dispatch the ``Build and test``
workflow and enable ``qml_trace_capture`` to run the dedicated Linux capture
job. Its uploaded archive contains the captures, manifest, capture log, QML
console output, JUnit report, and workflow metadata. The input defaults to
false, does not run for automatic workflow events, and artifacts are retained
for 30 days.
