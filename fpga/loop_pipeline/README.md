# Shared-loop audio pipeline

`loop_pipeline` connects transition settlement, state/job execution and media
storage under one processing operation. The Rust `pipeline::Pipeline` has the
same contract. See [SIMPLIFY.md](../../SIMPLIFY.md) for the design decision.

```text
commands -> settle transitions -> execute state and jobs -> advance clocks
                                      |
                                   failure -> latch fault -> silence until reset
```

## Processing and publication

The transition processor has one loop/request/journal bank. Settlement updates it
in place; state instructions read its stable settled records through the shared
RAM port while jobs execute. Successful completion advances logical clocks once.
Read ordinary state between quanta. A fault can leave partial diagnostic state,
media writes, jobs and events; it never establishes a resumable session snapshot.
There is no resource preflight, observation list, continuation, boundary token,
commit, rollback, replay, or selectable external job executor in the public flow.

Recording starts emit `BeginCapture` before `Capture`. The media store creates
the configured view if it is the next descriptor, or clears/reuses that writable
view on a retake. Configure playback and capture operands for an ordinary recording loop to use
the same stable writable view ID. Retakes reuse that identity; there are no
per-take host binding writes. Imported shared read-only views remain distinct. Creation is bounded
by physical descriptor capacity in RTL; invalid/gapped IDs are refused before
allocation. Shared read-only slices retain their chunks when the original view
is cleared. Reference counting, free-slot reuse and allocator checks remain.
Clearing a long take still walks its chunk list; this implementation establishes
no worst-case audio deadline.

Any required processing error, including arena/descriptor exhaustion, enters a
sticky fault. Record the cause, stage and logical frame, suppress failed output,
and reset/reload before processing again. Late posted-writeback faults take the
same path when observed; already published audio cannot be retracted. Coordinate
reset with outstanding memory traffic so old responses cannot enter a new epoch.
Malformed commands can be rejected at admission without faulting the engine.

## Rust API

Prepare the transition plan, program and persistent node state once. Apply
`submit`, `cancel`, `collect` and control writes between calls. `process` executes
one quantum; `process_block` executes a whole number of quanta over host slices.
Both execute actual jobs and return `Result<(), PipelineError>`. A failure in a
later quantum zeros **the entire host block**, including its successful prefix.
Subsequent calls return the latched error and zero the supplied output slices.
Recreate the engine and reload configuration/media to recover.

`trace_jobs(true)` and `emitted_jobs()` are optional diagnostic facilities;
`queued_jobs()` permits inspection of a failed queue. They do not require host
participation during execution. Standalone `TransitionProcessor::process()` also
settles and advances in one call. Its split settlement/advance helpers are private
to the Rust engine.

## RTL interface

Load the plan through the transition command port, program/state through `cfg_*`,
input RAM through `audio_cfg_*`, and any imported media through `store_*` while
idle. One `run_valid && run_ready` transfer latches the quantum descriptor and
arena base. The coordinator then automatically settles, starts the internal
state/job/media pipeline, and advances clocks after successful jobs. `done` pulses
on completion; `error_stage` is 0 success, 1 transitions, 2 state or 3 jobs/memory.
Error codes use the corresponding subsystem encoding.

`faulted` remains asserted after a processing error and `run_ready` stays low.
The error stage/code and transition status frame remain available for diagnostics.
The output-RAM readback path returns zero until successful completion and after
any fault, even when RAM contains a partially executed Output job. A host adapter
combining multiple runs into a block must withhold publication until all runs
succeed and silence the whole block if any fails. The Rust block API implements
that policy; a complete app/ARM transport remains future work.

Host traffic is blocked during processing. Program/media/debug ports remain
available between runs or for fault diagnostics subject to subsystem guards.
The transition coordinator reserves opcodes 9 and 11 internally: settlement and
clock advancement are ordered implementation phases, with no public token or
acknowledgement. `INTEGRATED=1` selects this private transition interface.

`monitor_*`, queue counts and failed-queue draining support verification. The
`job_enable` input permits controlled stalls. The production executor always
consumes jobs internally. These are fabric interfaces, not a finalized ARM AXI
register map.

