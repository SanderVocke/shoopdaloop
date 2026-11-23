# Shared graph compiler and state-program preparation

This `no_std` crate compiles `shoop_graph::GraphView` into a
`LogicalStateProgram`. It depends on the shared graph vocabulary, not the control
mirror, Rust processor model, RTL codecs, Marlin or runtime transports.

## Graph compilation

`rt_fpga_compiler::compile(view, CompileContext { quantum, limits })` captures one consistent
view. Logical IDs are names, not allocation addresses. Collection iteration order
does not select the execution order or change the generated program.

The compiler has three stages:

1. Validate identities, channel positions, connection directions, controls and
   references. Unsupported execution features retain an explicit feature error.
2. Expand channel connections into processing dependencies: input summation,
   strip input controls, per-channel playback and capture, loop gain/balance and
   smoothing, strip output summation/controls, buses and host output summation.
   Schedule loop synchronization separately, source before dependent; self sync is
   permitted. Playback reads existing media, so capture is a sink rather than a
   same-quantum playback dependency.
3. Resolve port aliases, remove unused computations and schedule dependencies
   deterministically. Identical computations share results. Assign scratch
   buffers and count emitted instructions, worst-case jobs, persistent values,
   histories, loop instances and host channels against target limits.

Each computed scratch buffer is written once per quantum, as required by both
job processors. There is no scratch reuse or in-place overwrite. A loop's first
channel meter remains readable after all jobs complete. An empty graph still
emits a silence instruction so it is an executable program.

Supported graphs have direct audio strips and loops, any channel count that fits
resources, multiple loops per strip, multiple buses, channel-specific fan-in,
fan-out and cross-channel connections. Gains, stereo balance, mute, manual input
monitoring and loop boundary smoothing use existing processor operations. Stereo
balance applies only to two-channel groups, matching the native engine. Explicit
loop input/output ports are also accepted; otherwise loops capture their strip's
processed input and contribute playback to its output. Unconnected inputs are
silence. No particular bus is required and Master is not hardwired.

`compile` and `compile_with_chunk_frames` accept primitive audio graphs.
`compile_composites` also accepts the bounded composite programs described below.
All three refuse MIDI routing, dry/wet or hosted processors, and automatic
monitoring. Graphs may include unused host MIDI ports. Audio dependencies that
cannot be scheduled within a quantum and cyclic sync chains are invalid.

## State identity and loading

Generated state bindings identify an owning strip, loop or bus, a purpose and a
channel. Loop gain is one shared constant control; each media channel and
smoothing history has its own identity. Immutable gain coefficients include their
value in their identity. Changing a strip or bus control therefore allocates new
configuration state without overwriting an active value. Unchanged loop gains,
media and histories retain their physical slots, even when unrelated objects or
scratch assignments change. Changing a continuing loop's media channel set
requires a new loop identity; the loader refuses an incompatible carry-over.

`loading::Allocator` retains one current and one pending layout. New allocations
use unused slots; old-only slots stay pinned until the switch fence completes.
Cancellation cannot make an old revision current again. Every logical program
supplies explicit state ownership bindings. Sync and scratch/history wiring lives
in instruction operands.

`Layout::media_views` maps each logical media index to its allocated state address.
The reference, worklet and Marlin loaders prepare only fresh media views. Their
media-view tables are dimensioned to the state-value pool, independently of the
number of loop cursors. This permits several media channels to share one loop
cursor. The frame clock, state pools and media survive graph switches.

## Target lowering

`StateProgramTarget` supplies limits and an associated compiled result. The same
logical program goes to both existing targets:

| Target | Implementation | Result |
| --- | --- | --- |
| Rust | `rt_fpga_reference::target::RustTarget` | Typed instructions, state values, loop plan and bindings |
| RTL | `rt_fpga_rtl::target::RtlTarget` | Packed instructions, state words, loop RAM images and bindings |

Targets resolve logical media indices using their supplied `media_views`. Both
`build_image` methods accept the ARM layout and emit fresh-state initializers.
The RTL target also supports a nonzero state base. The instruction set, packed
formats, processor RTL and switch/fence protocol are unchanged.

There is one compiler entry point: `rt_fpga_compiler::compile` takes a `GraphView`
and a `CompileContext`. The Rust and RTL target modules encode its shared program;
they are not independent graph compilers. Supported quanta remain
1/16/32/64/128, subject to target capacities.

