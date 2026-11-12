# Routed resource usage

These numbers come from the existing fully routed `zedboard_bench` checkpoint,
using Vivado 2021.1 SW Build 3247384 on `xc7z020clg484-1`. No new synthesis was
needed to obtain them. The measured RTL revision is
`9386dffd14408df52b87179e834b1bdc383bf07c`.

The profile has 64 loops, Q=32, one dummy input, eight mixed outputs, four read
and four write buffers, eight metadata cache lines, four reference cache lines,
256 views and an external arena with 8,192 sample chunks. Compiled capacities
include 257 program entries, 192 state entries, 321 job entries and 185 sample
buffers. Counts are specific to these capacities.

## Total benchmark RTL

| Resource | Used | Zynq-7020 capacity | Usage |
| --- | ---: | ---: | ---: |
| LUTs, including LUT RAM | 25,861 | 53,200 | 48.61% |
| Flip-flops | 18,399 | 106,400 | 17.29% |
| BRAM36 equivalents | 55.5 | 140 | 39.64% |
| DSP48 | 4 | 220 | 1.82% |
| Occupied slices | 8,585 | 13,300 | 64.55% |

The block RAM count is 51 RAMB36 plus nine RAMB18 primitives. LUT usage includes
84 LUTs used as distributed RAM. Loop audio in external DDR does not occupy
these BRAMs.

The processing core without the autonomous harness uses **24,917 LUTs, 16,222
flip-flops, 50 BRAM36 equivalents and four DSPs**.

These are out-of-context RTL results. PS7, ILA, VIO, the debug hub and final
board clocking are excluded. Full board utilization must be measured after the
missing IP catalog is supplied. Resource usage alone does not establish board
timing closure.

## Approximate subsystem breakdown

| Subsystem | LUTs | Flip-flops | BRAM36 equivalents | DSPs |
| --- | ---: | ---: | ---: | ---: |
| Transition processor, including loop/request/specification storage | 11,832 | 7,399 | 24 | 0 |
| Media store, metadata and sample caches | 7,834 | 5,957 | 7.5 | 0 |
| Job processor, scratch RAM and divider | 2,693 | 1,437 | 8 | 4 |
| State processor, program/state RAM and job queue | 1,183 | 707 | 7.5 | 0 |
| Shared AXI master and DMA | 1,221 | 669 | 0 | 0 |
| Other processing glue and read-intent queue | 154 | 53 | 3 | 0 |
| Autonomous harness: controls, boot/events and statistics | 944 | 2,177 | 5.5 | 0 |
| **Total** | **25,861** | **18,399** | **55.5** | **4** |

The transition processor accounts for about 46% of LUTs; media storage and
metadata/cache control account for about 30%. All four DSPs are in the job
processor. The harness contributes about 3.7% of LUTs and 11.8% of flip-flops.

Hierarchy LUT attribution is approximate because Vivado can combine LUTs across
module boundaries. The other-processing-glue row is the residual after the
named subsystems, so the rows sum to the measured total. BRAM36 equivalents
count a RAMB18 as half a RAMB36.

Evidence: [complete utilization](validation/2021.1/utilization.rpt),
[hierarchical utilization](validation/2021.1/utilization_hierarchical.rpt) and
[machine-readable summary](validation/2021.1/resources.json).
