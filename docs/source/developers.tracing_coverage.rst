Rust tracing coverage inventory
===============================

``docs/tracing_coverage.csv`` accounts for every production Rust module in the
retained workspace. Cargo integration tests and example/benchmark binaries are
validation tools and are intentionally outside the production inventory.

Classifications
---------------

``instrumented_direct``
  The module contains gated tracing at a meaningful runtime boundary.

``instrumented_indirect``
  The module executes inside a named application, UI, engine, worklet, or
  persistence boundary; another span would duplicate the owning operation.

``excluded``
  Build-time or logging-declaration code for which runtime instrumentation does
  not apply or would recurse.

``planned_direct`` and ``planned_indirect`` are temporary classifications. The
final check rejects them.

Validation
----------

Run during source changes::

  python3 scripts/check_tracing_coverage.py

The merge gate requires a closed inventory::

  python3 scripts/check_tracing_coverage.py --require-closed

The verifier compares exact tracked Rust module paths, rejects duplicate or
stale rows, requires a context and rationale, validates classifications, and in
closed mode rejects planned rows. Instrumentation behavior is additionally
covered by tracing gates, realtime allocation tests, native capture lifecycle
tests, and manual capture/parser checks; inventory completeness alone is not a
performance or realtime-safety proof.
