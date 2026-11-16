# Rust functional processor reference

This restores the functional transition/state/job and logical-memory implementation from
`354305d4697d2e6068e28d4a2e369f08974e434d`, before its deletion in `16679ba7`.
The audio instructions cover the retained hardware, including constant,
arithmetic/balance and restart-smoothing operations. The Rust-only sync prepass
extends SP with loop execution; this section has no RTL implementation yet.

The crate is `no_std` with `alloc`. It contains:

- `transitions`: control-side command admission, cancellation and retained
  request/completion bookkeeping, plus timing-plan compilation.
- `pipeline`: SP/JP/media coordination, outcome collection and fault containment.
- `state`: a sync-ordered loop prepass followed by audio-ordered state execution,
  job emission and read hints. SP owns loop cursors, pulse flags and active plans.
- `jobs`: sample buffers, Q8.24 processing, persistent smoothing histories and
  memory operations through an explicit `Memory` trait.
- `fixed`: independent saturating arithmetic and sample-duration ramps.
- `records`: caller-supplied read-only records with generation checks.
- `storage`, `chunk_memory`, `resolver`: logical media ownership, slices,
  recording/readback, synchronous chunk storage and advisory read resolution.

Programs are supplied explicitly. SP's sync section is generated from a timing
plan; its audio instruction list is explicit. Isolated audio fixtures can still
supply read-only records directly with no sync section. There is no audio graph
compiler, owning-request adapter, ARM protocol or application backend. `serde`
and `schema` are optional fixture/data-description features, not a hardware
transport contract. The sync prepass and combined functional pipeline have no
corresponding active RTL top on this branch.

## Functional contract

`TransitionPlan::compile` prepares primitive-loop timing and fixed capacities;
`sync_instructions()` emits source-before-dependent `ProcessLoop` instructions.
`Program::sync_instructions` runs before `Program::instructions`. The prefix must
visit every configured loop exactly once in compiled order, even when the audio
program has a different order or does not use every loop.

`TransitionProcessor::prepare(&plan)` prepares control-side request storage;
`StateProcessor::with_loops(plan, memory)` prepares SP-owned loop state.
`submit(&mut sp, command)` validates and inserts a planned transition into that
state without executing it. Play, stop, record and seek support absolute timing
or a delay in source-loop pulses. Each loop has exactly two active plan slots;
a third submission is rejected with `Capacity` without replacing existing plans.
Cancellation frees a plan slot. Resolution also frees the SP slot immediately,
while the completed/cancelled request remains in TP until collected; collecting
increments its request-slot generation. The native trace harness applies this
two-plan admission profile around the native resolver's global request limit.

`sp.run` performs the entire sync and audio program. The prepass applies due
plans, resolves wraps/repeat waits, propagates pulses and executes pulse-triggered
plans. Each SP loop slot exposes `pulse_this_quantum`, its two `planned` entries
(including remaining pulse skips), next-boundary logical `state`, and frozen
current-quantum `render` state. `AfterTicks(skip=0)` fires on the next eligible
pulse; a positive skip decrements once per eligible source pulse. Starting or
retriggering a loop can generate a pulse too. At most one pulse is delivered per
loop per quantum. Self-sync is handled locally after that loop's pulse.

Before advancing position/capture length, SP saves the render cursor and
first-capture flag. The audio pass reads that saved state, including the position
used by smoothing restart, while persistent cursors already describe the next
boundary. A next position equal to content length is wrapped in the following
quantum's sync pass, not prematurely during advancement. Pulse and priority scratch
are reset on each loop visit. No SP operation or clock update runs after JP.

SP records resolved outcomes in a prepared journal. `tp.harvest(&mut sp)` drains
those outcomes into retained requests; it executes no loop logic. Admission and
cancellation harvest automatically. Direct callers harvest before inspecting or
collecting completions. SP can execute multiple quanta without TP activity;
unharvested outcomes remain buffered within the prepared request capacity.
`sp.frame()`, `sp.loop_state()`, `sp.loop_states()` and `sp.events()` expose execution.
Quantum lengths are 1/16/32/64/128; loop lengths and requested positions must be
quantum-aligned. Synchronization follows the native resolver's canonical acyclic
dependency order. Self-sync is allowed; multi-loop cycles are rejected with
`InstantaneousDependencyCycle`. The compiled image retains singleton `components`
in dependency order. Composites, timelines, batches, program replacement and
capture monitoring remain outside this primitive-loop model's scope.

