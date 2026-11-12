# Queue consumer for the FPGA gain demonstrator

`Adapter<D>` consumes the same `rt_runtime::executor::Control` requests as the
native `GraphRuntime`. It is an ARM/service-thread bridge: it may allocate during
validation and lowering, and it makes no service-time guarantee. Only `D: Device`
executes samples. The included recording double checks traffic and performs no
DSP. The optional `rt_fpga_sim` crate implements `Device` using Marlin.

The profile is Q=32, mono input/output, up to two `GainProcessor<Q824>` instances,
one initial commit at frame zero, and duration-zero typed gains at multiples of
32. Past aligned gains retain the existing scheduler's late-delivery semantics.
Unaligned controls are rejected with `InvalidFrame`; ramps, MIDI, resources,
staging and other operation families are rejected with `InvalidParameter`.
Live commits/installs/removals return `Busy`. IDs and epochs are checked before
activation. This is a typed whitelist, not a way to synthesize arbitrary Rust
processors.

Both runtimes declare stable slots during preparation. The common producer first
queues `Remove` then next-generation `Install` and `Commit`. Startup removals
return owned processors in ordinary completion bundles. Installs consume the
prepared processor only on success. No new wire protocol replaces the queues:
Rust objects and completion/retirement ownership remain on the service thread.
`Control::channel` factors only the existing queue/scheduler construction.

The lowerer accepts the actual ordered production descriptor and installed
processor descriptions. The adapter validates the `PreparedPlan` with production
`PlanValidator` before lowering and checks the small device capacities before
writing it. The fixture compiler uses this same lowerer, including its loop
instructions. This crate has no loop/media executor.

On activation the device receives persistent instructions, ramp state and an
empty logical transition topology. Activation is acknowledged after device load
succeeds. Gain delivery writes state through a stable binding and completes the
existing execution receipt. It does not compile/reload the program. A device
failure latches a fault; queued future gains are cancelled with terminal receipts,
and further commands are returned as rejected. A failed partial configuration
requires recreating/resetting the device; it is never acknowledged as active.

`Metrics` separates graph validation/lowering from device configuration time.
`image_bytes` counts configuration payload: 96-bit instructions, 129-bit state
rounded to bytes, empty TP spec/prepare records and media control initialization.
It excludes port control/framing, readbacks and clock waiting. The simulator
reports cycles separately. Neither host timing nor this small profile establishes
FPGA deadline or large-graph scalability.
