# FPGA state processor

Simulation now uses the complete [Rust/Marlin testbench suite](../../src/rust/rt_fpga_testbench/README.md):

```sh
cargo run --locked -p rt_fpga_testbench --features sim -- state
```

The Python simulator entry names are aliases for this runner and accept its
current options. The native suite guide describes profiles, replay, VCDs and
reports. Historical measurements are marked separately.

A synthesizable SystemVerilog state processor with one multicycle execution
state machine, synchronous program/state RAMs and a finite job FIFO. A compiled
sync-ordered prefix executes loop transitions, wraps, pulses and cursor updates;
the following audio-ordered section emits jobs. Command admission, sample
processing, view storage and host transport remain outside this block.

The TP-free media audio pipeline connects it to sample/job execution.
Its logical audio instructions read the loop's saved render state when a sync
prefix is present, or caller-supplied read-only records for isolated fixtures.
View handles and ramps remain in node-state RAM.

## Modules

- `rtl/state_processor.sv`: top-level configuration, memories, engine, and FIFO.
- `rtl/sp_engine.sv`: the single fetch/decode/execution FSM for sync and audio
  instructions, sharing read stages, operand/cursor registers, descriptor
  register and a 65-bit arithmetic adder.
- `rtl/sp_ram.sv`: synchronous read/write RAM without payload reset.
- `rtl/sp_job_fifo.sv`: synchronous RAM FIFO with a registered output head.
- `rtl/state_processor_defs.svh`: descriptor widths, opcodes, and result codes.
- `rtl/sp_loop_defs.svh`: loop-state and two-plan storage layout.

`PROGRAM_DEPTH`, `STATE_DEPTH`, and `JOB_DEPTH` default to 256 each. Each depth
must be in `1..65536`; non-power-of-two depths are supported. Other width
parameters are derived implementation details and should retain their defaults.
The FIFO count includes the prefetched output head.

## Integrated loop instructions

Opcode `0x0c` is `ProcessLoop`: arg0 is the loop slot and arg1/arg2 are its
generation halves; the remaining bits are zero. `run_instruction_count` counts
the combined sync/audio program. `run_sync_count` is the length of its prefix.
Every prefix instruction processes a loop. Loop instructions are forbidden afterward.
The compiler supplies canonical source-before-dependent order. Per-loop frame
stamps reject duplicate visits, unprocessed sources and stale render state.
Generation and capacity checks precede RAM addressing. Self-sync is handled
locally, with at most one delivered pulse per loop per quantum.

The instruction applies due absolute plans, resolves natural wraps/repeat waits,
propagates source pulses and applies tick-triggered plans. Each loop has two
physical plan slots. Higher sequence numbers win among eligible plans; stop
cancels older plans and suppresses pending pulses. A zero remaining tick count
fires on the next eligible source pulse. Operations are stop/play/record/seek,
matching the Rust reference's primitive-loop profile and outcome semantics.

Before advancing, the instruction saves render position and first-capture
status. Content length is shared with the render view because advancement does
not change it. The audio section consumes that snapshot; next-boundary position/capture
length have already advanced. `sync_frame` advances after the final prefix
instruction, before audio execution. No SP operation runs after JP. Managed
execution failures latch until reset; computed-ahead state is not rolled back.
Reset clears control/frame state but retains RAM; reload loop records before
resuming managed execution. Supported sync quantum sizes are 1/16/32/64/128.

`LOOP_DEPTH` defaults to `STATE_DEPTH`, supports 1..4096 slots and is independent
of the audio state-vector count. Each slot occupies sixteen 64-bit RAM words at
`16 * loop_slot + word_index` (128 bytes). Idle-only configuration selects this
RAM using `cfg_loop=1` (which takes precedence over `cfg_state`); writes use bits
63:0 of the existing configuration bus and reads zero-extend to 129 bits.
Initialize all sixteen words; `sp_loop_defs.svh` defines word addresses and flag
positions. Both program sections access the same loop record.

| Word | Value |
| --- | --- |
| 0 | Valid [0], generation [32:1], mode [34:33], repeat [35], source-valid [36], source slot [52:37], pulse [53], capture-start [54], processed-valid [55], newer plan index [56] |
| 1 | Source generation [31:0], plan 0 flags [36:32], plan 1 flags [41:37] |
| 2 | Next position |
| 3 | Content length (also render length) |
| 4 | Capture frames |
| 5 | Cycle count |
| 6 | Content revision |
| 7 | Revision watermark (also capture revision) |
| 8 | Render position |
| 9 | Processed quantum frame |
| 10 / 13 | Plan 0 / 1 effective frame |
| 11 / 14 | Plan 0 / 1 remaining pulse skips |
| 12 / 15 | Plan 0 / 1 position |

