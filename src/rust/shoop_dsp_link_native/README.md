# Native DSP-link services

`Session` owns authenticated control and UDP negotiation for the existing server.
It can supply a portable `Descriptor`; TCP handles, credentials and addresses
remain in this native crate. The standalone measurement CLI reuses these session
operations without moving its source clock or capture arrays into the library.

`udp::Adapter::poll` services prepared memory endpoints outside rendering.
`udp::Worker` runs that adapter on a native thread; `wait_until` is only for host
or test coordination outside callbacks. `stop` returns the adapter; pause/save/
statistics calls and final cleanup remain with the control owner. Dropping a
worker requests stop without joining from an unknown caller context.
Control disconnection or two seconds with sent work outstanding but no advancing
result retires the native link. Duplicate replies do not renew progress.

These UDP/TCP requirements do not apply to generic DSP links. A future shared-
memory or socket IPC adapter supplies its own preparation, access/liveness and
buffer ownership while retaining the same portable graph node and DSP service.
