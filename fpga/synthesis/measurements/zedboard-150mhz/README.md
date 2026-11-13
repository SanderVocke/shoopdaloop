# ZedBoard individual subsystem timing at 150 MHz

Work branch: `optimize/zedboard-150mhz`, based on
`proto/fpga-rust-reference` at `2bdd10c979d4bc6ad2ca953002b4a481be38a9e3`.
Measurements use Vivado 2021.1, SW Build 3247384, on `xc7z020clg484-1`.

| Subsystem | Initial routed setup slack (ns) | Selected routed setup slack (ns) | Hold slack (ns) | Status |
| --- | ---: | ---: | ---: | --- |
| State processor | -1.168 | +0.259 | +0.008 | Meets 150 MHz |
| Job processor | -2.498 | +0.025 | +0.008 | Meets 150 MHz |
| Memory/cache (`media_store_axi`) | -4.326 | Pending | Pending | Optimization in progress |

These are independent out-of-context measurements with retained fabric launch
and capture registers. The selected constraint is 6.666 ns, conservatively
150.015 MHz, with 0.200 ns user uncertainty. No false paths or multicycle
exceptions are used. Initial runs used 6.666666667 ns, rounded by Vivado to
6.667 ns; that 1 ps difference slightly favors the initial results.
The whole integrated board, PS7, DDR controller and physical I/O require their
own timing closure and hardware validation.

## Configuration and reproduction

Use [the subsystem runner](../../subsystems.py) from the repository root:

```sh
python3 fpga/synthesis/subsystems.py state --threads 2 --vivado /path/to/vivado --output work/state-150
python3 fpga/synthesis/subsystems.py job --threads 2 --vivado /path/to/vivado --output work/job-150
python3 fpga/synthesis/subsystems.py memory --threads 2 --vivado /path/to/vivado --output work/memory-150
```

The output directory must be empty. Each run freezes the RTL, generated context,
constraints, Tcl flow, parameters and SHA256 manifests before starting Vivado.
The process exits unsuccessfully if either routed setup or hold slack is
negative. Synthesis/placement estimates alone do not establish closure.

- State: 512 program entries, 512 state entries, 512 jobs.
- Job: 256 scratch buffers, 32 frames, eight external inputs and outputs,
  media enabled, 32-frame chunks.
- Memory: 64 views, 2,048 physical chunks, 32-frame chunks, 32-chunk allocation
  blocks, four read and four write buffers, 8/4/2 table/reference/stack cache
  lines, full metadata and reclamation, shared AXI64 endpoint.

The public source image is `siliconbootcamp/xilinx-vivado:stable`, pinned to
`sha256:d24586405894b07f6af006d5cb15ba01c7e8585de97b28b6121fcf6282db7a5f`.
Its complete vendor layer is
`sha256:187ac31317b800d0b7782a3aff7fdedd200a27303a22121d7c3a6a623b36bf26`.
The downloaded layers were SHA256-verified. The unmodified vendor installation
was mounted read-only into a Docker runtime using the image's original OS
dependencies, with a writable non-root home and the original system libudev
preloaded. Later image overlays replacing vendor Tcl files were excluded.
See [the existing installation record](../../../zedboard/VALIDATION.md#fresh-vivado-20211-build)
for this public-image setup. The missing IP catalog produces a checkpoint-time
catalog warning; these RTL-only runs instantiate no catalog IP.

## Retained evidence

`state/` and `job/` contain the selected post-route setup/hold summaries,
critical paths, core/total/hierarchical utilization, timing-constraint checks,
route status, DRC, exact timing context, run Tcl and provenance manifests.
Both have zero unconstrained internal endpoints and zero routing errors.
DRC contains warnings only: omitted PS7 and unused context capture outputs;
the job run additionally reports DSP pipelining suggestions and inferred LUT
equation terms with unused connected inputs.

The runs record the actual Git revision and dirty-tree state at launch.
The selected state RTL matches commit `4a9d0cdb`; the selected job RTL matches
`01f0946d`. Their source manifests also list libraries that are read but not
elaborated for those tops. Changes to the independent memory library therefore
do not invalidate those selected state/job implementations.
`baseline/` retains the initial routed summaries and their source provenance.
Large checkpoints and complete frozen source trees remain in the ignored
`work/150mhz/` run directories.

| Selected core | LUTs | Flip-flops | BRAM36 equivalents | DSPs |
| --- | ---: | ---: | ---: | ---: |
| State | 1,388 | 947 | 7.5 | 0 |
| Job | 3,575 | 2,278 | 10 | 4 |

## Implementation and verification

The state engine captures admission checks, RAM operands, ramp elapsed sums and
shared-loop responses before evaluating them. The job processor stages buffer
and port validation, scratch samples, signed operand preparation, four 16x16
partial products, product assembly and saturation. Bounded run counters and a
constant validated chunk shift remove unnecessary wide arithmetic. Gain retains
one sample per clock and Mix two reads per sample after pipeline fill.

Memory work stages command admission, physical chunk commands, DMA address
mapping/bounds, metadata RAM results and tree capacity checks. Foreground
descriptor results are retained before hint requests can reuse the RAM port.
Maintenance accounting includes its pending work charge in both effective
credits and published totals. A simulator cleanup fix selects each model's
Verilator thread context before destroying it, allowing multi-model suites to
exit correctly.

Validation so far includes state default/small/minimum profiles; audio default
and small profiles with 150 seeded cases each; smoothing extremes; AXI32 and
AXI64 bounds, bursts, stalls, faults and reset; four chunk profiles with 64 seeded
cases each; the metadata geometry/cache matrix; media/retained ownership and
cleanup budgets; exact production scenario replay and production fault/reset
containment. Rust formatting, warning-denied testbench builds, the test-attribute
policy and generated AXI-wrapper checks pass.

The 64-loop Q=32, blocked AXI64 production workload also passes exact fixed-point
and floating-oracle comparisons. A memory-validation intermediate took 23,522
cycles for its first quantum and 21,842–21,854 for the next three in the
instant-memory model. These are functional simulation cycles, not measured DDR
service or full-board 150 MHz results. Final memory timing and final workload
measurements remain pending.

`validation/` retains native runner records with exact arguments, results, model
parameters and RTL source hashes for the state, audio, smoothing and AXI suites.
Those subsystem hashes match the current branch.
