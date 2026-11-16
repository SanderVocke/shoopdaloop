# Standalone SP: integrated loop instructions

Measured on 2026-11-16 using the same pinned Yosys 0.52+45 build
(`ea2c1e7ac`, OSS CAD Suite 2025-04-15) for both source snapshots. The baseline
is commit `39db3bb7f8ffb10350ae809a6e9ecde2dae6b6b0`, before RTL loop execution.
Baseline sources and the first synthesis were preserved before RTL edits.

Both runs synthesize standalone `state_processor` with program/state/job capacities
256/256/256, using `synth_xilinx -family xc7 -flatten -noiopad -noclkbuf`. The new
version additionally has `LOOP_DEPTH=256`, two plans per loop and saved render
cursors. Prefix and audio instructions share the same 256-entry program RAM.
These are capacities, not a claim that 256 sync instructions plus an audio
program fit simultaneously. All core interfaces remain live during synthesis.
The baseline has no owned-loop storage.

| Mapped resource | Before | After | Increase | Increase % |
| --- | ---: | ---: | ---: | ---: |
| LUT cells (LUT1–LUT6) | 1,871 | 7,120 | +5,249 | 280.5% |
| Flip-flops | 1,198 | 2,955 | +1,757 | 146.7% |
| RAMB36E1 | 6 | 21 | +15 | 250.0% |
| RAMB18E1 | 3 | 3 | +0 | 0.0% |
| DSP48E1 | 0 | 0 | +0 | — |
| MUXF7 | 17 | 884 | +867 | 5100.0% |
| MUXF8 | 2 | 267 | +265 | 13250.0% |
| CARRY4 | 50 | 123 | +73 | 146.0% |

BRAM expressed in 36-Kib equivalents increases from **7.5 to 22.5**, an increase
of **15 equivalents (+200%)**. The raw 36-Kib block count grows by 15; the three
18-Kib blocks are unchanged. The increase includes 256 owned loop records, their
two planned transitions, render snapshots, frame stamps, and execution logic.

The added operations run inside the existing SP FSM. Its read stages, operand
register, position/length registers, job/event descriptor register, and arithmetic
adder are shared between the instruction sections. There is no separate sync
processor or request/reply link to another execution controller. Loop RAM is
additional storage, instantiated using the existing generic `sp_ram`.

## Interpretation

The target is the Xilinx 7-series cell family used by the ZedBoard's XC7Z020.
Counts are mapped synthesis cells, not Vivado placed/routed utilization. LUT
counts sum LUT1–LUT6 instances; MUXF7/F8 and carry cells are listed separately.
They are not slice counts. No board I/O pads or clock buffer are included. No
frequency, timing closure, power or post-route packing claim is made.

Simulation validation uses Verilator 5.032. The new suite compares 960 sync
quanta against the Rust model (jobs, full loop state, pulse events, planned
transition outcomes/countdowns and render/next cursors), plus 120 SP/JP quanta
with exact sample/buffer comparisons and malformed-prefix/generation/address
bounds/sticky-fault checks. The retained state/audio/smoothing/reference suites
and default/small/minimum SP lint/generic/ECP5 gates also pass.

## Evidence and reproduction

[Before counts](measurements/sp-sync/before-resources.json),
[after counts](measurements/sp-sync/after-resources.json),
[before provenance](measurements/sp-sync/before-provenance.json), and
[after provenance](measurements/sp-sync/after-provenance.json) preserve tool version,
parameters and hashes of every Verilog source/header.

The full local sources, scripts, logs and mapped netlists are in
`/tmp/shoop-sp-resources/final-before` and `/tmp/shoop-sp-resources/final-after`.
Use the pinned tool build above and the checked-in measurement entry point:

```sh
mkdir -p /tmp/sp-baseline
git archive 39db3bb7 fpga/state_processor/rtl | tar -x -C /tmp/sp-baseline
python3 fpga/state_processor/measure_resources.py \
  --rtl /tmp/sp-baseline/fpga/state_processor/rtl --output /tmp/sp-before
python3 fpga/state_processor/measure_resources.py \
  --loop-depth 256 --output /tmp/sp-after
```

Output directories must be empty. The script preserves input sources before
synthesis; synthesis does not read mutable working-tree files afterward.
