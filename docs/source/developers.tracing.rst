Tracy profiling and QML test captures
--------------------------------------

ShoopDaLoop can expose its frontend tracing data to Tracy 0.13.1. Start the
application with ``--tracing`` and connect the Tracy profiler for a live
session.

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
