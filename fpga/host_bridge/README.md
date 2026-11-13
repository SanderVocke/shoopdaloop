# CPU host bridge

`host_core` puts an AXI-lite register map in front of the canonical
`loop_pipeline_axi` core, so a CPU can drive every host port. On the ZedBoard,
that CPU is the ARM, through M_AXI_GP0. The core keeps its own AXI3 master for
DDR. `rt_fpga_adapter::bridge::Bridge` is the matching driver. It implements the
adapter's `PortIo`, so the shared port driver loads images and runs quanta through
these registers exactly as it does through simulated pins.

`generate.py` reads the core's port list and writes these files:

- `rtl/host_core.sv`, the top level;
- `rtl/host_map.svh` and the Rust `bridge/map.rs`, the register map;
- `rtl/host_capacity.svh`, the capacity readback.

Wide ports determine the window sizes. `rtl/host_body.svh` is the hand-written
bridge. Run `python fpga/host_bridge/generate.py --check` to detect a stale map.

## Register map

The map has 32-bit registers, byte addresses below 4 KiB, full-word writes and one
AXI-lite transaction at a time. Unmapped addresses read as zero.

| Register | Meaning |
| --- | --- |
| `ID`, `VERSION` | `0x53484f50` and the map version |
| `CONTROL` | Bit 0 resets the core; bit 1 clears the sticky done flag |
| `STATUS` | Core fault, error stage and code; one busy bit per port; reset in progress; AXI quiescence; sticky done; memory fault |
| `CAPACITY` | The core's compiled parameters, in the generated order |
| `QUANTA`, `LAST_CYCLES` | Completed runs since reset, and the cycles from acceptance to done of the last one |
| `OUTPUT_WORDS`, `DIGEST` | Count and rotating digest of output sample writes |

Each core port has a window of parameter registers and a `*_GO` register.

1. The CPU writes the parameters and then `*_GO`.
2. The bridge holds the port's valid until the core is ready.
3. It captures the response and clears the port's busy bit.
4. The CPU polls `STATUS` for that bit, then reads the result registers.

`GO` writes are ignored while the port is busy. Host transactions belong between
quanta, as for every other host of the core.

| Window | Core port |
| --- | --- |
| `RUN` | The 8-word quantum descriptor. Busy until `done`; completion sets the sticky done flag |
| `TP_*` | Transition commands: 20-word data, 26-word response and error |
| `CFG_*` | Program or state memory. Control bit 0 selects state memory, bit 1 writes |
| `AUDIO_*` | Sample ports. Control bits 1:0 are the space, bit 2 writes |
| `STORE_CFG_*` | Media store configuration and diagnostics. Control bits 2:0 are the space, bit 3 writes |
| `STORE_*` | Media commands |

Media commands stream samples through 64-word FIFOs:

- **Appends:** the CPU writes `STORE_TX` while `STORE_TX_SPACE` is nonzero.
- **Reads:** the CPU reads `STORE_RX` while `STORE_RX_LEVEL` is nonzero.
- **Completion:** latches the code, empty flag and result view, and counts accepted
  samples in `STORE_WRITTEN`. Samples still queued in the TX FIFO are discarded.

## Reset and memory isolation

A core reset first blocks the core from starting new AXI bursts, then waits for
outstanding bursts to finish, and only then holds the core in reset for four
cycles. A reset therefore never abandons a DDR transaction. The reset also clears
every port window, the counters and the FIFOs.

## Audio

With `EXTERNAL_IO=1` the core generates its own input as
`(sample_address+1)*37 XOR quanta`, the same signal as the ZedBoard benchmark.
Its output only feeds the digest. Neither crosses the register map. With
`EXTERNAL_IO=0` the `AUDIO_*` window exchanges samples instead, as the Marlin
simulator does.

## Verification

Lint the bridge together with the core, for both audio modes:

```sh
verilator --lint-only -sv --top-module host_core -Ifpga/state_processor/rtl \
  -Ifpga/host_bridge/rtl -GEXTERNAL_IO=1 <core sources> fpga/host_bridge/rtl/host_core.sv
```

The core sources are the same list `rt_fpga_sim` builds. `rt_fpga_adapter`'s
`tests/bridge.rs` checks the driver's register traffic against a scripted bus.