Each five-bit plan flag group contains valid [0], tick-timing [1], operation
[3:2], and retrigger [4]. Reserved bits are zero. All cursors, counters, revisions,
and effective frames retain 64 bits. Capture revision always equals the revision
watermark: recording sets both to the next watermark and other operations leave
both alone. A recording finalized by stop/play copies that watermark into content
revision. There is no separately stored capture revision or render length.

The caller writes prepared plans directly while idle and updates the newer-plan
bit according to the two distinct full sequence numbers; preserve the other bits in word 0. This
rank remains fixed during execution, including after a plan resolves, so it also
preserves priority between absolute and pulse-triggered operations. Word 1 source
identity must be preserved when updating its plan flags. Consume each plan-slot
resolution before replacing its corresponding plan. No full sequence number or
request ID participates in the SP execution datapath.

`sync_event_valid/ready/data` streams 256-bit descriptors. Backpressure stalls
the same SP execution FSM and preserves the descriptor. Fields are kind [7:0],
reason [15:8], outcome [23:16], reserved [31:24], loop slot [63:32], payload
[127:64], operation flags [191:128], and frame [255:192]. Kinds are transition=0,
wrap=1, pulse=2, resolution=3. For transitions, payload is position and flags are
operation bits [1:0] plus retrigger [2]. For resolutions, payload is plan index
0/1, flags are zero, and outcome is applied=0/superseded=1/cancelled=2/failed=3.
Failed reasons use the transition error codes (invalid state=5, invalid frame=6,
exhausted=8, empty content=9). A consumer can update last-pulse history from
actual pulse events. Last pulse is not required to execute the graph and is not stored in loop RAM.

A consumer must accept or buffer events; there is no additional event FIFO in SP. The descriptor register is shared with ordinary job emission. The
Rust functional model retains complete logical state; the testbench's compact
image codec and pulse observer check its physical projection and plan-slot
outcomes directly. The Rust TP and request/completion binding layer are removed.

The existing SP FSM uses word reads/writes and a shared 65-bit adder. It caches
flags, position, length and capture count, while cycle/revision/plan data are
accessed in RAM as needed. Operation-specific change checks avoid comparing or
multiplexing entire loop records. There is no separate sync execution controller.

`audio_pipeline` exposes these ports for combined SP/JP operation. The existing
media/AXI wrappers retain explicit-record mode (`run_sync_count=0`); exporting
the new control ports through those wrappers is future integration work.

Run `cargo test --locked -p rt_fpga_testbench --features sim --test sync` for
1,152 sync quanta compared to Rust (including full-width sequences admitted out
of order), 120 combined SP/JP quanta compared sample for sample, wide counter and
overflow cases, and malformed-prefix/generation/bounds/fault checks.

### Bounded composite spans

A timing-only loop uses the ordinary position and length words. Word 0 bits
60–63 hold the same-quantum snapshot, snapshot restart, explicit program kind
and wrap marker, in that order. Word 4 holds the previous rendered position or
all ones when the previous render was stopped. Word 6 holds the unit in frames.
Word 1 bits 42–45 hold repeat wait, authored capture ownership, composite-child
timing and the composite timing-slot marker.

`ProcessCompositeChild` (opcode `0x14`) uses the usual child slot and generation.
Bits [71:56] name its sync source, with `0xffff` for no source. Bits [87:72]
give the number of following data rows. Each parent starts with a header row
containing its slot [15:0], generation [47:16], span count with low bits
[53:48] and high bits [79:70], and whole-program audio writer count [69:54]
when authored recording is present. The span rows follow that header within
the bounds of the instruction's row count. Each 96-bit span row contains the
inclusive start frame [31:0], exclusive end frame [63:32], and mode [65:64]
(1 Play, 2 Stop, 3 Record). The positions are relative to the parent's pass.
Every row counts toward program and sync prefix lengths. Normal instruction
fetch skips the data rows.

The compiler puts parents in priority order and visits each parent before its
children. The processor reads the preceding and current parent positions to
find due actions. It evaluates the first eligible parent before saving child
render state. Direct plans remain part of the same absolute and pulse passes.
Seek uses the shared adder for a bounded remainder calculation. Ordinary
playback and zero-offset starts bypass division.

The media wrapper provides conservative capture credits from available
sample blocks, table pages and retired-root slots. The compiler counts all
audio capture lanes in a recording program. The state processor samples the
credits at the start of the quantum and admits authored Record when the
compiled count fits. This assumes prepared, distinct writable views and
exclusive job-processor writes during the quantum. A refused start keeps the
old take. An authored writer that runs out of space closes its accepted
prefix. The job processor continues to consume the usual capture and
playback jobs.

