# BRAM audio job processor

This milestone executes Input, Silence, Gain, Mix, and Output jobs using reusable
BRAM quantum buffers. `audio_pipeline` connects the job processor to the existing
state processor's FIFO. The comparison bench runs the same program and samples
through the actual Rust state and job processors.

The ordinary top defaults to `ENABLE_MEDIA=0` and refuses ReadView and Capture.
The optional media command/stream interface connects those jobs to the bounded
external-memory store in `fpga/media_store`; its combined top and tests exercise
recording, playback and shared ranges. BRAM here holds scratch and I/O buffers.

`fpga/loop_pipeline` connects transition processing through this wrapper's shared
loop read port. Logical playback/capture instructions use committed loop records
directly. The wrapper also supports `run_external_jobs`: descriptors then transfer
with `external_job_valid && external_job_ready` through `monitor_data`. After the
FIFO empties, `external_done_valid && external_done_ready` reports completion or
`external_error`. The wrapper stays busy until that acknowledgement. Its internal
job processor remains the default path. The connected top enables media through
`media_audio_pipeline`, executing actual recording/playback against its external
sample-memory port. Its README defines external error encoding and
clock/transaction ownership.

## Structure

- `rtl/job_processor.sv`: serial job execution, buffer readiness, sample work.
- `rtl/jp_sample_ram.sv`: synchronous 32-bit RAM requested as block RAM.
- `rtl/jp_divider.sv`: shared unsigned 64/32 restoring divider, 64 iterations.
- `rtl/audio_pipeline.sv`: state execution followed by job execution.
- `simulate.py`, `tests/`: cocotb/Verilator transport, stimulus, Rust comparison.
- `check.sh`: lint and generic/ECP5 synthesis for standalone and combined tops.

Defaults: 32 instructions, 16 state entries, 32 FIFO jobs, at most 32 frames,
16 scratch buffers, two input ports, two output ports. The milestone graph uses
six instruction/buffer IDs and outputs only to port zero; port one checks unused
output clearing. The small bench uses 16/8/16 program/state/job entries, eight
buffers, and a three-frame stride.

Sample addresses are `buffer_or_port * MAX_FRAMES + frame`. The stride does not
change with quantum length. Default logical sample storage is 2,560 bytes:
2,048 scratch bytes and 256 bytes each for input/output. Physical allocation is
larger because of block-RAM granularity. Sample RAM contents are not reset.

## Arithmetic

Samples/gains are raw signed Q8.24 i32 values. Mix uses a 33-bit sum and clamps
to i32. Gain multiplies unsigned magnitudes using one registered 32x32 multiplier,
drops 24 low product bits, reapplies the sign, and saturates. This truncates toward
zero, including negative products smaller than one raw sample unit.

For a live ramp, initialization divides
`abs(target - start) * elapsed / duration` to obtain the initial change/remainder,
and `abs(target - start) / duration` for the step. The unsigned 64-bit product
covers extreme i32 endpoints and u32 counters. Subsequent samples advance the
quotient/remainder by addition, comparison, and subtraction. Descending ramps
subtract the magnitude, preserving Rust's truncation. At duration the gain is
the exact target. Constant, zero-duration, and ended ramps skip the divisions.
The job snapshot is immutable; the state processor advances ramps between quanta.

There is no combinational divider or division per sample. Generic checks forbid
`$div`, `$mod`, and latches. ECP5 maps the shared multiplier to four 18x18 DSPs.

### Streaming gain and mix

Gain overlaps synchronous scratch reads, multiplication, scaling and scratch
writes. Each issued read captures its own gain value; ramp progression advances
with issued samples, so multiple in-flight samples retain the correct gains.
The shared multiplier still initializes ramps and processes samples. Scaling
and saturation retain their registered stage and existing bitwise behavior.
After pipeline fill, the engine writes one gain result per clock.

Mix alternates its two operand reads on the existing single scratch read port.
It retains the first operand while reading the second and overlaps subsequent
reads with result writes. It writes one mixed result every two clocks, including
when both sources refer to the same buffer. No RAM port or multiplier is added.

Both paths drain their last write before publishing destination readiness and
accepting the next job. Reset clears validity; an asynchronous media fault
aborts in-flight work and suppresses further stream writes. `job_enable` still
gates admission only, leaving accepted streams running. For quantum Q, constant
gain job intervals are Q+6 clocks and mix intervals are 2Q+4, including job
handoffs. Live ramps retain the existing divider initialization before streaming.

## Interfaces

All interfaces share one clock and synchronous active-high reset. Reset aborts
work and clears control, FIFO contents, and buffer readiness. It retains RAM.

`job_processor` accepts `run_valid && run_ready`, latching quantum, logical
buffer/input/output counts, and job count. Quantum must be `1..MAX_FRAMES`;
other logical counts can be zero and must fit physical capacity. After admission
it clears readiness and every logical output sample, then accepts exactly the
declared number of 256-bit job descriptors. Packing is defined in
`fpga/state_processor/README.md`. Jobs execute serially.

`job_enable` gates ready during descriptor fetch. Tie it high normally, or low
to pause acceptance. Accepted jobs finish normally. Producers retain valid/data
while stalled. `done` pulses for one cycle; `error_code` remains latched.

