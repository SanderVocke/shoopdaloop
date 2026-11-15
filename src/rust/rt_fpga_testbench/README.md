# Native FPGA component verification

The restored [Rust component reference](../rt_fpga_reference/README.md) is exercised
by `--test reference`. The same explicit programs, records and samples run through
independent Rust models and real RTL; jobs, state, errors, buffers and output
samples are compared exactly. Existing vector tests remain independent checks.

The reference crate also has Rust-only transition and full functional pipeline
tests: `cargo test --locked -p rt_fpga_reference --all-features`. They compare
primitive-loop traces to the current native resolver and exercise TP -> SP -> JP
with logical media. The RTL benches below remain TP-free.

`rt_fpga_testbench` preserves the Marlin/Verilator simulation framework and
component drivers. It needs Verilator **5.032**, a C++ compiler and Rust. Simulation
is opt-in; default native and Wasm builds do not depend on Marlin.

```sh
bash scripts/build_verilator.sh
export PATH="$PWD/work/verilator/bin:$PATH"
python3 fpga/axi_memory/build_dramsim3.py
cargo test --locked -p rt_fpga_testbench --features sim
cargo check --locked -p rt_fpga_testbench
python3 scripts/check_fpga_testbench_coverage.py
```

The DRAMsim3 builder needs CMake and patch and fetches its pinned dependency.
Use the repository build environment guidance. The local Marlin patch remains
in `third_party/marlin-verilator`.

Focused runs:

```sh
cargo run --locked -p rt_fpga_testbench --features sim -- state --config small
cargo run --locked -p rt_fpga_testbench --features sim -- audio --config small
cargo run --locked -p rt_fpga_testbench --features sim -- pipeline --config small --memory-model fixed
cargo run --locked -p rt_fpga_testbench --features sim -- pipeline --config small --memory-model dramsim3 --axi-data-width 32
cargo run --locked -p rt_fpga_testbench --features sim -- deferred --memory-model dramsim3
```

`pipeline` is the TP-free SP -> JP -> media/cache execution harness. Explicit
instructions, initial state and samples exercise exact output, recording/readback,
retained ranges, persistent state, prefetch, partial faults and reset across runs.
It does not resolve transitions, emulate ARM or execute application requests.
The separate `state` and `smoothing` suites supply logical records directly.

Other suites cover chunk, media, metadata, ownership, address resolution,
reclamation, AXI32/64, delayed memory and standalone audio I/O. `cargo test` runs
all retained test targets, including realistic-memory cases. The CLI `all` runs
its listed suites using the selected memory mode; it is not the complete test
matrix. Stress duration and random cases can be reduced for iteration.

Model compilation, elaborated port widths, wide packing, independent instances,
clock/edge agents, ready/valid monitors, watchdogs, tracing, probes, memory agents,
DRAMsim3 and reproducible reports remain reusable. Set `SHOOP_VERILATOR` to choose
the executable and `SHOOP_VERILATOR_JOBS` to limit model build concurrency.
`--output` selects report output; `--waves` enables VCD for focused debugging.
Model caches live under `work/marlin-suite`; remove that directory for a fresh run.

The coverage inventory preserves all 75 historical definition hashes: 62 retained
obligations and 13 explicitly retired TP/host/system obligations. It is checked
against actual source functions and Cargo test targets. The old transition,
production RTL, ARM control, continuous full-loop, demo and ZedBoard commands are
removed. Production CPU scenarios and numeric checks remain in
`rt_backend_testbench`, independently of hardware.

Production CPU analysis also offers `rt-backend-testbench graph`: canonical node
and connection ordering for numeric error bounds, obtained from the production
compiler. It emits no instructions, physical media layout or ARM transport data.
The independent Python float/fixed gate uses that ordering and logical sessions.
