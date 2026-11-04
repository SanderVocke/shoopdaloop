# New engine backend: the engine

This crate uses `no_std` with `alloc`, including its optional offline testing host.
Targets must support pointer-width, 32-bit and 64-bit atomics. Hosts provide an
allocator, panic handling and audio/MIDI I/O; preparation and cleanup stay outside
render callbacks. The engine does not supply RTOS startup or scheduling.

The portable engine and complete session-service stack can be checked for
Cortex-A without a standard library:

```sh
cargo build --locked --lib --no-default-features \
  -p shoop_rt_engine -p shoop_app -p shoop_rt_backend -p shoop_script_resources \
  --target aarch64-unknown-none
```

Use `armv7a-none-eabi` for 32-bit Cortex-A; CI builds both targets.

The session service may allocate while handling commands, projections and media.
Run it separately from the render callback with an appropriate memory budget.

`shoop_rt_engine` is the application's real-time composition for the new engine
backend. It is compiled once for every environment: native hosts, the browser's
AudioWorklet and separate native processes all run this crate. It speaks only
engine terms (strips, loop slots, connections, routes, telemetry) through the
[`shoop_rt_protocol`](../shoop_rt_protocol) schema and depends on no application
crate; `scripts/check_rt_engine_boundary.py` enforces that.

## Owners

The fixed-quantum migration also exposes `fixed::prepare(..., Quantum)`, returning
prepared engine parts with the production strip/fader/bus/reader DSP and the direct
loop bank. Its `runtime(graph)` installs tick-aligned loop and typed-node controls
and owning routing publication. DSP implementations and capacity calculations are
shared with the production interval engine; fixed work buffers use Q frames.
Tests cover record/play through the complete signal chain, exact MIDI offsets,
meters, epoch staging, multi-tick ramps and one bulk-read allowance per callback.

These parts are not selected by application/native/worklet startup yet. Logical
slot lifecycle, sync/program behavior, atomic graph/media/owner transactions,
bulk/export and protocol endpoint integration must reach parity before cutover.
Direct fixed runtimes require divisible host blocks; arbitrary hosts must use the
constant-latency `BufferedHost` adapter. Changing Q requires stop/reprepare.

`prepare(&EngineDescription, DriverStatus)` allocates everything and returns an
`Engine` with three owners:

- `ControlEndpoint` implements the shared transport's `Endpoint`. A host hands it to a
  carrier (`InProcess` locally, the common record server for worklet/IPC) and pumps it.
  `deliver` and `poll` are bounded and do not allocate, lock or wait, so the same
  code can run between render quanta on the audio thread.
- `Render` is called once per audio callback with absolute frames
  (`render.next_frame()`), the host's audio buffers and MIDI ports. It makes
  exactly one runtime `process` call, then drains MIDI recovery into
  `midi_recovery(port)`, a separate output to send before the next callback. On
  an error it silences output, requests MIDI resets and stays faulted: stop
  callbacks and use `into_stopped().cleanup()` off-render. If the endpoint is
  dropped, rendering goes silent after a MIDI reset.
- `HostStatus` carries driver status, xruns and callback load from the host into
  engine telemetry, from any thread.

`Engine::outbox()` sizes an outbox for the description's credits and one round of
telemetry.

## Description and pools

`EngineDescription` gives the sample rate, the largest callback, named host port
slots (at most 64 per kind and direction), pool segments, audio channels per strip
direction (also the channels of every loop), prepared bus count/width and the default bus
channels/connections, the initial and largest loop boundary smoothing, the ramp for
control changes and removals, a control lead for hosts whose control side runs
concurrently with rendering, storage sizes and the client's credits.

Pools are lists of `Segment { strips, loops }`. Slot indices count through the
segments; each segment owns a contiguous range of runtime nodes. Every slot's
processors are prepared up front, but only occupied slots are in the active graph
and only existing loops are in the loop topology, so inactive slots cost no
rendering or resolver work. This milestone prepares all segments at once;
another segment can be appended without moving existing slots.

