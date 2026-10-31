# New engine backend: the protocol

`shoop_rt_protocol` is the transport-neutral protocol between an application
adapter ([`shoop_rt_backend`](../shoop_rt_backend)) and the engine
([`shoop_rt_engine`](../shoop_rt_engine)). It defines plain data, the connection
lifecycle, credit limits and a binary codec. It has no semantics and no
dependencies: what a request does is the engine's business, what it means to the
application is the adapter's.

## Messages and lanes

`ClientMessage` (`Hello`, `Request`, `Resync`, `Bulk`, `Goodbye`) and `EngineMessage`
(`Welcome`, `Rejected`, `Result`, `Telemetry`, `State`, `Bulk`, `LimitsChanged`,
`Disconnected`) travel in three lanes:

- **Reliable**: requests, their results, lifecycle and state records. Never dropped.
- **Telemetry**: latest values (engine, strip, loop and bus records). A full lane
  drops its oldest record.
- **Bulk**: chunked reads of loop media, one `BulkChunk` per `BulkRead`.

`Limits` announces the client's credits: at most `reliable_credits` unanswered
requests and `bulk_credits` unanswered reads. `shoop_rt_transport::client::ProtocolClient` enforces them
(`ClientError::Busy`) and correlates results; the engine refuses requests beyond
them as `Overloaded`.

Every value has a bounded size: names, MIDI messages, peaks and chunks are fixed
arrays (`MAX_NAME_BYTES`, `MAX_MIDI_BYTES`, `MAX_STRIP_CHANNELS`,
`BULK_CHUNK_SAMPLES`, `BULK_CHUNK_BYTES`), so encoding and queueing need no
allocation.

## Lifecycle

- `Hello { version }` is answered with `Welcome { version, capabilities, limits,
  driver }`, or `Rejected` if the version differs; both sides must speak
  `PROTOCOL_VERSION` exactly.
- `Resync` asks for the engine's confirmed state as `State` records between
  `Begin { epoch }` and `End { epoch }`: first `Generations`, the highest
  generation used on any strip and any loop slot, then host ports, strips with
  their controls, routes and connections, loops, and the Master bus.
- `LimitsChanged` replaces the limits at any time: capacities and credits are
  current values, not handshake constants.
- `Goodbye` ends the connection; the engine answers `Disconnected(Shutdown)`, keeps
  its state and accepts a new `Hello`. `Disconnected(Fault)` and
  `Disconnected(ProtocolError)` end it from the engine's side.

## Identities

The client chooses the identities of what it creates. `StripId` and `LoopSlotId`
wrap a `SlotId { index, generation }`: a prepared slot and its generation. The
engine refuses an ID whose slot is occupied, outside the pool or not newer than the
slot's last generation, so a stale ID never addresses a new occupant. IDs are not
bounded by an initial capacity. A reconnecting client takes generations above the
floors of `Generations`.

## Codec

`codec` encodes one message into a record: a little-endian `u16` total length
followed by the message, every field little-endian, at most `MAX_RECORD_BYTES`.
Encoding writes into a caller's buffer and decoding reads a borrowed slice; neither
allocates. Decoding checks every tag, length and name and rejects trailing bytes
(`CodecError`).

## Connections and carriers

Shared connection state, credits, queues and typed/record delivery live in
[`shoop_rt_transport`](../shoop_rt_transport). This schema crate has no transport,
engine or host dependency. Every encoded carrier reuses this codec; typed local
connections can skip encoding entirely.

```sh
cargo test -p shoop_rt_protocol
```


## Host setup

`host::{HostRequest, HostReply}` is a separate, versioned host-control family.
Discovery is paginated; settings, segments and selected MIDI ports are bounded
records. Device names/errors use UTF-8 `host::Text` (256 bytes). The host converts
these values to its own configuration and implements them before preparing an
engine. `codec::{encode_host_request, decode_host_request, encode_host_reply,
decode_host_reply}` reuse the canonical record framing; carrier bootstrap remains
outside this schema.
