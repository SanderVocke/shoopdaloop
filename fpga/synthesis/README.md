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

See [measured results](measurements/RESULTS.md) for the routed timing, utilization,
source hashes and validation evidence for the integrated DDR-backed design.

## Pipeline choices

Most added stages are at command or job boundaries. Existing RAM waits are used
for work that can be prepared early. No additional per-sample cycles are added.

| Area | Work moved or simplified | Extra cycles |
| --- | --- | --- |
| Transition processor | Stage timestamp rounding, transition decisions and Finish accumulation. Cache quantum masks and checks during existing RAM waits. | Submit +2; evaluated transition intent +1; whole-quantum Finish +1. |
| Media store | Stage append/slice/reclaim planning and configuration. Prepare sample spans and offsets during table waits. | Nonempty Append +6; Slice +4; Reclaim +3; configuration +2 plus its metadata wait. |
| Metadata | Separate node counting, capacity arithmetic and comparison; register requests; narrow cache word counters. Use a RAM template with one read port and one write port. | Write/capacity-check +1; request boundary +1 per operation. |
| AXI | Stage address mapping and validation; bound chunk transfer counters. | +1 per chunk command, independent of beats or later bursts. |
| Gain | Register multiplier operands and precompute signed ramp steps. | +1 fill/drain cycle per Gain job; sample cadence unchanged. |
| State processor | Compare captured playback position and length during the existing request phase. | Playing-but-empty/finished silence +1; active playback unchanged. |
| Prefetch/cache | Convert hints before FIFO storage; prepare spans during table waits; use fixed priority scans and separate demand priority from command validation. | None. |

The hint FIFO stores frame positions/lengths and an overflow-valid bit. Its chunk
shift is constant within a quantum, and a new run resets the FIFO. Preparing the
conversion on the write side preserves hint order, latency and range checks.

Speculative fills yield to a pending command while the cache is idle. If the
command is a write blocked by full buffers, an outstanding buffer fill or a dirty
writeback already takes priority. This removes command-shape arithmetic from
prefetch arbitration while preserving demand priority.

## Functional verification

The common runner compares production Rust and RTL processing, output/media
samples, loop state, events and outcomes. Useful targeted checks are:

```sh
cargo run --locked -p rt_fpga_testbench --features sim -- production --config odd --block-chunks 32 \
  --directed --random-cases 3 --axi-data-width 64
cargo run --locked -p rt_fpga_testbench --features sim -- production-faults --block-chunks 32
cargo run --locked -p rt_fpga_testbench --features sim -- production --config 8x8 --benchmark64 \
  --block-chunks 32 --axi-data-width 64
cargo run --locked -p rt_fpga_testbench --features sim -- metadata
cargo run --locked -p rt_fpga_testbench --features sim -- continuous --p 16 --frame-cycles 1600 \
  --axi-data-width 64 --stalls
python fpga/axi_memory/generate_tops.py --check
```

The scenario runners build the production oracle automatically. See
[verification setup](../verification/README.md) for simulator dependencies.