Producers require an unready destination; consumers require ready sources. All
IDs are checked against the logical buffer count. A destination becomes ready
after its last sample is written. Fan-out and repeated sources are supported.
Readiness resets each quantum, allowing storage reuse. Output jobs can overwrite
a port; ports without jobs stay zero.

### Configuration

The idle-only sample configuration port transfers on `cfg_valid && cfg_ready`.
`cfg_space` selects input (0), output (1), or scratch (2); space 3 is invalid.
`cfg_address` indexes a 32-bit sample word using the stride above. Writes carry
raw bits in `cfg_write_data`; reads return raw bits in `cfg_read_data`.
`cfg_response_valid` pulses in the cycle after acceptance for reads and writes.
Write/error responses return zero. Out-of-capacity accesses report
`cfg_response_error` without accessing a truncated/aliased address. One request
can be outstanding; configuration has priority over a simultaneous run request.

The combined top retains the original state `cfg_*` port and names sample
configuration `audio_cfg_*`. Both block while the pipeline is busy. Its run
command supplies state metadata plus logical buffer and I/O counts. Load inputs
before each quantum. `state_done` means jobs are queued; overall `done` means
sample execution finished. Command metadata is latched at acceptance.

`buffer_ready` exposes readiness. Host scratch readback is physically allowed
while idle, but only ready buffers have defined architectural samples.
`monitor_valid/ready/data` observe actual FIFO transfers. After failure,
`drain_valid`/`drain_ready` allow inspection of remaining jobs via `monitor_data`.
Successful runs leave the FIFO empty.

### Errors and partial effects

| Job result code | Meaning |
| --- | --- |
| 0 | Completed |
| 1 | Invalid buffer/source/destination/port |
| 2 | Invalid shape or logical capacity |
| 3 | Unsupported ReadView/Capture |
| 4 | Unknown opcode/internal phase |

The combined top reports `error_stage=1` for state errors, retaining the state
processor's codes, and 2 for job completion/error using the table above. State
updates precede sample work and survive job failure. The failing job is popped;
later jobs remain. Earlier sample writes and ready buffers remain. Invalid
admission does not clear previous ready/output contents.

This matches Rust for the supported subset. Unknown packed opcodes, unsupported
media jobs, zero quantum, and physical-capacity rejection have hardware-specific
expectations. Bench input arrays must have exactly the quantum's length; the
transport does not model an incomplete sample array as a hardware request.

## Rust trace boundary

`fpga-reference trace/check` additionally accept `audio_case/audio_trace` payloads.
A case supplies program/initial state, queue capacity, logical buffers/outputs,
and per-step settled writes and raw input samples. Results contain emitted and
unconsumed jobs, state, outputs, and one entry per scratch buffer: samples if
ready, null otherwise. They also identify the failing/completed stage and status.

`run_audio_case` invokes actual Rust `StateProcessor::run` and
`JobProcessor::run`, retaining buffers/outputs across steps. Optional media snapshots
enable Rust recording/playback and shared-range comparisons; the media-enabled RTL
bench compares those cases too.
Python validates the generated schema and compares complete results; it does
not implement reference sample arithmetic. The format change adds payload
variants within version 2. Programs explicitly specify `chunk_frames`; media
jobs use `position_chunks` and `length_chunks`. Arithmetic ramps retain sample
units. Normal media execution requires the configured physical chunk size;
BRAM-only and external-job runs retain their separate profiles.

## Run and reproduce

Use the repository's development environment for Cargo. Put Verilator, Make, and
a C++ compiler on PATH, then use the existing cocotb dependencies:

```sh
python3 -m venv work/fpga-sim-venv
. work/fpga-sim-venv/bin/activate
python -m pip install -r fpga/state_processor/requirements.txt
python fpga/job_processor/simulate.py
```

The runner builds the reference tool and runs both configurations. `--config`,
`--seed`, `--random-cases`, `--waves`, and `--output` select a run. Replay failures
using `--case` with the matching single configuration. Expected results are
regenerated by Rust. Failure artifacts include case, expected trace, observed
results, and timing. VCD recording is optional. Logs, xUnit results, build files,
and summaries default to `work/fpga-job-sim`.

A saved three-quantum version of the concrete milestone graph is included:

```sh
python fpga/job_processor/simulate.py --config default \
  --case fpga/job_processor/examples/two_input_mix.json \
  --waves --output work/fpga-job-example
```

This also exercises single-case replay and records a VCD in the output directory.

Regenerate the shared schema after changing boundary types:

```sh
cargo build --locked -p rt_fpga_testbench
target/debug/fpga-reference schema > scripts/fpga_testbench/schema.json
```

For lint and synthesis with Yosys, its ECP5/ABC files, and Verilator:

```sh
fpga/job_processor/check.sh
```

Logs/netlists default to `work/fpga-job-synthesis`; the optional first argument
changes that directory. Both standalone and combined tops are checked. There
is no placement/routing or timing-closure check.

## Initial results

The following initial cycle/mapping results describe the scalar sample engine.
Current measurements are in the pipelining checkpoint below.

