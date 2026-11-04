# Click preview

The click dialog retains two audio destinations:

- **System** plays on the client machine, through native Rodio/default system
  output or the browser's Web Audio destination. It does not use the looping
  driver's output routes. Native builds enable `system-preview` by default;
  this feature is independent of the host's `cpal` and `jack` features.
- **Master** plays on the RT host, including when the UI is connected over a
  socket or trusted-LAN TCP. Mono click audio feeds every channel of the chosen
  bus before gain, balance, mute and output routing. Playback uses the graph's
  audio clock and driver. Bus FX remain a separate restoration under P08.

Both buttons audition audio clicks; MIDI click generation/insertion is supported,
but MIDI-to-audio audition is outside the existing preview contract. Preview does
not create loops, alter recording content, or become session-file media.

The existing dialog resolves Master by its name at request time. There is no
fallback to another bus when Master is absent or renamed. Once resolved, a request
carries the bus identity, so renaming or reordering cannot redirect an admitted
preview. Removing that destination cancels playback. System output availability
is independent of Master availability; browser output still requires permission
for Web Audio playback.

## Ownership and cancellation

The client generates PCM through a separate codec worker, with generation fencing
and one active audition across both destinations. System PCM crosses the native
application mailbox or browser cooperative runtime to the platform player.
Callbacks only update the matching queued/playing request. Stop, replacement and
session-epoch changes discard queued PCM and obsolete generation results.

Master uses authority-fenced session commands to stage mono PCM. Each record
contains at most 256 finite samples and fits the 1280-byte wire limit. The client
pipelines at most 16 records; the service reserves at most 4,000,000 mono frames
(16 MB). Sample rate must match the host. The backend uses its existing prepared
content upload/validation machinery, independently of persisted loop media.

An RT source cursor is pinned until playback or its stop operation finishes. The
bus mixer uses preallocated buffers and render-class resource reads across page
boundaries. Control polling reclaims the source; the callback neither allocates
nor releases heap owners. Playback status comes from RT telemetry and completion
receipts. Musical edits do not wait for a preview's duration; session replacement
stops and drains it before replacing the graph.

Stop invalidates unsent chunks and cancels host staging/playback. Losing the UI
lease, replacing the session or removing the destination cancels host audition.
Reattachment never restarts a preview. Engine disconnect cleanup retains the
source until the RT cursor has stopped and drops answers belonging to the old
connection.

## Validation

- `shoop_rt_engine/tests/preview.rs`: output samples, stereo replication, gain,
  mute, natural completion across page boundaries, Stop, resource reuse, bus
  removal and engine disconnect. The harness checks heap-free control/render work.
- `shoopdaloop` persistence preview tests: System PCM delivery, obsolete callbacks,
  Master playback, cancellation during upload, lease loss, session replacement
  and unchanged loop identities/content revisions.
- Native application actor test: System PCM crosses the production mailbox
  independently of the host driver.
- `shoop_host/tests/recovery.rs`: actual independent host playback over socket and
  shared-memory connections, confirmed bus output and unchanged loop content.
- `shoop_rt_protocol/tests/protocol.rs`: preview commands and telemetry round-trip
  through the engine wire codec.
- `shoop_app_connection/tests/network.rs`: Master staging/playback/cancellation
  through throttled serialized records, bounded ingress and unchanged media.

Application protocol version is 15; engine protocol version is 5. Rebuild clients
and hosts together. Native cross-platform audio-device and hosted browser audible
acceptance remain required before closing P15; compile checks do not prove output
on a physical device.
