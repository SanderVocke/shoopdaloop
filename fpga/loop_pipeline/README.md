# Shared-loop audio pipeline

`loop_pipeline` connects transition resolution, node state execution, sample jobs
and persistent media. Playback/capture instructions read the transition processor's
committed loop RAM directly. No loop decisions are copied into state memory.

```text
Host commands and observations
             |
     Transition processor
       candidate loop bank
             |
     preflight / retry
             |
           commit
             |
     committed loop bank ------ read port -----> State processor
       mode, position, length                    program + node state
             |                                         |
             |                                     job FIFO
             |                                         |
             |                                   Job processor
             |                                         |
             |                          scratch BRAM + media store
             |                                         |
             |                              external sample RAM port
             |                                         |
             +--------- finish after successful samples +
```

## Ownership and publication

The transition processor alone writes logical mode, position, length, revisions
and timing metadata. It preserves its two banks and commits with the existing
bank selector. During state/job execution, that bank stays fixed. The state
processor requests loop records by slot and checks the full generation before
using them. A stopped or repeat-waiting loop emits Silence from playback, while
capture emits a job only in recording mode.

The shared read port multiplexes the existing loop RAM's synchronous read port.
It is available only between transition commands in the committed phase. A read
returns valid, generation, mode, content length and position in one response.
There is no additional decision RAM, per-loop copy pass or shared writer.

Media view handles stay in node state. Gains keep their own ramp state. Signal
nodes, logical loop IDs and media views retain independent identities, allowing
multiple signal nodes to read the same loop and future timing-only composites.
Composite execution itself remains unsupported.

## Rust flow

`rt_fpga_reference::pipeline::Pipeline` owns the transition processor, persistent
program, node state and FIFO. `preview` permits resource preparation and retries.
`run_state` commits the chosen token and passes a borrowed `CommittedLoops` view
directly to state execution. `run_jobs` executes the actual Rust job processor
and advances logical time only after success. `run` combines those phases for a
quantum without an intervening preparation callback.

An external executor can consume descriptors with `pop_job`, then acknowledge
successful completion with `finish_jobs`. An undrained queue, stale token or
duplicate completion cannot advance time. State/job errors retain partial
effects, freeze the committed clock and fault the coordinator. Recreate/reset
the pipeline after such a failure.

The compiler's `LogicalPlayback` and `LogicalCapture` nodes generate
`ProcessLogicalPlayback` and `ProcessLogicalCapture` instructions. Their operands
hold a loop ID, view-state address and buffer ID. The isolated fixture profile's
raw-state playback/capture instructions remain supported. The connected path
does not use `StateBindings::export`; that copying API has been removed.

## RTL control

Defaults are 8 loop slots, 16 request slots, 32 events per journal bank, 64 program
entries, 32 node-state entries and 64 FIFO jobs. Sample defaults are Q <= 32,
16 scratch buffers and two ports in each direction. All logic shares one clock
and synchronous active-high reset. RAM payloads survive reset and require a
complete configuration reload. Media defaults are 32 views, 32 physical slots
and 32 frames per chunk. These are deliberately small validation capacities;
sample payloads live behind the external memory port, while descriptors, chunk
tables, reference counts and the free stack use on-chip RAM.

The transition command/response interface retains the standalone packed format.
The coordinator owns Preview, Commit and Finish (opcodes 9, 10, 11). Those host
commands remain unaccepted. Other commands transfer while idle, paused for
preflight, or faulted, subject to the transition processor's own phase guards.
No command can change the loop bank while state/jobs execute.

1. Load the timing plan, signal program, node state and input samples through
   their existing configuration interfaces.
2. Load the observation list. `run_valid && run_ready` latches the entire quantum
   descriptor and starts Preview. Quantum must equal the prepared timing plan.
3. `preflight_error` and `boundary_token` retain the preview result. Inspect
   decisions with Read decision and that token. Use host media commands and node
   state writes here to create/clear views and install their handles.
4. To retry, clear/replace observations, then transfer
   `preflight_valid && preflight_ready` with `preflight_retry=1`. Otherwise the
   same handshake commits and starts state/job execution. A rejected preview
   returns an error without committing or advancing time. Its frozen transition
   inputs remain available for another preview.
5. On success, the coordinator issues Finish and pulses `done`. On a state/job
   failure, it pulses `done` without Finish and blocks further runs until reset.

`preflight_valid` means the preview is available for acknowledgement. It drops
temporarily while transition responses, configuration or host media work are
outstanding; the token/result remain stable. Commit/retry transfers only with
`preflight_valid && preflight_ready`. This prevents a held acknowledgement from
committing before preparation finishes. State execution also waits for pending
configuration responses. Program/node-state, sample and host media access are
blocked during execution. The entire run descriptor, including `arena_base`, is
latched at acceptance; later input changes cannot redirect sample traffic.
`error_stage` is 0 success, 1 transition, 2 state or 3 jobs. Error codes within a
stage retain the underlying block's definitions. Committed state, journals, node
state and remaining FIFO descriptors can be inspected after a fault.

## Sample execution and memory interface

With `run_external_jobs=0`, the internal job processor executes Input, Silence,
Gain, Mix, Output, ReadView and Capture. `media_audio_pipeline` connects the last
two to `media_store`: recording appends actual input samples; playback reads
stored samples into scratch BRAM, including wrapped and retained-range reads.
Finish waits for successful completion of all sample work and memory responses.

`store_cmd_*`, `store_cfg_*` and host sample streams are available while idle,
paused for preflight or faulted, subject to the store's own readiness/fault guards.
Host commands support Create, Slice, Clear, Remove, Reclaim, Append and Read.
They share the store with jobs and cannot interleave with job execution. A fresh
recording needs a prepared empty view (or a cleared reusable view); resetting
logical recording length does not clear physical media. Playback and capture
can use separate view-state words so a refused retake preserves the old playback
handle. There is no sealed/unsealed view state.

