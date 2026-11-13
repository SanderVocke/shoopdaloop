# Routed resource usage

These numbers come from the fully routed **complete ZedBoard deployment**, using
Vivado 2021.1 SW Build 3247384, IP Build 3246043 on `xc7z020clg484-1`.
They include PS7 integration, the autonomous benchmark, ILA, VIO, JTAG debug hub
and board reset. The measured RTL revision is
`9386dffd14408df52b87179e834b1bdc383bf07c`.

The profile has 64 loops, Q=32, one dummy input, eight mixed outputs, four read
and four write buffers, eight metadata cache lines, four reference cache lines,
256 views and an external arena with 8,192 sample chunks. Compiled capacities
include 257 program entries, 192 state entries, 321 job entries and 185 sample
buffers. Counts are specific to these capacities.

## Total deployment

| Resource | Used | Zynq-7020 capacity | Usage |
| --- | ---: | ---: | ---: |
| LUTs, including LUT RAM and shift registers | 30,857 | 53,200 | 58.00% |
| Flip-flops | 27,104 | 106,400 | 25.47% |
| BRAM36 equivalents | 68 | 140 | 48.57% |
| DSP48 | 4 | 220 | 1.82% |
| Occupied slices | 10,877 | 13,300 | 81.78% |

Block RAM comprises 63 RAMB36 and ten RAMB18 primitives. LUTs include 108 used as
distributed RAM and 282 as shift registers. External DDR audio does not occupy
these BRAMs. The PS7 and its DDR controller are hard silicon; their internal
resources are not PL LUT/flip-flop/BRAM usage.

The design generated a bitstream with zero DRC errors and zero routing errors.
The requested 100 MHz clock does not meet setup timing: WNS is -0.425 ns.
No timing tuning was performed. See [validation](VALIDATION.md).

## Approximate subsystem breakdown

| Subsystem | LUTs | Flip-flops | BRAM36 equivalents | DSPs |
| --- | ---: | ---: | ---: | ---: |
| Transition processor, including loop/request/specification storage | 11,856 | 7,396 | 24 | 0 |
| Media store, metadata and sample caches | 8,305 | 5,966 | 7.5 | 0 |
| Job processor, scratch RAM and divider | 2,802 | 1,442 | 8 | 4 |
| State processor, program/state RAM and job queue | 1,194 | 707 | 7.5 | 0 |
| Shared AXI master and DMA | 1,253 | 669 | 0 | 0 |
| Other processing glue and read-intent queue | 170 | 55 | 3 | 0 |
| Autonomous harness: controls, boot/events and statistics | 669 | 2,174 | 5.5 | 0 |
| ILA | 929 | 1,975 | 12.5 | 0 |
| VIO | 3,190 | 5,934 | 0 | 0 |
| JTAG debug hub | 473 | 753 | 0 | 0 |
| Board reset and glue, including shared-LUT residual | 16 | 33 | 0 | 0 |
| **Total** | **30,857** | **27,104** | **68** | **4** |

The transition processor accounts for about 38% of deployment LUTs; media/cache
control accounts for about 27%. All four DSPs are in the job processor. ILA, VIO
and the shared debug hub together use about 15% of LUTs and 32% of flip-flops.
The ILA capture memory uses 12.5 BRAM36 equivalents. The wide VIO status readout
is the largest part of the debug logic.

The processing core without the autonomous harness or board/debug IP uses
**25,580 LUTs, 16,235 flip-flops, 50 BRAM36 equivalents and four DSPs**.

Hierarchy LUT attribution is approximate because Vivado can combine LUTs across
module boundaries. The two glue rows are residuals after the named subsystems,
so the rows sum to the measured total. The reset hierarchy itself reports 17
LUTs; its shared-LUT-adjusted residual is 16. BRAM36 equivalents count a RAMB18
as half a RAMB36.

Evidence: [complete utilization](validation/2021.1/board_utilization.rpt),
[hierarchical utilization](validation/2021.1/board_utilization_hierarchical.rpt)
and [machine-readable summary](validation/2021.1/board-resources.json).

## Earlier standalone benchmark result

The earlier out-of-context `zedboard_bench` checkpoint excludes PS7, ILA, VIO,
the debug hub and board clocking. It uses **25,861 LUTs, 18,399 flip-flops, 55.5
BRAM36 equivalents, four DSPs and 8,585 occupied slices**. It was reused for the
initial resource analysis without rerunning synthesis.

The full-board benchmark hierarchy itself uses 26,249 LUTs, 18,409 flip-flops,
55.5 BRAM36 equivalents and four DSPs. Differences from the standalone result
reflect different synthesis/placement contexts and optimizations. Use the full
board hierarchy above to assess debug overhead rather than subtracting results
from separate builds.

Retained standalone evidence: [total utilization](validation/2021.1/utilization.rpt),
[hierarchy](validation/2021.1/utilization_hierarchical.rpt) and
[summary](validation/2021.1/resources.json).
