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

The default simulation memory path now uses the shared synthesizable AXI
endpoint. See [AXI memory and DRAMsim3](../axi_memory/README.md) for detailed
timing, bus-width options, and the retained legacy word-port mode.

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
Finish waits for successful logical completion of sample work. Captures may
complete into the chunk write pool while DDR writeback continues. Use host store
opcode 7 (Fence) before observing backing memory. Testbench snapshots do this
explicitly; it is outside the measured audio execution interval.

The media frontend uses [chunk memory](../chunk_memory/README.md), with configurable
`READ_BUFFERS` / `WRITE_BUFFERS`, shared coherence and a background transfer engine.
Job-FIFO lookahead supplies advisory read hints. The `memory_fault` output reports
late errors even after a quantum completed. The coordinator freezes further
execution, reports stage 3/code 8 after foreground quiescence and requires reset.
It preserves already advanced clocks. Reset also invalidates the chunk buffers;
fence first when posted writes must be retained across a planned reset.

`store_cmd_*`, `store_cfg_*` and host sample streams are available while idle,
paused for preflight or faulted, subject to the store's own readiness/fault guards.
Host commands support Create, Slice, Clear, Remove, Reclaim, Append, Read and Fence.
They share the store with jobs and cannot interleave with job execution. A fresh
recording needs a prepared empty view (or a cleared reusable view); resetting
logical recording length does not clear physical media. Playback and capture
can use separate view-state words so a refused retake preserves the old playback
handle. There is no sealed/unsealed view state.

The integrated AXI port transfers whole chunks through bursts of at most 16
beats. The internal `BURST_MEMORY` interface carries a 64-bit byte address and
sample count, 32-bit sample streams, and an aggregate completion; the direct
core also retains acknowledged word mode. `MEMORY_RESPONSE_CYCLES` defaults to
one and can impose additional response delay for simulation. The memory
adapter must flush outstanding traffic on reset. A physical bus fault reports
job code 8, freezes logical time and requires reset; partial sample effects can
survive, including provisional metadata allocations. Codes 5/6/7 report
InvalidView/MemoryFull/InvalidState.

The packed store interfaces and snapshot layout are specified in
[`media_store/README.md`](../media_store/README.md). Physical DDR controllers,
large-session metadata and board audio/host transport remain
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
python fpga/loop_pipeline/simulate.py --media --memory-model legacy --response-cycles 7 --random-cases 10
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

