# Click preview — P15 complete

P15 is complete as accepted on 2026-11-04. System and Master preview, cancellation,
destination switching, persistent Master identity and deployment coverage are
finished. The implementation and recorded evidence below satisfy the accepted
scope; no additional testing is required for closure.

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

Master is an explicit bus identity in the service projection and saved session.
Renaming, reordering and save/load preserve that identity. Removing the destination
cancels playback and disables Master preview, even if another bus is named Master.
New Session restores the default destination. Older documents without a designation
adopt a uniquely named Master; ambiguous names leave Master unavailable.
System output availability is independent of Master availability. Browser playback
waits for a running AudioContext; rejected or stalled resume fails the request and
permits a retry.

## Ownership and cancellation

The client generates PCM through a separate codec worker, with generation fencing
and one active audition across both destinations. The browser retrieves generated PCM through the existing bounded codec image
transfer, rather than treating a worker handle as samples. System PCM crosses the native
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

Application protocol version is 20; engine protocol version is 7. Rebuild clients
and hosts together.

### Available-device acceptance (Linux, 2026-11-04)

- Native System: generated clicks reached a PulseAudio null sink through the actual
  Rodio/ALSA output path. Both channels peaked at 0.18427; natural completion and
  explicit Stop passed, with a zero final half-second capture.
- Master CPAL: a separate host process used ALSA/PulseAudio at 48 kHz with a
  1,024-frame buffer. Socket and shared-memory clients passed; both channels
  measured 0.25 for the supplied 0.25 PCM and returned to silence after Stop.
  A 256-frame buffer underruns under this shared environment's workload.
- Master JACK: a separate capture client connected to the host's actual JACK
  output ports at 48 kHz / 1,024 frames. Each carrier measured 0.25 on both
  channels, then zero after Stop. The server used its dummy hardware driver.
- Hosted Chromium: the real codec worker, System Web Audio output and Master
  AudioWorklet path passed the preview canary with zero xruns. Rejected resume
  reported failure and recovered; output capture peaked at 0.18661 on both
  channels and ended in silence. The unresolved-resume timeout and real browser
  microphone denial/retry probe also passed with zero xruns; captured peaks were
  0.18781 on each channel, followed by silence.

The opt-in native tests are `native_system_preview_device_acceptance` in the app
and `native_master_preview_device_acceptance` in host recovery. Run them with
nextest `--run-ignored only -E 'test(<name>)'` on an isolated audio device.
The host test reads an AudioDriverConfig JSON from `SHOOP_PREVIEW_DRIVER_CONFIG`;
`SHOOP_PREVIEW_LOG_DIR` optionally retains host logs. JACK measurement is internal
to the test; System/CPAL require an external device/monitor capture. Hosted canary
commands and failure probes are documented in the application README.

### Completion and accepted validation boundary

The recorded output measurements use Linux virtual devices and separate local
processes. Physical listening on macOS/Windows and a second physical client/RT
machine were not performed. The user accepted this boundary and closed P15
without additional testing or waiting for further CI results. Outstanding CI is
assumed successful for this closure, rather than recorded as an observed pass.
The retained canaries and opt-in device tests remain available for maintenance.
Bus FX stays under P08.
