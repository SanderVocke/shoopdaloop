# Socket record transport

`Port` implements the common `RecordPort` with a nonblocking TCP stream, prepared
queues per lane, bounded reads/writes and retained partial-write offsets. Reliable
and bulk records backpressure; queued telemetry can discard its oldest record.
`client::Client` and `server::Server<E>` are the shared record adapters specialized
to this port, not separate protocol implementations.

`setup::Listener` binds an ephemeral loopback address with a random launch token;
`setup::connect` authenticates a launched peer. Pass the token over a private
launch channel, never arguments/logs. Physical connect/accept direction does not
determine logical engine/client roles. Host configuration and process supervision
are outside this crate.

`cargo test -p shoop_rt_transport_socket` runs the shared lifecycle/credit/bulk
contract across a subprocess, plus framing, authentication, EOF and real socket
backpressure/retry tests. No transport-specific musical scheduling or latency
compensation is performed.
