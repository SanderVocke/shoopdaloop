# Real-time state snapshots

## Goal

Publish complete, self-consistent snapshots of application display state directly
from the audio callback's real-time processing owner to the control side. Control
then forwards accepted snapshots to the UI. The UI consumes the newest complete
state instead of repeatedly querying individual objects.

This is an incremental migration. First implement publication alongside the
existing communication layer. Then move UI observation to snapshots. Keep the
entire existing command, completion, execution-result, telemetry, query, and
ownership-return machinery operational during both phases.

The longer-term direction is fire-and-forget delivery for suitable UI commands.
Changing command semantics is a separate phase, not a prerequisite for snapshots.

## Implemented scope and review map

Both snapshot milestones below are implemented. The UI adapter uses snapshots as
its only observation source for ordinary loops and mixers; the legacy display
selector has been removed. `SnapshotDelivery` still selects push or pull delivery.
Existing raw protocol clients may use the compatibility telemetry path until they
opt into snapshots. Commands, reliable results, exact queries, media, composite
and plugin observation, and ownership reclamation retain their existing protocols.

Control refreshes its timing cache directly from validated, epoch-matched ring
copies on every control poll, independently of connection state, UI requests,
delivery mode, and push rate. It updates matching active and staged loop entries
without regressing timing/content revisions. Input completion waits for cached
state captured after the operation's effective frame instead of sending a timing
query. Reconnection uses that cache. The background `TimingSnapshot` request and
input timing-query steps are removed. The graph-setup query that obtains a media
identity for retirement remains, because scalar snapshots do not transfer media
lifetimes. Bootstrap configuration resynchronization also remains.

The implementation is separated into these modules:

- `rt_runtime::snapshot`: fixed-slot ring, atomic payload words, publication and
  validated copy, and the generic settled-state observer interface.
- `shoop_rt_engine::snapshots`: application capture, prepared storage, configuration
  association, and control-side projection.
- `shoop_rt_protocol::snapshot`: schema version 1, fixed-width little-endian header,
  bounded record encoding, and transport chunks. Engine protocol version is 28.
- `shoop_rt_backend::snapshots`: bounded assembly, whole-publication validation,
  application to the UI mirror, and observation selection.

The scalar field inventory for this migration is:

| Group | Snapshot fields and real-time source |
| --- | --- |
| Header | Capture audio frame, active graph/topology epochs, schema version, record and object counts; runtime and settled resolver. Publication sequence is ring/transport metadata. |
| Ordinary loops | Identity/generation, gain, balance, repeat-sync; loop planner rows and resolver state. |
| Loop transport | Current mode, pending mode/delay, recording-armed and capture-incomplete flags, position, length, content and timing revisions; settled resolver and pending transitions. |
| Loop media metadata | Raw audio/MIDI lengths and channel playback windows, including per-channel gain, offsets, alignment, preplay, and optional raw lengths; planner cut. No media bytes or owning leases are transferred. |
| Loop meters | Per-channel peaks from the fader's independent snapshot accumulators. |
| Strip controls | Input/output gain and balance, output mute, manual monitoring, automatic-monitoring policy and actual demand; settled strip processors. Values describe applied control targets, rather than intermediate ramp samples. |
| Strip meters/activity | Input/output channel peaks, MIDI input/output activity and latest short input message; independent snapshot accumulators and RT-written observations. |
| Buses | Identity/generation, gain, balance, mute, and per-channel peaks; active bus processors. |

Composite program state, inserted processor/plugin state, host driver/load/xrun
diagnostics, memory reports, preview state, and profiling remain on their existing
paths. Host diagnostics are not represented as a coherent RT snapshot. The audio
frame in the header is the snapshot clock. Composite observation also remains
legacy when snapshot observation is selected for ordinary loops and mixers.

Capture runs at the first settled tick processed by each host callback, after
activation-dependent processor configuration and monitoring settlement and before
audio processing for that tick. Callbacks that merely buffer input publish no new
state. Meters/activity cover the processing interval since the preceding capture;
they have independent accumulators so legacy reads cannot consume them. A skipped
publication can lose an intermediate meter peak, as expected for this lossy path.

