# Live native JACK graph control

The native application and independent `shoop_host` expose the external audio/MIDI
graph through `SessionSnapshot.host_graph` and `AppSnapshot.host_graph`. Clients send
`Command::SetHostGraphConnected(HostGraphRequest)` or the matching `AppIntent`.
The same command/result/snapshot path works in process, over socket/shared-memory
IPC, and over trusted-LAN TCP. Application protocol version 20 is required at both
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
notifications update preallocated atomic counters. The callback uses preallocated I/O slots and bounded owner handoff queues; graph
queries and registration stay on the control worker. Publication is confirmed after
the callback acknowledges its binding. Removal clears audio, emits MIDI reset and
returns owners for off-thread unregistration after a short callback fence. Normal deactivation allows the MIDI reset block
to cross the graph before removing ports, with a bounded deadline.

The shared channel bounds outstanding operations at 64, ports at 4096, links at
8192, publications at 4096, labels/selectors at 256 bytes, and diagnostics at 512
bytes. JACK's native registration index table is separately bounded at 65536 slots.
Oversized/unreadable graphs become unavailable with diagnostics instead of being
reported as complete partial inventories. Worker shutdown refuses outstanding work.

Track audio/MIDI, sync audio and audio-bus output ports are published dynamically.
JACK starts without public ports; the session owns every public endpoint. Private
prepared slot counts derive from session pool capacity, with up to 1024 slots per media type and
direction. Slots are reused only after callback retirement and native unregistration.
Creation/removal results aggregate engine and provider acknowledgements; publication
failure rolls back creation.

The normal connection dialog receives live physical inventory and confirmed links
through the shared service view. Session capture saves opaque peer selectors and
intended/observed links; reload waits for new publications and bindings and retains
missing-peer intent for hotplug restoration. Local selectors survive client name
suffixes. Default Master playback selects a physical device with enough channels;
track inputs select physical capture endpoints. External patchbay edits remain
visible and are captured. MIDI buses and global FX control are outside this feature.

Socket IPC and TCP clients can detach/reconnect without stopping playback or
recreating ports. Shared-memory sessions retain their existing one-attachment limit.

Run `python3 scripts/run_native_jack_tests.py` for strict acceptance against an
isolated JACK dummy server. It includes shared provider conformance, rapid port
replacement, audio/MIDI hotplug, graph control/observation and UI reconnect during
playback over the existing native session carriers. The dynamic workflow starts
without a fixed port bank, creates a track and bus, records/replays audio and
MIDI, restores a session while peers are absent, reconnects them after hotplug and
removes the published ports. Additional tests cover prepared capacity, routing past
64 ports, MIDI reset, callback heap guards and provider-independent rollback.
