# Application and host port model

The application uses one normalized connection model on native and browser targets.

- **Application ports** belong to a stable owner. Track ownership distinguishes sync and main tracks; each active Lua MIDI registration has a deterministic script/registration owner and never requires a fabricated track.
- **Host ports** are the currently discovered external endpoint inventory. Their stable ID, display name, data type, and direction are independent of application ports. An empty host inventory is valid and does not hide application ports.
- **Confirmed links** come only from authoritative backend/worklet snapshots.
- **Pending links** are application-owner requests awaiting confirmation. Rejection, disappearance, saturation, and timeout remain visible without changing confirmed truth.

A link is compatible when data types match and directions oppose. The application/backend boundary validates this before mutation. The Connections dialog derives a four-column graph from the normalized inventories: System sources → ShoopDaLoop sinks → ShoopDaLoop sources → System sinks. External system ports are grouped by client/device name and application ports by track or script owner. Host-inventory rows owned by the resolved ShoopDaLoop application instance are excluded from the outer system columns, so each ShoopDaLoop port appears only in its inner application column. Audio, MIDI, and multi-track filters hide graph content without changing connection state. Audio and MIDI endpoint labels, connectors, and routes use consistent type colors rather than letter prefixes; endpoint hover text names the type explicitly. Dragging a source connector to a compatible user-managed sink emits a typed `HostPortId`; confirmed routes are drawn as lines and a user-managed line can be clicked to disconnect. The graph does not retain a second candidate list.

Track links are user-managed. Direct tracks publish Audio/MIDI input and output roles. Native External dry/wet tracks publish dry Audio inputs and sends, wet Audio returns and outputs, and optional dry MIDI input/send in deterministic index order. Carla tracks publish only dry inputs, wet outputs, and optional dry MIDI input; Carla's own endpoint ports and internal wiring never enter the host connection graph. Session capture/restoration retains exact confirmed host IDs for every public role.

Lua control links are owner-managed because script regex/autoconnect policy remains authoritative; the GUI marks their confirmed routes as managed and disables competing graph mutation. Script control-port IDs remain stable across stop/restart and disappear while stopped. Browser track and Lua control consumers share canonical `webmidi:source|sink:<MIDIPort.id>` host rows without duplicate namespaced copies. Before permission, denial, or on unsupported browsers, application ports remain visible with an empty MIDI host inventory.

The user-managed **Global FX Control MIDI In** port is application-owned and appears only in the all-tracks Connections graph. CC 0–119, channel pressure, and pitch bend fan out to every MIDI-capable FX processor without entering loop channels, recording, playback media, or automation. Inactive processors retain only the latest value per control and receive it when normal processing resumes; global MIDI never wakes sleeping DSP, and a saturated restore drains over bounded later blocks. Other MIDI messages are filtered. Connecting one source here and to a track input is additive: absolute controls can be applied twice, relative controls may behave incorrectly, and the regular track copy can be recorded. Processor mappings remain processor-owned, and a host API that cannot open one endpoint twice reports an unconfirmed connection rather than deduplicating traffic.

## Latency observations and cue identity

Each normalized application port also publishes independent capture and playback observations: minimum/maximum frames, exact/range/estimated/manual/unknown certainty, sample rate, and revision. Numeric/revision changes do not rebuild topology. Browser worklet protocol snapshots carry the same bounded values; absent APIs remain unknown rather than invented zero.

A track latency policy may select either its stable application output identity or one confirmed normalized host identity as the cue path. Application IDs are translated to backend IDs at the command boundary and translated back in authoritative snapshots; session documents retain the stable application ID. Host IDs use the same opaque `HostPortId` as the Connections graph. A missing selection remains visibly missing. More than one output connected to the selected host is path ambiguity: recipe resolution fails and diagnostics increment instead of choosing a route.

Provider path intervals are scoped. A JACK/end-to-end capture observation and a separately enabled backend-buffer component that describes the same interval are rejected as overlapping, preventing double counting. Processor and selected output intervals remain independent where providers establish that separation.

## Web Audio endpoints

After the explicit browser audio enable action, device channels are configured before track commands are replayed. Negotiated microphone channels appear as `webaudio:capture_N` host outputs and destination channels as `webaudio:destination_N` host inputs. Output-only mode has no capture host ports. A separate Web MIDI gesture publishes connected browser inputs as host outputs and browser outputs as host inputs. MIDI control can operate without audio; track MIDI waits for the AudioWorklet clock. Capture and destination endpoints are device boundaries, not an External effects host: the current browser processor catalog is empty and does not advertise External or Carla.

The AudioWorklet owns audio and track-MIDI routing truth. Bounded protocol commands mutate links, and subsequent worklet snapshots publish normalized application ports, host ports, and confirmed links. A main-thread hub owns Web MIDI permission, physical callbacks, script subscriptions, and browser output sends. The render callback consults fixed-capacity routes and queues without allocation or browser calls. Disconnecting a route changes actual event/audio flow; stale input after hotplug is dropped nonfatally.

Initial audio links preserve prior startup behavior but are explicit confirmed state:

- mono capture feeds mono input;
- a single capture channel fans out to stereo inputs;
- stereo capture maps by channel;
- mono output fans out to available destination channels;
- multi-channel output maps by channel and clamps excess channels to the last destination.

Session capture stores audio confirmed IDs and desired Web MIDI IDs. Transactional replacement removes startup audio defaults before restoring saved links, so a user's disconnected route is not silently re-enabled. Device loss removes confirmation while retaining desired MIDI identity; return of the same opaque browser ID reconnects compatible track routes and script regex policy. Web MIDI timing, payload limits, capacities, and permission behavior are specified in `web_midi_contract.md`.
