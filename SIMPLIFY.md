# Simplify the FPGA engine

## Decision

Simplify the FPGA Rust model and RTL around a single processing operation and
fatal processing errors. Remove resource preflight, refusal feedback, and retry
from the app-to-engine interface. Simplify the engine itself rather than hiding
its existing transaction protocol behind an ARM adapter.

Out of memory is a critical error. We accept silence and an explicit engine fault
instead of preserving uninterrupted playback or recovering individual recordings.
Normal musical behavior remains the goal; graceful behavior during exhaustion is
deliberately no longer a requirement.

The standalone FPGA Rust engine and direct SystemVerilog design now implement
this contract. The starting point was `23887b00` on
`proto/fpga-rust-reference`. App-connected simulator/ARM transport work remains a
separate integration step; the native app planner retains its own transactions.

## Processing and failure contract

The normal flow is:

```text
apply commands -> resolve transitions -> execute state/jobs -> advance clocks
                                             |
                                      processing failure
                                             |
                                      latch engine fault
```

Recording-resource creation and binding become ordinary engine work. The engine
creates or reuses the recording views it needs when a recording actually starts.
ARM does not inspect candidate decisions, allocate views during a quantum, or
send continuation messages.

If allocation or other required processing fails:

1. Latch a fault with its cause and processing location/frame.
2. Discard the failed block's output and return silence.
3. Return silence on subsequent host callbacks without resuming processing.
4. Keep fault/status reporting and reset available to the app.
5. Require explicit engine reset and configuration reload before processing again.

Internal state and sample storage may contain partial effects after a fault. There
is no rollback and no replay of the failed quantum. The faulted state is diagnostic
state, not a valid session snapshot or a point from which execution can resume.

Publish audio only after successful completion. Output RAM may contain partial
results, so entering a fault state alone does not implement silence. The audio
adapter must suppress failed output, including the rest of a host block when
failure occurs partway through its quanta.

Buffered capture writes can fail after an earlier quantum has completed. Such a
late memory fault takes the same fatal path when observed; already published
audio cannot be retracted. Reset must coordinate outstanding memory traffic so
old responses cannot enter the new engine epoch.

Malformed, unsupported, or stale commands can still be refused at admission.
Ordinary command validation remains distinct from failure during execution.

## Remove existing transaction machinery

| Removed machinery | Implemented replacement |
| --- | --- |
| Host preflight pause, acknowledgement, and retry | Automatic internal progression through a quantum |
| Capture-unavailable/resource-refused observations | Allocation failure enters the fatal fault path |
| Host-visible preview/commit/finish operations and boundary tokens | One processing operation with completion or fault |
| Candidate and committed copies of loop/request/journal tables | Update a single state bank in place where duplication exists only for retry/rollback |
| Per-quantum copies of state and pending-request indexes | Remove copies along with candidate banks; retain necessary boundary scratch initialization |
| Prepared recording reservations followed by cancellation and retry | Create/bind/use resources normally; fault if required work fails |
| Selectable external job executor and its completion protocol | Internal RTL job execution in the production interface |

Single-bank processing requires an explicit publication rule: normal state is
read or published between quanta. The host cannot concurrently inspect an intact
previous state while the engine modifies it. After a fault, partial state may be
available for diagnostics, but it is not published as a successful result.

Settled loop state must still remain stable while the state/job processors use
it. Ordered processing phases can provide that guarantee without a second copy
of all transition state. Internal phase sequencing remains useful even when
preview/commit are no longer separate operations or public concepts.

Optional job tracing and other debug facilities can remain in test tooling
without becoming requirements of the production app interface.

## Keep the necessary mechanisms

Preserve scheduling, synchronization, canonical request arbitration, loop IDs and
generations, fixed-point arithmetic, media ownership/reference counts, shared
ranges, allocator bounds checks, reclamation, and AXI backpressure/coherence.
These implement normal functionality and memory correctness.

The job path propagates media allocation errors to the pipeline's fault state.
Logical recording starts emit an internal `BeginCapture` job before capture.
It creates the configured next descriptor or clears/reuses an existing writable
view. Configure playback and capture for an ordinary recording loop with the
same persistent view identity; retakes do not need host binding writes. Gapped
IDs are invalid and cannot trigger allocation of all intervening descriptors.
Descriptor or chunk-table exhaustion uses the same fatal policy as sample-arena
exhaustion. The Rust functional media store remains elastic; finite descriptor
capacity is exercised in RTL and via a failing Rust memory implementation.

Do not add reservation/undo/retry machinery to recover from these failures. Keep
the checks that detect them and prevent invalid memory accesses.

## Optional conservative arena guard

A boundary check can fault early if available chunks are below a proven upper
bound on allocations for the next quantum. Count both reusable chunks and unused
slots below the arena limit.

With one append per capture stream and chunk size equal to the quantum, at most
one new payload chunk is needed per stream. A loop-count times channel-count bound
is sufficient only if it includes every allocating stream. Different chunk sizes,
additional history streams, or multiple capture operations require the appropriate
larger bound. Shared read-only grabs retain existing payload chunks, although they
still consume metadata.

This guard is optional. The simplified design does not depend on proving that
particular bound: any unexpected allocation failure takes the same fatal path.

## App and Zynq interface

Establish the software engine contract before specifying AXI registers or queues:

```text
configure(compiled_graph, limits)
submit(command, timing, sequence)
process_block(input_buffer, output_buffer, frames)
poll_events()
import_media(...) / export_media(...)
reset()
```