The external port transfers one 32-bit sample per request at a 64-bit byte
address, with at most one request outstanding. Reads and writes both require a
response; request/response stalls are supported. `MEMORY_RESPONSE_CYCLES` defaults
to one and can impose additional response delay for simulation. The memory
adapter must flush outstanding traffic on reset. A physical bus fault reports
job code 8, freezes logical time and requires reset; partial sample effects can
survive, including provisional metadata allocations. Codes 5/6/7 report
InvalidView/MemoryFull/InvalidState.

The packed store interfaces and snapshot layout are specified in
[`media_store/README.md`](../media_store/README.md). Physical DDR controllers,
bursts/caching, large-session metadata and board audio/host transport remain
outside this milestone.

With `run_external_jobs=1`, `external_job_valid` and `monitor_data` expose the
ordered FIFO, accepted with `external_job_ready`. After all descriptors drain,
`external_done_ready` allows `external_done_valid` to report completion.
`external_error=0` means success. Nonzero external codes use the Rust `Error`
order: invalid_graph=1, invalid_state=2, invalid_buffer=3, invalid_view=4,
invalid_shape=5, queue_full=6, memory_full=7.

Draining descriptors alone cannot advance logical clocks. The separate control
regression deliberately stalls external descriptors/completion and can report
media failures; sample equivalence uses the internal RTL executor and store.

## Behavioral comparison

The additive `pipeline_case` / `pipeline_trace` JSON variants combine a transition
plan with a persistent program, node state and quantum steps. Each step contains
the existing transition actions/previews, node-state writes and either `external`
completion status or `audio` input samples. Results retain complete transition
decisions, requests/events/clocks, emitted jobs, node state and errors. Audio
steps also retain exact scratch/output samples and FIFO effects. With an initial
`media` snapshot they retain complete media snapshots after every quantum.

```sh
python fpga/loop_pipeline/simulate.py
python fpga/loop_pipeline/simulate.py --config default --random-cases 30
python fpga/loop_pipeline/simulate.py --media --random-cases 30
python fpga/loop_pipeline/simulate.py --media --response-cycles 7 --random-cases 10
python fpga/loop_pipeline/simulate.py --media --config default \
  --case fpga/loop_pipeline/examples/record_play_samples.json
python fpga/loop_pipeline/simulate.py --config default \
  --case fpga/loop_pipeline/examples/record_play.json
python fpga/loop_pipeline/simulate.py --config default \
  --case work/fpga-loop-pipeline/default/failure.case.json --waves
bash fpga/loop_pipeline/check.sh
```

Use the repository's Cargo environment, Verilator and the existing cocotb Python
environment. The simulator runs default, small and non-power-of-two odd configurations.
Expected results come from the actual Rust pipeline. The tests use reverse signal
order relative to loop IDs, one-quantum recordings, delayed sync, legal cycles,
repeat waiting, retries/refusals, retained completions, sparse/stale IDs and full
32-bit generations. They also check run-metadata latching, blocked commands,
descriptor backpressure, delayed/failed completion, partial failures and reset.
No backend arithmetic or transition settlement is reimplemented in Python.

The media comparison additionally checks exact recorded/replayed samples, gain
and mix arithmetic, retakes, refused recordings, retained read-only ranges and
complete media ownership/free-list snapshots. It uses a stalled sample-memory
slave initialized with nonzero garbage, detecting missing zero-fill as well as
wrong published samples. Separate protocol checks cover preparing views during
preflight, eager acknowledgement during a blocked host read, blocked host traffic
during jobs, bus faults, and reset with an outstanding sample transaction.

Failure artifacts contain replayable cases, expected traces and observed results.
Lint/generic synthesis checks include minimum depths; ECP5 mapping is an estimate
without placement/routing or a clock-frequency guarantee.

## Verification checkpoint

Verilator 5.032 and cocotb 1.9.2 passed these comparisons against Rust across
default, small and odd configurations:

| Campaign | Cases | Quantum boundaries |
| --- | ---: | ---: |
| Actual media, nominal response delay, 30 seeded sessions per configuration | 118 | 1,682 |
| Actual media, seven-cycle response delay, 10 seeded sessions per configuration | 58 | 629 |
| External executor and BRAM control regression, 30 seeded sessions per configuration | 179 | 7,719 |

The sample campaigns made 19,938 reads and 18,292 writes through the actual RTL
memory port, excluding host-preparation/fault protocol tests. Every configured
loop slot records and plays in a directed case (eight loops with Q=32 in the
default configuration). All preflight, bus-fault, clock-hold, blocked-host and
pending-memory reset/recovery checks passed in both memory-delay campaigns.
The checked-in sample fixture also replays with seven-cycle responses.

The Rust reference/testbench suite passed 65 tests, and the Python cross-language
contract passed 17 tests. These checks use the existing Rust pipeline and store;
the integration required no changes to their sample semantics.

Yosys 0.52 mapped the default connected top to ECP5 using 17,713 LUT4,
11,186 flip-flops, 2,781 CCU2C carry cells, 83 DP16KD block RAMs,
68 distributed RAM cells and four MULT18X18D DSPs. These capacities are
8/16/32 for loop/request/event tables, 64/32/64 for program/state/job storage,
and 32/32/32 for media views/slots/chunk frames.
The wide rows favor a simple first implementation and incur RAM granularity
costs. These are complete connected-top counts, not a sum of earlier standalone
figures. Lint and generic synthesis also passed 3/5/11 and 1/1/3 loop-table
configurations. No placement, routed timing or board transport is included.
