# Mixer architecture

## Purpose

The mixer is a bounded, post-track-output routing graph. It is not a general representation of the recording, looping, monitoring, or dry/wet processor graph inside a track.

The mixer begins where a track has produced its final output signal and ends at sinks. This boundary keeps loop timing, recording semantics, input monitoring, track processors, and latency-sensitive dry/wet behavior inside the existing track implementation.

## Graph boundary

The conceptual flow is:

```text
existing track internals
        |
        v
track output channel -----------------------------> sink
        |
        +----> bus channel sum -> bus processing -> bus output channel -> sink
```

The mixer contains only:

- track output channels;
- buses and their channels;
- sinks;
- directed routes between the allowed endpoints.

The mixer does not expose track inputs, loop channels, dry sends, wet returns, MIDI paths, input monitoring, or synchronization as graph entities.

## Entities

### Track output channel

A track output channel is a stable mixer source. Its signal is taken after the track's output processing, including its processor topology and output gain, balance, and mute controls. Changing mixer routing must not alter recording, loop playback, monitoring, or the signal heard by another route from the same track output.

A track output may fan out to any number of bus channels and sinks. Direct track-to-sink routing remains valid; use of a bus is optional.

### Bus

A bus owns an ordered, arbitrary-length set of audio channels. Channel count is not globally restricted to mono or stereo. Channel indices are stable within the lifetime of the bus; labels such as `Left` and `Right` are presentation metadata rather than routing semantics.

Each bus channel has:

1. a summing input that accepts zero or more track-output routes;
2. a bus processing path shared according to the bus processor's channel contract;
3. a bus output that can fan out to zero or more sinks.

A bus with no incoming routes produces silence. A bus with no outgoing routes is audible nowhere. No implicit Master routing is part of the engine model: application policy may create a Master bus or default routes, but all effective routes remain explicit.

Future bus processing may include gain, balance, mute, metering, and ordered post-sum inserts. Those controls are bus behavior, not additional graph entity kinds.

### Sink

A sink is a terminal audio consumer. It never produces audio and cannot be routed onward. Initial sinks are external system/device audio inputs. Future sinks may include bounded recording/export consumers, but adding such a sink must not expand the mixer into the track-input or loop graph.

### Route

A route is an independently identified directed connection. The permitted shapes are:

- track output channel to bus input channel;
- track output channel to sink;
- bus output channel to sink.

Bus-to-bus, sink-to-anything, bus-to-track, and routes involving track inputs or loop channels are forbidden. This restricted topology is acyclic by construction.

Routes are additive. Multiple track outputs routed to one bus channel are summed in a deterministic order. A source may fan out without one destination consuming or modifying the signal seen by another destination.

The first implementation uses unity-gain audio routes. The route model may later carry level, mute, and other send parameters without changing the allowed topology. Channel mapping is expressed by explicit per-channel routes; stereo pairing and convenient mapping gestures belong in the application/UI layer.

## Control plane and realtime plane

The application and backend own the desired mixer topology. The audio callback executes an immutable prepared topology.

Topology changes follow this sequence:

1. validate endpoint identity, ownership, type compatibility, and allowed edge shape;
2. build a complete prepared routing/scheduling snapshot off the realtime path;
3. atomically install the schedule and its matching active route table;
4. publish the route as confirmed only after the active graph contains it.

While a replacement is being prepared, the previous schedule must continue using its previous route table. A new route must never be observed by an old schedule, and a failed mutation must leave the active graph unchanged. Allocation, locking, graph construction, and unbounded work remain forbidden in the audio callback.

Fader, balance, mute, meter, and future route-level controls are realtime parameters rather than topology edits. Adjusting them must not rebuild the graph.

## Identity and ownership

Track, bus, bus-channel, sink, and route identities are typed and stable across snapshots. Runtime arena indices, port names, and display labels are not persistent identities.

The backend is the authority for active routing. Application snapshots distinguish confirmed routes from pending requests and failures. The UI must never infer confirmation from a drag operation or maintain a competing routing truth.

Bus-owned external output ports remain application ports at the host boundary. Bus input channels are mixer destinations, not host ports. A bus channel may be presented as one row with an input connector on the left and an output connector on the right even when those facets use distinct backend identities.

## Backend normalization

Native, dummy, browser Worker, and AudioWorklet implementations expose the same logical buses, channels, routes, and confirmation semantics. Concrete driver ports and engine-internal ports are adapter details.

Shared mixer descriptions and validation rules should be lowered into backend-specific handles rather than reconstructing mixer semantics independently in every backend. Backend capability differences may affect available sinks, but not the meaning of a route.

## Connections presentation

The Connections dialog is a view of both host-boundary connections and mixer routes. Its logical output side is:

```text
ShoopDaLoop track sources -> Buses -> System sinks
                         \----------> System sinks
```

A bus column therefore contains sink facets on its left and source facets on its right. The dialog emits typed route intents, displays only backend-confirmed links as confirmed, preserves pending/error presentation, and allows direct track-to-system routes to bypass the bus column.

Track-scoped filtering limits visible track endpoints and routes but does not change bus identity or routing state. Audio/MIDI filters hide presentation only; buses are audio entities.

## Persistence

Buses, channel identities, mixer routes, and bus-to-system connections are session state. Serialization records stable identities and explicit routes, including the meaningful absence of routes. It must not encode runtime graph indices or infer routing from names.

Session replacement and audio-driver switching reconstruct the mixer transactionally alongside tracks. Legacy sessions without buses receive an explicit migration policy that preserves their direct track-to-system connections and introduces no new audible path.

## Initial Master-bus sandbox

The first implementation creates exactly one stereo bus named **Master**. It starts with no track inputs and no system-sink connections. Existing direct track-to-system routing remains available and unchanged.

Users may independently route either track output channel to either Master channel, keep or remove direct track-to-system routes, and route either Master output channel to compatible system sinks. The Master bus has no user controls, processors, add/remove operation, or implicit routing in this stage.

This sandbox validates the durable mixer boundary, identities, route authority, realtime installation, backend parity, persistence, and Connections-dialog model before adding general bus management, mixer controls, recording sinks, or post-processing.

## Bus-control increment

The second implementation increment retains the fixed stereo Master and adds one built-in post-sum processing stage. Master has bus-wide gain, stereo balance, and mute parameters. Gain applies uniformly to all channels; balance is valid only for exactly two ordered channels and attenuates the opposite side without boosting; mute silences all outputs while retaining gain, balance, and routes. These controls update realtime audio-port parameters without rebuilding the prepared topology.

Each bus output channel publishes a post-gain, post-balance, post-mute peak. Meter values are transient telemetry; gain, balance, and mute are session state and survive replacement, resampling, and compatible driver switching. Native, dummy, Worker, and AudioWorklet snapshots use the same normalized control and meter contract.

The main UI presents one vertically ordered bus block per bus in the right sidebar above the logo. A block contains the bus name, channel-aware peak meter, mute, volume fader, and a balance dial only for stereo buses. Lua exposes the same control state and mutations through the application intent/backend authority path. This increment still adds no bus management, bus-to-bus routing, route levels, solos, or editable insert processors.
