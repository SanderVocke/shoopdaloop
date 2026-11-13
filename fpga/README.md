# FPGA verification

All component and integration simulations use the shared native
[`rt_fpga_testbench`](../src/rust/rt_fpga_testbench/README.md) crate. Its optional
`sim` feature uses Marlin/Verilator, with functional SV memory, fixed-latency AXI,
DRAMsim3 AXI and legacy component transport agents.

```sh
python fpga/axi_memory/build_dramsim3.py
cargo test --locked -p rt_fpga_testbench --features sim
cargo run --locked -p rt_fpga_testbench --features sim -- production
```

The [coverage inventory](../src/rust/rt_fpga_testbench/coverage.json) maps every
baseline test definition to its Rust implementation and regression target. Python
remains for RTL generation, hardware preparation and common input datasets. The
`simulate*.py` entry names forward to the native runner; HDL driving, monitors,
memory services and scoreboards are Rust.

VCD output is opt-in through `--waves` for focused debugging. Routine runs and
CI do not enable it; the small trace-lifetime regression removes its files.

Component READMEs describe the RTL contracts. The native suite README is the
current reference for simulation commands, architecture, debugging and extending
the tests. Historical measurement reports retain the tools and commands used
when those measurements were collected.