Buses use a separate prepared pool, defaulting to eight slots of eight channels.
Route masks store eight channel bits per bus slot. Bus IDs include generations;
create/remove requests, controls, meters and resync all name the exact occupant.
The default stereo bus is created as an ordinary active slot, can be removed, and
is absent from an empty replacement. Inactive buses do not render. Bus processors
use the existing ramps, banked routes and staged replacement configuration; no FX
stage is installed. Native setup, worklet configuration and protocol limits carry
the same bounds.

## Signal chain

| Node | Ports | Function |
| --- | --- | --- |
| Strip input | two banks of input channels; capture and monitoring feeds; MIDI in; MIDI capture and monitor feeds | Bank cross-fade, input gain and balance, monitoring ramp with release of monitored notes, input meters, injected MIDI |
| Loop playback and capture | the loop library's nodes | Recording and playback decided by the loop resolver |
| Loop fader | one audio channel set | Fade-out for removal, loop meters |
| Strip output | the sum of its loop faders and the monitoring feed; two output banks | Output gain, balance, mute and presence ramps, output meters |
| Bus | Prepared bus channels; two output banks | Sum of routed strips; gain, stereo balance, mute, presence and meters |

Stereo balance uses the legacy linear factors on two-channel stages. A strip
output channel `c` takes input channel `c mod inputs`. Routes connect single strip
output channels to single bus channels; `Limits::buses` and `bus_channels` report
prepared capacity.

## Requests

Requests keep their order per target (a strip, a loop or a bus). Controls,
loop transitions, loop gain and balance and MIDI injection are forwarded to the
runtime as soon as nothing they depend on is pending, and answered with their
execution's outcome. Structural requests (create and remove strips and loops,
sync sources, connections, routes) run as transactions, one at a time: at each
poll, the structural requests that are queued and can go together are validated
against the model they build up and become one transaction with one activation,
so a strip with its loops, a set of sync changes or a set of connections and
routes activates at once. Their order is kept: once one has to wait, for example
to reuse a slot that the same transaction frees, later ones wait too. Clear and
`SetSmoothing` run as transactions of their own.

While a transaction is pending, requests for untouched strips and loops do not
wait. A loop transition waits while its loop is created or removed or its strip's
structure or connections change; a delayed one also waits for its loop's sync
source change, while an immediate one does not depend on it. Gains wait only for
the creation or removal of their loop or strip.

Every request is validated: client-chosen slot IDs must be in the pool
(`Capacity`), unoccupied and of a newer generation (`InvalidId`); values must be
finite and balances within [-1, 1] (`InvalidValue`). Requests beyond the client's
credits are refused `Overloaded`; the default description allows 256. `SetSmoothing` applies the loop library's smoothing operation to
every loop and to loops created later; values above the prepared limit, which
`Limits` announces, are refused `InvalidValue`.

Loop transitions follow the legacy rules. `delay: Some(n)` waits for the
(n+1)-th clock edge of the loop's sync source; a loop without a source counts its
own wraps. A loop without a sync source that is not playing, and any request
without a delay, transitions at once and cancels planned transitions. A delayed
request cancels planned transitions whose remaining delay is not smaller than its
own. Delayed requests are answered when admitted; immediate ones when applied.

## Structural transactions

A transaction first prepares everything without touching the music: node-pair
edits for added or removed loops, a validated LTP1 timing image of the new loop
topology, and a staged graph built in a prepared plan. The three stages start
together; the node-pair edits and timing records follow in the next poll, the
records scheduled one callback apart (the loop library accepts one append per
callback) with validation after the last. It then schedules the activation at an
exact frame and answers every request of the transaction when it is confirmed:

- Adding a strip or loop activates at the next frame with freshly reset
  processors; a new loop generation starts empty. Additions need no fade.
