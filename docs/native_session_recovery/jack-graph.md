# Live native JACK graph control

The native application and independent `shoop_host` expose the external audio/MIDI
graph through `SessionSnapshot.host_graph` and `AppSnapshot.host_graph`. Clients send
`Command::SetHostGraphConnected(HostGraphRequest)` or the matching `AppIntent`.
The same command/result/snapshot path works in process, over socket/shared-memory
IPC, and over trusted-LAN TCP. Application protocol version 16 is required at both
ends. Engine and carrier protocols are unchanged.

The shared [port graph contract](port-graph.md) defines identities, capabilities,
publication states, generic errors, logical mappings and provider responsibilities.
JACK-specific native identifiers and naming rules stay in the JACK implementation.

Connection requests carry the provider epoch, opaque local/peer endpoint IDs, and
the desired state. Names are display labels; clients resolve choices to IDs using
the current inventory. Links reference IDs as well. A local endpoint must have a
publication key and match its peer's media type and opposite direction. Endpoint
replacement is refused even when the same name is reused. Renames and unrelated
hotplug preserve existing endpoint identities. Snapshot revisions describe updates
without rejecting otherwise valid requests after unrelated graph changes.

Observed links touch the host's published endpoints. Accepted edits remain pending
until the resulting graph has been read and confirmed. External patchbay changes,
hotplug and rename appear through snapshots without a frontend refresh command.

All JACK queries, allocations and mutations run on the control worker. JACK
notifications update preallocated atomic counters. The callback retains its fixed
bank and performs no graph work. Normal deactivation allows the MIDI reset block
to cross the graph before removing ports, with a bounded deadline.

The shared channel bounds outstanding operations at 64, ports at 4096, links at
8192, publications at 256, labels/selectors at 256 bytes, and diagnostics at 512
bytes. JACK's native registration index table is separately bounded at 65536 slots.
Oversized/unreadable graphs become unavailable with diagnostics instead of being
reported as complete partial inventories. Worker shutdown refuses outstanding work.

The configured JACK bank is still fixed. JACK supports discovery, connections and
renaming, and reports dynamic publication/removal as unsupported until safe engine
bindings are implemented. Live graph edits are not saved as session/startup intent.
There is no new patchbay UI or MIDI bus support. Logical application routing remains
in `connections` and is separate from physical graph connections.

Socket IPC and TCP clients can detach/reconnect without stopping playback or
recreating ports. Shared-memory sessions retain their existing one-attachment limit.

Run `python3 scripts/run_native_jack_tests.py` for strict acceptance against an
isolated JACK dummy server. It includes shared provider conformance, rapid port
replacement, audio/MIDI hotplug, graph control/observation and UI reconnect during
playback over the existing native session carriers.
