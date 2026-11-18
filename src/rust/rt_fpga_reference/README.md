# Rust functional processor reference

Functional models of the retained state, job and memory processors, restored from
`354305d4697d2e6068e28d4a2e369f08974e434d` and extended with SP-owned loop
execution. Synchronization and cursor evolution execute in SP instructions;
external commands enter through the optional RT interface. There is no separate
transition processor.

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

The core models have no TP command admission, retained request table or old ARM
protocol. The optional `rt-interface` implements the current command/response and
snapshot profile, and an optional `shoop_host` profile provides restricted
application integration. Native CPU request processing remains in the production
runtime. `serde` and `schema` describe fixtures and values, rather than a transport
contract.

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
quantum-aligned. Composites, timelines and capture monitoring remain outside this
primitive-loop model. Program replacement is coordinated by the loader described
below; SP does not allocate or manage program lifetimes itself.

## Shared program preparation

[`rt_fpga_compiler`](../rt_fpga_compiler/README.md) supplies the shared logical
program and `StateProgramTarget` trait. Its single graph compiler consumes the
full `GraphView`, supporting direct multichannel loops and strips, multiple buses,
channel routing, gain/balance/mute, manual monitoring and boundary smoothing within
target resource limits. `target::RustTarget` encodes the typed SP program, initial
state, loop plan, resources and bindings. Media-view handles are supplied by the
caller after preparing media resources. Raw instruction vectors live in test support.

The carrier fixtures use this preparation path for their mono loop programs and
derive their gain/meter bindings from its result. RTL targets consume the same
logical definitions and produce their own packed instructions and physical state
layout. Neither target requires binary compatibility with the other.

## Internal RT interface

Enable `rt-interface` for a compiled primitive-loop graph. The public control
endpoint is `rt_interface::Endpoint`, a re-export of the concrete queue-backed
`shoop_rt_transport::rt::Endpoint`. It implements `carrier::Endpoint` directly
and works with `ProtocolClient`, `InProcess` and record carriers. There is no
additional control-port trait. The native engine uses the same in-memory channel
constructor while retaining its existing control endpoint and execution handlers.

Control and execution have separate owners. `Endpoint` delivers messages and
polls available responses/publications; `Processor` runs the SP/JP pipeline only
when its driver calls `process` or `process_block`. Neither polling nor client
connection lifetime advances processing. Frame numbers in responses and snapshots
are observations, not pacing controls. Driver/sample-rate metadata describes the
configuration and limits push snapshot delivery; it does not schedule processing.

Prepare the `Pipeline`, explicit SP program, loop bindings and initial memory,
then construct the two owners:

```rust
use shoop_rt_transport::{carrier::{InProcess, Outbox}, client::ProtocolClient, rt};
use shoop_rt_protocol::{immediate::FIFO_RECORDS, snapshot};
use rt_fpga_reference::rt_interface::{Endpoint, Processor, Settings};

let capacity = snapshot::capacity(0, bindings.len(), 0).unwrap();
let (control, execution) = rt::channel(FIFO_RECORDS, 10, capacity)?;
let endpoint = Endpoint::new(control, Settings {
    graph: configuration.graph,
    topology: configuration.topology,
    limits,
    driver,
})?;
let mut processor = Processor::prepare(
    pipeline, execution, configuration, bindings,
)?;
let mut client = ProtocolClient::new(InProcess::new(
    endpoint, Outbox::new(256, 8, 0),
));
```

`Configuration` supplies the initial graph/topology epochs and the aligned initial
frame. Loops start stopped with the lengths configured in the graph. A
`LoopBinding` supplies the loop identity, an optional gain-ramp address, an
optional mono meter buffer and initial gain/timing metadata. The processing driver
supplies JP, media and fixed-point samples to `Processor`. The control endpoint
requires matching epochs, prepared limits and descriptive driver status.

Live loading uses `loading::Allocator`, `RustTarget::build_image`, and
`rt_interface::program_memory`. A `PreparedProgram` contains a bound program,
fresh-slot initializers and ARM-only bindings. The processing side incrementally
prepares unused media views, then reports readiness. Publication freezes the image;
`SwitchProgram` is ordered with ordinary commands at a quantum boundary. Continuing
state, recording media and DSP history stay at their existing addresses. Retired
programs return through a bounded queue for destruction on the control thread.
Completion waits for the memory fence and never follows merely from a snapshot
showing the new revision. See [the implementation notes](../../../docs/fpga/live-loading.md)
and [the loading contract](../../../LOADING_DESIGN.md).