Prepared capacity is `44 + 128 + 512*strips + 1024*loops + 256*buses` bytes per
slot, using the engine's existing fixed pool limits. Existing admission checks
refuse objects beyond those pools before activation. Ten slots are allocated;
there is no live resizing. The schema includes existing fixed-capacity playback
windows, rather than the proposed stereo-only compact encoding. The 64-loop,
eight-strip, one-bus smoke session produced 28,081 bytes, within a 70,060-byte
prepared slot. One debug-build control copy took approximately 223 microseconds
on the development host; this is a single observation, not a deadline guarantee.

The ring uses 32-bit atomic words and sequentially consistent ordering. Its even
32-bit publication sequence never wraps: after 2,147,483,647 publications it stops
publishing with `Error::Exhausted`, retaining the last publication. Preparing a new
engine is required to start a fresh sequence domain. Control exposes publication
errors and the snapshot frame; the adapter exposes its last accepted frame.

Control uses the existing active and staged models to associate epochs, continuing
to forward old active state while a replacement is pending. Once the model has
confirmed activation, unknown/old epochs are rejected. Reliable configuration
markers follow structural results and resynchronization. UI chunks carry sequence,
offset, and total length. Missing, malformed, out-of-order, obsolete, or unknown-
configuration publications do not update the mirror. Only a fully assembled and
validated publication applies its scalar fields. Legacy scalar observations and
success replies cannot overwrite the migrated display fields in snapshot mode.

Focused checks cover ring wrap with concurrent readers, stalled consumption,
sequence exhaustion, capacity refusal, graph activation and slot reuse, continued
publication during pending graph changes, malformed and incomplete UI assembly,
protocol round trips, push/pull delivery selection, and control-cache/reconnect
progress without UI snapshot requests. Existing engine control and
adapter checks exercise legacy operation and command/media handling alongside the
new path. No hardware implementation or broad performance campaign is included.

## Placement and implementation scope

The snapshot ring is an internal real-time-to-control communication mechanism.
Its generic storage, publication, and validated-read protocol belong in
`rt_runtime`, alongside its existing internal communication mechanisms. The
application-specific snapshot schema and collection of loop, strip, and bus state
belong in `shoop_rt_engine`, using loop behavior from `shoop_loops`. The runtime must not depend on those
application-specific types; the exact interface for supplying snapshot data is an
implementation design task.

The flow is: audio callback captures and publishes into the shared-memory ring;
control copies and validates a snapshot; control serializes and forwards accepted
state over the application transport; the UI consumes it. Network communication,
serialization, client pacing, and handling slow clients stay outside the audio
callback. The ring is not a network buffer or the application transport itself.

Implement only the Rust software path in this work. Do not build ARM-specific or
FPGA-specific code, register interfaces, RTL, device drivers, or hardware tests.
Keep the logical protocol straightforward to map onto such hardware later:
producer-owned publication metadata, fixed slots, explicit sequences, and
consumer-side validation. Future hardware mapping is a design consideration, not
an implementation deliverable or dependency of the Rust migration.

## Snapshot contents and capture boundary

A snapshot contains all supported scalar display state for the active session:
ordinary loops, strip mixers, output buses, and relevant engine status. Initial
fields include loop modes, pending transitions, position, length, revisions,
volume, balance, recording flags, repeat-sync settings, and stereo peaks; strip
input/output controls, monitoring, meters, and MIDI activity; and bus controls and
meters. Inventory the exact fields and their sources before implementation.

Include the object identities and generations, active graph and topology epochs,
audio frame, snapshot sequence, schema version, and object counts. Do not repeat
graph topology or structural relationships in each snapshot. Control supplies
those configurations to real-time processing and retains immutable descriptions
indexed by the corresponding versions. A complete snapshot means complete live
state for an identified configuration, not a complete session description.