SP emits a complete quantum's ordered jobs before JP consumes them. Gain state
advances when its job is emitted; failed later instructions retain earlier state
writes and emitted jobs. Empty/undersized queues and invalid state are reported.
`run_with_records` supplies logical playback/capture/smoothing inputs directly;
the records contain no future actions or transition requests.

JP validates ready/destination buffers, performs integer sample operations, and
preserves history across quanta. `begin` resets buffer readiness; `reset` also
invalidates smoothing history. `with_history_slots` permits physical history
capacity to exceed the run's logical sample-buffer count. `Memory::supports_media`
describes a media-disabled JP configuration; enabled memory resolves chunk-based
read windows and accepts capture jobs.

`Pipeline::prepare` prepares TP admission storage, SP loop/node state and FIFO,
and fills an omitted sync prefix from the timing plan. The caller supplies JP
and memory. `process` handles one quantum; `process_block` handles whole quanta.
SP settles loops and computes next cursors before emitting audio jobs; TP harvests
outcomes and JP executes samples. A processing failure latches its stage and
silences the entire host block. Loop clocks, plans and cursors may already have
advanced when audio emission or JP fails. There is no rollback or retry; recreate
the pipeline to recover. An invalid host block is rejected before execution,
without latching a fault. No application backend or hardware interface is installed.

These are functional models, not cycle simulators. The logical memory model is
not an AXI/cache/DDR timing model and does not predict physical allocation order
or delayed reclamation. Transport framing, malformed packed instructions,
hardware pipeline fault containment and bus/reset timing remain RTL harness obligations.
There is no claim that all possible programs, capacities or hardware faults have
been exhaustively verified.

## Verification

```sh
cargo build --locked -p rt_fpga_reference --no-default-features
cargo test --locked -p rt_fpga_reference --no-default-features
cargo test --locked -p rt_fpga_reference --all-features
cargo test --locked -p rt_fpga_testbench --features sim --test reference --test state --test audio --test smoothing
```

`transition_traces` runs TP admission plus SP sync execution and the native `shoop_loops`
resolver independently against identical commands. It compares each boundary's
acceptance/refusal, cancellations, completions, retained requests, complete loop
state, ordered events and next frame. Coverage includes 100 seeded sessions of
80 quanta, all supported quantum sizes, empty content, request-slot reuse,
generation rejection, repeat waits, same-boundary propagation, sparse indices,
large integers, self-sync and multi-loop cycle rejection. Native loop dependencies
are test-only. With `serde`, traces also round-trip through JSON.

`pipeline` covers recording/playback, retained slices and compute-ahead failures.
Its successful transition traces must equal both sync-only SP and native resolver
traces. An additional session
checks gain, constant, arithmetic, smoothing, seek, stop/restart and retake against
independent integer sample expectations. `transitions` preserves the allocation,
recording-view reuse, host-block silence, latched fault and recovery tests.
These tests need no Verilator or DRAMsim3.

`sync` checks autonomous SP execution with reversed audio order, same-boundary
pulse chains, repeat waits, render/next cursor separation, delayed outcome
harvesting, two-plan admission/reuse, retake flags and invalid sync prefixes.

The Marlin tests use Verilator 5.032 and the existing native component drivers.
They exercise the shared audio instruction subset with caller-supplied records
and no Rust sync prefix. They independently run the same inputs through Rust and RTL and compare
emitted jobs, state writes, failure prefixes, remaining jobs, buffer contents and
every output sample exactly. No audio tolerance or alignment is applied.

Coverage includes all twelve current SP opcodes, generation/mode rejection,
logical record delays, FIFO admission/exhaustion, persistent ramps, saturating
gains/mixes, all arithmetic modes, interleaved smoothing histories, restarts,
physical history indices beyond the logical buffer count, invalid JP targets,
media-disabled operation refusal, and SP failures after earlier successful quanta.
Seed 20261115 drives 80 audio
sessions of six quanta. Additional TP-free memory sessions compare capture,
wrapped playback, clear/retake and logical sample readback for Q=4/8/32.

The five historical chunk-memory tests and fifteen independent media ownership,
slice, snapshot and capacity tests are retained. The former compiler/backend test
is replaced by direct state/job/media comparisons. Existing production CPU
numeric checks and explicit RTL vectors remain separate validation sources.
