# Rust functional processor reference

This restores the functional transition/state/job and logical-memory implementation from
`354305d4697d2e6068e28d4a2e369f08974e434d`, before its deletion in `16679ba7`.
It covers the retained hardware, including constant, arithmetic/balance and
restart-smoothing instructions added after that historical revision.

The crate is `no_std` with `alloc`. It contains:

- `transitions`: bounded primitive-loop scheduling, synchronization, logical state,
  retained requests/completions and ordered events.
- `pipeline`: transition settlement, read-only record publication, state/job/media
  execution and successful-quantum clock advancement.
- `state`: persistent programs, typed state, ordered job emission and read hints.
- `jobs`: sample buffers, Q8.24 processing, persistent smoothing histories and
  memory operations through an explicit `Memory` trait.
- `fixed`: independent saturating arithmetic and sample-duration ramps.
- `records`: caller-supplied read-only records with generation checks.
- `storage`, `chunk_memory`, `resolver`: logical media ownership, slices,
  recording/readback, synchronous chunk storage and advisory read resolution.

Programs are supplied explicitly. SP can consume caller-supplied records directly
or records produced by the functional transition model. There is no graph
compiler, owning-request adapter, ARM protocol or application backend. `serde`
and `schema` are optional fixture/data-description features, not a hardware
transport contract. The transition model and combined functional pipeline have
no corresponding active RTL top on this branch.

## Functional contract

`TransitionPlan::compile` prepares primitive-loop timing and fixed capacities.
`TransitionProcessor::process` settles and advances one quantum. Play, stop,
record and seek requests support absolute timing or a delay in source-loop ticks.
Completed/cancelled requests occupy their slots until collected; reuse increments
the request generation. Events and state are available for trace inspection.
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

`Pipeline::prepare` allocates the transition engine, SP, FIFO and record scratch;
the caller supplies JP and memory. `process` handles one quantum; `process_block`
handles a whole number of quanta. Each quantum settles transitions, publishes
generation-checked records (including the first-capture flag), executes SP and
JP, then advances clocks on success. Gain, constant, arithmetic and smoothing
use the current SP/JP implementations. A processing failure latches its stage
and silences the entire host block; earlier state/media effects remain and the
failed quantum does not advance clocks. Recreate the pipeline to recover. An
invalid host block is rejected without latching a fault. Neither this coordinator
nor the transition engine installs an application backend or hardware interface.

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

`transition_traces` runs the functional TP and the current native `shoop_loops`
resolver independently against identical commands. It compares each boundary's
acceptance/refusal, cancellations, completions, retained requests, complete loop
state, ordered events and next frame. Coverage includes 100 seeded sessions of
80 quanta, all supported quantum sizes, empty content, request-slot reuse,
generation rejection, repeat waits, same-boundary propagation, sparse indices,
large integers, self-sync and multi-loop cycle rejection. Native loop dependencies
are test-only. With `serde`, traces also round-trip through JSON.

`pipeline` restores the combined functional trace tests for recording/playback,
retained slices and failed-state clock handling. Its successful transition traces
must equal both standalone TP and native resolver traces. An additional session
checks gain, constant, arithmetic, smoothing, seek, stop/restart and retake against
independent integer sample expectations. `transitions` preserves the allocation,
recording-view reuse, host-block silence, latched fault and recovery tests.
These tests need no Verilator or DRAMsim3.

The Marlin tests use Verilator 5.032 and the existing native component drivers.
They independently run the same explicit inputs through Rust and RTL and compare
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