Control retains the active configuration and pending replacements until their
outcomes are known. Before exposing a snapshot, it must resolve its active
versions to the matching retained configuration. Unknown versions are rejected or
held within bounded storage until their configuration is available; never guess
the layout from the currently displayed graph. After switching to a newer active
configuration, discard delayed snapshots from older versions. Configuration
retirement must respect snapshots still being interpreted. A newly connected UI
receives the matching configuration through the existing resynchronization path
before applying live state for it.

Audio/MIDI content, ownership-bearing objects, and individual command outcomes are
not scalar display state. Keep their existing protocols. Exact-frame queries also
remain available. Decide explicitly whether channel playback windows and other
infrequently changing metadata belong in the initial snapshot schema.

Capture at a defined, settled processing boundary inside the true real-time
owner: after eligible changes and same-frame settlement, before processing the
next audio interval. The frame identifies that boundary. All fields must describe
that same boundary, including any graph activation. Do not assemble a purportedly
coherent snapshot by reading independently changing control-side cells.

Meter values summarize a defined interval ending at the capture boundary, rather
than an instantaneous sample. Specify that interval and ensure parallel legacy
observation does not destructively consume the new path's meter data.

## Snapshot ring

### Slot capacity and session limits

All slots have the same fixed payload capacity, chosen during preparation from
explicit maximum loop, strip, bus, and other supported record counts. Each
publication carries its actual payload length and object counts; smaller sessions
use only part of a slot. Include headers, metadata, alignment, and the maximum
representation of optional fields in capacity calculations, with checked size
arithmetic. The callback never resizes storage.

Before admitting a session/configuration change, control verifies that its full
snapshot representation fits the prepared limits. Reject changes that exceed
those limits with an explicit capacity error, before activation, while keeping the
current session running. The first implementation does not support live ring
resizing; replacing storage is a separate future design task. The initial session
must pass the same capacity checks during preparation.

An unexpected overflow during capture is an invariant failure: abandon the
in-progress publication, leave the latest completed publication available, and
expose a bounded error indicator outside the snapshot payload. Never allocate,
truncate, or advertise incomplete state as a completed snapshot. The failed slot
must remain invalid until a subsequent complete write publishes it successfully.

### Slot publication protocol

Use a fixed-capacity ring of preallocated snapshot slots, initially considering
ten slots. There is one producer (real-time processing) and one consumer
(control). Each slot has a publication sequence that distinguishes an in-progress
write from a completed publication. A separate producer-owned indicator advertises
the latest completed slot and its sequence.

The producer never waits for the consumer. It cycles through the slots and may
overwrite old publications. This is a latest-state transport, not a reliable FIFO:
intermediate snapshots may be lost deliberately.

### Producer

1. Select the next slot and mark its sequence as in progress before modifying any
   payload data.
2. Fill the complete snapshot from real-time-owned state at the capture boundary.
3. Mark the slot complete with a new publication sequence, after all payload
   writes are visible.
4. Advertise that slot and sequence as the latest completed publication.

All storage and maximum object counts are prepared off the callback. Publication
must have bounded work and perform no allocation, deallocation, locking, waiting,
or I/O. Schema capacity must cover the active configuration before activation;
never publish a truncated snapshot as complete.

### Consumer

1. Read the latest completed slot indicator.
2. Read that slot's sequence and reject an in-progress or unexpected sequence.
3. Copy the entire payload into consumer-owned storage.
4. Read the slot's sequence again, with the required memory ordering.
5. Accept the copy only if both sequence reads match the advertised completed
   publication. Otherwise discard it and attempt a newer publication later.

Do not expose or interpret the candidate payload before validation. Never retain
a reference into the ring as accepted UI state. Bound retries per control poll;
if none succeeds, keep the last accepted snapshot. Detect absence of an initial
publication and expose snapshot age so stale state is distinguishable from live
state. Reject older sequences rather than rolling the display backward.

