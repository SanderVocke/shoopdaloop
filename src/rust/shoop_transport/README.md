# Shared bounded record delivery

`shoop_transport` owns the protocol-independent `RecordPort` contract, delivery
lanes, byte limits/errors, fixed-capacity buffers and length-prefix framing.
It has no production dependencies. A record includes its little-endian u16 total
length and is at most 1280 bytes. Reliable/bulk delivery refuses on saturation;
telemetry may replace older values.

`shoop_transport_socket` provides versioned keyless connection/inspection setup and bounded socket
records. `shoop_transport_shared_memory` provides process-shared queues and
liveness monitoring. Neither interprets app or engine messages. A protocol adapter
owns encoding, sequencing, credits and lifecycle semantics.

Engine-specific typed/encoded connection behavior remains in `shoop_rt_transport`.
The old socket/shared-memory crate names re-export these shared implementations
and retain engine-specific client/server aliases during migration. Implementations
are not duplicated. App attachment and service semantics belong to
`shoop_app_connection`, independently of byte delivery.

The existing engine carrier contracts exercise socket and shared-memory delivery.
Buffer/framing unit tests now live in this crate; their Miri inventory registration
moved with them without changing workloads or seeds.

Reliable records take priority over unsent low-priority records. Bulk, telemetry
and profiling rotate fairly. A partially written record always finishes before
switching lanes; a queued record with no bytes written can yield to reliable
traffic without allocating another queue slot.
