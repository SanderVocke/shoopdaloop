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

Validated with Rust 1.99.1, Verilator 5.032 and GCC 12.2 on Linux x86_64.
Marlin is behind `sim`; ordinary production and Wasm builds do not depend on it.
Simulation wall time is not evidence of a board deadline.
