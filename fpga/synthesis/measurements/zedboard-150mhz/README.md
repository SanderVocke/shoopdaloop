# ZedBoard individual subsystem timing at 150 MHz

Work branch: `optimize/zedboard-150mhz`, based on
`proto/fpga-rust-reference` at `2bdd10c979d4bc6ad2ca953002b4a481be38a9e3`.
Measurements use Vivado 2021.1, SW Build 3247384, on `xc7z020clg484-1`.

| Subsystem | Initial routed setup slack (ns) | Selected routed setup slack (ns) | Hold slack (ns) | Status |
| --- | ---: | ---: | ---: | --- |
| State processor | -1.168 | +0.259 | +0.008 | Meets 150 MHz |
| Job processor | -2.498 | +0.025 | +0.008 | Meets 150 MHz |
| Memory/cache (`media_store_axi`) | -4.326 | +0.043 | +0.008 | Meets 150 MHz |

These are independent out-of-context measurements with retained fabric launch
and capture registers. The selected constraint is 6.666 ns, conservatively
150.015 MHz, with 0.200 ns user uncertainty. No false paths or multicycle
exceptions are used. Initial runs used 6.666666667 ns, rounded by Vivado to
6.667 ns; that 1 ps difference slightly favors the initial results.
The whole integrated board, PS7, DDR controller and physical I/O require their
own timing closure and hardware validation.

## Configuration and reproduction

Use fresh output directories and Vivado 2021.1. From the repository root:

```sh
python3 fpga/synthesis/subsystems.py state --threads 2 --vivado /path/to/vivado --output work/state-150
python3 fpga/synthesis/subsystems.py job --threads 2 --vivado /path/to/vivado --output work/job-150
```

Memory uses extra setup margin during placement, then routes the saved placement
at the original 0.200 ns uncertainty. Reproduce those two stages with:

```sh
python3 fpga/synthesis/subsystems.py memory --threads 1 --strategy timing --stage place --vivado /path/to/vivado --output work/memory-placement
mkdir work/memory-150
cd work/memory-150
/path/to/vivado -mode batch -nojournal -log vivado.log -source ../../fpga/synthesis/measurements/zedboard-150mhz/memory/run.tcl -tclargs ../memory-placement/post_place.dcp
```

Check that `post_route.json` contains `"met_150mhz": true`. The subsystem runner
freezes RTL, context, constraints, Tcl, parameters and SHA256 manifests before
starting Vivado. Its route mode exits unsuccessfully if setup or hold is
negative. Synthesis and placement are diagnostic stages; routed setup and hold
establish closure.

- State: 512 program entries, 512 state entries, 512 jobs.
- Job: 256 scratch buffers, 32 frames, eight external inputs and outputs,
  media enabled, 32-frame chunks.
- Memory: 64 views, 2,048 physical chunks, 32-frame chunks, 32-chunk allocation
  blocks, four read and four write buffers, 8/4/2 table/reference/stack cache
  lines, full metadata and reclamation, shared AXI64 endpoint.

Memory placement uses Explore optimization, ExtraNetDelay_high placement and
AggressiveExplore physical optimization with 0.900 ns setup uncertainty. The
selected routing stage reopens that placement checkpoint, restores 0.200 ns
uncertainty, and runs routing, AggressiveExplore physical optimization and routing
again. The 6.666 ns clock and hold uncertainty remain fixed. No placement region
constraint is used in the selected result.

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

`state/`, `job/` and `memory/` contain the selected post-route setup/hold
summaries, critical paths, core/total/hierarchical utilization, timing-constraint
checks, route status, DRC, exact timing context, run Tcl and provenance manifests.
All three have zero unconstrained internal endpoints and zero routing errors.
Memory has 17,301 fully routed nets. DRC has no errors or critical violations;
its warnings cover omitted PS7 and unused context capture outputs. The job run
also reports DSP pipelining suggestions and inferred LUT equation terms with
unused connected inputs.

`memory/placement/` retains the actual placement inputs, reports and original
run provenance. `memory/run.tcl` is the selected checkpoint-routing script;
its provenance records the input checkpoint SHA256. Large checkpoints and frozen
source trees remain in the ignored `work/150mhz/` directories.

The selected state RTL matches commit `4a9d0cdb`, job RTL `01f0946d`, and memory
RTL `f872a5f7`. The runs record their actual launch revisions and dirty-tree
state. Source manifests also include libraries read but unused by each top;
later changes to unused libraries do not invalidate an implementation.
`source-audit.json` records the checked source hashes and timing results.
`baseline/` retains the initial routed summaries and source provenance.

| Selected core | LUTs | Flip-flops | BRAM36 equivalents | DSPs |
| --- | ---: | ---: | ---: | ---: |
| State | 1,388 | 947 | 7.5 | 0 |
| Job | 3,575 | 2,278 | 10 | 4 |
| Memory/cache | 10,289 | 7,354 | 7.5 | 0 |

## Implementation and verification

The state engine captures admission checks, RAM operands, ramp elapsed sums and
shared-loop responses before evaluating them. The job processor stages buffer
and port validation, scratch samples, signed operand preparation, four 16x16
partial products, product assembly and saturation. Bounded run counters and a
constant validated chunk shift remove unnecessary wide arithmetic. Gain retains
one sample per clock and Mix two reads per sample after pipeline fill.

Memory stages command admission, physical chunk commands, DMA address mapping,
metadata RAM results, tree capacity and slice decisions. Foreground descriptor
results are retained while hints reuse the RAM port. A transition token marks
prepared phase controls. Cache replacement searches are balanced; misses capture
selected lines before directory updates, and DMA reserves a line before reading
its tag. Execution fields are narrowed after full-width validation. Metadata
stores a parent-root predicate and simplifies page-address arithmetic.
Maintenance accounting includes pending work in effective credits and published
totals. These boundaries add command, planning and transfer-setup cycles while
retaining active sample-stream cadence; [the implementation summary](../../README.md)
describes the individual latency effects.

The Verilator model destructor selects its own thread context before cleanup,
allowing suites with independent models to exit correctly.

`validation/` retains native runner records with exact arguments, results,
parameters and RTL hashes. The retained model hashes match the selected branch.
Checks include state default/small/minimum profiles; audio default and small
with 150 seeded cases each; smoothing extremes; AXI32/64 bounds, bursts, stalls,
faults and reset; chunk default/small/odd/minimum profiles with 64 seeded cases;
legacy word transfers; the metadata geometry/cache matrix; direct media,
reclamation budgets, exact production replay, and production fault/reset tests.
The measured memory profile and odd/minimum chunk geometries pass RTL lint.

The exact production 64-loop benchmark also passes with 2,048 chunks, 32-chunk
allocation blocks and AXI64. Its retained cycle record uses instant simulated
memory:

| Iteration | Total cycles | State cycles | Job cycles |
| --- | ---: | ---: | ---: |
| 1 | 23,872 | 1,733 | 19,880 |
| 2 | 22,208 | 1,733 | 19,552 |
| 3 | 22,212 | 1,733 | 19,556 |
| 4 | 22,207 | 1,733 | 19,551 |

These cycle counts describe RTL simulation. Hardware DDR latency and whole-board
clock closure are separate measurements.
