# Retained verification contract

Verify explicit SP instruction/state inputs, emitted jobs, JP arithmetic and
sample output, media ownership, memory/cache coherence and reset/error behavior.
The TP-free `media_audio_pipeline` supplies the execution integration gate.
Exact Q8.24 samples, arithmetic ordering and persistent ramps remain required;
component tests use production math and independent vectors. Memory physical
allocation can differ while logical content and ownership invariants remain fixed.

Logical SP records are supplied as explicit fixtures. No musical transitions,
request arbitration, host receipts, ARM packet interface or full application FPGA
backend is under test. Production CPU scenarios retain their own logical behavior
and float/fixed numerical checks without an RTL image compiler.

All 75 original verification identities are accounted for in the
[coverage inventory](../../src/rust/rt_fpga_testbench/coverage.json), including
explicit retirement of 13 TP/host obligations. See the
[run guide](../../src/rust/rt_fpga_testbench/README.md) and
[retirement ledger](../../docs/fpga/retirement-ledger.md).
