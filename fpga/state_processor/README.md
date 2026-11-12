# FPGA state processor

A synthesizable SystemVerilog prototype of `rt_fpga_reference::state::StateProcessor`.
It supports the six raw-state node instructions plus shared-loop playback/capture,
a multicycle single-issue engine,
synchronous program/state RAMs, and a finite job FIFO. It implements state execution
only. Transitions, logical clocks, sample processing, view storage, and the host
transport are outside this block.

`fpga/loop_pipeline` connects it to transition processing and sample/job execution.
Its logical instructions read the settled loop RAM directly while view handles
and ramps remain in node-state RAM. The raw-state instruction ABI remains usable
for isolated fixtures.

Status: lint, synthesis, and behavioral comparison against the actual Rust state
executor pass for the configurations and cases documented below. These checks
do not constitute formal equivalence or a clock/deadline guarantee.

## Modules

- `rtl/state_processor.sv`: top-level configuration, memories, engine, and FIFO.
- `rtl/sp_engine.sv`: fetch/decode, operand reads, job generation, and ramp updates.
- `rtl/sp_ram.sv`: synchronous read/write RAM without payload reset.
- `rtl/sp_job_fifo.sv`: synchronous RAM FIFO with a registered output head.
- `rtl/state_processor_defs.svh`: descriptor widths, opcodes, and result codes.

`PROGRAM_DEPTH`, `STATE_DEPTH`, and `JOB_DEPTH` default to 256 each. Each depth
must be in `1..65536`; non-power-of-two depths are supported. Other width
parameters are derived implementation details and should retain their defaults.
The FIFO count includes the prefetched output head.

## Clock and control protocol

All transfers occur on the rising edge of `clk`. `reset` is synchronous and active
high. Reset aborts execution and empties the FIFO; program and state RAM contents
are neither cleared nor rolled back. Load the necessary entries before execution.

The configuration port is an idle-only request interface:

- A request transfers when `cfg_valid && cfg_ready`.
- `cfg_state=0` selects program memory; `cfg_state=1` selects state memory.
- `cfg_write=1` writes `cfg_write_data`; otherwise it reads.
- `cfg_address` is a 16-bit entry index. Out-of-capacity accesses produce an error
  response without accessing RAM.
- `cfg_response_valid` is asserted during the cycle immediately after acceptance,
  for both reads and writes. There is no response backpressure. Observe
  `cfg_response_error` and, for successful reads, `cfg_read_data` in that cycle.
- Writes and failed reads return zero data. Program reads are zero-extended to
  129 bits; program writes use only the low 96 bits.
- One request may be outstanding. Configuration has priority over a simultaneous
  run request. Both configuration and run acceptance are blocked while a
  configuration response is pending.

A quantum starts when `run_valid && run_ready`. This latches
`run_instruction_count`, `run_state_count`, `run_quantum`, and `run_chunk_frames` and checks
`run_job_budget` against the available FIFO capacity. These inputs are unsigned
32-bit values. `run_state_count` is the logical length of the state vector, which
may be smaller than the physical RAM. Only the first `run_instruction_count`
program entries execute. Counts exceeding physical capacities are rejected.

Admission first requires an empty FIFO and at least `run_job_budget` free entries,
matching Rust queue-capacity admission. Use the compiled `Program::jobs_per_quantum` as the
budget. A budget understated by a hand-written program can also cause a queue-full
error during execution. Such a failure retains earlier jobs and state writes.
The quantum must be positive and an integral number of chunks. The RTL chunk
size is a positive power of two. Media position/length words are chunk indices
and counts. Arithmetic ramp counters retain samples. Shared loop records still
carry frames; logical playback converts them at the input boundary.

`busy` covers execution. Configuration cannot modify state or program while busy.
The engine executes:

```text
FETCH -> DECODE -> (READ_REQUEST -> READ_RESULT)* -> EMIT -> NEXT
```

Disabled capture bypasses `EMIT`. Each RAM read takes one request cycle and one
result cycle. Consecutive instructions observe earlier state writes, including
aliased state addresses. Gain writes occur on the same edge that queues the job.

`done` is a one-cycle completion pulse, including on error and for empty programs.
`error_code` remains latched until the next accepted run or reset. `instruction_pc`
identifies the current/failing instruction and equals the instruction count after
successful execution. On an admission rejection it is zero.

Jobs transfer when `job_valid && job_ready`. The descriptor remains stable while
valid and stalled. FIFO draining is blocked while busy, so the external consumer
sees only a completed or failed run's job prefix. After completion, drain the
FIFO before starting the next run. `queued_jobs` reports its current occupancy.
`done` means job generation and state updates have completed; it does not mean
the job processor has executed the jobs. FIFO head prefetch can add latency before
the first job becomes valid and between accepted jobs.