Ten slots reduce the chance of overlap; their number does not prove safety. A
consumer can pause indefinitely. Safety comes from detecting overwritten reads,
not from assuming that the producer cannot catch up. Continuous successful reads
require the consumer occasionally to finish a copy before that slot is reused.

## Memory safety and hardware mapping

Version validation alone does not make concurrent ordinary Rust memory accesses
safe. Reading non-atomic payload memory while it is overwritten is a data race,
even if validation later rejects the copy. The software implementation must use
a sound concurrent representation, such as atomic payload words with a validated
sequence protocol. A packed wire layout can be separate from that representation.
Do not implement this with an unchecked concurrent memcpy or ordinary references
into mutating payload storage.

Specify and review sequence width, indivisible metadata accesses, initialization,
memory ordering, and wrap handling. A sequence must not repeat during a possible
read; define an explicit rollover policy rather than assuming a finite counter
can never wrap. An initial conservative software ordering is preferable to an
unproved optimization. Atomic operations used on the callback must be supported
without hidden blocking on the target.

As a future design consideration only, an FPGA producer and ARM consumer could
use the same logical protocol:
only the producer writes slot metadata and payload, and the consumer validates a
private copy. No shared read-modify-write exchange is required. Define bus access
widths, completion of FPGA writes, CPU/device memory barriers, cache coherence or
cache maintenance, and clock-domain crossings explicitly. Rust atomic ordering
alone is not a specification for device memory visibility. These hardware
requirements are deferred; no hardware implementation is part of this work.

## Publication and display cadence

RT publishes at the selected settled boundary of every host callback that executes
processing, regardless of UI delivery mode or rate. There is no RT publication
rate control: worst-case capture work remains part of every processing callback.
Callbacks that only stage buffered input retain the last publication.

Snapshot observation supports both control-to-UI delivery policies on every
carrier, selected through `SnapshotDelivery`:

- Native `InProcess` defaults to `Pull`.
- Browser targets, including browser `InProcess`, default to `Push` at 60 Hz.
- Encoded carriers (`RecordClient`), including network, IPC, and browser message
  ports, default to `Push` at 60 Hz. Custom asynchronous carriers use that default
  unless they override the carrier hint.

The adapter sends its selection after the handshake, and it can switch policy at
runtime with `RtBackend::set_snapshot_delivery`. For example:

```rust
backend.set_snapshot_delivery(SnapshotDelivery::Pull)?;
backend.set_snapshot_delivery(SnapshotDelivery::push(120).unwrap())?;
```

A push rate must be a nonzero `u16` frequency; `SnapshotDelivery::push(0)` returns
`None`. The control endpoint spaces unsolicited publication starts by at least
`ceil(sample_rate / max_hz)` audio frames. Callback/poll granularity and transport
backpressure can reduce the achieved rate. A slow or stopped audio clock produces
no extra publications or catch-up bursts. Undelivered push chunks in the control
outbox are superseded when a newer publication starts; other telemetry is retained.

In pull mode, each bounded adapter `pump` (including a UI `poll`) attempts at most
one request. An explicit `request_latest_snapshot` is also available. The protocol
client permits only one outstanding pull, coalescing further attempts while it
awaits completion. Control copies the latest valid ring publication without
waiting for RT; requests do not enter the RT command queues. Unchanged or
unavailable data produces a completion without a payload. The UI keeps its last
accepted state and continues drawing. Requested chunks and their terminal
`SnapshotReady` message use the reliable lane so dropped telemetry cannot strand
a pull. Snapshot requests have separate admission from ordinary command credits.

The delivery setting and request ordering are control-side state, communicated
through the existing reliable channel; RT does not synchronize with these settings.
An in-progress transfer finishes using its original push/pull policy before a new
transfer adopts a changed setting. Disabling snapshot observation cancels a pending
pull with a terminal message. Disconnect resets pending delivery state. Explicit
pulls are also supported in push mode and bypass the unsolicited push timer.
Network/browser pulls remain asynchronous and never block the drawing loop.

