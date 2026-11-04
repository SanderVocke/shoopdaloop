# Live native JACK graph control

The native application and independent `shoop_host` expose the JACK server's current
physical graph through `SessionSnapshot.host_graph` and `AppSnapshot.host_graph`.
Clients send `Command::SetHostGraphConnected(HostGraphRequest)` or the matching
`AppIntent`. The same command/result/snapshot path works in process, over socket or
shared-memory IPC, and over trusted-LAN TCP. Application protocol version 15 is
required on both ends; the engine and carrier protocols are unchanged.

## Contract

A graph snapshot contains a random driver epoch, monotonic graph revision, availability,
a bounded list of audio/MIDI ports, and observed connections touching the host's bank.
Port names are the actual JACK names, including any client-name collision suffix.
`owned` identifies bank ports. Directions are JACK directions: an owned input connects
to a peer output, and an owned output connects to a peer input of the same data type.
Third-party connections are excluded from the link list and cannot be edited.

Commands supply the epoch and revision from the client's latest snapshot, the full
owned `local` port name, a compatible `peer`, and the desired `connected` state.
The host rejects stale graphs, unknown endpoints, incompatible directions/types,
and attempts to edit a connection without an owned local endpoint. Already-matching
connections are idempotent. Stale requests receive `RevisionConflict`; clients refresh
and let the user retry. Revision checking is conservative across the entire graph,
including unrelated port registration/removal/rename.

The service applies its existing session authority, controlling-lease fencing, operation
identity/deduplication, pending/result limits and reconnect reconciliation. Accepted
operations remain pending until the JACK worker has queried the actual resulting link.
A refusal never invents a confirmed link. Snapshots also reflect changes made by an
external patchbay and port hotplug without a client refresh command.

All graph queries, allocations, validation and JACK connect/disconnect calls run on
the driver's control worker. JACK notification callbacks only increment atomic change
counters. The process callback retains its fixed bank and performs no graph work.
The worker bounds outstanding operations at 64, ports at 4096, links at 8192, names
at 256 bytes and diagnostics at 512 bytes. An oversized/unreadable graph is unavailable
with a diagnostic; it is not published as a complete partial graph. Shutdown makes
retained control handles unavailable and refuses queued work. Normal deactivation
waits for two further callbacks within a bounded deadline so the MIDI reset block
can reach connected peers, including feedback edges.

JACK has no atomic compare-and-connect operation. Epoch/revision checks and coherent
reads detect stale observations, including same-name port replacement, but another
JACK client can still change the graph between validation and mutation. Confirmation
reports an observed state, not a permanent connection guarantee.

## Scope and next step

The host still publishes the configured fixed audio/MIDI bank. This change does not
register ports per track or bus, add MIDI buses, add a patchbay UI, or save live physical
graph edits into session files/startup settings. Logical application-to-bank routing
remains in `connections`; it is separate from this physical graph. Non-JACK hosts
publish unavailable graph state and refuse graph commands as unsupported.

The next step is track audio/MIDI and audio-bus port lifecycle parity with master,
using this graph contract and the independent host's ownership of the JACK client.

Socket IPC and TCP clients can detach/reconnect without stopping playback or
recreating ports. Shared-memory sessions retain their existing one-attachment limit;
graph discovery/control works during that attachment. No carrier lifecycle behavior
is changed here.

## Validation

`python3 scripts/run_native_jack_tests.py` runs strict acceptance against an isolated
JACK dummy server. The graph case runs through in-process, socket IPC, shared-memory
IPC and trusted-LAN TCP delivery. It checks audio/MIDI peer hotplug, compatible and
invalid edits, idempotence, stale epoch/revision and same-name replacement, external
patchbay changes, actual recorded/replayed audio and MIDI, and socket/TCP UI
reconnection while playback continues. Portable service tests cover deferred
completion, refusal, authority fencing and retained results after detachment.
