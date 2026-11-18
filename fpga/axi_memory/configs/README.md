# DDR3 profile provenance

`zynq7010-ddr3-1066-x16.ini` is an illustrative feasibility-study profile. It is
not extracted from a particular board's schematic, memory-part datasheet, PS
register dump or measurement. The name describes the intended class of system,
not a verified Zynq-7010 board configuration.

The format and controller model come from the pinned
[DRAMsim3 DDR3 configuration](https://github.com/umd-memsys/DRAMsim3/blob/29817593b3389f1337235d63cac515024ab8fd6e/configs/DDR3_4Gb_x8_1600.ini).
The hardware values below are chosen working assumptions, using a DDR3-1066
clock and plausible DDR3 timing values rounded to cycles. They are not a claim
of compatibility with that example configuration's particular speed bin, nor a
guaranteed conservative bound for every DDR3 part.

| Parameter | Assumption | Interpretation |
| --- | --- | --- |
| Clock / data rate | tCK = 1.875 ns | 533.33 MHz clock, 1066.67 MT/s |
| Channel | One x16 channel | 2.133 GB/s theoretical peak |
| Geometry | One rank, eight banks, 32,768 rows, 1,024 columns | One 4 Gbit x16 device, 512 MiB |
| Native burst | BL8 | 16 bytes over four DRAM clocks |
| CL / CWL | 7 / 6 clocks | 13.125 / 11.25 ns |
| tRCD / tRP | 8 / 8 clocks | 15 / 15 ns |
| tRAS | 20 clocks | 37.5 ns |
| tRFC | 139 clocks | 260.625 ns |
| tREFI | 4,160 clocks | 7.8 us, ordinary-temperature refresh |
| tWR / tRTP / tWTR | 8 / 4 / 4 clocks | 15 / 7.5 / 7.5 ns |
| tRRD / tFAW | 6 / 27 clocks | 11.25 / 50.625 ns |

The 32-entry unified transaction queue, eight-entry per-bank command queues,
open-page policy and address mapping are DRAMsim3 modeling choices, not a model
of the Zynq PS scheduler. The unified queue permits small write batches to drain
without the separate write-buffer threshold. The model includes native command
timing, bank/row effects and refresh, but no competing CPU/DMA traffic or measured
PS/HP bridge overhead. Power-current values are placeholders and are not used to
claim power estimates.

`zturn-v2-ddr3-1066-x32.ini` replays the 1 GiB, 32-bit memory geometry in the
MYIR Z-turn V2 Ver 1 schematic. The model uses one rank with two 4 Gbit x16
devices and a 32-byte BL8 native burst. The generated PS7 register values give
tRCD and tRP of seven clocks, tRAS of 21 clocks, tRC of 28 clocks, tFAW of 27
clocks, and tRFC of 161 clocks. The model uses those delays and derives tRC by
adding tRAS and tRP. The scheduling policy and remaining illustrative timings
come from the x16 study. PS arbitration, ARM and GEM DMA traffic, and board
calibration require measurements. [Z-turn V2 plan](../../../PLAN_ZTURN_V2_FPGA_BACKEND.md)

Set `SHOOP_DRAMSIM3_CONFIG` to an absolute INI path to select the memory
profile for a simulation run. The FPGA workflow runs the Q32 16-loop and 64-loop
trace with both profiles and asserts exact stereo samples and a 66,666-cycle
budget per quantum at 100 MHz and 48 kHz.

```sh
SHOOP_DRAMSIM3_CONFIG="$PWD/fpga/axi_memory/configs/zturn-v2-ddr3-1066-x32.ini" \
  cargo test --locked -p rt_fpga_testbench --features sim --test loading \
  looping_workloads_with_thirty_two_frame_chunks_and_dram_timing -- --nocapture
```
