# AXI sample memory

The implementation follows [PLAN_DDR3.md](../../PLAN_DDR3.md). Baseline:
`8f64c32d` on `proto/fpga-rust-reference`, existing PR #969. The existing cores
retain their word port. Integrated tops put the common synthesizable AXI master
between that port and external memory. No accesses bypass AXI in these tops.

## Endpoint contract

The endpoint accepts a command on `command_valid && command_ready`: byte address,
write flag, AXI SIZE (log2 bytes), and LEN (beats minus one). Supported commands
are naturally aligned INCR transfers of 1–16 beats, at most the configured bus
width, contained in one 4 KiB page. The data bus is 32 or 64 bits; address width
is 32 bits. IDs are fixed at zero. One command remains outstanding until the
last read beat or write completion is consumed.

Write beats use `write_valid/ready`, bus-width data, and byte strobes; the endpoint
generates WLAST. Read beats expose bus-width data, error, and LAST with their own
ready/valid handshake. Write completion has a separate ready/valid/error port.
Invalid command shapes complete locally with an error and no AXI transaction.
Independent AW/W handshakes and response backpressure must be supported.

The word adapter converts one 32-bit request into one SIZE=2, LEN=0 command.
Lane placement and strobes follow the byte address on a wider bus. Logical
addresses are translated as `physical_base + (address - arena_base)` within a
configured arena byte count. Underflow, overflow, and out-of-window requests
fail locally instead of truncating high address bits. Default physical placement
is `0x01000000`; all existing synthetic 64-bit arena bases remain usable.

On hardware the endpoint attaches through an AXI4-to-AXI3 compatibility wrapper:
LEN is at most 15 and WID is fixed zero. Zynq-7000 HP supports 32/64-bit buses.
This work does not supply the PS, DDR controller/PHY or board clock/reset logic.

Word reads complete at R acceptance and writes at B acceptance. Cache fence and
retirement drain earlier AXI transactions and preserve visibility; they do not
claim physical DDR completion. Simulation reset coordinates the master and slave
and discards old responses. Resetting only the master against a live PS port
requires a separate system drain/reset policy.

## DDR3 model assumptions

DRAMsim3 is pinned in `dramsim3.lock` (MIT; retain its upstream license in the
downloaded source). It models timing, not sample contents. A shared Python store
owns the bytes and applies WSTRB. Cocotb serves AXI; ctypes calls a small native
C ABI, polls completions and advances multiple DRAM clocks per fabric cycle.

The supplied profile assumes one 4 Gbit x16 device, eight banks, 32,768 rows,
1,024 columns, one rank, and 512 MiB total. Its 1.875 ns clock gives DDR3-1066
and a 16-byte native BL8 transfer. The example fabric clock is 100 MHz. These
are illustrative conservative timings, not a specification for every Zynq-7010
board or a specific memory part. Power values are placeholders, not power
estimates. Physical placement, clocks and profile hash belong in every report.

The profile uses an open-page unified transaction queue. Upstream DRAMsim3 posts
write callbacks after admission, before physical completion. Separate read/write
queues can retain small write batches indefinitely; the unified queue avoids
that threshold behavior. The native bridge must expose a real idle/drain check
including issued writes, rather than treating posted callbacks as DDR drain.

Each word pays native-burst timing. An AXI burst covers each native block once;
independent word requests are never grouped by this bridge. DRAMsim3's internal
write merging and read forwarding are controller behavior, counted separately
from logical AXI traffic. Refresh advances even while AXI is idle or stalled.

The model approximates DDR service, not the exact PS controller, HP bridge or
concurrent CPU traffic. Observed maxima are measurements, not hard WCET bounds.

## Isolated checks

With Verilator, Yosys, a C++ compiler and the Python requirements installed:

```sh
python fpga/axi_memory/simulate.py
python fpga/axi_memory/simulate.py --fixed-cycles 7 --output work/axi-fixed
bash fpga/axi_memory/check.sh
```

The suite tests both bus widths, reads/writes of 1/2/16 beats, upper word lanes,
byte strobes, native-block crossings, AW-before-W and W-before-AW, stalled
responses, local address/shape rejection, errors and coordinated reset. Both
endpoints pass warning-clean lint and ECP5 synthesis; synthesis is not place and
route or a frequency guarantee.
