# Portable session connections

`no_std` with `alloc`; no concrete carrier, OS, clock, device host or Lua dependency.
Native hosts and future Cortex-A control tasks share the same implementation.

- `Session<P: RecordPort>` drives encoded service requests and confirmed state.
  Call `advance(elapsed)`, `receive(service)`, then `write(service)`. The composition
  root ticks the service once per turn, separately from the connection count.
  After `alive(service)` becomes false, detach its `ConnectionId` and retire its
  carrier. `with_network` adds heartbeat policy and bounded snapshot/media
  scheduling. Initial data endpoint identity greeting belongs to composition.
- `Management<P>` emits its identity greeting, dispatches fenced requests and
  retains a response across backpressure. `pump` returns whether to keep it.
  Device cleanup remains a host responsibility.
- `Upload` validates and segments immutable images cooperatively. Feed received
  records to `received` to recycle upload credits. The native client projection
  remains outside this crate.
- `Heartbeat` consumes matching probes/replies using injected elapsed time.
  `HeartbeatConfig` validates interval/timeout bounds; the host supplies a nonzero
  connection-local sequence seed.

These are control-task components. Framing/byte queues in `shoop_transport` are
heap-free after preparation; service projections and persistence may allocate.
See [LAN setup and the RTOS adapter contract](../../../docs/trusted-lan-networking.md).
