# Runtime adapter milestone validation

2026-11-12, Linux x86_64, Rust 1.99.1, GCC 14.2, Verilator 5.032,
Marlin 0.21.1 with the documented owned-context bridge patch. Upstream FPGA
branch changes through `2b099206` were merged during implementation.

## Differential and protocol checks

- Five Marlin tests pass together, without serializing the simulations: direct
  gain/silence, odd-depth wide FIFO, queue-driven chained/parallel gains, original
  record/playback replay, and an injected real RTL fault.
- Both queued fixtures compare all 320 raw output samples exactly, plus transport
  completions, execution receipts, sequences, epochs and effective/actual frames.
  Each accepted graph compiles once. Eight typed gains write state without graph
  reloading. Inputs, graph and actions are checked-in replayable JSON.
- The original record/stop/play/wrap fixture compares nine quanta, logical loop
  state and all 64 captured samples with production Q8.24. This is a direct driver
  regression, not looping support in the queue adapter.
- Six adapter tests use recording/stalling devices without sample DSP. They check
  ownership return, activation acknowledgement, capacity/credit retention, unread
  receipts, cancellation, stale IDs/epochs, unsupported operations, invalid plans,
  silent lowering, live-edit rejection and device faults.
- The shared production-fixture tests pass (four tests), including the Q8.24/float
  oracle comparison. The affected runtime library tests pass (95 tests), with
  additional transport/scheduler integration coverage in the workspace run.

## Example measurements

A fresh `work/marlin-adapter-acceptance` directory built/loaded the RTL model in
29.17 s. The next model load reused its library in 85.64 ms, and the direct gain
demo reused it in 80.90 ms. No C++ rebuild occurred for graph or gain data.

| Queue replay | Graph validation/lowering | Configuration | Instructions | State words | Payload bytes | Total clocks |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Parallel gains | 404.49 µs | 1.106 ms | 5 | 2 | 334 | 6260 |
| Chained gains | 28.48 µs | 477.54 µs | 4 | 2 | 322 | 5531 |

These are individual host observations taken while the native workspace suite
was running, not latency distributions or ARM deadline claims. Configuration
includes handshakes, verification readbacks and deliberate idle stalls. Payload
bytes exclude framing/readbacks; total clocks include reset/configuration, raw
sample transfers, TP commands, deterministic stalls and ten quanta of processing.
They are not isolated job-processor cycle counts. The direct gain replay passed
ten quanta in 5067 total clocks.

## Reproduction

From the repository root with the documented Rust/C++/Verilator prerequisites:

```sh
cargo run -p rt_fpga_sim --features sim --bin fpga-runtime-demo -- \
  src/rust/rt_fpga_sim/fixtures/parallel.json work/marlin-replay work/parallel.vcd
cargo run -p rt_fpga_sim --features sim --bin fpga-runtime-demo -- \
  src/rust/rt_fpga_sim/fixtures/chain.json work/marlin-replay
cargo run -p rt_fpga_sim --features sim --bin fpga-marlin
cargo test -p rt_fpga_sim --features sim --test marlin
cargo test -p rt_fpga_adapter -p rt_backend_testbench --lib
```

The new run/build path invokes Rust, Verilator, Perl and C++/make; it invokes no
Python or cocotb. The existing cocotb suites remain independent. A fresh artifact
directory rebuilds the model; reusing it caches C++ independently of graph data.

## Workspace gates

Formatting, the Rust test-attribute policy and warning-denying workspace build
pass. Native `shoopdaloop` and Wasm `shoop_rt_host_worklet` dependency trees contain
no Marlin/Verilator. Both `shoopdaloop` and `shoop_rt_host_worklet` build for
`wasm32-unknown-unknown` with warnings denied. The feature-enabled simulator also
builds with warnings denied.

The broad native run used:

```sh
SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace \
  --features shoop_engine/app_backend,shoop_backend/native-fx --profile ci
```

It exercised 3220 tests: 3209 passed, 11 failed, 11 skipped. Seven failures were
LAN host startup deadlines, one a recovery startup deadline, one a settings-write
deadline, and two involved a host test binary overwritten by an overlapping
workspace build (including a missing `testing`-feature configuration field).
All 11 failed tests passed on one isolated retry with the same feature selection,
`--test-threads 1` and no concurrent builds (142.79 s). The added stalled-device
activation test also passed in its focused six-test adapter gate. This records
the original failure rather than presenting the first broad run as clean.

No PR CI was inspected, and the previously waived 64-loop benchmark was not
repeated. PR #979 targets `proto/fpga-rust-reference`; only the implementation
branch was pushed. No automated review findings were present at delivery.
