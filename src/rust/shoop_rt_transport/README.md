# Shared engine transport

Portable connection machinery for `shoop_rt_protocol`: `ProtocolClient` owns
handshake, correlation and credits; `carrier::{Endpoint, Carrier, Outbox}` define
the boundary. `in_process::InProcess` delivers typed messages directly without
serialization. `records::{RecordClient, RecordServer, RecordPort}` reuse the
canonical codec over any record carrier. Receive-side failure is terminal and
reported even without another send; idle polls remain distinct from disconnect.

`buffer` and `framing` provide prepared byte storage and record batching shared
with the worklet. The `contracts` feature exposes reusable test endpoints and
contracts, with no production engine/application dependency. Native test fixtures
can coordinate an independent peer without changing the protocol.

Concrete mechanisms live in `shoop_rt_transport_message_port`,
`shoop_rt_transport_socket` and `shoop_rt_transport_shared_memory`; hosting belongs
to `shoop_rt_host_<environment>`. This crate depends only on the protocol.

```sh
cargo test -p shoop_rt_transport
```