Each accepted read-job enqueue also pulses `read_intent_valid` with
`read_intent_data`. This advisory output is independent of normal FIFO draining
and cannot stall state execution. The combined pipeline buffers these intents
and resolves them while state execution continues.

## Packed ABI (format-version-3 bench)

Bit positions below describe packed integers, independent of a host transport's
byte order. A byte-oriented transport should encode the least significant byte
first. All fields are unsigned except the two signed fixed-point ramp endpoints.

This bounded ABI supports 16-bit instruction operands: state addresses, buffer
IDs, and port indices must fit `0..65535`. An encoder must reject larger Rust
`usize` values rather than truncate them. State words, positions, lengths, and
view IDs retain all 64 bits. Comparison with Rust assumes a 64-bit `usize` host.
The state processor passes view IDs through without consulting storage.

### Instructions: 96 bits

| Bits | Field |
| --- | --- |
| 7:0 | Opcode |
| 23:8 | Argument 0 |
| 39:24 | Argument 1 |
| 55:40 | Argument 2 |
| 71:56 | Argument 3 |
| 87:72 | Argument 4 |
| 95:88 | Reserved |

| Opcode | Instruction | Arg 0 | Arg 1 | Arg 2 | Arg 3 | Arg 4 |
| --- | --- | --- | --- | --- | --- | --- |
| `0x00` | Input | Port | Destination | — | — | — |
| `0x01` | Playback | View address | Position address | Length address | Enabled address | Destination |
| `0x02` | Gain | Ramp address | Source | Destination | — | — |
| `0x03` | Mix | Left | Right | Destination | — | — |
| `0x04` | Capture | View address | Enabled address | Source | — | — |
| `0x05` | Output | Source | Port | — | — | — |

Reserved and unused instruction fields are ignored; encoders should set them to
zero. Unknown opcodes cause `INVALID_PROGRAM`.

### State values: 129 bits

Bit 128 is the type tag: zero for `Word`, one for `Ramp`.

| Type | Bits | Field |
| --- | --- | --- |
| Word | 63:0 | Unsigned 64-bit value |
| Word | 127:64 | Ignored; encode as zero |
| Ramp | 31:0 | Signed Q8.24 start |
| Ramp | 63:32 | Signed Q8.24 target |
| Ramp | 95:64 | Unsigned duration |
| Ramp | 127:96 | Unsigned elapsed |

Enabled operands accept any nonzero 64-bit word. Disabled playback reads only
its enabled operand and emits silence. Disabled capture reads only its enabled
operand and emits nothing. Unused addresses and types are not checked.

Enabled playback reads position, then length, checks `length != 0` and
`position < length`, then reads the view. Capture reads the view after enabled.
Gain requires a ramp. Every actual read checks the logical state-vector bound and
the expected type. There is no position update, wrapping arithmetic, capture
finalization, sample-memory validation, or record/play mutual-exclusion check.

Gain emits the original ramp and updates state to
`elapsed = min(elapsed + quantum, duration)`, with a 33-bit sum. This is equivalent
to Rust's saturating addition followed by clamping, even for extreme counters and
zero duration. The other ramp fields are preserved. No multiplier or divider is
required by the engine.

### Shared-loop instructions and read port

Opcodes `0x06` (logical playback) and `0x07` (logical capture) use the same
96-bit instruction layout: arg0 is loop slot, arg1/arg2 are the low/high halves
of its 32-bit generation, arg3 is the view-state address, and arg4 is destination
or source buffer respectively. The bounded slot/address/buffer fields remain
16 bits. Rust's `LogicalPlayback` / `LogicalCapture` compiler nodes emit these
instructions without allocating mode, position or length words in node state.

The `loop_read_valid/ready/slot` port requests one record. A transferred request
gets one `loop_response_valid` pulse with `loop_response_error` and 164-bit data:
valid at bit 0, generation u32 at 1, mode at 33, length u64 at 35 and position u64
at 99, and capture-start at 163 (`capture_frames == 0`). The RAM owner admits
these requests only in its settled phase. The
engine checks presence and generation, then reads the view operand when needed.
Playback uses the same `plays()` predicate as Rust, including silence at a held
repeat endpoint. Capture emits only in recording mode. On a new take it first emits BeginCapture,
then Capture; include the extra preparation job in the static queue budget. The state processor never
writes this port or advances the loop clock. A standalone shared-memory client
must supply responses; backpressure pauses the engine.

### Jobs: 256 bits

