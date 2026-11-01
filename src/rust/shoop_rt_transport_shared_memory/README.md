# Shared-memory transport

Both native ends exchange encoded records through six iceoryx2 0.10.0 queues:
reliable, telemetry and bulk in each direction. Fixed 1280-byte payloads contain
bounded codec records, never Rust pointers/owners. Reliable/bulk backpressure is
refusal/retry; telemetry uses safe oldest-drop overflow. `client`/`server` reuse
the common record adapters without another message implementation.

A private `Descriptor` carries version, record width, queue capacities and a fresh
session namespace. `Port::create` and `open` validate the descriptor and negotiated
layout. The safe threadsafe IPC API owns mapping alignment, relative addressing,
publication and recycling. An optional TCP handle detects liveness only; ordinary
protocol records never cross that handle. Namespace-scoped dead-peer cleanup
reclaims abandoned resources without touching the live engine's runtime memory.
No separately running shared-memory service is required.

Native subprocess tests cover shared contracts, bounded queue models, 10,000
coherent full-width publications, bad layouts, producer death and reclamation.
These are OS IPC tests; Miri tests portable record/buffer ownership separately.

```sh
cargo test -p shoop_rt_transport_shared_memory
```