ARM retains app-protocol handling and compilation of supported graphs. The engine
owns transition execution, recording-resource lifecycle, media, and sample work.
Command admission and final execution outcomes remain distinguishable, without
requiring the host to participate in intermediate processing phases.

Use the same semantics for the Rust engine, a persistent RTL simulator, and Zynq.
The later Zynq transport can map commands/status to an ARM-visible AXI peripheral
and bulk buffers to DDR. The existing FPGA AXI master continues to access sample
memory. Software-backed memory is sufficient for app-connected simulation.

The dummy host can process blocks serially without wall-clock pacing. Physical
audio hosts continue their callbacks with silence if the engine faults.

## Implementation and validation

The Rust model and RTL now expose a single process/run operation. Their internal
settlement/advance phases remain ordered around actual sample work. Rust's
`process_block` silences the complete host block when any quantum fails; RTL
suppresses output readback from an unsuccessful run. A future host adapter must
aggregate quanta with the same whole-block publication rule.

The comparison format is version 3. Old observation/preview/token and external
completion documents are rejected, and checked-in examples/schema/drivers have
been migrated.

Validation compares normal scheduling, sync cycles, recording/playback, shared
ranges and samples against successful Rust cases and the independent native
transition resolver. Focused tests cover arena/descriptor exhaustion, partial
Output work, entire-block silencing, latched faults, reset/reload and late
writeback faults. Allocator/reference checks and AXI behavior remain in place.
No conservative arena guard or reservation layer was added: ordinary allocation
failure is sufficient.

Clearing/reusing a long recording still walks its old chunks synchronously.
Neither that work nor DDR service currently has a hard deadline certificate.
Reset is coordinated with old memory responses by the simulation service; board
integration must provide the corresponding epoch/quiescence rule.

The matched measurements below isolate simplification on `23887b00`, before
integrating upstream cached-playback pipelining from `54c616cf`. They record that
checkpoint; the integrated design also includes the newer playback datapath.

Matched Yosys 0.52 ECP5 mapping, before and after simplification at that checkpoint:

| Resource | Before | After | Change |
| --- | ---: | ---: | ---: |
| LUT4 | 28,472 | 25,304 | -11.1% |
| Flip-flops | 13,913 | 13,442 | -3.4% |
| DP16KD block RAMs | 93 | 91 | -2 |
| DSP multipliers | 4 | 4 | unchanged |

These are the default `loop_pipeline` core capacities: 8 loops, 16 requests,
32 events, 64 instructions/jobs, 32 state words, 16 buffers, two input/output
ports, 32 views/slots, 32-frame chunks, four read/write buffers and the word-memory
port. AXI master/physical DDR controller and placement/routing are excluded.
Logical mutable tables have one bank, but wide/shallow RAM padding limits the
mapped BRAM saving. No routed frequency is established.

At 64 loops, standalone transition processing takes 860–2,849 cycles for the
8×8 workload (previously 928–2,917) and 2,252–3,660 for the sync-cycle workload
(previously 2,320–3,728). Increasing logical request capacity from 64 to 130 no
longer adds a table-copy pass: the latter workload remains 2,252–3,660 cycles.
These counts exclude command admission/readback and sample execution.

The matched four-quantum 64-loop playback benchmark uses 32-frame quanta,
128 request slots, 192 jobs/quantum and burst AXI32 with the instant memory model:

| Active cycles | Before | After |
| --- | ---: | ---: |
| First quantum (64 Play requests) | 20,541 | 20,408 |
| Each following playback quantum | 18,969 | 18,836 |
| Transition portion of following quantum | 1,006 | 873 |

Each quantum saves 133 active cycles; steady whole-pipeline time falls 0.7%.
State/job execution and sample traffic are unchanged for this workload. Before
counts exclude the verifier's 1,540–2,308-cycle host preflight pauses; current
simplified counts include the entire accepted run because that pause no longer exists.
The model is instantaneous AXI service, not a physical DDR timing measurement.
Reproduce with `python fpga/loop_pipeline/simulate.py --media --config 8x8 --timing`.

Validation passed: 64 reference/testbench Rust tests, 491 selected native
runtime/engine/host tests (one additional test skipped), 19 Python contract tests,
and 84 cocotb test groups across transition/state/job/media/pipeline/AXI profiles.
The combined pipeline exercised 117 cases / 1,735 steps plus dedicated fatal-fault
and reset tests. Additional runs used fixed seven-cycle AXI64 responses and the
legacy seven-cycle word port. The warning-denying workspace build, format check,
test-attribute policy, generated-wrapper check, lint and generic/ECP5 synthesis
also passed. These are behavioral comparisons, not formal equivalence.

After rebasing onto upstream `54c616cf`, the same instant-memory AXI32 benchmark
passes at 17,793 cycles for the first quantum and 16,221 cycles for each following
quantum. This integrated result includes both simplification and upstream playback
pipelining; the synthesis table above remains the isolated earlier checkpoint.
Revalidation passed 64 Rust tests, 19 Python tests and 45 cocotb groups, including
42 combined-pipeline cases / 385 steps plus fault/reset suites, playback cadence,
chunk streaming, delayed AXI64 and legacy retake replays. Both-width lint and the
warning-denying workspace build also passed.

Machine-readable capacities, counts and source-manifest hashes are in
[the measurement report](fpga/loop_pipeline/measurements/simplification.json).
Run current mapping with `bash fpga/loop_pipeline/check.sh`; the previous
source is available at `23887b00`.

The native app planner also uses preparation/retry to coordinate captures and
graph activation. That broader transaction machinery is a separate audit; changing
the FPGA engine's OOM policy does not justify deleting all native transactions or
claim support for app features outside the FPGA model's current scope.
