# FPGA execution components

The retained hardware is SP, JP, media/memory/cache, prefetch and AXI services,
plus independently useful audio I/O. There is no transition processor or current
ARM/application backend. The TP-free media audio pipeline connects the execution
components for verification.

The [MIDI components](midi/README.md) implement event-only looping through the
existing SP and shared audio/MIDI JP. Concrete read/append/merge/output jobs cross
blocks without a 256-message quantum quota. Bulk MIDI payloads and intermediate
lists live in DDR; small sequence/list/cursor tables use BRAM. Rust and RTL omit
MIDI instrument-state tracking and restoration. Audio keeps its existing caches.

The [shared-execution study](../docs/fpga/midi-shared-jobs.md),
[compact-execution study](../docs/fpga/midi-compact-vivado-7020.md), and earlier
[event-only experiment](../docs/fpga/midi-stateless-rtl.md) describe previous designs
and measurements; they are not results for the current data-job implementation.

See the [architecture](../FPGA_ARCHITECTURE.md),
[native Marlin suite](../src/rust/rt_fpga_testbench/README.md) and
[retirement ledger](../docs/fpga/retirement-ledger.md).
