# Zynq core timing experiment

Individual ZedBoard measurements use Vivado 2021.1 and the
`xc7z020clg484-1` device:

```sh
python3 fpga/synthesis/subsystems.py state --threads 2 --vivado /path/to/vivado --output work/state-150
python3 fpga/synthesis/subsystems.py job --threads 2 --vivado /path/to/vivado --output work/job-150
python3 fpga/synthesis/subsystems.py memory --threads 2 --vivado /path/to/vivado --output work/memory-150
```

Each run freezes its sources and timing context in an empty output directory,
records hashes and parameters, and runs synthesis, placement, routing and
post-route physical optimization. Setup and hold must both pass the 150 MHz
constraint with 0.2 ns uncertainty. `--stage synth` and `--stage place` stop at diagnostic checkpoints; neither
establishes timing closure. Interfaces use retained fabric
launch/capture registers, with no false paths or multicycle exceptions.
The period is conservatively rounded down to 6.666 ns (150.015 MHz), avoiding
Vivado's rounding of 6.666667 ns to a period slightly slower than 150 MHz.
`--strategy timing` uses Explore optimization, ExtraNetDelay_high placement and
AggressiveExplore routing with 0.9 ns setup uncertainty during implementation.
Every reported checkpoint restores the same 0.2 ns measurement uncertainty;
hold uncertainty and the 6.666 ns clock remain unchanged.

The state profile has 512 program/state/job entries. The job profile has 256
32-frame buffers, eight external input/output ports, media enabled and 32-frame
chunks. The memory profile measures `media_store_axi`, including sample caches,
metadata caches/storage, reclamation and the shared AXI64 master, with 64 views,
2,048 chunks, 32-chunk allocation blocks, four read/write buffers and 8/4/2
table/reference/stack cache lines. These are independent subsystem measurements;
the complete integrated board design requires its own timing closure.

The [selected 150 MHz measurements](measurements/zedboard-150mhz/README.md)
include routed state/job/memory results, exact reproduction commands and native
validation provenance. Memory uses tighter placement followed by routing at the
original uncertainty.

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

The current 150 MHz work adds register boundaries to admission, RAM results,
arithmetic and memory commands. Functional results and ready/valid handshakes
are preserved; command latency increases. Gain still processes one sample per
clock once filled, and Mix uses two reads per sample. Arithmetic and active
smoothing use additional per-sample stages.

| Area | Work moved or simplified | Latency effect |
| --- | --- | --- |
| State processor | Capture run checks, state RAM operands, ramp elapsed sums and shared-loop responses before evaluation. | Admission and affected operand/loop operations take additional cycles. |
| Job processor | Capture buffer/port checks, RAM samples and signed operands; register four 16x16 partial products before assembly and scaling. Prepare fraction thresholds and parallel gain additions. | Additional dispatch, fill/drain and arithmetic stages; Gain/Mix sample cadence is retained. |
| Media store | Capture command checks, metadata requirements, append/slice checks and slice offsets. Buffer physical chunk commands and retain shared RAM results. Separate sample Fence completion from metadata flushing. | Additional admission/planning cycles, one preparation cycle per phase/configuration transition and one per physical chunk command; active sample-stream cadence is retained. |
| Metadata | Capture root RAM and tag matches; separate tree checks, node counting, capacity arithmetic/comparison and trim decisions. Bound line/scan/tree-level counters; retain only the parent-root predicate needed by allocation. | Additional setup cycles per metadata/cache operation and trim iteration; sample-word cadence is retained. |
| AXI | Capture logical addresses before offset mapping, then validate captured bounds before burst preparation. Validate endpoint commands after admission. | Additional descriptor/command setup cycles, independent of the number of sample beats. |
| Prefetch/cache | Bound physical IDs and progress counters; balance parallel replacement searches; stage hint validation, demand/speculative directory updates and DMA tag capture after line reservation. Reserve the sample RAM port by stream phase. Register maintenance grant/accounting eligibility. | One extra demand-miss directory cycle, one DMA tag-setup cycle and additional hint setup/directory cycles; writeback scheduling and grant activation can begin one cycle later. Demand stream cadence is retained. |

The earlier integrated 100 MHz measurements retain their own frozen RTL
and hashes. Their measured cycle counts predate these 150 MHz pipeline stages.

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
