# Socket transport

Native `client` and `server` aliases compose one bounded `Port` with the common
record client/server. It adapts loopback TCP streams to canonical protocol records
with fixed-capacity reliable/telemetry/bulk queues, retained partial writes and
incremental length-prefix parsing. Reliable and bulk queues refuse when full;
telemetry may replace an older queued record. EOF and malformed/truncated records
are terminal errors, distinct from idle. All socket work runs outside rendering.

`setup::Listener` binds an ephemeral loopback endpoint. The peer authenticates
with a fresh token passed privately by its host; tokens are never command-line
arguments. Each host decides connection and engine lifetime. This carrier has no
engine, application or process-management dependency. Remote-network deployment
is outside this milestone.

```sh
cargo test -p shoop_rt_transport_socket
```
