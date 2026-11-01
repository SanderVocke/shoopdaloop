# Shared-memory record transport

`Port::create` creates six per-launch services (one per direction/lane);
`Port::open` validates and joins their descriptor. Both implement the common
record port and use the same codec/client/server as sockets and MessagePort.

The descriptor identifies the layout version, fixed 1280-byte record storage,
exact queue capacities and a random session. It travels over private launch
bootstrap, not the engine schema. `iceoryx2 = 0.10.0` owns mapping/alignment,
relative addressing, publication/recycling and sample lifetimes; no Rust engine
owner or raw pointer crosses the mapping. Reliable/bulk queues reject full sends;
telemetry safely replaces its oldest queued sample. This copies encoded records,
not zero-copy engine media. The threadsafe service variant permits moving a port
between control threads; its local synchronization never reaches rendering.

Attach a private socket with `with_liveness`: an empty queue is idle, whereas EOF
is terminal. No normal protocol record is forwarded through that socket. Drop
removes this peer's handles; `cleanup_dead_peer` invokes the dependency's dead-node
cleanup within the session namespace. No application session reconnect is supplied.

`cargo test -p shoop_rt_transport_shared_memory` covers descriptor rejection,
bounded FIFO/latest-value models and recycling, the shared subprocess contract,
10,000 coherent concurrent payloads, and peer kill/reclamation during publication.