A compact scalar snapshot for 64 stereo loops, eight stereo strips, and one stereo
bus is estimated at about 5 KB, or about 9 KB with active-channel timing metadata.
These are design estimates, not measurements or current Rust struct sizes. Existing
fixed-capacity channel-window encoding is larger. Benchmark callback capture cost,
copy/decode cost, validation failures, snapshot age, and UI frame time.

The UI applies complete accepted snapshots to existing objects and skips obsolete
ones. Cursor interpolation may use the audio frame and playback information, but
must respect stops, seeks, mode changes, and configuration changes.

### Legacy mutation observation

The adapter retains successful legacy mutation completions until a fresh,
validated state publication has been applied. Reliable results can precede
rate-limited push snapshots, so a success result alone must not expose an older
recording/display state as the result of a stop. Failures and disconnect outcomes
are released without waiting for another publication. Capture preparation also
waits for fresh observation before evaluating recording state.

This uses the existing legacy completion storage and snapshot freshness boundary.
Immediate command groups bypass this completion mechanism entirely; callers
observe their intended conditions through snapshots.

## Migration

### First milestone: publication available to control

Deliver a documented scalar field inventory and schema, the generic preallocated
ring in `rt_runtime`, coherent application-state capture in `shoop_rt_engine`,
and a control-side reader that validates copies and associates them with retained
configuration versions. Use fixed prepared limits with pre-activation capacity
checks. Make the new code clearly distinguishable from legacy communication.

This milestone is complete when control can obtain complete coherent snapshots
from actual callback processing, including across graph activation and object
generation changes, with focused smoke coverage for overwrite detection, a
stalled reader, and capacity refusal. Keep the UI on its existing observation path
and keep all legacy communication operational. Broad correctness/performance
campaigns are not part of this first pass; record limitations and targeted timing
observations rather than claiming measured performance from estimates.

### Completion of the snapshot migration

After the publication milestone, integrate delivery to the UI and switch ordinary
scalar display state to accepted snapshots, with a single observation source for
migrated fields. Completion means the UI consistently uses snapshots for the inventoried state, resolves configuration versions correctly,
and skips obsolete publications without accumulating a backlog. Command/results,
media and ownership communication remain operational. Redundant timing queries
and the temporary legacy display selector have been removed.

Removing legacy communication, converting commands to fire-and-forget, replacing
media/ownership protocols, live ring resizing, and any ARM/FPGA implementation are
outside these milestones. They require subsequent designs and work.

### Execution sequence

1. **Inventory and define.** Record the exact schema and capture point. Separate
   live display state from media transfer, ownership reclamation, exact queries,
   and command outcomes. Keep new transport and projection code in clearly named
   modules with an explicit field inventory.
2. **Publish alongside legacy communication.** Add the preallocated ring, capture
   logic, and control reader. Keep UI behavior and all old communication paths
   working. Smoke-test coherent snapshots, overwrite rejection, a stalled consumer,
   and graph/slot-generation changes. Compare against legacy observations at
   matching frames and epochs where possible.
3. **Move UI observation.** Add an explicit selection between legacy observation
   and snapshots during development, then remove the temporary selector after
   migrating control timing dependencies. Switch ordinary display state as a
   coherent group; legacy replies must not overwrite newer snapshot state. Continue draining legacy completions to release
   credits and reclaim owners. A slow UI receives the latest complete snapshot,
   not a backlog.
4. **Revisit commands separately.** Migrate appropriate commands to fire-and-forget
   only after snapshot observation works. Preserve ordered dependencies, musical
   scheduling, failure reporting, and reliable ownership/media workflows until
   their replacements are explicitly designed and reviewed.

During a requested graph change, snapshots of the old active configuration remain
valid. Switch the displayed active configuration only when a validated snapshot
identifies the replacement as active and its context is available. If activation
fails, continue displaying the actual active configuration. An optimistic pending
UI view must remain distinct from the confirmed audio state.

The publication, UI delivery, and control timing-cache migrations are implemented.
Command semantics and media lifetime protocols remain separate future work.