The endpoint handles `Hello`/`Goodbye`, immediate command groups and snapshot
observation, push/pull configuration and pull requests. It intentionally ignores
legacy `Request`, `Resync` and `Bulk` messages, advertises no legacy capabilities,
and announces zero request/bulk credits. Unsupported commands inside an immediate
group receive individual `Unsupported` results. Command responses retain FIFO
credits until they can enter the carrier's reliable outbox; a full outbox leaves
the response unread. Snapshot transfers copy one complete publication before
chunking it, so later processor publications cannot change an in-flight reply.

Supported commands are `LoopGain`, `Transition` (stop/play/record, including
`now` and two future steps), and `ClearPlannedTransitions`. Unsupported operations
produce `Unsupported` responses. Groups validate configuration/deadline first;
commands then execute in order and report individual outcomes. A command failure
does not roll back earlier commands or suppress later commands. `Scheduled` means
future steps remain installed, rather than that those steps already executed.

The interface's future steps use SP-owned slots with ordered pulse semantics:
all steps observe the same pre-transition projected source pulses, positive skips
decrement, and due steps execute first/second in order. Self clocks fall back to
the target loop. An invalid scheduled transition clears its future steps and
increments the snapshot failure count. Immediate recording is idempotent while
already recording, and scheduled playback preserves an already playing cursor.
The earlier sequence-arbitrated plan API remains available for the existing RTL
component fixtures; both profiles share the same two physical plan slots.

One call processes an integral number of quanta. The command publication cutoff
is taken once at callback entry. Responses use the first processing frame;
commands published during processing wait for the following callback. Response
space is reserved at submission and released when consumed. SP's first settled
state is published before JP runs; later quanta in the same callback do not replace
that snapshot. Meters accumulate across the callback for the next publication.
Snapshots retain complete-state/latest-publication semantics under reader stalls.
Invalid callback shapes preserve pending commands. A pipeline failure latches,
marks publication failure, silences output and makes later groups return
`CannotApply`; computed state and accepted commands are not rolled back.

This endpoint supports unedited audio media through channel-specific views.
Topology changes use a staged program and ordered switch, rather than mutating the running graph in place.
Immediate balance and strip/bus commands, MIDI, grabs/history and arbitrary media
edits remain unsupported. Balance and strip/bus controls can be part of compiled
graph configurations. Native physical capture-resource preflight is not modeled;
media/job failures use the reference pipeline fault contract. Arbitrary partial
host callbacks and native host buffering are outside this endpoint. The endpoint
supports the existing carrier protocol subset. The optional `shoop_host`
`fpga-reference` feature uses the shared graph compiler and automatic host-managed
activation. The UI can operate multichannel loops and strips within resource
limits. Dummy, CPAL and JACK drivers supply timing and audio according to build
features; the current renderer exposes one capture and two playback channels.
See the [host instructions](../shoop_host/README.md#rust-fpga-reference-with-the-ui)
for startup and supported controls. Application clients and UI-runtime tests
exercise live graph replacement through the outward-facing socket API.

The `rt_interface` tests compare responses and all four loop snapshot record
classes through the carrier endpoint exactly against the production native
endpoint over 640 callbacks at Q=1/16/32/64/128, including callbacks with multiple
quanta and varied command groups.
Other checks cover exact recording/playback/gain samples, callback cutoff,
reserved response capacity, invalid groups/values, reader stalls, allocation-free
scalar processing, latched faults and processing after the client is dropped.
Transport tests additionally cover reliable response stalls, coherent multi-chunk
push/pull snapshots, rate limits, version rejection and reconnects. Run:

```sh
cargo test --locked -p rt_fpga_reference --all-features --test rt_interface
cargo test --locked -p shoop_rt_transport --features rt-interface --test rt_endpoint
```

Rust reference, browser reference and Marlin hosts share ARM-side graph
compilation/allocation and use the same command/response queues and snapshots.
Marlin encodes the program with `RtlTarget` and drives the RTL command/snapshot
frontend. Driver-owned rendering remains independent of carrier delivery and
polling; Marlin serializes simulator operations on one owner thread.

## Audio and memory

SP emits ordered jobs before JP consumes them. Gain advances when its job is
emitted; failed later instructions preserve earlier writes/jobs. Isolated audio
fixtures can supply read-only logical records with `run_with_records` and omit
the sync prefix. Programs contain explicit audio instructions; there is no audio
graph interpreter in the execution model; the shared compiler emits these
instructions before loading.

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
