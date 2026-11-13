# Queue-driven mono loop validation

2026-11-12–13, Linux x86_64, Rust 1.99.1, GCC 14.2, Verilator 5.032,
Marlin 0.21.1 with the existing owned-context patch, Yosys 0.52.
Implementation branch: `feat/fpga-mono-looper`; upstream checked through
`bc833fc3` (`proto/fpga-rust-reference`).

## Differential coverage

Six checked-in queue traces cover 132 quanta; two seeded traces add 96 quanta.
Every output word matches production Q8.24 exactly. Queries, transition events,
revisions, media metadata, resource-read bytes, cursor positions, completion and
execution receipts, requested/delivered/terminal frames and graph/topology epochs
are compared at native observation cuts. Scoped opaque handles use a bijective
correspondence instead of comparing their incidental slot numbers.

The principal 80-quantum trace records and exports two different-length takes,
retains the first across re-recording, reads 896 bytes through owning queued
resource requests, checks partial words/page boundaries/EOF, releases and tests
stale handles, then repeatedly records into an eight-chunk arena. Its three gain
changes and all loop transitions leave one compiled program installed. Other
traces cover empty playback, supersession, unread result pressure, pending/export/
read-buffer exhaustion and the difference between playback and latest media
when recording restarts, plus delivered cancellation and recovery/normal ordering.

Protocol doubles check actual configuration traffic, stable media/state bindings,
settlement backpressure, delayed query completion, stale identities/epochs,
unsupported operations/timing, live-edit refusal, ownership return and physical
failure handling. They implement no sample processing or loop state machine.
Real RTL tests cover sticky SP faults, arena exhaustion and settlement failure
with retained diagnostics/reset. Device-capacity faults are tested separately
from comparisons with the elastic native store.

The existing gain-only queue adapter, chained/fan-in gain replays, direct loop
regression, silence/reset/backpressure and odd-depth FIFO tests remain in place.

## Measurements and reproduction

The 80-quantum trace took 66,469 total simulation clocks: reset/configuration,
control/media traffic, deterministic stalls, reads and audio processing included.
It installs five SP instructions, three state words and 451 configuration payload
bytes. Payload accounting excludes framing, verification readbacks and clock waits.

An initial model build/load measured 45.485 s; a cached model load measured
102.621 ms. Graph validation/lowering measured 48 µs and 36 µs respectively;
configuration measured 717 µs and 1,418 µs. These individual host observations
were taken with other validation jobs running. They are not distributions, ARM
service-time guarantees, DDR timing estimates or isolated JP cycle counts.

```sh
cargo run -p rt_fpga_sim --features sim --bin fpga-mono-demo -- \
  src/rust/rt_fpga_sim/fixtures/mono-loop.json work/marlin-mono work/mono.vcd
cargo test -p rt_fpga_sim --features sim
cargo test -p rt_fpga_adapter -p rt_backend_testbench -p rt_runtime -p shoop_loops --lib
```

`result.json` records canonical outcomes and metrics. A fresh output directory
builds the C++ model; reusing it caches that model independently of graph/control
data. The mono CLI/build/replay invokes no Python. The complete RTL regression
suite also runs natively through `rt_fpga_testbench`.

## Local gates

- All 16 Marlin/native-contract/protocol integration tests pass, including the
  five existing regressions and the new settlement-error/reset check. The final
  six-fixture/two-seed replay gate also passes after adding recovery ordering.
- Focused Rust library gates pass: 96 runtime, 24 loop, six gain-adapter and four
  production-fixture tests, including the existing Q8.24/float accuracy check.
- Warning-denying workspace and feature-enabled simulator builds pass, as do
  formatting and the Rust test-attribute policy. Both the native application and
  Wasm worklet dependency trees exclude Marlin/Verilator. The application and
  worklet both build for `wasm32-unknown-unknown` with warnings denied.
- Independent media-store ownership/protocol tests pass with odd capacities,
  four random cases and fixed memory latency. Nineteen production/RTL scenarios
  pass with stalls, fixed memory latency and fault injection, with the existing
  production float/Q8.24 accuracy checks included.
- Generated AXI tops are current. Media store and media/audio pipeline lint,
  generic synthesis and ECP5 mapping pass. Full loop pipeline lint/generic checks
  pass for default, small and minimum configurations, as does default ECP5 mapping.
  The actual mono profile (`REUSE_VIEWS=1`, eight views, 128 slots, TP 4/8/16)
  additionally passes warning-denying lint and generic synthesis/invariant checks.
  These are synthesis checks, not placement/routing or timing closure.

The broad native command was:

```sh
SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace \
  --features shoop_engine/app_backend,shoop_backend/native-fx --profile ci
```

It ran 3,222 tests: 3,214 passed, eight failed and eleven were skipped. The eight
failures were four LAN profile-publication deadlines, two host recovery/startup
deadlines and two native UI driver/settings deadlines. All eight passed on one
isolated retry with the same feature selection, `--test-threads 1` and no
concurrent builds (93.670 s). The first broad run is not represented as clean.
Its compilation took 8m53s and execution 890.580s; the isolated retry built the
same-feature application test binary once before running the selected tests.
PR CI is deliberately not inspected. The previously waived 64-loop benchmark is
not repeated. This milestone does not establish an audio deadline or integrate
the profile into the GUI, a board endpoint or DRAMsim3.


## Delivery

All work is committed and pushed on `feat/fpga-mono-looper`.
[PR #981](https://github.com/SanderVocke/shoopdaloop/pull/981) targets
`proto/fpga-rust-reference`. Review submissions, inline comments and conversation
comments were checked; no actionable findings were present at delivery. Only the
working branch was pushed. No PR CI was inspected and no merge was attempted.
