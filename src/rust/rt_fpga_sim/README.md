# Rust / Verilator integration

This optional native crate drives the existing TP/SP/JP pipeline and functional
AXI memory model through Marlin 0.21.1. There is no Python in its build or run
path. Existing cocotb suites remain independent.

From the repository root, with Rust, a C++ compiler and Verilator on PATH:

```sh
cargo run -p rt_fpga_sim --features sim --bin fpga-marlin
```

Optional positional arguments: scenario JSON, cached build directory, VCD path.
The default replay is `fixtures/instant-gains.json`. This direct demonstrator
accepts only current-boundary instant gain actions; it checks each output word
and the frame against the production Q8.24 oracle. It introduces deterministic
job stalls and holds TP responses under backpressure. Configuration transfers
verify full-width readback. Waits have cycle limits and return errors.

The simulation top has explicit widths, a Rust-driven clock and fixed small
capacities. Configuration and execution use the existing hardware ports; no
Rust sample executor substitutes for RTL. Each cycle evaluates low/high/low,
and a VCD records all three phases with monotonic timestamps. Reset drops host
valids before asserting reset. Models borrow their Marlin runtime; they must be
dropped before it. Create/build the runtime before starting any audio callback.

The wrapper caps C++ build parallelism at two jobs. Set `SHOOP_VERILATOR` and
`SHOOP_VERILATOR_JOBS` to override the executable and job count. Marlin caches the
compiled model separately from graph/configuration data. Loading another graph
or changing a gain does not rebuild C++.

Validated with Rust 1.99.1, Verilator 5.032 and GCC 14.2 on Linux x86_64.
Marlin is behind `sim`; ordinary production and Wasm builds do not depend on it.
Simulation wall time is not evidence of a board deadline.

## Queue-driven comparison and replay

```sh
cargo run -p rt_fpga_sim --features sim --bin fpga-runtime-demo
cargo test -p rt_fpga_sim --features sim --test marlin
```

The demo accepts the same optional positional arguments (scenario, cache, VCD).
The default is `fixtures/parallel.json`; `fixtures/chain.json` exercises successive
rounding stages. Both include signed low-bit samples, future and late aligned
controls. The same producer submits ordinary queue requests to native Q8.24 and
`rt_fpga_adapter`, including slot retirement/installation and the production plan.
The adapter receives no fixture image. The comparison checks raw samples,
completions, owning retirement presence, execution receipts, epochs and frames.
On failure, rerun the printed scenario with an explicit VCD path. Device faults
retain the underlying driver diagnostic in `Simulator::last_error`.

The integration tests also cover silence, reset, backpressure, bounded timeouts,
746-bit odd-depth FIFO wrap, and an injected real RTL fault. One original
record/stop/playback/wrap fixture runs directly through Marlin, checking audio,
loop state and captured media against production Q8.24. The direct media loader
supports initially empty views for this regression; importing arbitrary stored
media and routing loop commands through the queue adapter remain outside scope.

Each model owns a separate Verilator context. A small vendored patch to the
pinned bridge fixes stale trace callbacks retained by the default context; see
[patch details](../../../third_party/marlin-verilator/SHOOP_PATCH.md). Keep models
and traces on their creating service thread. Traces close before models, and
models/context are destroyed before their Rust runtime. Parallel independent
runtime/model tests verify this ownership; no test serialization is required.

[Validation measurements](VALIDATION.md) distinguish C++ build/load, runtime graph
validation/lowering, configuration and clock counts. Ordinary CPU/Wasm dependency
trees contain no Marlin; normal app code still uses its native runtime.
