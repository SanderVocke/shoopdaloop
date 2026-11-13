# BRAM audio job processor

Simulation now uses the complete [Rust/Marlin testbench suite](../../src/rust/rt_fpga_testbench/README.md):

```sh
cargo run --locked -p rt_fpga_testbench --features sim -- audio
```

The Python simulator entry names are aliases for this runner and accept its
current options. The native suite guide describes profiles, replay, VCDs and
reports. Historical measurements are marked separately.

This processor executes Input, Silence, Constant, Gain, Arithmetic, Smoothing, Mix, and Output jobs using reusable
BRAM quantum buffers. `audio_pipeline` connects the job processor to the existing
state processor's FIFO. The comparison bench runs the same program and samples
through production numeric primitives and independent arithmetic specifications.

The ordinary top defaults to `ENABLE_MEDIA=0` and refuses ReadView, Capture and BeginCapture.
The optional media command/stream interface connects those jobs to the bounded
external-memory store in `fpga/media_store`; its combined top and tests exercise
recording, playback and shared ranges. BRAM here holds scratch and I/O buffers.

`fpga/loop_pipeline` connects transition processing through the shared settled
loop read port. Its production executor always runs jobs internally.
`BeginCapture` (job opcode 7, view in payload 0) uses media command 8 to create or
clear/reuse the recording view, then waits for completion without sample traffic.
The media command bus is four bits. The media-enabled top executes actual sample
recording/playback. Its wrapper no longer has an external descriptor/completion
mode. Debug monitoring and failed-queue draining remain available.

## Structure

- `rtl/job_processor.sv`: serial job execution, buffer readiness, sample work.
- `rtl/jp_sample_ram.sv`: synchronous 32-bit RAM requested as block RAM.
- `rtl/jp_divider.sv`: shared unsigned 64/32 restoring divider, 64 iterations.
- `rtl/audio_pipeline.sv`: state execution followed by job execution.
- `simulate.py`: alias for the Rust/Marlin `audio` regression suite. Shared drivers
  and arithmetic/protocol checks live in `src/rust/rt_fpga_testbench`.
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

Gain uses no combinational divider or division per sample. Generic checks forbid
`$div`, `$mod`, and latches. ECP5 maps the shared multiplier to four 18x18 DSPs.

### Application coefficients and restart smoothing

Constant fills a scratch buffer with one raw word. Arithmetic combines scratch
buffers using the same signed multiply/truncate/saturate rules as gain. Its
balance and one-minus modes build the production backend's coefficients while
preserving the order of each rounding stage. Arithmetic serializes two reads
on the existing scratch port and reuses the registered multiplier.

Smoothing retains last output, anchor, elapsed count and active ramp duration for
each history index (bounded by BUFFERS). The four words live in synchronous RAM;
a validity bit per index makes reset constant-time without resetting the RAM.
Each job loads one history, processes its samples in local registers and commits
its final history. Restart latches the previous output as anchor and the next
duration. A changed duration applies at the next restart; stopped playback clears
its history. Configuration writes preserve history.

The shared divider initializes a Q0.24 weight and quotient/remainder step once
per active smoothing job. Per-sample addition and carry generate exactly
`floor(2^24 * elapsed / duration)` without division. Durations remain arbitrary,
including lengths shorter than Q and lengths that cross quantum boundaries.
Each active sample uses the shared multiplier once to evaluate
`anchor + trunc0((raw - anchor) * weight / 2^24)`, keeping the signed difference
wide and clamping only the final result. At the end of the ramp, samples pass
through exactly. This changes fixed-point rounding from the previous two-product
crossfade; production Rust uses the same operation.

The application compiler uses a stable index per prepared playback channel.
Rust/Marlin tests compare to the production renderer. The independent smoothing
regression covers signed extremes, 0/1/3/31/32/33/144/UINT32_MAX durations,
interleaved histories, stop/restart, changed durations, reset and short quanta:

```sh
cargo run --locked -p rt_fpga_testbench --features sim -- smoothing
```

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
| 3 | Unsupported media job |
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

## Verification

Full musical/state/media behavior is compared with the production float and
Q8.24 runtime via [common sessions](../verification/README.md). Static lowering
uses production signal and timing compilation. There is no software FPGA executor.
Standalone tests preserve hardware-specific protocol, configuration, stream,
cache, capacity, fault and reset checks without a duplicate looping backend.

```sh
cargo run --locked -p rt_fpga_testbench --features sim -- audio --random-cases 150
```

The subsystem's `check.sh` retains lint and synthesis entry points. Current
coverage and local evidence are recorded in [the retirement ledger](../verification/MIGRATION.md)
and [subsystem gates](../verification/subsystem-gates.json). [Historical results](HISTORY.md)
retain the earlier reference-backed measurements; their commands are retired.
