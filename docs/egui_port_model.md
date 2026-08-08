# egui application and host port model

The egui application uses one normalized connection model on native and browser targets.

- **Application ports** belong to a stable owner. Track ownership distinguishes sync and main tracks; each active Lua MIDI registration has a deterministic script/registration owner and never requires a fabricated track.
- **Host ports** are the currently discovered external endpoint inventory. Their stable ID, display name, data type, and direction are independent of application ports. An empty host inventory is valid and does not hide application ports.
- **Confirmed links** come only from authoritative backend/worklet snapshots.
- **Pending links** are application-owner requests awaiting confirmation. Rejection, disappearance, saturation, and timeout remain visible without changing confirmed truth.

A link is compatible when data types match and directions oppose. The application/backend boundary validates this before mutation. The egui matrix derives compatible cells from the normalized inventories and emits a typed `HostPortId`; it does not retain a second candidate list.

Track links are user-managed. Lua control links are owner-managed because script regex/autoconnect policy remains authoritative; egui displays their confirmed host truth but disables competing cell mutation. Script control-port IDs remain stable across stop/restart, disappear while stopped, and use raw MIDI endpoint IDs for stable host identity. With the Wasm null MIDI service, enabled APC registrations remain visible while the MIDI host inventory and confirmed-link set are empty.

## Web Audio endpoints

After the explicit browser audio enable action, device channels are configured before track commands are replayed. Negotiated microphone channels appear as `webaudio:capture_N` host outputs and destination channels as `webaudio:destination_N` host inputs. Output-only mode has no capture host ports. Track audio and MIDI application ports remain visible in either mode; browser MIDI host inventory is empty because Web MIDI is out of scope.

The AudioWorklet owns routing truth. Bounded protocol commands mutate links, and subsequent worklet snapshots publish normalized application ports, host ports, and confirmed links. The render callback consults fixed-capacity device-channel routes without allocation. Disconnecting a route changes actual staged input or destination mixing; a failed mutation is reported without stopping the worklet.

Initial audio links preserve prior startup behavior but are explicit confirmed state:

- mono capture feeds mono input;
- a single capture channel fans out to stereo inputs;
- stereo capture maps by channel;
- mono output fans out to available destination channels;
- multi-channel output maps by channel and clamps excess channels to the last destination.

Session capture stores confirmed host IDs. Transactional replacement removes startup defaults before restoring saved links, so a user's disconnected route is not silently re-enabled. Device loss/retry republishes host inventory and reconciles pending requests against authoritative snapshots.
