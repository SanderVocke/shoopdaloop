# Shared-loop processing pipeline

`loop_pipeline` connects transition resolution, node state execution and job
execution. Playback/capture instructions read the transition processor's committed
loop RAM directly. No loop decisions are copied into state memory.

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
             |                          BRAM job processor OR external executor
             |                                         |
             +--------- finish after successful jobs ---+
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
complete configuration reload.

The transition command/response interface retains the standalone packed format.
The coordinator owns Preview, Commit and Finish (opcodes 9, 10, 11). Those host
commands remain unaccepted. Other commands transfer while idle, paused for
preflight, or faulted, subject to the transition processor's own phase guards.
No command can change the loop bank while state/jobs execute.

1. Load the timing plan, signal program, node state and input samples through
   their existing configuration interfaces.
2. Load the observation list. `run_valid && run_ready` latches the entire quantum
   descriptor and starts Preview. Quantum must equal the prepared timing plan.
3. `preflight_valid` holds the preview result, `preflight_error` and
   `boundary_token`. Inspect decisions with Read decision and that token. State
   configuration is available here for physical view preparation.
4. To retry, clear/replace observations, then transfer
   `preflight_valid && preflight_ready` with `preflight_retry=1`. Otherwise the
   same handshake commits and starts state/job execution. A rejected preview
   returns an error without committing or advancing time. Its frozen transition
   inputs remain available for another preview.
5. On success, the coordinator issues Finish and pulses `done`. On a state/job
   failure, it pulses `done` without Finish and blocks further runs until reset.

Preflight acknowledgement waits for outstanding transition responses and active
configuration requests. State execution also waits for pending configuration
responses. Program/node-state and sample writes are blocked during
execution. Metadata changes after run acceptance have no effect on that run.
`error_stage` is 0 success, 1 transition, 2 state or 3 jobs. Error codes within a
stage retain the underlying block's definitions. Committed state, journals, node
state and remaining FIFO descriptors can be inspected after a fault.

## Sample execution and deferred media hardware

With `run_external_jobs=0`, the connected BRAM job processor executes the existing
Input/Silence/Gain/Mix/Output subset. ReadView and Capture still return unsupported.
This connects all three actual RTL processors for supported programs.

With `run_external_jobs=1`, `external_job_valid` and `monitor_data` expose the
ordered FIFO, accepted with `external_job_ready`. After all descriptors drain,
`external_done_ready` allows `external_done_valid` to report completion.
`external_error=0` means success. Nonzero external codes use the Rust `Error`
order: invalid_graph=1, invalid_state=2, invalid_buffer=3, invalid_view=4,
invalid_shape=5, queue_full=6, memory_full=7.

Draining descriptors alone cannot advance logical clocks. The simulation's
external executor deliberately stalls both descriptors and completion and can
report media failures. It acknowledges work without performing media sample
access. Real external-memory playback/capture hardware remains future work.
The Rust pipeline tests separately execute actual recording and playback samples.

## Behavioral comparison

The additive `pipeline_case` / `pipeline_trace` JSON variants combine a transition
plan with a persistent program, node state and quantum steps. Each step contains
the existing transition actions/previews, node-state writes and either `external`
completion status or `audio` input samples. Results retain complete transition
decisions, requests/events/clocks, emitted jobs, node state and errors. Audio
steps also retain exact scratch/output samples and FIFO effects.

```sh
python fpga/loop_pipeline/simulate.py
python fpga/loop_pipeline/simulate.py --config default --random-cases 30
python fpga/loop_pipeline/simulate.py --config default \
  --case fpga/loop_pipeline/examples/record_play.json
python fpga/loop_pipeline/simulate.py --config default \
  --case work/fpga-loop-pipeline/default/failure.case.json --waves
bash fpga/loop_pipeline/check.sh
```

Use the repository's Cargo environment, Verilator and the existing cocotb Python
environment. The simulator runs default and non-power-of-two small configurations.
Expected results come from the actual Rust pipeline. The tests use reverse signal
order relative to loop IDs, one-quantum recordings, delayed sync, legal cycles,
repeat waiting, retries/refusals, retained completions, sparse/stale IDs and full
32-bit generations. They also check run-metadata latching, blocked commands,
descriptor backpressure, delayed/failed completion, partial failures and reset.
No backend arithmetic or transition settlement is reimplemented in Python.

Failure artifacts contain replayable cases, expected traces and observed results.
Lint/generic synthesis checks include minimum depths; ECP5 mapping is an estimate
without placement/routing or a clock-frequency guarantee.

## Verification checkpoint

With Verilator 5.032 and cocotb 1.9.2, default and small configurations passed
119 cases and 5,144 quantum boundaries: 59 directed cases / 344 boundaries plus
60 seeded sessions / 4,800 boundaries. Both coordinator protocol/reset tests
passed. After integrating read-only media ranges, the Rust reference/testbench
suite passed 63 tests, five elastic-runtime tests passed, and the Python
cross-language contract passed 16 tests. A fresh connected comparison passed
69 cases / 1,144 boundaries across both configurations. The combined Rust trace
also verifies playback of an unaligned retained slice after clearing its source.

The standalone state and BRAM audio campaigns also passed after the interface
changes. Shared-loop comparison covers descriptor execution for media operations
and actual BRAM samples for supported jobs. Actual recording/playback samples
run through the connected Rust processors in the integration tests.

Yosys 0.52 mapped the default connected top to ECP5 using 15,657 LUT4,
10,102 flip-flops, 1,939 CCU2C carry cells, 74 DP16KD block RAMs,
68 distributed RAM cells and four MULT18X18D DSPs. These capacities are
8/16/32 for loop/request/event tables and 64/32/64 for program/state/job storage.
The wide rows favor a simple first implementation and incur RAM granularity
costs. These are complete connected-top counts, not a sum of earlier standalone
figures. Lint and generic synthesis also passed 3/5/11 and 1/1/3 loop-table
configurations. No placement, routed timing or board transport is included.
