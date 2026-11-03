# Scoped processing and shared loop plans

`rt_runtime` can subdivide a loop region while invoking a downstream DSP processor
once for the actual host callback. A separate callback-planning profile also lets
playback and capture surround DSP without a second musical clock or an added block
of scheduling latency.

The [execution contract](scoped-processing-contract.md) defines admission,
activation, failure and ownership rules. The [implementation plan](scoped-processing-plan.md)
records validation and delivery. This foundation does not enable production bus
FX slots, Carla, application dry/wet modes or the separate remote DSP stack.

## Choose the execution profile

| Requirement | Profile |
| --- | --- |
| Existing planner, including decisions based on actual processing observations | Default interleaved planner; interval processors form one child region |
| Full-block DSP before or after that region | Override `Processor::cadence()` with `Cadence::Callback` |
| Playback before DSP and capture after DSP, sharing timing | `runtime::callback::CallbackPlanner` and its callback media nodes |

The default remains `Cadence::Interval` and `PlanningMode::Interleaved`. Existing
processors and applications retain their execution profile. Forward `cadence()`
when wrapping a processor in an enum or adapter; `Box<P>` already forwards it.

An interleaved region is atomic in its parent dependency graph. Interval playback
feeding callback DSP is valid. Interval playback feeding callback DSP and then
returning to interval capture is refused as a scope contraction cycle. For that
second path, use the shared callback planner: its playback and capture consumers
apply slices of the same prepared timing plan in ordinary signal order.

Callback cadence names the root host callback, including irregular callback sizes.
It does not convert a plugin's required quantum or add an internal buffering
pipeline. A callback processor cannot read transient interleaved decisions or
publish interval observations. Callback planners admit callback consumers only.

`Catalog::with_dependencies` declares fixed readiness edges separately from signal
connections. `GraphRuntime::prepare_dependencies` installs them before graph
publication; compile using the resulting catalog. They consume the shared prepared
edge allowance and incremental validation work. Missing predecessors, stale
generations and combined signal/control cycles are refused. Their node generations
remain fixed until stopped re-preparation; removing one is refused with `Busy`.
Musical sync relations remain the loop resolver's separate responsibility.

## Assemble shared playback and capture

Prepare an indexed resource store with coordinated capture groups, imported audio
and optional MIDI snapshots, a primitive-loop topology, and one `Binding` per
primitive. Each binding names distinct playback and capture NodeIds. The same
LoopId owns both media phases and advances once. Playback selects the retained
import; completed capture becomes `latest` without replacing that playback source.

`CallbackPlanner::prepare` returns its planner and media nodes. Wrap those nodes and
your DSP in one application processor type, forwarding ports, planning access and
cadence. Add signal edges `playback -> DSP -> capture` and optional DSP host-output
edges. Pass the planner's `bindings()` into
`GraphRuntime::prepare_with_initial_graph`. The runnable example contains a complete
assembly and a second assembly using the existing interleaved `LoopPlanner`.

The first shared profile supports primitive playback timing, seeks, stops and sync,
plus explicit capture windows. Composite/application mode restoration is later
integration work; the existing interleaved loop API retains those capabilities.
Only callback-boundary planner operations are admitted, but their payload can name
future transition frames, resolver timing rules and capture windows starting or
ending inside a callback.

`Operation::Capture { target, start, end }` stays pending until actual media
completion or refusal. Capture starts obtain the routed DSP MIDI state immediately
before `start`. Input readiness therefore precedes take preparation/reservation.
Audio and MIDI are staged into one group and commit together only after successful
consumer execution. A callback can contain many loop wraps without repeating that
group publication. Capture may continue across callbacks.

`Reply::Captured` gives the effective window and MediaId. Its terminal receipt may
arrive at the enclosing callback end; an accepted request is not successful
capture. Export with `Operation::Export` to obtain an owning ExportId. Retain that
owner while reading the returned media and explicitly release it afterward.
Copied IDs, sources and read indices do not create leases. Old exports remain
readable when later capture fails.

Prepare the constructor in a startup resource store; it creates domain resources
there. Discard the construction store on a constructor error. Render-side capture
admission has its own atomic refusal guarantees. After callbacks stop, consume the
runtime with `into_planner_and_resources()` and call the planner's `release_all()`
to release leases, groups and its capture resources.

## Bounds and failures

The shared adapter requires `span_limit == config.limits().max_frames`. Every
owner reserves one span per possible frame boundary before rendering begins;
undersized preparation fails rather than risking partially advanced timing. On
the evidence host, 32 spans occupy 8,704 bytes per owner. The general storage cost
is `owners * max_frames * size_of::<DecisionSpan<CallbackDecision>>()`, in addition
to prepared decisions, MIDI trackers, resource groups and processor buffers.

Audio buffers reuse the existing callback stride. MIDI accumulation is allocated
only for runtimes registering callback processors. Ordinary interval-only runtimes
avoid that additional storage. Crossing a scope translates MIDI offsets once and
uses callback-wide event/byte bounds. Actual capture-window event/byte capacity is
checked before activating the recording take.

Rendering and fault cleanup use prepared storage and the same outer ingress,
transfer, validation, event and recovery allowances. Required resource reads and
writes retain their independent work accounting. Consumers neither renew budgets
nor allocate, release heap owners, lock, wait or perform I/O.

After a post-publication fault, outer time advances once. Uncommitted capture is
cancelled; failed requests never return successful capture. Later callbacks do not
replay the failed block. MIDI overflow uses reserved reset traffic and refuses
capture of a truncated stream. Hosts must drain `drain_midi_recovery()` into their
separate recovery sink; ordinary MIDI remains suppressed while resets are pending.

Interior scheduled commands targeting callback processors/planners return
`InvalidFrame` at their exact delivery frames. Use host MIDI buffers for
sample-offset event streams. Interleaved region commands preserve their existing
sample-accurate cuts. Mid-callback activation can change internal child routing
while preserving every callback node and parent-facing connection. Parent changes
are refused; displaced plans stay owned by prepared stages until off-render
retirement. See the full matrix in the contract.

## Reproduce the proof

Run from the repository root in the environment described by the repository build
instructions:

```sh
cargo run --locked -p shoop_loops --example scoped_processing
cargo test --locked -p rt_runtime --test scoped_processing
cargo test --locked -p shoop_loops --test callback_runtime
cargo test --locked --release -p rt_runtime --test scoped_processing \
  -p shoop_loops --test callback_runtime
```

The example checks independent audio/event oracles, real captured samples and MIDI
checkpoints, and prints FX `(absolute frame, call size)` records, timing work and
plan storage. It exercises both signal paths, irregular callbacks, an origin above
32 bits, one-frame loop boundaries and two synchronized owners. It needs no audio
device, plugin, network connection or host facilities.

The tests add actual capture exhaustion, retained exports, failed/missing consumer
completion, reserved MIDI recovery, incompatible scopes, declared dependency
cycles, exact-frame child activation, stale epochs and guarded owner retirement.
Allocator guards cover successful and failing callbacks. These are correctness
proofs, not physical-device latency or worst-case execution-time measurements.

When manually constructing a `ProcessBlock`, initialize its new borrowed `spans`
field (`&[]` for an ordinary interleaved block). Nonzero absolute origins must be
prepared consistently on both the runtime and the timing owner before requests
or callbacks begin. Portable library builds remain independent of Linux, sockets,
threads, Carla and legacy engine crates.
