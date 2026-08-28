# Web MIDI contract

The production Wasm application can use the browser Web MIDI API for direct-track MIDI and Lua control ports. Web MIDI is independent of microphone permission and AudioContext startup: controller scripts can run after Web MIDI access is granted even when browser audio has not been enabled. Direct-track MIDI needs the AudioWorklet clock to record, monitor, or play.

## Availability and permission

Web MIDI is offered only when `navigator.requestMIDIAccess` exists. The application never requests access at page load. **Enable Web MIDI + SysEx** explicitly requests access with SysEx enabled so the existing 256-byte Lua-control contract can be honored when the user and browser permit it.

The browser surface distinguishes awaiting gesture, requesting permission, running, denied, failed, and unsupported states. Denial and failure leave non-MIDI application behavior usable and expose retry. Hosted HTTPS and localhost are the portable environments. Direct-file support depends on browser policy and is not claimed where the API is absent or denied.

Chrome/Chromium is the production functional verification browser. Other browsers are supported only when they expose a compatible Web MIDI API; otherwise the application reports unsupported state without inventing endpoints.

## Endpoint identity and direction

A connected browser `MIDIInput` is a host source with ID:

```text
webmidi:source:<MIDIPort.id>
```

A connected browser `MIDIOutput` is a host sink with ID:

```text
webmidi:sink:<MIDIPort.id>
```

The opaque browser ID and direction form identity. Manufacturer and name form display text and regex input only. Map order, display name, and reconnect order never identify a port.

One main-thread hub owns `MIDIAccess`, physical ports, and one callback per input. State refreshes reuse handles whose device is connected and whose connection is open or pending, so notifications caused by a successful `open` do not reinstall handlers or create lifecycle churn. A connected handle observed as closed is replaced and reopened once. It fans input out to user-managed track routes and owner-managed Lua subscriptions. Both consumers publish the same canonical host rows. Track route confirmation comes from AudioWorklet snapshots; Lua link confirmation comes from the scripting manager's logical subscriptions.

Endpoint state changes remove current host truth without deleting desired track routes. Reappearance of the same stable endpoint restores compatible desired track routes and script regex autoconnect.

## Timing and event behavior

Web MIDI has no sample clock shared with AudioWorklet. Accepted track input is assigned to frame zero of the next available render quantum, matching the coarse timing class of CPAL+midir. Output preserves engine event order but incurs worklet polling, main-thread, and browser scheduling latency. No sample-exact Web MIDI claim is made. Latency capability is therefore coarse/manual or unknown; policy and frozen-take metadata can cross protocol version 18, but Web MIDI permission or device return changes only current observations and never retimes an existing take.

Track recording accepts nonempty messages of at most four bytes, the engine's fixed realtime payload. Lua control accepts nonempty messages of at most 256 bytes, including SysEx when permission is granted. Messages are rejected rather than truncated.

The worklet render callback never calls browser APIs, awaits promises, serializes data, allocates, or locks. Main-thread control code owns browser sends and asynchronous open/access state.

## Bounds and failures

| Boundary | Capacity | Failure behavior |
|---|---:|---|
| Hub logical input subscriptions | 256 subscriptions | Refuse the new subscription with a script diagnostic |
| Hub input queue per control subscription | 1,024 messages | Drop newest and increment the subscription drop counter |
| Hub track-input queue | 1,024 messages | Drop newest and increment track drops |
| One protocol MIDI batch | 128 messages | Reject malformed/oversized batches |
| Worklet staged input per track/quantum | 256 messages | Refuse excess events and publish the count with protocol overflow diagnostics |
| Worklet pending track output | 1,024 messages | Drop newest and publish overflow count |
| Recorded-track payload | 4 bytes | Refuse and count; never truncate |
| Lua-control payload | 256 bytes | Refuse and count; never truncate |

Live input batches are ephemeral and are never journaled. Endpoint inventory and desired routes are journaled in generation-safe order. Stale input after hot-unplug is discarded nonfatally. Worklet restart replays endpoint and route configuration without replaying old live input. A generation-current asynchronous port-open failure removes that endpoint from physical and script connection truth until a later state refresh successfully reopens it; failures from superseded refreshes cannot remove the newer handle but remain visible as stale-generation diagnostics. Output-send/open errors and permission failures remain observable without running browser APIs in render processing.

## Persistence

Existing session port connection lists store canonical Web MIDI host IDs. Explicit disconnect stores no route. A desired route to a temporarily missing stable endpoint remains in session/backend desired state and is confirmed only after the endpoint returns. Sessions written before Web MIDI support contain no such IDs and load unchanged.
