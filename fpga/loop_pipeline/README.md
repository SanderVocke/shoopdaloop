# Shared-loop audio pipeline

`loop_pipeline` connects transition settlement, state/job execution and media
storage under one processing operation. The behavioral comparison uses the production Rust runtime. See [SIMPLIFY.md](../../SIMPLIFY.md) for the design decision.

The media path supports [32-chunk allocation blocks](../media_store/BLOCK_ALLOCATION.md),
with block-level tables/counts/free IDs and chunk-level sample delivery. Select
`--block-chunks 32` in simulations; legacy regression fixtures default to one chunk.

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
There is no resource preflight, rollback, replay or selectable external job
executor in the public flow. An optional host settlement pause exposes the
pre-audio observation cut without predicting state on the host.

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
succeed and silence the whole block if any fails. Native resource refusals are checked separately from hardware fatal faults; a complete app/ARM transport remains future work.

Host traffic is blocked during processing. Program/media/debug ports remain
available between runs or for fault diagnostics subject to subsystem guards.
The transition coordinator reserves opcodes 9 and 11 internally. Host command
16 requests settlement and returns the ordinary retained TP response once that
phase completes. It leaves the coordinator at a pre-audio boundary: TP queries,
media operations and configuration are available, while the logical frame has
not advanced. The next `run_valid && run_ready` resumes with audio execution and
clock advancement, skipping a second settlement. Command 16 is admitted only
from idle; after its response, resume the run before requesting another boundary.
Ordinary runs without command 16 still perform all phases automatically.
`INTEGRATED=1` selects this coordinated transition interface. The pause provides
an observation cut, not a commit/rollback or resumable fault protocol.

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
Sample payloads use the chunk-memory service; descriptors and compact roots stay
on chip, while chunk-ID vectors, reference counts and free IDs use
[persistent DDR metadata caches](../media_store/DDR_METADATA.md). The generated
[`loop_pipeline_axi`](../axi_memory/README.md) top connects the existing buffered
chunk service to AXI, including backpressure, retirement and late fault handling.
The two DMA clients share one burst-fair AXI port. The legacy word-memory port
remains available as a functional diagnostic. Current DDR metadata timings and
replay commands are in [the measurement report](../media_store/measurements/README.md).

## Verification

Full musical/state/media behavior is compared with the production float and
Q8.24 runtime via [common sessions](../verification/README.md). Static lowering
uses production signal and timing compilation. There is no software FPGA executor.
Standalone tests preserve hardware-specific protocol, configuration, stream,
cache, capacity, fault and reset checks without a duplicate looping backend.

```sh
python fpga/loop_pipeline/simulate.py --directed --random-cases 30 --faults
python fpga/loop_pipeline/simulate.py --benchmark64 --config 8x8 --read-buffers 128 --metadata-lines 128 --memory-model dramsim3
```

The subsystem's `check.sh` retains lint and synthesis entry points. Current
coverage and local evidence are recorded in [the retirement ledger](../verification/MIGRATION.md)
and [subsystem gates](../verification/subsystem-gates.json). [Historical results](HISTORY.md)
retain the earlier reference-backed measurements; their commands are retired.