| Bits | Field |
| --- | --- |
| 7:0 | Opcode |
| 23:8 | ID 0 |
| 39:24 | ID 1 |
| 63:40 | Reserved, zero |
| 127:64 | Payload 0 |
| 191:128 | Payload 1 |
| 255:192 | Payload 2 |

| Opcode | Job | ID 0 | ID 1 | Payload 0 | Payload 1 | Payload 2 |
| --- | --- | --- | --- | --- | --- | --- |
| `0x00` | Input | Port | Destination | 0 | 0 | 0 |
| `0x01` | Silence | Destination | 0 | 0 | 0 | 0 |
| `0x02` | ReadView | Destination | 0 | View | Position | Length |
| `0x03` | Gain | Source | Destination | Start/target | Duration/elapsed | 0 |
| `0x04` | Mix | Left | Right | Destination, zero-extended | 0 | 0 |
| `0x05` | Capture | Source | 0 | View | 0 | 0 |
| `0x06` | Output | Source | Port | 0 | 0 | 0 |
| `0x07` | BeginCapture | 0 | 0 | View | 0 | 0 |

Gain's payload 0 is `{target[31:0], start[31:0]}` and payload 1 is
`{elapsed[31:0], duration[31:0]}`. Every unused job field is zero.

### Results

| Code | Meaning | Rust correspondence |
| --- | --- | --- |
| `0x00` | OK | `Ok(())` |
| `0x01` | QUEUE_FULL | `Error::QueueFull` |
| `0x02` | INVALID_STATE | `Error::InvalidState` |
| `0x03` | INVALID_PROGRAM | Hardware configuration/encoding error |

Hardware additionally rejects counts exceeding physical RAM capacities. Unknown
opcodes have no counterpart in the typed Rust enum. On a 64-bit Rust host,
converting a word to a view ID cannot overflow, so this block has no separate
`InvalidView` result. Storage and buffer validity checks belong to job execution.
Errors halt the run without rolling back its previously emitted jobs or state
writes. A gain whose job fails to queue does not update its ramp.

## Static checks and synthesis

Install Yosys (including ABC and ECP5 technology files) and Verilator, or place
the OSS CAD Suite tools on `PATH`, then run from any working directory:

```sh
/path/to/shoopdaloop/fpga/state_processor/check.sh
```

The optional first argument selects the output directory; the default is
`work/fpga-state-processor` under the repository. The script runs warning-fatal
Verilator lint, generic Yosys checks with memories retained and latches forbidden,
and ECP5 synthesis with post-mapping structural checks. It writes logs, tool
versions, and generic/mapped JSON netlists. It checks three depth configurations:
`256/256/256`, `7/11/5`, and `1/1/1` for program/state/jobs respectively.
It does not run a behavioral simulator or a testbench.

The default configuration's initial Yosys 0.52 ECP5 synthesis reports:

| Cell | Count |
| --- | --- |
| `DP16KD` block RAM | 15 |
| `LUT4` | 1,208 |
| `TRELLIS_FF` | 1,417 |
| `CCU2C` carry | 136 |
| `MULT18X18D` DSP | 0 |

Lint used Verilator 5.032. Resource counts are synthesis estimates; no board
pinout, placement, routing, timing closure, or clock-frequency claim is included.

## Behavioral comparison

The cocotb/Verilator bench uses the versioned JSON interface in
`scripts/fpga_testbench`. Expected results come from the actual Rust executable's
`trace` command. Python only creates stimulus, packs/unpacks the RTL ABI, drives
handshakes, and compares results; it does not implement a second state processor.

Run Cargo in the repository's development environment. With Verilator, Make, a
C++ compiler, and Python 3.12 available:

```sh
python3 -m venv work/fpga-sim-venv
. work/fpga-sim-venv/bin/activate
python -m pip install -r fpga/state_processor/requirements.txt
python fpga/state_processor/simulate.py
```

The runner builds `rt_fpga_testbench`, compiles three Verilator models, and runs
four cocotb tests per configuration. It exits nonzero for simulator failures,
missing or skipped tests, architectural mismatches, and protocol violations.
Set `SHOOP_FPGA_REFERENCE` to use an explicitly built reference executable instead.

For each case, the driver resets the block, loads and reads back the program and
initial state through the configuration port, then applies each step's settled
writes without resetting between quanta. It waits for completion, drains accepted
jobs with bounded randomized backpressure, and reads back every logical state
entry. The shared comparator checks exact job order, full state, and result,
including retained jobs and state writes after an execution failure. It also
checks for extra jobs after draining and stability of stalled descriptors.