- Removing a strip or loop first fades its output or fader over the prepared
  ramp, stops its loops and then activates the configuration without them.
- Connecting, disconnecting and routing write the new connection set to the
  silent bank of the strip or the Master bus, once per transaction however many
  changes it holds, and cross-fade between the banks at the activation frame. Unchanged destinations keep their signal; removed ones
  fade out and added ones fade in. MIDI connections change at the activation
  frame. Master outputs are connections like strip ports (`EnginePort::Bus`);
  the description's initial Master wiring is reported through resync.
- Changing a sync source activates a new timing topology; pending delayed
  transitions of that loop fail, as the loop library revalidates them.

Afterwards the displaced timing image, node-pair bank and graph plan return to
their prepared slots and a new loop's initial media residency is released, so
slots are handed over indefinitely. `Clear` stops the loop and then applies the
loop library's `Operation::Clear`.

## Telemetry, state and bulk reads

Rendering publishes latest values into prepared atomic cells: the loop
resolver's settled state at every boundary (through the planner's boundary
observer), clock edges per loop, and peaks and MIDI activity from the engine's
processors. `poll` emits Engine, Strip, Loop and Bus records at most every
`telemetry_interval_frames` (after every callback when zero), and a Loop record
whenever its planned transitions change. A recording loop reports its captured
length. Next mode and remaining delay come from the planned transitions and the
source's clock edges. Peaks are the maximum since the previous record. The
Engine record carries the frame, the callbacks rendered, the host's xrun count
and load, and the peaks of all host audio inputs and outputs. Loop state is the
state at the start of the last rendered callback.

`Resync` streams the confirmed state between `Begin` and `End` records, as
reliable outbox space allows. Right after `Begin`, a `Generations` record gives
the highest generation used on any strip and any loop slot, so a reconnecting
client can name free slots with newer generations.

Bulk reads export the loop's playback content once per session and hand a cursor
over its audio or MIDI to the bulk reader, a node that is always in the graph
and idle without a cursor. During rendering it copies up to four pages per
callback, within the resource copy budget reserved for it, into a lock-free ring
of sixteen pages. The endpoint cuts chunks from the ring, as many per poll as
reads are queued and bytes have arrived. In-order reads continue the session,
which ends, releasing its export, once its medium is taken; reading another loop,
medium or channel, or seeking backwards, stops the reader and starts a new
session. Audio chunks carry one channel or, with `AudioFrames`, whole frames of
every channel interleaved, so one session serves all channels; MIDI chunks carry
the stored records described in the protocol.

## Connection lifecycle

If rendering faults, output stays silent after a MIDI reset and the endpoint
ends the connection with `Disconnected(Fault)`, answering later requests
`NotConnected` and later `Hello`s with the same disconnect. Stop callbacks and
clean up the stopped owners.

`Hello` with another version is rejected. `Goodbye` ends the connection: queued
requests are discarded, transactions in progress complete without results, and
the engine keeps its state and keeps rendering. A new `Hello` starts a new
connection; the client resynchronizes. `ControlEndpoint::set_credits` changes the
credits within the prepared maximum and sends `LimitsChanged`.

## Offline host

The `testing` feature adds `testing::OfflineHost`: it pulls callbacks
synchronously with given inputs and pumps the endpoint through the in-process
carrier and a `ProtocolClient`. There are no threads and no real time. Its
`process`, `request` and `receive` do not allocate, so tests guard them with an
allocator check. The crate's tests use it for the legacy timing reference,
structural edits while rendering, telemetry, bulk reads, resynchronization,
credits and shutdown.

```sh
cargo test -p shoop_rt_engine
cargo test -p shoop_rt_engine --release
```

## Limits of this milestone

Pools are fixed after preparation (`GROWABLE_LIMITS` is not announced). As in
the legacy engine, turning monitoring off releases the notes the monitoring feed
passed through, while MIDI disconnects and output mute send no note cleanup.
Telemetry fields are independent latest values, not one coherent snapshot.
