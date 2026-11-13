# ZedBoard hardware results

## First hardware run, 2026-11-13

The default 64-loop deployment (Q=32, one generated input, eight mixed outputs,
64-bit AXI3 HP0, 100 MHz fabric clock) ran to completion twice on one ZedBoard.
Each run executed 1000 quanta, including 72 warmup quanta, with no gap and a
10-million-clock watchdog. Both runs ended STOPPED with no fault (status `0x2D`).

| | Run 1 | Run 2 |
| --- | ---: | ---: |
| Measured quanta | 928 | 928 |
| **Average quantum** | **26,037 cycles, 260.4 µs** | **26,052 cycles, 260.5 µs** |
| Minimum quantum | 25,585 cycles, 255.9 µs | 25,575 cycles, 255.8 µs |
| Maximum quantum | 26,873 cycles, 268.7 µs | 26,899 cycles, 269.0 µs |
| Wall cycles after warmup | 24,164,509 | 24,177,669 |
| Response backpressure cycles | 7,784,414 | 7,784,690 |

The traffic counters were identical in both runs: 357,475 read bursts
(34,005,952 bytes), 4,430 write bursts (549,760 bytes), zero address-channel
stalls and 512,000 output sample writes. Scheduling overhead outside the
measured quanta is about two cycles per quantum. The
[instant-memory simulation](VALIDATION.md#retry-after-timing-mr-975) of the same
RTL and workload measured 22,974..23,454 cycles (average 23,214) for quanta
72..79. On hardware, the average quantum takes about 12% longer.

The output digest is zero in both runs. Each quantum makes 512 output writes,
a multiple of the digest's 64-bit rotation, so quanta with identical output
cancel in pairs. A zero digest after 1000 quanta therefore suggests identical
output in every quantum, such as silence. Output correctness remains unverified.

Raw snapshots: [run 1](validation/hardware-2026-11-13/results-run1.json),
[run 2](validation/hardware-2026-11-13/results-run2.json) and the
[deployment manifest](validation/hardware-2026-11-13/deployment.json).

### Board, tools and build

- ZedBoard in JTAG boot mode with its onboard USB-JTAG. The PS reports silicon
  1.0, probably an engineering sample: `MCTRL[31:28]` = 0, `PSS_IDCODE`
  `0x03B27093`, PL JTAG IDCODE `0x03727093`. Production silicon is untested.
- Windows 11 host with Vivado 2023.2, SW Build 4029153: its hardware manager,
  `hw_server` and `xsdb`.
- RTL revision `9386dffd14408df52b87179e834b1bdc383bf07c`, the revision of the
  prebuilt bundle. The regenerated workload matches every bundle parameter.
- The bitstream was rebuilt with Vivado 2023.2 from that revision, with the reset
  fix below and Digilent board part `digilentinc.com:zedboard:part0:1.1`
  (vivado-boards `d8d43ed9`). Routed at 100 MHz: WNS -0.064 ns, TNS -0.175 ns over
  six endpoints, WHS +0.031 ns. This deployment is not timing qualified. It uses
  30,124 LUTs, 27,363 flip-flops, 68 BRAM36 tiles and four DSPs.
- The PS was initialized with the ZedBoard PS7 preset. It is identical to the
  `ps7_init.tcl` that the current build exports.

The second run used this branch's `load.tcl` and `control.tcl`.

### Problems found and fixed

1. **The prebuilt bundle cannot run the benchmark.** Its block design ties the
   reset block's `aux_reset_in` to 0, but that input is active-low. The
   benchmark therefore stays in reset: live status `0x08`, start and snapshot
   ignored, all statistics zero. The build now ties the input inactive.
2. **Board-file DDR settings did not initialize DDR on this board.** With the
   Digilent preset's silicon-1.0 sequence, the CPU read back corrupted DDR data
   even without a bitstream. The benchmark's first HP0 read never completed;
   the watchdog fired (`0xF4`) and the harness stayed in DRAINING. AMD's ZedBoard
   PS7 preset differs in 17 DDR register writes. Its sequence matches the
   [AMD ZedBoard FSBL initialization](../../platforms/zynq7000/boards/zedboard_ps7_init.tcl),
   which runs FreeRTOS from DDR on this board, and the benchmark ran with it. The build now applies this preset. Compared with
   the Digilent preset, its PL interface differs only by the unused EMIO
   Ethernet MDIO and SD write-protect ports. FCLK0, its reset and HP0 are
   unchanged.
3. **`control.tcl` could not run under Vivado's Tcl 8.5,** and Vivado 2023.2
   requires VIO output values padded to the probe width. A start that the
   harness ignored also produced a silent all-zero `results.json`; `measure` now
   fails unless the runner reaches STOPPED.
4. **`load.tcl` now checks DDR with a scratch write and readback** before the
   benchmark can access it.

### Operating notes

- An open Vivado hardware manager already serves `hw_server` on
  `127.0.0.1:3121`. The scripts can use it instead of starting a second server.
- With the hardware manager also connected, `rst -system` occasionally failed
  with debug memory-access errors. Once, after an aborted initialization, the
  PS debug access port kept reporting transaction errors that `rst -srst` could
  not clear; a power cycle recovered the board.
- Vivado 2023.2 reports `C_EXT_RESET_HIGH` on the reset block as read-only and
  propagates the same active-low value from `FCLK_RESET0_N`.

### Not covered

- RTL changes after `9386dffd` have not run on hardware.
- The build with the ZedBoard PS7 preset was checked through block-design
  generation. Its exported `ps7_init.tcl` and benchmark PS settings match the
  tested deployment, but no bitstream was built with this change.
- Timing closure, output correctness and production silicon.
