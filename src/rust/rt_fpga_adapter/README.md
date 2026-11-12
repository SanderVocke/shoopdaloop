# Queue consumers for FPGA demonstrators

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

## Primitive mono loop profile

`looping::LoopAdapter<D: LoopDevice>` adds a separate profile using the production
`LoopPlanner<Q824>` command/reply types and `primitive::Node<Q824>` processors.
The gain-only adapter above remains available. Both consume ordinary owning SPSC
queues; no hardware wire protocol or application backend switch is introduced.

Prepare one initially empty primitive loop with its real playback/capture bindings
and one instant gain node. Queue a production `PreparedPlan` commit for the fixed
`Input -> Capture`, `Playback -> Gain -> Output` graph. The adapter validates the
actual plan, topology, node identities and bindings, then uses the shared compiler
to install persistent SP instructions/state and TP configuration. Activation is
acknowledged after device loading succeeds. Bound loop nodes keep their original
generations. Live graph/node edits return `Busy`.

The supported subset is mono Q=32, chunks of 32 samples, with no MIDI:

- Record without monitor-previous, stop, and play from zero without retrigger,
  using `Timing::At` and quantum-aligned absolute runtime frames.
- Instant external gain changes, state queries, completed `Latest` and `Playback`
  exports, mono audio cursors, releases, and queued resource reads.
- Ordinary ticket cancellation, domain cancellation, future aligned operations,
  late delivery, and recovery-lane immediate stop/domain cancellation.

Other transition/timing/selection/resource operations are rejected explicitly.
TP executes transitions; SP/JP and device memory execute audio. No software loop
resolver, planner processing cycle or sample executor runs on the adapter path.
Only preparation, scheduling, protocol packing, handles and media leases run on
its service thread. Transitions and gains do not recompile or reset the program.

State/Export queries use TP command 16 to settle and pause before audio, matching
the production planner's query cut. The driver then resumes SP/JP and advances
clocks. Transport admission, delivery, TP admission and terminal domain replies
remain distinct. Runtime receipt ownership stays retained until completion.
State, revisions, transition events and terminal frames come from hardware.

Host-scoped, generation-checked media/export/snapshot identities correspond to
physical FPGA views. On a completed take, an immutable read-only slice retains
its chunk references before recording can clear the writable view. Playback and
latest take are tracked separately: restarting recording can publish a new latest
take without selecting it for playback. Export metadata follows production
capture-bank revisions, snapshot cuts and codecs; samples are read from RTL.
Cursors borrow media rather than export handles. Releasing an export invalidates
that export, while an existing cursor remains usable if playback/latest still
owns its content. Once all owners disappear, its view/chunks retire and the old
cursor fails even after slot reuse. Reads return their original owning buffer,
respect page boundaries, preserve unwritten bytes and handle partial words/EOF.

The Marlin device has 128 physical chunks, eight views, four TP loop slots,
eight TP request slots and sixteen events. This profile admits one loop, four
normal pending planner queries/transitions and one recovery request, up to four
exports, and a configured chunk limit in 1..128. Retained exports consume real
chunk capacity. Host leases permit the device's opt-in view reuse; raw view IDs
never cross this checked registry. The CPU oracle retains its elastic storage.
Exceeding an export/pending/buffer bound is an ordinary refusal; required audio
allocation failure is a sticky device fault. Driver disconnects/timeouts and RTL
faults terminate pending/future receipts and reject subsequent work. Reset and
recreate after faults; partial configuration is never acknowledged as active.

Loop metrics count compiled instructions, state words and TP/media configuration
payload separately from device configuration time. These are service-thread
measurements, not ARM callback or FPGA worst-case deadline guarantees. See the
simulator README for replayable queue-driven comparisons with production Q8.24.
