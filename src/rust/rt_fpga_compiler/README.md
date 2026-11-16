# Shared state-program preparation

This `no_std` crate contains the target-independent state-program description and
the `StateProgramTarget` trait. It has no dependency on the Rust processor model,
RTL codecs, Marlin, application graphs or runtime transports.

For now, `FixedProgram::emit` produces prepared fixture configurations only:

- `MonoLoops`: one or two initially empty mono loops, one input, a gain and output
  per loop, logical playback/capture, and loop 1 synchronized to loop 0.
- `SyncSmoothing`: the existing single-loop sync/input/gain/smoothing/output
  configuration, including its initial ramp and smoothing controls.

Both support Q=1/16/32/64/128. Target capacities can impose a smaller envelope.
There is no graph-description input, graph scheduling algorithm or runtime graph
upload protocol. Sync/audio order and initial settings are fixed by the fixtures.

The emitted `LogicalStateProgram` describes semantic operations, logical state
and buffer slots, loop identities/sync relationships, initial values, resource
requirements and control/meter bindings. It contains no packed instruction words
or physical state addresses. Validation checks the prepared resource envelope,
source-before-dependent sync order and referenced state/buffer types.

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

The reference carrier tests now build `MonoLoops` through `RustTarget` and retain
their independent production-native comparisons over 640 callbacks. The combined
SP/JP RTL test builds `SyncSmoothing` through both targets and compares 120 sample
quanta. A separate RTL test builds `MonoLoops` through both targets and compares
96 SP quanta. These RTL tests deliberately use different state addresses from
the reference. Packed target output is loaded directly into configuration RAM;
it is not reconstructed from the Rust instruction representation. Existing raw
vectors and native-resolver tests remain independent checks.

Compilation/preparation stays separate from `carrier::Endpoint` and processing
pacing. Future graph communication can supply the compiler input without changing
the execution endpoint or making message polling advance processing.

```sh
cargo test --locked -p rt_fpga_compiler
cargo test --locked -p rt_fpga_reference --all-features --test rt_interface
cargo test --locked -p rt_fpga_testbench --features sim --test compiler --test sync
```