## Media and shared records

The shared response is 164 bits: valid at 0, generation u32 at 1, mode at 33,
length u64 at 35, position u64 at 99, capture-start at 163. Capture-start means
recording with zero captured frames and causes one `BeginCapture` job. Full IDs
are validated before use. Node state does not duplicate loop modes or clocks.

Media commands are four bits; internal opcode 8 begins/reuses a recording view.
Sample payloads use the chunk-memory service; descriptors, references and chunk
indexes use synchronous on-chip RAM. The generated
[`loop_pipeline_axi`](../axi_memory/README.md) top connects the existing buffered
chunk service to AXI, including backpressure, retirement and late fault handling.
The legacy word-memory port remains available for focused comparisons.

## Reproduce checks

Use the repository Cargo environment, Python dependencies in `requirements.txt`,
Verilator, Make, a C++ compiler and Yosys on PATH:

```sh
cargo test --locked -p rt_fpga_reference -p rt_fpga_testbench
python fpga/loop_pipeline/simulate.py --config all --random-cases 30
python fpga/loop_pipeline/simulate.py --config default \
  --case fpga/loop_pipeline/examples/record_play_samples.json
python fpga/loop_pipeline/simulate.py --media --config 8x8 --timing
bash fpga/loop_pipeline/check.sh
```

The bench uses format-version-3 Rust traces. All normal pipeline runs execute
samples/media; `--media` is retained for command-line compatibility. Comparisons
include post-process loop/request/event state, jobs, node state, samples and media
snapshots. Dedicated tests cover automatic recording creation/reuse, allocation
and descriptor exhaustion, partial Output work, sticky silence, late memory
errors and reset with old traffic. Rust additionally checks whole-block silencing
when a later quantum fails. Protocol tests check admission and backpressure.

The timing workload has 64 logical loops, 128 request slots, 32-frame quanta and
192 jobs per quantum. It profiles four coordinated boundaries using the selected
AXI memory model. Current `active_cycles` covers the entire accepted run through
completion; there is no host inspection pause to subtract. Results include bus
activity and checked sample/state outcomes. Mapping and simulated cycle counts
establish neither routed frequency nor physical audio deadlines.

## Simplification measurements

The simplification checkpoint based on `23887b00`, before integrating upstream
cached-playback pipelining from `54c616cf`, mapped the default core to 25,304 LUT4
(previously 28,472), 13,442 flip-flops (13,913), 91 block RAMs (93) and four DSPs
(unchanged). The four-quantum 64-loop instant-memory playback benchmark saves
133 active cycles per quantum: steady playback took 18,836 instead of 18,969.
Previous active counts exclude verifier preflight inspection pauses.
After integrating `54c616cf`, the same AXI32 instant-memory fixture takes
17,793 cycles initially and 16,221 in steady playback, including both changes.
See [SIMPLIFY.md](../../SIMPLIFY.md) and the
[machine-readable report](measurements/simplification.json) for capacities,
measurement scope, validation and limits. These are mapped resources and simulated
cycles, with no placed/routed frequency claim.

## Historical measurements before simplification

