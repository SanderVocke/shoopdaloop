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

The mono profile uses a 128-chunk logical arena within the shared Marlin model,
with four TP loop slots, eight TP request slots and sixteen events. It admits one loop, four
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

## Memory reporting

After activation, `LoopAdapter::memory_report()` queries the device between
quanta and returns the shared backend `MemoryReport`. The simulator reads actual
RTL allocation counters through read-only configuration space 6, addresses 16
(usage) and 17 (geometry); Rust does not
infer occupancy from recording lengths or host leases. Sample storage is measured
in bytes using the configured block geometry and logical limit. Metadata pages
and view slots have independent capacities. Queries preserve the audio frame,
media state, and compiled program, and are refused before activation or after an
adapter fault. Unsupported device implementations can refuse the query.

The FPGA report deliberately leaves the aggregate accounted byte count absent.
The shared UI then displays the largest individual pool commitment as a percentage,
with a resource breakdown in the tooltip/profiling view. This measures storage
pressure, not a percentage of total RAM or remaining recording duration. Retained
exports and unused block tails still occupy physical capacity.

The mono replay includes these reports in each `result.json` frame's `memory`
entry, with readable pool names, counts, limits, and `pressure_percent`. Readback
advances device service clocks, not audio frames; replay clock totals include
that diagnostic work. A host can poll at a lower rate such as 5 Hz. The existing
mono adapter remains independently available alongside the application profile
below. Memory readback does not introduce a physical board transport.

The application adapter offers the same `memory_report()` query and publishes it
itself between quanta: once after configuration activation, then every
`sample_rate / 5` frames, matching the native runtime. Reports carry the
adapter's frame because the device counts from the latest program load. A
refused or invalid readback is skipped rather than faulting audio; the next
interval retries it.

## Default application Sync-loop profile

`engine::Adapter<D: EngineDevice>` consumes the production engine's actual
`EngineNode`, `LoopPlanner` and `EngineOp` queues. Production preparation supplies
the endpoint, node descriptions and observation cells through `external::Preparation`.
It does not construct a CPU graph executor or run the native loop planner.

This fixed audio-only session has one mono primitive Sync loop, one strip,
stereo Master, two prepared audio channels and no MIDI/FX. The compiler lowers
the submitted ordered graph and staged topology, including strip and bus stages,
240-frame control ramps and 144-frame restart smoothing. TP executes immediate
and scheduled record/play/stop, self-sync/repeat and clear; SP/JP execute samples.
Gain, balance, mute, monitoring and mode changes update state without recompiling.
Permitted routing activations install a new signal program while retaining TP
time, media and smoothing history. General session replacement remains unsupported.

The existing endpoint, owning queue protocol, receipt cuts, graph validation and
callback-to-quantum buffering are reused. Configuration/activation is acknowledged
after the device succeeds. Node installs and arbitrary Rust processors are refused.
The adapter retains staging owners, generation-checked media leases and bulk-read
bookkeeping on the host. Waveform bytes come from FPGA media and convert Q8.24 to
the app's float format. Faults terminate pending receipts and publish failed state.

The optional `rt_fpga_sim` application composition runs continuously as fast as
possible, without audio deadlines or rate pacing. Deterministic tests use stepped
batches. Its larger external-memory preset admits two five-second takes with the
previous waveform retained; smaller gain/mono fixtures retain their bounded
logical presets. See the simulator README for offline GUI and comparison commands,
capacity details and the remaining metadata-walk timing limitation.