See the [RTL reference](../../docs/fpga/rtl-reference/manual.md#composite-audio-loops)
for the supported behaviour and its role in live loading.

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

`busy` covers admission and execution. Configuration cannot modify state or program while busy.
Admission takes one validation cycle after run acceptance. A run refused because
the FIFO is nonempty completes immediately, preserving any stalled output job.
The engine executes:

```text
ADMIT -> FETCH -> DECODE -> (READ_REQUEST -> READ_CAPTURE -> READ_RESULT)* -> EMIT -> NEXT
```

Disabled capture bypasses `EMIT`. Each RAM read takes request, capture and operand
evaluation cycles. Shared-loop responses have a separate capture/validation cycle
before their decisions are applied. Consecutive instructions observe earlier state writes, including
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

## Packed RTL ABI

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
| `0x08` | Silence | Destination | — | — | — | — |
| `0x09` | Constant | Raw value low 16 | Raw value high 16 | Destination | — | — |
| `0x0a` | Arithmetic | Left | Right | Destination | Mode | — |
| `0x0b` | Smoothing | Loop slot | Generation low 16 | Generation high 16 | Control word address | Source |

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
`elapsed = min(elapsed + quantum, duration)`, using a 33-bit elapsed result from
the shared adder. This is equivalent
to Rust's saturating addition followed by clamping, even for extreme counters and
zero duration. The other ramp fields are preserved. No multiplier or divider is
required by the engine.

### Shared-loop instructions and read port

Opcodes `0x06` (logical playback) and `0x07` (logical capture) use the same
96-bit instruction layout: arg0 is loop slot, arg1/arg2 are the low/high halves
of its 32-bit generation, arg3 is the view-state address, and arg4 is destination
or source buffer respectively. The bounded slot/address/buffer fields remain
16 bits. Production-plan lowering emits these
instructions without allocating mode, position or length words in node state.

With `run_sync_count=0`, the `loop_read_valid/ready/slot` port requests one record. A transferred request
gets one `loop_response_valid` pulse with `loop_response_error` and 164-bit data:
valid at bit 0, generation u32 at 1, mode at 33, length u64 at 35 and position u64
at 99, and capture-start at 163 (`capture_frames == 0`). The RAM owner admits
these requests only in its settled phase. The
engine checks presence and generation, then reads the view operand when needed.
Playback uses the same `plays()` predicate as Rust, including silence at a held
repeat endpoint. Capture emits only in recording mode. On a new take it first emits BeginCapture,
then Capture; include the extra preparation job in the static queue budget.
In explicit-record mode SP does not write this port or advance the loop clock. A standalone shared-memory client
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
| `0x08` | Constant | Destination | 0 | Raw Q8.24 value | 0 | 0 |
| `0x09` | Arithmetic | Left | Right | Destination + mode | 0 | 0 |
| `0x0a` | Smoothing | Source | Destination | History index + frames | Playing/restart flags | 0 |

Arithmetic payload 0 has destination in bits 15:0 and mode in bits 31:16.
Modes are multiply (0), left balance (1), right balance (2), and one-minus (3).
Balance multiplies the left operand by `1-max(right,0)` or `1+min(right,0)`.
One-minus is for control coefficients in [0,1]; it ignores the right value but
still requires a valid ready source ID. Materializing a bank's coefficient before
subtracting avoids changing interpolation rounding by complementing ramp endpoints.

Smoothing's control `Word` packs destination u16, history index u16 and restart
length u32. It reads the settled loop identity without mutating it. In the current
chunk-aligned application profile, starts/wraps occur at position zero; this is
how that instruction derives restart. The emitted job has history index u32 and
frames u32 in payload 0, playing in payload 1 bit 0 and restart in bit 1. General
nonzero-position seeks require exposing the TP discontinuity bit to this producer.

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

Historical default configuration results before loop instructions, using Yosys
0.52 ECP5 synthesis, were:

| Cell | Count |
| --- | --- |
| `DP16KD` block RAM | 15 |
| `LUT4` | 1,208 |
| `TRELLIS_FF` | 1,417 |
| `CCU2C` carry | 136 |
| `MULT18X18D` DSP | 0 |

Lint used Verilator 5.032. Resource counts are synthesis estimates; no board
pinout, placement, routing, timing closure, or clock-frequency claim is included.
The current matched before/after 7-series synthesis comparison is recorded in
[the SP resource report](../../docs/fpga/sp-sync-resource-comparison.md).

## Verification

The [Rust reference](../../src/rust/rt_fpga_reference/README.md) supplies functional
loop/state/job expectations. Sync traces are checked against the native resolver;
Marlin tests compare RTL loop state, jobs, events, outcomes and exact samples.
Standalone tests retain hardware protocol, configuration, stream, capacity,
fault and reset checks.

```sh
cargo run --locked -p rt_fpga_testbench --features sim -- state
```

The subsystem's `check.sh` retains lint and synthesis entry points. Current
coverage and local evidence are recorded in [the retirement ledger](../verification/MIGRATION.md)
and [subsystem gates](../verification/subsystem-gates.json). [Historical results](HISTORY.md)
retain the earlier reference-backed measurements; their commands are retired.