**Everything below describes earlier implementations, through `54c616cf`.**
References to preflight, candidate banks, retry and external execution describe
those historical measurements. Use the contract above for current behavior.
The matched before/after simplification results are recorded in SIMPLIFY.md.

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
[playback sample delivery checkpoint](#pipelined-playback-sample-delivery).

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

This checkpoint uses word transfers. The combined burst results follow below.

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

### Combined pipelining and chunk bursts

This checkpoint precedes the cache-to-scratch read pipeline measured below.

Revision `f3236460` merges upstream `6d605b24` (gain/mix pipeline `928d27aa`)
with the chunk-burst implementation `e73c9870`. The merge needs no RTL changes.
All four quanta match the complete Rust trace in both DDR3 and instant-memory
simulations. A word-transfer control run uses the same merged source and DDR
model. All three were measured with a clean working tree, seed 20261110, the
same benchmark fixture, 128 read buffers, four write buffers, 64-bit AXI and an
assumed 100 MHz fabric clock.

```sh
python fpga/loop_pipeline/simulate.py --media --config 8x8 --timing \
  --read-buffers 128 --axi-data-width 64 --memory-model dramsim3 \
  --output work/merged-burst-profile
python fpga/loop_pipeline/simulate.py --media --config 8x8 --timing \
  --read-buffers 128 --axi-data-width 64 --word-transfers --memory-model dramsim3 \
  --output work/merged-word-profile
```

For the instant-memory control, select `--memory-model instant` and a different
output directory. The fixture still processes 64 mono loops, Q=32, 64 gains,
56 ordered mixes and eight outputs. Media positions advance on every quantum;
128 KiB of backing samples exceeds the 16 KiB read pool.

| Per playback quantum | Scalar + words | Pipelined + words | Pipelined + bursts |
| --- | ---: | ---: | ---: |
| Active time at 100 MHz | 297.48 us | 184.19–184.36 us | **164.84 us** |
| Exposed fill wait | 0 | 19.35–19.52 us | **0** |
| AXI read transactions | 2,048 | 2,048 | **64** |
| Native READ commands | 2,048 | 2,048 | **512** |
| Physical DDR data | 32 KiB | 32 KiB | **8 KiB** |
| DMA active time | 169.33–169.48 us | 169.91–170.08 us | **37.54–37.99 us** |

The combined result saves **132.64 us / 44.59%** against the scalar baseline,
or **1.805x throughput** at the same assumed clock. Bursts alone save another
**19.35–19.52 us / 10.51–10.59%** against the upstream word-transfer checkpoint.
The DDR run now exactly matches the instant-memory control: prefetch hides all
backing-memory service in this fixture. DMA activity overlaps compute and is
not an additive part of the active execution time.

| Boundary | Active cycles | Active time | Preflight wait | Raw run-to-done |
| --- | ---: | ---: | ---: | ---: |
| Apply 64 Play commands | 18,056 | 180.56 us | 15.40 us | 195.96 us |
| Playback 1 | 16,484 | 164.84 us | 23.08 us | 187.92 us |
| Playback 2 | 16,484 | 164.84 us | 15.88 us | 180.72 us |
| Playback 3 | 16,484 | 164.84 us | 15.88 us | 180.72 us |

Active time excludes the verifier's preflight pause, as in earlier checkpoints;
raw latency includes it. Setup, input loading and final snapshots remain outside
the run. In steady playback TP costs 10.06 us, SP 11.52 us, JP 143.21 us and
other handshakes 0.05 us. JP includes 70.40 us of cached reads, 24.32 us of gains,
38.08 us of mixes, 7.85 us of outputs and 2.56 us of output clearing. Cached
sample delivery is now the largest remaining job cost.

Steady active DDR data utilization is **2.3295%**, compared with 8.3316–8.3392%
for the pipelined word-transfer run; command utilization is **0.7519%**. Although
the active window is shorter, each quantum still transfers only 8 KiB physically.
At 48 kHz, active execution takes **24.726%** of the 666.67 us audio period;
the full-period DDR data utilization is **0.576%**. These are simulation estimates
with the illustrative DDR3 profile, without routed timing or competing DDR users.

Post-merge verification matches Rust for 51 connected DDR3 cases / 404 quanta
across the default, small and odd profiles, plus preparation/fault/reset protocol
tests. The 32-bit media-audio path passes 21 cases / 65 quanta, including faults
during active gain and mix writes. Generated-wrapper consistency checks and
warning-clean integrated lint pass at both bus widths.
Generic checks and ECP5 synthesis pass for the default eight-loop AXI top:
27,613 LUT4s, 14,153 flip-flops, 93 DP16KD blocks and four MULT18X18D blocks.
Compared with the burst-only top this adds 56 LUT4s and 145 flip-flops, with
unchanged BRAM and DSP counts. These capacities differ from the 64-loop timing
profile; synthesis does not establish routed frequency or board fit.

### Pipelined playback sample delivery

The cache overlaps successive synchronous RAM reads using independent valid bits
for the RAM return and output register. Backpressure holds both stages without
reissuing or dropping samples; the line remains pinned until its final sample
is consumed. The job processor writes each accepted ReadView sample directly
to scratch RAM and publishes its destination on the final write. This retains
the existing RAM ports, media interfaces and Rust behavior.

The same 64-loop fixture, seed, Q=32, 128 read buffers, four write buffers,
64-bit AXI and illustrative DDR3 profile produce the following measured results
at an assumed 100 MHz:

| Measurement | Before playback pipelining | After |
| --- | ---: | ---: |
| ReadView job, 32 samples | 110 clocks | **48 clocks** |
| All 64 ReadView jobs | 70.40 us | **30.72 us** |
| Job processor busy | 143.21 us | **103.53 us** |
| Steady active cycles | 16,484 | **12,516** |
| Steady active time | 164.84 us | **125.16 us** |
| Apply all 64 Play commands | 180.56 us | **140.88 us** |
| Exposed demand-fill wait | 0 | **0** |

All four complete quantum traces match Rust. Instant-memory and DDR3 runs have
identical active cycles and processor/job breakdowns. The measured saving is
**3,968 clocks / 39.68 us / 24.07%**, or **1.317x throughput** relative to the
previous checkpoint. A read job costs 32 sample clocks, two pipeline fill clocks
and 14 other clocks. The two fill clocks per chunk explain the 128-clock
difference from the estimate that replaced every three-clock sample iteration
with a single clock. Relative to the scalar/word baseline, execution is now
**2.377x faster** and takes **57.93% less active time**.

| Quantum | Active cycles | Active time | Preflight wait | Raw run-to-done |
| --- | ---: | ---: | ---: | ---: |
| Apply 64 Play commands | 14,088 | 140.88 us | 15.40 us | 156.28 us |
| Playback 1 | 12,516 | 125.16 us | 23.08 us | 148.24 us |
| Playback 2 | 12,516 | 125.16 us | 15.88 us | 141.04 us |
| Playback 3 | 12,516 | 125.16 us | 15.88 us | 141.04 us |

Preflight verifier pauses, setup and snapshots keep the same measurement
boundaries. TP remains 10.06 us in steady playback, SP 11.52 us, gains 24.32 us,
mixes 38.08 us, outputs 7.85 us, output clear 2.56 us and other handshakes 0.05 us.
Mix now has the largest individual job cost, followed by playback reads. At
48 kHz, active execution uses **18.774%** of the 666.67 us audio period.

Memory traffic remains 64 AXI bursts / 512 native READs / 8 KiB physically per
quantum. DMA activity spans 37.59–37.89 us and overlaps execution. Active DDR
data utilization rises to **3.068%** because the observation window is shorter;
full-period utilization remains 0.576%. There are no writes in this playback
fixture. A separate instrumented replay confirms consecutive playback samples
and scratch writes, while comparing startup and steady playback with Rust.

```sh
python fpga/loop_pipeline/simulate.py --media --config 8x8 --timing \
  --read-buffers 128 --axi-data-width 64 --memory-model dramsim3 \
  --output work/playback-stream-profile
```

Both-width integrated lint and generic/ECP5 synthesis pass. The default eight-loop
AXI top maps to 26,967 LUT4s, 14,187 flip-flops, 93 DP16KD blocks and four
MULT18X18D blocks. Relative to the previous top this adds 34 flip-flops and
retains its BRAM/DSP counts. This capacity profile differs from the 64-loop
benchmark; no routed timing or board fit is established.

Verification matches Rust for 63 connected DDR3 cases / 554 quanta plus
preparation/fault/reset checks. The single-read/single-write-buffer DDR3 profile
passes another 16 cases / 87 quanta. Word-AXI and delayed legacy loop paths each
pass 12 cases / 59 quanta. All four chunk profiles pass in 32-/64-bit burst and
legacy modes, including one-sample chunks, empty/partial reads, write-buffer
forwarding, stalls at every sample position and reset with pending data.
Media-audio comparisons pass 61 cases / 189 quanta at 64 bits and 21 cases /
65 quanta at 32 bits, plus the new cadence/publication/reset/fault protocol
checks. The BRAM arithmetic regression retains all 172 cases / 507 quanta and
its stream/reset checks. Rust source and wire formats are unchanged.
