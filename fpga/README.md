# FPGA execution components

The retained hardware is SP, JP, media/memory/cache, prefetch and AXI services,
plus independently useful audio I/O. There is no transition processor or current
ARM/application backend. The TP-free media audio pipeline connects the execution
components for verification.

See the [architecture](../FPGA_ARCHITECTURE.md),
[native Marlin suite](../src/rust/rt_fpga_testbench/README.md) and
[retirement ledger](../docs/fpga/retirement-ledger.md).