The measurements below precede chunk-memory integration. Current buffering,
sample comparisons, synthesis estimates and moving-playback timing are recorded
in the [chunk-memory checkpoint](../chunk_memory/README.md#verification-checkpoint).

Before streaming the TP passes, Verilator 5.032 and cocotb 1.9.2 passed these
comparisons against Rust across
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

That checkpoint's Rust reference/testbench suite passed 65 tests, and the Python cross-language
contract passed 17 tests. These checks use the existing Rust pipeline and store;
the integration required no changes to their sample semantics.

The streamed TP passes retain the same transaction, shared-read and sample
interfaces. An identical 18-quantum 64-loop replay (eight tracks, per-loop gains,
track mixing, Q=32, seven-cycle memory responses, no random stalls) matched Rust
after the optimization, including unchanged state/job cycle counts and memory
traffic. At an assumed 150 MHz:

| Measurement | Before streaming | After streaming |
| --- | ---: | ---: |
| TP, one quiet preview + commit + Finish | 15.89 µs | 6.52 µs |
| TP, three busy recording previews + commit + Finish | 73.61 µs | 50.60 µs |
| Whole quantum, worst recording case | 505.33 µs | 482.32 µs |
| Whole quantum, quiet all-loop playback | 321.75 µs | 312.37 µs |

TP/whole-quantum times exclude host preflight inspection pauses. The worst
accepted-run-to-done time including those pauses fell from 547.49 to 524.47 µs.
Fresh connected regressions passed 58 media cases / 629 quanta with seven-cycle
responses and 119 control cases / 2,919 quanta across default/small/odd, including
all preparation, fault and reset protocol checks. The standalone TP comparison
passed 205 cases / 10,274 boundaries across four depth profiles. Python contract
tests passed all 17 cases.

Yosys 0.52 mapped the default connected top to ECP5 using 19,842 LUT4,
11,189 flip-flops, 2,834 CCU2C carry cells, 83 DP16KD block RAMs,
68 distributed RAM cells and four MULT18X18D DSPs. These capacities are
8/16/32 for loop/request/event tables, 64/32/64 for program/state/job storage,
and 32/32/32 for media views/slots/chunk frames.
The wide rows favor a simple first implementation and incur RAM granularity
costs. These are complete connected-top counts, not a sum of earlier standalone
figures. Lint and generic synthesis also passed 3/5/11 and 1/1/3 loop-table
configurations. No placement, routed timing or board transport is included.
The streamed TP increases this complete top's LUT4 count by 2,129 (12.0%) from
17,713, while BRAM and DSP counts remain unchanged; it adds three flip-flops.

## Coordinated 64-loop benchmark with DRAMsim3 (word-transfer baseline)

The tables and opportunity analysis below record the scalar sample-engine
baseline. Current measurements are in the
[pipelined gain and mix checkpoint](#pipelined-gain-and-mix-checkpoint).

The `8x8` timing profile extends the media benchmark through the actual transition
processor, committed loop-read port, state/job processors, media frontend, chunk
cache and synthesizable AXI endpoint. It follows upstream TP-streaming revision
`c280613e`; profiling support is in `20dfca2a`. The same complete fixture passes
bitwise Rust comparisons in both instant-memory and detailed DDR3 modes.

```sh
python fpga/axi_memory/build_dramsim3.py
python fpga/loop_pipeline/simulate.py --media --config 8x8 --timing \
  --read-buffers 128 --word-transfers --memory-model dramsim3 --output work/coordinated64-dram
python fpga/loop_pipeline/simulate.py --media --config 8x8 --timing \
  --read-buffers 128 --word-transfers --memory-model instant --output work/coordinated64-instant
```

Ordinary `--config all` retains default/small/odd profiles. The large timing
profile is opt-in. Its saved `benchmark.case.json` can also be replayed using
`--media --config 8x8 --case PATH` without timing mode.

The fixture contains eight tracks of eight independent mono playback loops,
32-sample chunks/quanta, 512 samples per loop, one constant 0.5 gain per loop,
seven ordered mixes per track and eight outputs. It uses 64 loop slots, 128
request slots, 256 events, 256 program/job entries and 512 state entries, with
64 media views, 1,024 sample slots, 128 read buffers and four write buffers.
The media holds 128 KiB of audio versus 16 KiB in the read pool. Positions move
every quantum; each iteration reads 64 different chunks. Sequence zero starts
all loops through 64 actual Play commands. Later quanta have no new commands
and encounter four staggered natural wraps each.

The graph emits 192 jobs per quantum: 64 reads, 64 gains, 56 mixes and eight
outputs. Logical playback instructions obtain their positions/modes from TP
records rather than independent state words. Verification compares the complete
transition trace, jobs, state, samples and media snapshots with Rust. Random
backpressure is disabled for the benchmark, while normal regression retains it.

The following baseline predates the chunk-burst adapter. Current burst results
appear in [the AXI memory documentation](../axi_memory/README.md#chunk-burst-results).

### Measurement boundary and results

`execution-timing.json` records coordinator phases, TP states, job-processor
phases and per-job costs, plus DDR data/command utilization. Setup, command
submission, input loading and final snapshots occur outside the measured run.
During preflight the existing verifier reads candidate decisions and checks
committed state/requests/events. These intentional pauses are recorded under
`preflight_wait_cycles`, separately from active FPGA phases. Active time is the
sum of the observed phases excluding the entire PREFLIGHT wait, including its
acknowledgement cycle; an automatic approver would still require that handshake.
It is not the raw accepted-run-to-done latency.

All times below assume a 100 MHz fabric clock and the documented illustrative
DDR3-1066 x16 profile. No placement or routed frequency was measured.

| Quantum | Active phase cycles | Active time | Preflight wait | Raw run-to-done |
| --- | ---: | ---: | ---: | ---: |
| Start all 64 loops | 31,320 | 313.20 us | 15.40 us | 328.60 us |
| Playback 1 | 29,748 | 297.48 us | 23.08 us | 320.56 us |
| Playback 2 | 29,748 | 297.48 us | 15.88 us | 313.36 us |
| Playback 3 | 29,748 | 297.48 us | 15.88 us | 313.36 us |

Instant and DRAMsim3 runs have identical active/raw times, processor phase
counts and per-job costs across all four quanta. The original media-only
benchmark, rerun at `c280613e`, remains 28,996 clocks / 289.96 us. Its scope omits
TP and uses standalone playback state; logical playback here saves 256 state
cycles, while TP and coordination add 1,008 clocks in steady playback, for a
net 752-clock difference. The prior 150 MHz / fixed-delay TP checkpoint above
has a different workload and memory path; its absolute times are not this run's
before/after comparison.

Steady-playback active time has the following additive breakdown:

| Work | Count | Cycles | Time at 100 MHz | Share |
| --- | ---: | ---: | ---: | ---: |
| Gain jobs | 64 | 10,432 | 104.32 us | 35.07% |
| Mix jobs | 56 | 9,072 | 90.72 us | 30.50% |
| Cached media read jobs | 64 | 7,040 | 70.40 us | 23.67% |
| State processor | 192 instructions | 1,152 | 11.52 us | 3.87% |
| TP preview/commit/Finish and their handshakes | One boundary | 1,006 | 10.06 us | 3.38% |
| Output jobs | 8 | 785 | 7.85 us | 2.64% |
| Initial output clear | 256 samples | 256 | 2.56 us | 0.86% |
| Other audio/coordinator handshakes | — | 5 | 0.05 us | 0.02% |
| Total | | 29,748 | 297.48 us | 100% |

Job intervals span successive job acceptances; the final output includes job
completion. Job-processor busy time is 27,585 clocks (92.73% of active time).
Starting all loops raises TP work to 2,578 clocks / 25.78 us; SP/JP work stays
unchanged. At an assumed 48 kHz sample rate, 297.48 us is 44.62% of a 666.67 us
audio period; this workload measurement does not establish a hard bound.

### Memory activity and overlap

There are zero exposed demand-fill wait cycles. DRAMsim3 transfers 2,048 words
as 2,048 native READ commands per quantum: 32 KiB physically for 8 KiB of useful
samples, with no writes. Steady-playback DDR data utilization is 5.1634% and
command utilization is 1.4219%, measured over the active phases. Starting all
loops gives 4.9042% data utilization because its observation window is longer.
The data volume is unchanged. At 48 kHz, the full-period average remains 2.304%.

Read AXI response latency over all four detailed quanta averages 4.2709 fabric
cycles, median 4, p99 26 and maximum 33. The chunk DMA is active for
16,933–16,948 fabric clocks (169.33–169.48 us) per iteration. This activity
overlaps state/job processing and must not be added to the additive table.
It is mostly serialized word-transaction/control time; physical DDR data beats
occupy only 8,192 DRAM clocks / 15.36 us per iteration.

The instant-memory control confirms that faster external memory alone currently
removes no critical-path cycles. Efficient chunk bursts still project to one
quarter of the physical traffic and about 1.291% data-bus occupancy over the
same steady execution window. They also reduce the serialized transfer span,
which matters as the sample engine becomes faster and for competing traffic.

### Optimization opportunities

1. **Pipeline the sample datapath and its RAM accesses.** Gains and mixes account
   for 65.56% of steady active time. Every 32-sample gain takes 163 clocks and
   each mix 162: approximately five clocks per sample. Gain executes separate
   read-request, read-result, multiply, product-result and store states. Mix
   sequentially reads both operands and then stores. These are issue-rate and
   data-movement costs; merely making multiplication faster addresses only part
   of them. The measured JP phases spend 11,776 clocks on operand read stages,
   4,096 on multiply/product-result stages and 6,144 on stores, across the
   applicable jobs. Overlap those stages across samples before replicating
   whole node processors.

   A one-sample-per-clock gain pipeline would ideally save up to 8,192 clocks
   (81.92 us); a two-clock-per-sample mix using the existing single read port
   would ideally save 5,376 (53.76 us). These are per-sample-loop projections,
   before pipeline fill/drain and any newly exposed memory waits. Two source
   reads per mix set a port-bandwidth constraint; one-clock mixes require a
   different RAM organization or operand forwarding.

2. **Stream cached samples into scratch RAM more efficiently.** A read job
   takes 110 clocks despite a resident chunk and zero DDR wait. The cache read
   FSM requests, collects and sends each sample, and JP has a separate store
   state. Pipeline both sides of this path; just changing DDR bursts does not
   remove these hit-path cycles. Moving the roughly three-clock sample path
   toward one sample per clock projects up to 4,096 saved clocks (40.96 us),
   before overhead and backpressure. This is the next large independent cost.

3. **Keep chunk bursts alongside those datapath changes.** The adapter now
   primarily buys traffic/headroom, with zero measured critical-path benefit.
   The gain/mix projections together would reduce active compute toward 161.8
   us, already below the word-mode roughly 169 us serialized fill span even before
   its start offset. Memory would then become exposed unless its transaction
   overhead also shrinks. The actual speedup needs another combined RTL/DRAM
   run; the ideal savings above cannot simply be added to predict final runtime.

4. **Consider forwarding/fusion after establishing streaming throughput.**
   Read → gain → mix currently materializes intermediate scratch buffers and
   rereads them. Chaining these operations can avoid accesses and reduce storage.
   Preserve every existing fixed-point rounding/clipping point and the ordered
   saturating mix sequence. Algebraic reassociation or a wider accumulator can
   change bitwise results. Scratch-buffer liveness/reuse is also useful for area,
   although it alone does not remove the current sample-loop cycles.

5. **Keep TP/SP optimization proportional to their remaining cost.** Eliminating
   all steady TP work could save at most 10.06 us (3.38%) here; all SP work is
   11.52 us (3.87%). TP copy/validation/Finish streaming consumes 262 of its 1,006
   clocks. More complex incremental state machinery to remove those passes has
   a small total-time ceiling in this playback case. Likewise, VLIW state issue
   does not address the dominant JP loops. Retry-heavy recording/sync workloads
   need separate profiling before applying this prioritization to them.

The first implementation step suggested by this profile is a pipelined gain/mix
execution path, retaining the same job and fixed-point contracts, with chunk
bursts prepared to keep memory ahead of it. Ramp dividers are unused by the
constant-gain fixture, and output clearing is under 1%; neither is the first
timing target for this workload.

### Capacity and verification limits

This is a simulation capacity profile, not a small-board fit result. The current
dense `VIEWS*SLOTS` chunk-ID RAM alone reserves 64*1,024*4 = 256 KiB despite only
1,024 live IDs (4 KiB). Scratch reserves another 32 KiB and the read pool 16 KiB,
before TP/SP/program RAM and physical BRAM padding. Compact or external chunk-ID
tables are therefore an important area issue. Such metadata accesses are not
included in the DDR traffic above; current metadata is local RAM.

Both memory modes compare all four quanta with the actual Rust pipeline. A
regular small-profile DRAMsim3 run also passes directed, seeded and preparation/
fault/reset protocol tests after adding the opt-in profiling mode. RTL behavior
and the Rust reference are unchanged by this instrumentation. PR CI is not
inspected. The clock, x16 DDR profile and absence of other DDR users remain
feasibility-study assumptions.

### Pipelined gain and mix checkpoint

The gain/mix implementation in `928d27aa` pipelines samples within each job while
retaining one scratch read port, one write port, the shared multiplier and the
same job order/arithmetic. All four complete quantum traces match Rust in both
instant-memory and DRAMsim3 runs. The source commit was measured with a clean
working tree using the same fixture, seed, capacities and 100 MHz assumption
as the baseline above. Reproduction commands are unchanged.

| Measurement | Scalar baseline | Pipelined |
| --- | ---: | ---: |
| Constant gain, clocks/job at Q=32 | 163 | 38 |
| Mix, clocks/job at Q=32 | 162 | 68 |
| All 64 gain jobs | 104.32 us | 24.32 us |
| All 56 mix jobs | 90.72 us | 38.08 us |
| Steady active time, instant memory | 297.48 us | 164.84 us |
| Steady active time, DRAMsim3 | 297.48 us | 184.19–184.36 us |
| Applying all 64 Play commands, DRAMsim3 | 313.20 us | 199.91 us |

The sample loops remove exactly 13,264 clocks / 132.64 us of compute work:
8,000 clocks from gains and 5,264 from mixes. With DRAMsim3 the actual steady
reduction is 38.03–38.08%, or a 1.614–1.615x speedup. At 48 kHz, active execution
occupies about 27.6% of the audio period, compared with 44.62% before.

Per-quantum detailed measurements, with preflight verifier pauses still separate:

| Boundary | Active cycles | Active time | Exposed fill wait | Raw run-to-done |
| --- | ---: | ---: | ---: | ---: |
| Apply 64 Play commands | 19,991 | 199.91 us | 19.35 us | 215.31 us |
| Playback 1 | 18,419 | 184.19 us | 19.35 us | 207.27 us |
| Playback 2 | 18,436 | 184.36 us | 19.52 us | 200.24 us |
| Playback 3 | 18,431 | 184.31 us | 19.47 us | 200.19 us |

TP remains 10.06 us in steady playback (25.78 us when starting all loops), state
processing remains 11.52 us, outputs remain 7.85 us, output clear 2.56 us and
other handshakes 0.05 us. Instant-memory JP busy time falls from 275.85 to
143.21 us. Detailed memory adds 19.35–19.52 us of newly exposed fill waits,
raising JP time to 162.56–162.73 us. Thus the pipeline achieved the predicted
compute savings, and the previously hidden serial chunk-fill path now limits
part of the speedup.

The cached-hit read work itself is still 70.40 us; with these fill waits the
64 read jobs cost 89.75–89.92 us. The serialized chunk DMA now runs for about
170 us per quantum. DDR traffic is unchanged at 2,048 word transactions / 32 KiB
physical data per iteration. Data-bus utilization during steady active execution
rises from 5.16% to 8.33–8.34% because the same transfers occur in less time.
At a fixed 48 kHz audio cadence the full-period bandwidth demand remains 2.304%.

Chunk bursts are now a useful next timing target as well as a bandwidth target:
the instant-memory comparison identifies about 19.5 us of current exposed memory
cost. Cached sample streaming is still the largest remaining non-DDR job cost,
but speeding it up without improving fills would expose more memory wait.
Neither path is changed by this checkpoint. The constant-gain fixture does not
measure divider-heavy live-ramp performance; arithmetic/ramp behavior is covered
by the separate Rust comparisons.

Verification includes 432 BRAM directed/seeded cases / 1,287 quanta plus stream
cadence and reset tests; 55 media pipeline cases / 165 quanta plus active-stream
fault tests; and 45 connected DRAMsim3 cases / 318 quanta plus preparation/fault/
reset protocol tests. A 64-bit, single-read/single-write-buffer media run also
passes multi-chunk and wrap cases. Standalone/combined and integrated synthesis
checks pass, with no additional DSP or BRAM in the isolated arithmetic processor.
All timings remain simulation cycle counts at an assumed clock, without routed
timing or a hard real-time guarantee. PR CI is not inspected.
