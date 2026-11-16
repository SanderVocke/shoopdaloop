# Retained verification contract

Verify explicit SP instruction/state inputs, emitted jobs, JP arithmetic and
sample output, media ownership, memory/cache coherence and reset/error behavior.
The TP-free `media_audio_pipeline` supplies the execution integration gate.
Exact Q8.24 samples, arithmetic ordering and persistent ramps remain required;
component tests use production math and independent vectors. Memory physical
allocation can differ while logical content and ownership invariants remain fixed.

Logical SP records are supplied as explicit fixtures in isolated audio tests.
The integrated `sync` suite instead initializes owned loop state and writes the
two SP plan slots directly, comparing wraps, pulses, mode transitions, countdowns,
slot outcomes and render/next cursors against Rust. A native-resolver oracle
independently checks the functional SP. TP request admission, retained completion
lifecycles, ARM packets and an application FPGA backend are outside this contract.
Production CPU scenarios retain independent logical and numerical checks.

All 75 original verification identities are accounted for in the
[coverage inventory](../../src/rust/rt_fpga_testbench/coverage.json), including
explicit retirement of 13 TP/host obligations. See the
[run guide](../../src/rust/rt_fpga_testbench/README.md) and
[retirement ledger](../../docs/fpga/retirement-ledger.md).
