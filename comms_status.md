# Communication migration status

## Background and direction

We are moving the application away from per-operation request/reply bookkeeping
toward grouped fire-and-forget commands and complete state snapshots. The goal is
simple, bounded real-time execution that can later map to hardware. The current
implementation is Rust-only; ARM/FPGA interfaces and drivers are not implemented.

There are two communication boundaries: UI/application to control, and control
to hard real-time processing inside `rt_runtime`. Networking and browser delivery
belong outside the internal RT/control bridge.

The new command path carries complete groups through a bounded SPSC FIFO. RT
captures the published cutoff at callback entry and consumes that prefix at the
first eligible processing boundary. Commands have no execution deadlines, request
IDs, cancellation tickets or individual execution replies. Each group carries
the snapshot configuration version and uses addresses within that configuration.
RT checks the header once, then attempts commands in order. Individual failures
leave state unchanged and do not suppress later commands; diagnostics are
aggregate. Local submission can still report lack of capacity.

The new observation path publishes coherent, configuration-versioned snapshots
from the first settled processing tick of each audio callback. Callbacks that
only buffer a partial quantum publish no new state. Control reads the newest
complete snapshot and delivers it to the UI. Native in-process delivery defaults
to pull; browser and network delivery default to push. Both modes are supported,
and drawing can use the latest accepted state without waiting for a response.

Snapshots show current conditions, not the execution history of individual
commands. UI logic should use those observations when possible. Operations that
cannot yet fit this model stay outside the migrated subset until separately
designed. The existing communication machinery remains operational alongside the
new paths during this incremental migration.

Detailed designs and implementation records are in [commands.md](commands.md)
and [SNAPSHOTS.md](SNAPSHOTS.md). This document is an overview of the migration
boundary, not an additional implementation plan.

## What is already migrated

- Ordinary loop and mixer display uses snapshots, including cursor position,
  playback mode, gain/balance/mute, meters and loop timing/media metadata. Control's
  timing cache also uses snapshots; redundant background timing queries and the
  temporary legacy display selector were removed. Snapshots contain no media bytes
  or owning resource leases.
- The explicit `RtBackend::immediate_group` API supports loop gain/balance, strip
  input/output gain/balance and output mute, bus gain/balance/mute, and immediate
  restart-at-zero/stop for prepared ordinary audio loops. Unsupported playback
  conditions fail individually; recording finalization is not an immediate stop.
- The internal command ring stores actual group sizes. Its 256 records support
  128 singleton groups or up to 248 commands in larger groups; each application
  message currently holds 1–32 commands. Complete publication and a fixed cutoff
  prevent RT from waiting for an unfinished group or chasing a moving producer.
- Immediate groups coexist with legacy graph, media and recording operations.
  Control preserves ordering between paths and reconciles snapshot state before
  releasing later legacy work. Consecutive immediate groups need no intervening
  snapshot. There is no permanent mutation lock or re-preparation requirement.

## Areas still using legacy paths

| Area | Current boundary and migration complication |
| --- | --- |
| **Ordinary UI command wiring** | Normal `Backend` mutation methods still use legacy requests, including gain, balance and basic playback. The new immediate API is available but ordinary controls have not been switched to it. This is relatively straightforward integration work: route suitable actions through groups and replace assumptions about execution replies with snapshot observation. |
| **Other simple controls** | Monitoring policy, playback defaults, smoothing settings and some effect parameters remain on their existing paths. Some may be easy additions, but each needs bounded execution and defined individual failure behavior. Not every remaining legacy command is intrinsically difficult. |
| **Graph changes and processor lifecycle** | Creating/removing loops, strips, buses and processors, changing routing, and replacing a session use staged operations. They require preparation, coordinated activation, configuration-version changes and safe retirement of displaced objects. A simple scalar command does not cover that lifecycle. |
| **Media transfer and persistence** | Loading recordings, exporting audio/MIDI and saving sessions use content operations and bulk transfers. They involve chunked reliable data, revisions, stable read cuts and resource ownership. Latest-state snapshots cannot replace reliable delivery of media bytes or preserve an export's lifetime. |
| **Recording and capture operations** | Recording, replacing, grabbing input history and clearing content manage writers, reserved storage and completed takes. They may need preparation, coordinated changes and finalization into immutable media. Their effects extend beyond changing a mode field. |
| **Musical timing and synchronization** | Quantized transitions, repeat-sync, iteration seeking, synchronization relationships and timing edits depend on musical boundaries, revisions and interactions with existing actions. Their migration needs a design for those musical semantics while keeping future-deadline scheduling out of the immediate FIFO. |
| **Composites, processed playback and MIDI** | Composite programs can coordinate nested child actions and iterations. Processed playback includes dry/wet roles and captured processor state. MIDI needs correct note termination and state restoration. These behaviors cannot be reduced to the ordinary prepared-audio start/stop handler. Some individual effect parameters may still be straightforward to migrate. |
| **Additional feedback streams** | Composite state, effect/synth state, recorded processor metadata, preview state, profiling, memory reports and engine-health information retain separate state/telemetry paths. Some already push updates. Migration requires deciding which fields belong in coherent display snapshots and which should remain independent streams, with suitable capture consistency, size and delivery rates. |
| **Connection setup and resynchronization** | Version/capability negotiation, inventory and structural configuration resynchronization remain. Live snapshots refer to a known configuration rather than repeating the graph topology, so a newly connected UI still needs that context. Reconnect must discard unsent work and avoid automatic command replay. These control-plane services need not all become RT commands or display snapshots. |

Compatibility telemetry also remains available for raw protocol clients that have
not enabled snapshot observation. The ordinary UI display no longer selects that
legacy display mode. Effect and composite observation remain on their existing
paths even when ordinary loop/mixer snapshots are enabled.

## Ownership returns and resource retirement

The legacy completion/return machinery also transfers owned graph plans,
processors, payload buffers and media-related owners back to control for reuse or
reclamation. Destruction and unbounded cleanup must stay outside the audio callback.

This is a separate requirement from acknowledging command execution. Removing
per-command replies must preserve a reliable way to retire and return resources.
Lossy latest-state snapshots cannot carry that responsibility, and the immediate
FIFO deliberately contains value commands without owned heap objects. A future
migration can redesign the ownership-return channel without adding request IDs
or execution replies to ordinary immediate commands.

The remaining work must preserve configuration identity, ordering between command
paths, musical behavior and object lifetimes while keeping RT execution bounded.
The completed snapshot and command experiments provide the transport foundation;
the remaining application and lifecycle migrations are still separate work.