Raw instructions for processor, numeric and relocation checks live only in test
support. These vectors do not accept graph descriptions or provide compilation
APIs. Compact protocol loop records expand to a full `shoop_graph::Graph` before
entering the same compiler as complete graph uploads.

## Verification

Graph tests cover multichannel layouts, independent sync scheduling, arbitrary
routing, deterministic output, single-write scratch and exact resource limits.
Reference tests execute recording/playback and live replacement while preserving
all channels. An independent native comparison compiles the actual control
mirror and compares monitoring, gain, balance and multiple-bus routing samples.
RTL tests compare multichannel job streams, smoothing and actual audio samples.
A complete stereo upload/record/play/reload scenario runs through both the Rust
reference and Marlin carrier/renderer pair. Existing raw processor, loading,
native resolver and application tests remain in place.

```sh
cargo test --locked -p rt_fpga_compiler
cargo test --locked -p rt_fpga_reference --all-features
cargo test --locked -p shoop_rt_backend --test targets_native
cargo test --locked -p rt_fpga_testbench --features sim --test graph_compiler --test targets --test loading
cargo test --locked -p shoop_rt_host_marlin --features sim
```

See [the loading design](../../../LOADING_DESIGN.md) for the ownership contract.

## Composite audio programs

`compile_composites(view, context, chunk_frames)` lowers prepared, quantum-aligned
regular and explicit composite programs. It covers empty regular composites,
nested playback and seek, shared children with a deterministic action priority,
and authored Play, Stop and Record on Direct audio loops. Children may have
their own sync source. The compiler checks the combined acyclic action and sync
graph. Composite slots own timing only.

Both targets charge span rows and parent headers to the instruction budget.
A target can have up to eight parents and each parent-child relation can have
up to 32 spans. A composite's duration and unit fit in 32-bit frame counts.
Capture uses existing primitive views and jobs. Rust preflights the selected
quantum's writes, while RTL checks conservative capacity for all compiled
audio capture lanes. Near capacity, RTL can refuse work that would fit the
actual selected writers.

The prepared graph transport carries the same programs to both hosts. It
publishes the active version and settled transport state. Low-level plans
provide frame-addressed seek and scheduling. The hosts' immediate transition
commands support direct changes on composite timing slots at the processing
boundary. The UI editing capability remains separate. Authored MIDI capture,
Replace, dry/wet modes and graphs with inter-loop timing cycles are outside
the current FPGA profile.

The existing loader stages edited tables in the inactive program region. It
checks affected loops before switching and retains displaced allocations until
the memory fence. Supported cuts are stopped, a compatible independent restart,
and a future-only edit with the next unit protected. A prepared host can hold a
single compatible switch request until the next restart. The caller can retry a
candidate after a refused cut. Edits that change timing geometry require a
stopped cut.

```sh
cargo test --locked -p rt_fpga_reference --all-features
cargo test --locked -p rt_fpga_testbench --features sim --test graph_compiler
cargo test --locked -p shoop_rt_host_marlin --features sim --test graph_compiler
```

## Mixed MIDI work

`LogicalStateProgram::work_pass` contains both audio and MIDI operations.
`with_midi(&[MidiBinding { loop_id, context }])` appends ordered MIDI operations
to an audio graph's compiled program. Context slots are prepared external MIDI
resources, independent of relocated loop/state addresses. Contexts determine
MIDI tie order and remain stable across image replacement; loop operands are
relocated by the normal allocator. Input/destination indices and MIDI channel
masks remain prepared reference-session bindings. General MIDI strip/bus graph
routing and application enablement remain separate work.

The compiler checks phase order, unique bindings, operand bounds and conservative
queue capacity (`4 + 8 * MIDI bindings` additional jobs). Unsupported targets
reject these operations explicitly. Rust and packed RTL lowering emit MIDI instructions;
the normal state processor derives concrete capture/playback jobs from settled
loop state. It never delegates a whole quantum to another MIDI scheduler.

Calling `with_midi(&[])` explicitly enables input/history and maintenance jobs
without bound loops. Omitting `with_midi` preserves an audio-only program.

The RTL target packs `ProcessMidi` as SP opcode `0x0e`; the shared JP dispatches
job kinds `0x20..0x2a`. [The execution contract](../../../docs/fpga/midi-shared-jobs.md)
describes settled operands, generation checks, queue limits and publication.
