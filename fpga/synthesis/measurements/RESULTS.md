# Routed Zynq-7020 result

The integrated DDR-backed 64-loop core meets **100 MHz** in Vivado 2021.1
(Build 3247384) on `xc7z020clg400-1`, the Zybo Z7-20 device. After routing and
post-route physical optimization, setup slack is **+0.002 ns** and hold slack is
**+0.005 ns**. All 58,725 setup/hold endpoints pass, and all 42,832 routable nets
are routed without errors.

The clock period is 10.000 ns, with 0.200 ns user uncertainty in addition to
Vivado's jitter allowance. There are no false paths, multicycle exceptions or
unconstrained internal endpoints. The remaining setup margin is very small;
integration or changes to placement, constraints or tool version require another
timing check.

## Scope and utilization

This measures `loop_pipeline_axi` in a synchronous registered fabric context.
All core interfaces have retained physical launch/capture registers. The design
includes transition/state/job processing, sample and metadata caches, and the
shared AXI master. It excludes PS7, its DDR controller, the ARM control adapter,
audio peripherals and board pin mapping. No bitstream or physical board test was
performed.

The profile has 64 loops, 128 requests, 256 events, 512 program/state/job entries,
256 sample buffers, eight input/output ports, 64 views, 2,048 sample chunks,
32 frames per chunk/quantum, 32 chunks per allocation block, four read/write
cache buffers and AXI64.

| Resource | Core | Zynq-7020 capacity | Usage |
| --- | ---: | ---: | ---: |
| LUT | 26,347 | 53,200 | 49.52% |
| Flip-flop | 18,522 | 106,400 | 17.41% |
| BRAM36 equivalent | 56 | 140 | 40.00% |
| DSP | 4 | 220 | 1.82% |

The timing context adds 135 LUTs and 4,854 flip-flops, for totals of 26,482 LUTs
and 23,376 flip-flops. It adds no BRAM or DSPs.

**This configuration still does not fit a 7010:** 26,347 LUTs exceed that device's
17,600. DDR-backed metadata has reduced BRAM usage enough to fit its nominal
60-block capacity, but the LUT requirement remains decisive. No 7010 build was
run; this conclusion uses the existing resource measurements.

## What was needed after upstream integration

Upstream advanced through `09700c9f` with DDR metadata, block allocation, shared
AXI, audio I/O and production-oracle verification. The optimizations were merged
with those changes, including moving address-mapping stages into `chunk_dma`.

The first merged implementation failed setup by 4.651 ns. Metadata node-count
arithmetic, capacity comparison and request admission formed long control paths.
Separating capacity arithmetic added one cycle to metadata writes/capacity
checks. Registering the metadata request boundary added one cycle per operation.
Cache word counters and DMA counts were narrowed to the actual transfer bounds.
A RAM template with one read port and one write port removed Vivado's conflicting
write-port inference warnings.

Other measured paths were shortened without adding cycles: hint conversion and
overflow validation now happen before FIFO storage; a preparation-time member
check runs during an existing RAM wait; speculative cache-fill arbitration uses
pending-demand priority without command-validation arithmetic. The existing
sample-streaming cadence is preserved. The full command/job latency table is in
the [runner README](../README.md).

The final route was 0.156 ns short. Vivado's post-route
`phys_opt_design -directive AggressiveExplore`, followed by routing, closed the
remaining Finish write-enable paths. This optimization changes no cycle counts
and is included in the checked-in implementation flow. The final worst path is
from the coordinator's phase register to metadata DMA address validation.

| Routed measurement | Setup WNS | Hold WHS |
| --- | ---: | ---: |
| Initial merged design, `6e0019e1` | −4.651 ns | +0.045 ns |
| Metadata capacity/RAM changes, `d53a5255` | −0.497 ns | 0.000 ns |
| Hint/control changes, `c1089b34` | −0.517 ns | +0.004 ns |
| Request/counter changes, `bb885dda` | −0.156 ns | +0.005 ns |
| Same RTL after post-route physical optimization | **+0.002 ns** | **+0.005 ns** |

## Latency and verification

The deterministic 64-loop playback comparison takes 23,346 steady cycles versus
23,344 before the request boundary, an increase of **2 cycles / 0.0086%**. At
100 MHz that is 233.46 µs per steady quantum, with 20 ns added in this workload.
First-quantum execution changes from 25,680 to 25,682 cycles. Prefetch overlaps
almost all of the added metadata latency in this case. These are native instant
memory-model results without random stalls, not a real DDR or app wall-clock
latency prediction.

All **76 cocotb test cases** in the recorded validation groups pass. They include
19 common production/RTL scenarios with stalls and processing faults, nine
standalone media tests across three capacities, all eight metadata cache/transport
configurations, transition/cache/Gain/Mix/AXI32/64 component tests, the 64-loop
production comparison and continuous 16-channel audio. Comparison tests contain
multiple directed/random scenarios. Full configured-core/context lint,
generated-wrapper drift checking and `git diff --check` also pass.

Post-route DRC has no errors or critical warnings. Its eight warnings comprise
six suggestions for additional DSP pipelining, one for intentionally unused
timing-context capture outputs, and one for the omitted PS7. Synthesis has no
errors or critical warnings.

## Evidence and reproduction

The measured RTL revision is `bb885ddad1bbaa6a69c229cd0deddcc898d5b84b`.
Post-route optimization is committed in flow revision
`9becd0bd39baac6e8e71438d9333f702b6d908ba`. Every measured RTL/header SHA256 matches
both the committed source tree and the final working tree. The run's dirty flag
reflects README/report work; the measured RTL itself was committed and pushed
before implementation. The report commit adds no RTL changes.

- [Machine-readable results](results.json), including validation groups and cycle counts.
- [Timing summary](selected/post_route_timing_summary.rpt), [core utilization](selected/post_route_core_utilization.rpt), [whole-context utilization](selected/post_route_utilization.rpt).
- [Timing checks](selected/post_route_check_timing.rpt), [routing status](selected/post_route_route_status.rpt), [DRC](selected/post_route_drc.rpt).
- [Source hashes](selected/source-manifest.json), [profile/tool setup](selected/setup.txt), [constraints](selected/core.xdc), [refinement record](selected/refinement.json).

Reproduce with the [synthesis runner](../run.py) and an installed Vivado:

```sh
python3 fpga/synthesis/run.py --vivado /path/to/Vivado/bin/vivado \
  --output work/vivado-reproduce
```

The checkpoint refinement wrote all implementation reports successfully; its
one-off summary formatter lacked a Tcl variable. The JSON summary was exported
from the completed timing report, and the formatter was corrected. The checked-in
full flow defines that variable. This did not change the netlist or constraints.