Verilator 5.032 / cocotb 1.9.2, seed 20261110:

| Configuration | Directed cases/quanta | Seeded cases/quanta |
| --- | --- | --- |
| Default, Q=1/3/32 | 77 / 230 | 150 / 450 |
| Small, Q=1/3 | 55 / 157 | 150 / 450 |
| Total | 132 / 387 | 300 / 900 |

All 432 cases and 1,287 quanta matched Rust bit for bit. Directed cases compare
18,572 intermediate/output samples plus state, readiness, and FIFO effects.
Seeded cases add independent sample/ramp data and settled updates. Both protocol
tests passed: invalid addresses/spaces, poisoned unused outputs clearing, FIFO
pauses, blocked writes, reset during ramp division, RAM retention/restart, and
explicit unsupported-capture refusal.

The reference contract's 17 Rust tests and 10 Python tests pass. The warning-denying
reference build, shared test-attribute policy, and format/diff checks pass. The
full workspace suite was not rerun.

### Cycles

The milestone graph is `Input 0 -> Gain A` and `Input 1 -> Gain B`, followed by
Mix and Output 0. With two live ramps, Q=32, and acceptance always enabled:

| Phase | Clock periods |
| --- | --- |
| State execution | 28 |
| Job execution, both ramps live at entry | 1,113 |
| Wrapper handoff/completion | 3 |
| Total, accepted run to done | 1,144 |

At an assumed 150 MHz, total execution is 7.627 microseconds. This includes
clearing two output buffers and consuming the FIFO, but excludes host loading
and readback. It processes all 32 frames. After both ramps end, the same graph
takes 878 periods (5.853 microseconds); each live ramp initialization adds 133
periods. These are behavioral counts, not a clock-frequency guarantee.

### Synthesis

Yosys 0.52, default capacities:

| ECP5 cell | Job processor | Combined pipeline |
| --- | --- | --- |
| LUT4 | 1,405 | 2,674 |
| TRELLIS_FF | 1,055 | 2,610 |
| DP16KD block RAM | 3 | 3 |
| MULT18X18D DSP | 4 | 4 |
| TRELLIS_DPR16X4 distributed RAM | 0 | 209 |
| CCU2C carry | 549 | 700 |

At these small capacities, combined state/program/FIFO storage uses distributed
RAM; sample banks use block RAM. Counts are synthesis estimates. LUT RAM,
multiplexers, and packing need separate consideration when sizing a device.
No board I/O mapping or routed timing result is claimed.

These initial mapping counts precede the shared-loop/external-execution ports.
See `fpga/loop_pipeline/README.md` for the connected top's current mapping and
capacities.

## Gain/mix pipelining checkpoint

Revision `928d27aa` retains all 432 directed/seeded cases and 1,287 quanta from
the two profiles above, matching Rust bitwise. The new protocol test verifies
one-/two-clock write cadence, ordered destinations and publication only after
the final write. Forty reset placements cover fill/drain, partial writes,
one-sample and odd-size quanta, followed by Rust-checked restart. Media protocol
tests inject faults during both gain and mix with valid writes in flight.

The Q=32 two-input milestone now takes 800 clocks while both ramps are live
(state 28, jobs 769, handoff 3), then 534 clocks after both ramps end. Previously
these totals were 1,144 and 878. Ramp division costs are unchanged.

Paired standalone ECP5 mapping with the pre-pipeline source at `52c70d51`:

| Cell | Scalar | Pipelined |
| --- | ---: | ---: |
| LUT4 | 1,423 | 1,617 |
| TRELLIS_FF | 1,055 | 1,200 |
| DP16KD | 3 | 3 |
| MULT18X18D | 4 | 4 |

Standalone/combined lint and generic/ECP5 checks pass. The default eight-loop
AXI top maps to 26,991 LUT4, 14,064 flip-flops, 93 DP16KD and four MULT18X18D;
its integrated tops pass lint at both bus widths. No routed clock timing was
measured. See the [full 64-loop remeasurement](../loop_pipeline/README.md#pipelined-gain-and-mix-checkpoint)
for actual TP/state/job/DRAM overlap and the newly exposed memory waits.
The later [combined chunk-burst checkpoint](../loop_pipeline/README.md#combined-pipelining-and-chunk-bursts)
removes those exposed waits while retaining the gain/mix pipeline.

## Playback sample delivery

ReadView samples write scratch RAM directly on the media valid/ready handshake,
with no intervening per-sample store state. The destination becomes ready on
its final write; the job still awaits media completion and propagates failures.
Reset and a reported memory fault suppress further playback writes. Combined
with the cache read pipeline, resident chunks sustain one sample per clock.
Rust arithmetic, media semantics and job formats are unchanged.

The [playback timing checkpoint](../loop_pipeline/README.md#pipelined-playback-sample-delivery)
measures 48 clocks per Q=32 ReadView job, down from 110, and 125.16 us per
64-loop playback quantum at an assumed 100 MHz. Protocol tests check consecutive
scratch addresses/writes, publication on the last write, resets during the
stream, an unrelated prefetch fault during playback, and Rust-checked restart.
