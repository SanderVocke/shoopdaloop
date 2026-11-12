# Zynq core timing experiment

Run the complete 64-loop core through Vivado synthesis, placement and routing:

```sh
python3 fpga/synthesis/run.py --vivado /path/to/Vivado/bin/vivado --output work/vivado-current
```

The output directory must be empty. The runner freezes RTL sources, records their
SHA256 hashes and the Git revision, and generates a registered timing context.
Reports include core and whole-context utilization, setup/hold timing, constraint
checks, routing status, DRC and checkpoints. `timing.json` records worst setup and
hold slack; a failing 100 MHz result makes the runner exit unsuccessfully.

The target is the Zybo Z7-20's `xc7z020clg400-1`, with 64 loops, eight input/output
ports, 32-frame quanta, 32-chunk allocation blocks, 2,048 sample chunks and AXI64.
The clock period is 10 ns with 0.2 ns user uncertainty. Actual launch/capture
registers retain every core port and provide synchronous fabric boundary paths.
No false paths or multicycle exceptions are used. Source and sink registers are
synthetic timing context; functional verification uses the protocol testbenches.

This measures `loop_pipeline_axi`, including metadata caches and the shared AXI
master. It excludes PS7, its DDR controller, the ARM control adapter and audio I/O.
There is no bitstream or physical board signoff. Integration must re-close timing.
The context's register/LUT overhead is reported separately from core utilization.

Before upstream integration, the optimized 64-loop core met 100 MHz with +0.010 ns
setup and +0.005 ns hold slack in Vivado 2021.1. That run used the earlier on-chip
metadata design, 1,024 sample chunks and 256 program/job entries; it is not a
measurement of the current DDR-backed metadata design. Post-integration results
will be recorded here after the pull request is pushed and remeasured.