Queue capacity is the actual elaborated `JOB_DEPTH`; the runner does not silently
substitute a larger FIFO. Out-of-vector settled writes are rejected by the host
driver before starting execution, matching the Rust trace orchestration. This
tests the driver boundary; in-program address/type checks execute in RTL.
Operands exceeding the bounded 16-bit ABI are rejected, not truncated. View IDs
emitted in JSON must fit that interface's 32-bit ID representation; state words,
positions, and lengths retain their full 64-bit representation.

Directed tests cover every instruction and job variant, both disabled paths,
high-bit nonzero enable values, extreme ramp endpoints/counters, zero and maximum
quantum sizes, aliased ramps, simultaneous playback/capture, logical and physical
address bounds, type/range errors, partial failure, FIFO admission and exhaustion,
full FIFO, pointer wrap across repeated quanta, the last program/state entries,
and a program generated by the Rust graph compiler. Cases exceeding a physical
configuration's program/state capacity are counted separately in the summary.

Two additional protocol tests per configuration cover configuration arbitration
and response timing, blocked writes/restarts while busy, invalid counts/opcodes,
nonempty-FIFO refusal, and reset both during execution and with undrained jobs.
Reset checks include retention of state/program RAM and a subsequent Rust-matched
run from the retained state. These hardware-specific checks have explicit
protocol expectations rather than invented Rust enum equivalents.

The initial run used Verilator 5.032, cocotb 1.9.2, and seed `20261110`:

| Program/state/job depth | Directed cases | Seeded cases | Compared quanta |
| --- | --- | --- | --- |
| 256/256/256 | 69 | 200 | 693 |
| 7/11/5 | 70 | 200 | 682 |
| 1/1/1 | 40 | 200 | 605 |
| Total | 179 | 600 | 1,980 |

All 779 cases and all six additional protocol/reset test groups passed. No RTL
correction was required. This is finite behavioral coverage, not an exhaustive
proof, gate-level simulation, or timing closure.

### Execution latency

The driver measures clock periods from the rising edge accepting a run to the
rising edge asserting `done`. Directed summaries record counts per case/quantum;
replays write `replay-cycles.json`. `null` means a settled write failed before a
run was submitted. Immediate admission errors and empty programs complete at the
accepting edge and therefore record zero elapsed periods.

The compiler-generated test uses six nodes at Q=32:

```text
Input 3 -> Gain (0.5) -> Mix -> Output 11
                         ^  |
Playback (view 17) -------+  +-> Capture (view 17)
```

Playback and capture are both enabled. Its persistent program executes Input,
Playback, Gain, Mix, Capture, and Output, taking `4 + 12 + 6 + 4 + 8 + 4 = 38`
periods. All three tested quanta measured 38. At an assumed 150 MHz, this is
`38 / 150,000,000 = 0.2533 microseconds` per state execution.

The supplied record/play example instead branches the gain output to capture
and to the mix, with playback supplying the other mix input. Its five measured
executions took `32, 32, 35, 35, 29` periods:

| Mode | Periods | Time at assumed 150 MHz |
| --- | --- | --- |
| Capture enabled, playback disabled | 32 | 0.2133 microseconds |
| Capture disabled, playback enabled | 35 | 0.2333 microseconds |
| Both disabled | 29 | 0.1933 microseconds |

The measurement ends with jobs queued. It excludes initial program loading,
settled-state writes, FIFO draining, transition resolution, and execution of
sample jobs or external-memory accesses. The frequency is an estimate input,
not a placement/routing result.

### Reproduction and diagnostics

Logs, xUnit results, and JSON coverage summaries go to `work/fpga-state-sim` by
default. `--output` changes that location. `--seed` and `--random-cases` select
the deterministic stimulus, and `--config default|small|minimum` selects one
configuration. `--depths PROGRAM STATE JOBS` overrides the presets.

On a case failure, the bench saves `failure.case.json`,
`failure.expected.trace.json`, and `failure.observed.json` under the configuration's
output directory. Replay one case or trace with its matching physical capacity:

```sh
python fpga/state_processor/simulate.py --config small \
  --case work/fpga-state-sim/small/failure.case.json --waves
```

The original upstream example has a six-entry FIFO, so use explicit depths:

```sh
python fpga/state_processor/simulate.py --depths 32 32 6 \
  --case scripts/fpga_testbench/examples/record_play.json \
  --waves --output work/fpga-state-example
```

`--case` runs only the replay comparison. Expected results are regenerated through
Rust even when the input is a saved trace. `--waves` records a VCD alongside the
simulation log; it is optional for the normal regression. Transition and sample
processing remain outside this bench.
