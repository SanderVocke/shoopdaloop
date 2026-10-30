# New engine backend: the engine

`shoop_rt_engine` is the application's real-time composition for the new engine
backend. It is compiled once for every environment: native hosts, the browser's
AudioWorklet and, later, a separate process all run this crate. It speaks only
engine terms (strips, loop slots, connections, routes, telemetry) through the
[`shoop_rt_protocol`](../shoop_rt_protocol) schema and depends on no application
crate; `scripts/check_rt_engine_boundary.py` enforces that.

## Owners

`prepare(&EngineDescription, DriverStatus)` allocates everything and returns an
`Engine` with three owners:

- `ControlEndpoint` implements the protocol's `Endpoint`. A host hands it to a
  carrier (`InProcess` natively, a record server in the worklet) and pumps it.
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
direction (also the channels of every loop), Master channels, loop boundary
smoothing, the ramp for control changes and removals, a control lead for hosts
whose control side runs concurrently with rendering, storage sizes and the
client's credits.

Pools are lists of `Segment { strips, loops }`. Slot indices count through the
segments; each segment owns a contiguous range of runtime nodes. Every slot's
processors are prepared up front, but only occupied slots are in the active graph
and only existing loops are in the loop topology, so inactive slots cost no
rendering or resolver work. This milestone prepares all segments at once;
another segment can be appended without moving existing slots.

## Signal chain

| Node | Ports | Function |
| --- | --- | --- |
| Strip input | two banks of input channels; capture and monitoring feeds; MIDI in; MIDI capture and monitor feeds | Bank cross-fade, input gain and balance, monitoring ramp, input meters, injected MIDI |
| Loop playback and capture | the loop library's nodes | Recording and playback decided by the loop resolver |
| Loop fader | one audio channel set | Fade-out for removal, loop meters |
| Strip output | the sum of its loop faders and the monitoring feed; two output banks | Output gain, balance, mute and presence ramps, output meters |
| Master | Master channels | Sum of routed strips; gain, balance, mute, meters; channel `c` feeds host output `c` |

Stereo balance uses the legacy linear factors on two-channel stages. A strip
output channel `c` takes input channel `c mod inputs`. A mono strip feeds every
Master channel; wider strips feed Master channel `c mod channels`.

## Requests

Requests are handled in arrival order. Controls, loop transitions, loop gain and
balance and MIDI injection are forwarded to the runtime as soon as they reach the
head of the queue and answered with their execution's outcome. Structural
requests (create and remove strips and loops, sync sources, connections, routes)
and Clear run as one transaction at a time and hold back later requests until
their activation is confirmed. Every request is validated against the confirmed
state: client-chosen slot IDs must be in the pool (`Capacity`), unoccupied and of
a newer generation (`InvalidId`); values must be finite and balances within
[-1, 1] (`InvalidValue`). Requests beyond the client's credits are refused
`Overloaded`. `SetSmoothing` only accepts the prepared smoothing.

Loop transitions follow the legacy rules. `delay: Some(n)` waits for the
(n+1)-th clock edge of the loop's sync source; a loop without a source counts its
own wraps. A loop without a sync source that is not playing, and any request
without a delay, transitions at once and cancels planned transitions. A delayed
request cancels planned transitions whose remaining delay is not smaller than its
own. Delayed requests are answered when admitted; immediate ones when applied.

## Structural transactions

A transaction first prepares everything without touching the music: node-pair
edits for added or removed loops, a validated LTP1 timing image of the new loop
topology, and a staged graph built in a prepared plan. It then schedules the
activation at an exact frame:

- Adding a strip or loop activates at the next frame with freshly reset
  processors; a new loop generation starts empty.
- Removing a strip or loop first fades its output or fader over the prepared
  ramp, stops its loops and then activates the configuration without them.
- Connecting, disconnecting and routing write the new connection set to the
  silent bank of the strip and cross-fade between the banks at the activation
  frame. Unchanged destinations keep their signal; removed ones fade out and
  added ones fade in. MIDI connections change at the activation frame.
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
source's clock edges. Peaks are the maximum since the previous record.

`Resync` streams the confirmed state between `Begin` and `End` records, as
reliable outbox space allows. Bulk reads export the loop's playback content once
per session and read it through a resource cursor into a prepared page buffer;
in-order reads continue the session, which ends with its last chunk. Audio
chunks carry one channel; MIDI chunks carry the stored records described in the
protocol.

## Connection lifecycle

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

Pools are fixed after preparation (`GROWABLE_LIMITS` is not announced). Loop
boundary smoothing is fixed at preparation. MIDI connection changes do not send
note cleanup. Telemetry fields are independent latest values, not one coherent
snapshot.
