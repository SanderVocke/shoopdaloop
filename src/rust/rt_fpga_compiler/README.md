# Shared state-program preparation

This `no_std` crate contains the target-independent state-program description and
the `StateProgramTarget` trait. It has no dependency on the Rust processor model,
RTL codecs, Marlin, application graphs or runtime transports.

`FixedProgram::emit` retains the prepared fixture configurations:

- `MonoLoops`: one or two initially empty mono loops, one input, a gain and output
  per loop, logical playback/capture, and loop 1 synchronized to loop 0.
- `SyncSmoothing`: the existing single-loop sync/input/gain/smoothing/output
  configuration, including its initial ramp and smoothing controls.

Both support Q=1/16/32/64/128. Target capacities can impose a smaller envelope.
`graph::GraphDescription::compile` also accepts one to sixteen initially empty
mono loops. It schedules a source-before-dependent sync pass independently of the
audio pass, rejects cycles except self sync, and emits input/silence, playback,
gain, capture, summation and stereo Master output operations. Every buffer is
written once per quantum, including intermediate sums. Input and output routing
are explicit per loop. This restricted graph has no processors, MIDI or smoothing.

The emitted `LogicalStateProgram` describes semantic operations, logical state
and buffer slots, loop identities/sync relationships, initial values, resource
requirements and control/meter bindings. It contains no packed instruction words
or physical state addresses. Validation checks the prepared resource envelope,
source-before-dependent sync order and referenced state/buffer types.
The logical representation also permits a loop-free program, used to retain
processing time while no loop states are owned.

`StateProgramTarget` supplies limits and an associated `CompiledProgram` type.
`compile(fixed, target)` emits, validates and builds one fixture. Call `emit` once
and pass the same logical program to multiple targets when comparing executions.

| Target | Implementation | Compiled result |
| --- | --- | --- |
| Rust | `rt_fpga_reference::compiler::RustTarget` | Typed SP program, initial values, prepared loop plan, resources and bindings using Rust addresses/buffer IDs |
| RTL | `rt_fpga_testbench::compiler::RtlTarget` | Packed 96-bit instructions, 129-bit state values, sixteen-word loop images, resources and physical bindings |

Media-view values are resolved against externally prepared `media_views` passed
to each target. Missing views and insufficient capacities fail before processor
preparation or RTL loading. The RTL target can place state at a nonzero
`state_base`; it relocates instruction operands and returned bindings together.
There is no binary-compatibility requirement between the compiled results.

`loading::Allocator` binds logical instances against current occupancy. Continuing
instances retain their loop/value/history allocations and receive no initializers.
New instances reserve unused slots; old-only slots remain pinned until the switch
fence completes. A layout carries concrete addresses, generations, current/next
revisions and the inactive instruction region. Both targets' `build_image` methods
accept that layout and produce sparse fresh-state writes. Sync and scratch/history
wiring lives in program operands. See [the loading design](../../../LOADING_DESIGN.md).

The reference carrier tests now build `MonoLoops` through `RustTarget` and retain
their independent production-native comparisons over 640 callbacks. The combined
SP/JP RTL test builds `SyncSmoothing` through both targets and compares 120 sample
quanta. A separate RTL test builds `MonoLoops` through both targets and compares
96 SP quanta. These RTL tests deliberately use different state addresses from
the reference. Packed target output is loaded directly into configuration RAM;
it is not reconstructed from the Rust instruction representation. Existing raw
vectors and native-resolver tests remain independent checks.

Compilation/preparation stays separate from `carrier::Endpoint` and processing
pacing. The temporary reference host receives graph metadata as one bounded
immediate group from `RtBackend`, builds this logical graph and hands the Rust
configuration to its independently paced worker for initial and live installation.
The RTL target accepts the same graph and shares the ARM allocator. Marlin's
program upload and switch-FIFO bridge exercise live instruction regions and the
media completion fence; its complete runtime-command/snapshot frontend remains
separate work.

```sh
cargo test --locked -p rt_fpga_compiler
cargo test --locked -p rt_fpga_reference --all-features --test rt_interface
cargo test --locked -p rt_fpga_testbench --features sim --test compiler --test sync
```
