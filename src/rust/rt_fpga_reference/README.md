# Rust functional processor reference

Functional models of the retained state, job and memory processors, restored from
`354305d4697d2e6068e28d4a2e369f08974e434d` and extended with SP-owned loop
execution. The remaining transition processor has been removed so that its future
replacement can be designed independently.

The crate is `no_std` with `alloc`. It contains:

- `state`: a sync-ordered loop prefix followed by audio-ordered execution, job
  emission and read hints. SP owns cursors, pulse flags and two planned
  transitions per loop.
- `transitions`: loop graph compilation and shared loop/operation/event types.
- `pipeline`: SP/JP/media coordination and fault containment.
- `jobs`: sample buffers, Q8.24 processing, smoothing histories and memory jobs.
- `fixed`, `records`: independent arithmetic/ramps and explicit read-only records.
- `storage`, `chunk_memory`, `resolver`: logical media ownership, slices,
  recording/readback, chunk storage and advisory read resolution.

There is no TP command admission, cancellation API, retained request table,
completion collection, ARM protocol or application backend. Native CPU request
processing remains in the production runtime. `serde` and `schema` describe
fixtures and values, rather than a transport contract.

## SP configuration and execution

`TransitionPlan::compile` compiles primitive-loop topology, timing and event
capacity; `sync_instructions()` emits source-before-dependent `ProcessLoop`
instructions. This is a graph compiler, not an execution processor.
`StateProcessor::with_loops(plan, memory)` initializes loop/node state.
`Program::sync_instructions` runs before `Program::instructions`, visiting every
configured loop exactly once in compiled order.

`sp.write_plans(loop_id, [plan0, plan1])` directly replaces the two physical plan
slots. `None` clears a slot. Each `PlannedTransition` contains `operation`,
`sequence` (priority), `effective` frame, `on_pulse` and `remaining_ticks`.
The caller prepares these values; SP performs no request allocation, timing
rounding or lifecycle management. The write validates identity, distinct plan
priorities, position alignment and availability of a sync source atomically.
A resolved slot can be reused on the next configuration boundary.

`sp.run` executes the complete sync/audio program. The prefix applies due
absolute plans, resolves wraps/repeat waits, propagates source pulses and applies
pulse-triggered plans. A zero pulse countdown fires on the next eligible source
pulse; a positive countdown decrements once per eligible pulse. At most one pulse
is delivered per loop per quantum. Self-sync runs locally after that loop's pulse.
Multi-loop dependency cycles are rejected with `InstantaneousDependencyCycle`.

Each `LoopSlot` exposes its two plans, `pulse_this_quantum`, next-boundary logical
`state`, and current-quantum `render` snapshot. SP saves the render cursor and
first-capture flag before advancing persistent position/capture count. The audio
pass uses this snapshot, including smoothing restart position. A cursor at content
length wraps in the following quantum's sync prefix. There is no post-JP SP pass.

`sp.resolutions()` contains `(target, plan_index, outcome)` for the last quantum;
it is cleared at the beginning of the next sync run. Outcomes are applied,
superseded, cancelled by an applied stop, or failed. These are SP execution
observations with the same plan-slot identity as RTL, not retained requests or
completions. `sp.events()`, `sp.frame()`, `sp.loop_state()` and `sp.loop_states()`
expose musical events and execution state. Event/outcome buffers are prepared
upfront and repeated execution does not allocate.

Supported quanta are 1/16/32/64/128. Loop lengths and operation positions must be
quantum-aligned. Composites, timelines, batches, program replacement and capture
monitoring remain outside this primitive-loop model.

## Audio and memory

SP emits ordered jobs before JP consumes them. Gain advances when its job is
emitted; failed later instructions preserve earlier writes/jobs. Isolated audio
fixtures can supply read-only logical records with `run_with_records` and omit
the sync prefix. Programs contain explicit audio instructions; there is no audio
graph compiler.

JP validates buffers and performs integer sample processing. `begin` resets
readiness; `reset` also invalidates smoothing history. `with_history_slots`
permits history capacity beyond the logical buffer count. The `Memory` trait
supports media-enabled and disabled configurations.

`Pipeline::prepare` initializes SP and FIFO, filling an omitted sync prefix from
the compiled graph. The caller supplies JP and memory. `write_plans` configures SP
slots directly; `process` handles one quantum and `process_block` handles whole
quanta. A processing failure latches its stage and silences the complete host
block. Cursors and plans may already have advanced; recreate the pipeline to
recover. Invalid host block shapes are rejected before execution without latching.

The compact RTL loop image still uses sixteen 64-bit words. The testbench codec
prepares its priority bit and compares slot-index outcomes directly. Its pulse
observer checks last-pulse history from actual events. There is no request-ID or
sequence-to-completion binding layer. Standalone SP/SP-JP implement the prefix;
media/AXI wrappers retain explicit-record fixtures.

These are functional models, not AXI/cache/DDR timing models. Physical allocation
order and delayed reclamation can differ while logical samples and ownership
remain equal. RTL transport, malformed packed instructions, bus timing and reset
remain component-harness obligations.

## Verification

```sh
cargo build --locked -p rt_fpga_reference --no-default-features
cargo test --locked -p rt_fpga_reference --no-default-features
cargo test --locked -p rt_fpga_reference --all-features
cargo test --locked -p rt_fpga_testbench --features sim --test sync --test reference --test state --test audio --test smoothing
```

`transition_traces` configures SP plans directly and compares boundaries with the
independent native resolver. It checks full loop state, events, active plans,
countdowns, outcomes and frame over 100 seeded sessions of 80 quanta. Directed
cases cover recording/repeat waits, same-boundary chains, self-sync, sparse source
IDs, wide priorities/effective frames and cycle rejection. Native request IDs are
used only inside that test oracle to observe native results. They are absent from
the functional SP interface.

`sync` checks autonomous evolution, reversed audio order, render/next cursor
separation, retake flags, plan-slot reuse, atomic configuration validation and
latched failures. `execution` retains no-allocation chains, recording-view reuse,
host-block silence, latched memory faults and recovery. `pipeline` compares SP/JP
with logical media and native loop results, including gain, arithmetic, smoothing,
seek, stop/restart, retake and retained slices. TP request/admission/cancellation
lifecycle tests are retired.

The Marlin `sync` target compares 1,152 directly configured Rust/RTL quanta and
120 combined SP/JP quanta exactly, plus malformed programs, generations, bounds
and wide counters/overflow. The remaining suites retain explicit record fixtures,
independent numeric vectors, RAM/handshake monitors, memory agents and DRAMsim3.
Chunk-memory and media ownership/slice/capacity tests remain in place.
