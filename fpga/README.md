# FPGA execution components

The retained hardware is SP, JP, media/memory/cache, prefetch and AXI services,
plus independently useful audio I/O. There is no transition processor or current
ARM/application backend. The TP-free media audio pipeline connects the execution
components for verification.

The [cache-less MIDI components](midi/README.md) provide shared DDR event storage,
state restoration, history grabs and loop execution against the Rust reference.

See the [architecture](../FPGA_ARCHITECTURE.md),
[native Marlin suite](../src/rust/rt_fpga_testbench/README.md) and
[retirement ledger](../docs/fpga/retirement-ledger.md).
