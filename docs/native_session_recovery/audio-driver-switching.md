# Native audio driver switching

P14 restores confirmed device and engine transitions under the running session
service. The service retains application identities, connection authority and
operation results while its native composition root replaces the render host and
engine adapter. The independent host keeps its listening endpoints alive.

## Scope and ownership

| Configuration | Native scope | Client delivery |
| --- | --- | --- |
| Dummy | All supported native platforms | Embedded typed queues, socket and shared memory |
| CPAL | Built platform hosts, including ALSA/JACK, WASAPI/ASIO and CoreAudio where available | Same session commands and results |
| Direct JACK | Builds with native JACK support and a running server | Same commands, with dynamic audio/MIDI graph publication restored |
| Browser and firmware | Live native driver switching unavailable | Explicit capability refusal; existing rendering and protocol behavior remain |

CPAL's JACK host and the direct JACK backend remain separate choices. Build
availability, device/server availability and actual device qualification are
distinct. Mock and virtual-device tests do not establish physical-device behavior.

`AudioDriverController` is injected by the native composition root. Its bounded
requests carry service-generated tickets. A native worker owns discovery, device
opening/closing, conversion and startup-preference writes; none runs on a render
callback. The service polls results and retains confirmed work across client loss.
The RT adapter remains independent of device and application hosting code.
The worker accepts one outstanding service ticket, with two bounded request slots
and one result slot. Shutdown waits at most five seconds for lifecycle cleanup.
A stalled OS device call retains its worker and remains pending; the service never
starts a concurrent transition or claims that a timed-out device was reclaimed.

The shared `shoop_format_conversion` layer owns frame scaling, dependency timing,
audio/MIDI conversion and frozen composite geometry. Session-document and native
capture adapters share those algorithms without introducing a host/service
dependency on archive codecs or `shoop_session`.

## Transition contract

Discovery and preflight do not stop current audio. A request produces a bounded
confirmation describing source and target format, interruption, stopped transport
and conversion. CPAL preflight pins default/index/partial device selections to
resolved host and exact device names; those identities appear in confirmation and
become the saved active profile. Confirmation is scoped to its client authority and request ID;
unconfirmed preparation expires after 60 seconds or on owner loss. Automatic CPAL
buffer selection is explicit: preflight leaves callback size unresolved, confirmation
names the device-default policy, and opening publishes the observed size. Engine
capacity is never presented as a negotiated device buffer.

Confirmed admission checks active, armed, scheduled and composite-driven recording
and conflicts with persistence or pending structural work. During capture through
publication, other session mutations and shutdown refuse Busy. Recording is never
stopped merely to make switching admissible. The check runs again before teardown.

The service captures immutable media and metadata, prepares an identity-preserving
candidate, rechecks the target format, and then closes the old engine connection.
Strict native startup never substitutes Dummy for a failed target. After the new
adapter restores all media and mappings, the service republishes graph bindings,
controls and routing intent and confirms runtime success. Loops start stopped;
pending musical commands, held notes, previews and rolling histories are transient.
Client Lua VMs are neither serialized nor automatically restarted.

Public track, loop, bus, channel and port identities survive. Backend handles and
the persistence epoch change. Stale engine replies and transfers cannot operate
on the replacement. Missing physical ports retain connection intent and a
diagnostic rather than binding to a similarly named destination.

Failure before teardown leaves the prior runtime intact. Later failure reconstructs
the original-format session on its prior driver. The original immutable cut remains
owned until restoration/publication succeeds. Failed rollback publishes a fatal
state with both errors and retains bounded recovery data. A new socket controller
can inspect the retained fatal result and request orderly host shutdown; musical
mutations remain refused. A target rate change
requires renewed consent, restoring the source first if opening the target was
necessary to discover it.

## Timing and capacity

Use the persistence contract's rational source-first dependency conversion and
quantum rounding. Convert each original channel once using its final timing map.
Audio retains prefix/suffix media, MIDI retains packet order and frame precision,
and signed offsets, capture alignment and preplay follow the same origin map.
Frozen composite geometry and authored timing basis remain distinct.

Host media/metadata/object/event limits apply to capture and conversion. Account
for the source image, converted image, upload scratch, target runtime and retained
rollback owners separately. Preparation failure and exhaustion report an explicit
result. Neither finite capacity nor allocation guards establish physical-device
deadline guarantees.

## Startup preferences

Runtime switching and saving its startup preference have distinct results. A failed
save keeps the new driver active and enables retry without another transition.
The service retains this state so retry does not require the originating UI draft.

Embedded applications write the native settings store. Independent hosts write
their own driver-preference file: explicit `--driver-preferences`, otherwise a
sidecar to `--config`, otherwise `native-host-audio.json` in the host's application
configuration directory. Launch messages may supply the path explicitly. Startup
loads configuration, then the saved driver preference, then explicit audio command
line overrides. Exported connection profiles are never used as preference stores.
Library hosts without a store report unsaved preferences explicitly.

## Validation

The implementation plan defines P14 A1 through A8. Production tests cover the
native service and independent process routes, both session carriers, exact media
and timing, identity preservation, recording conflicts, cancellation, rate
reconfirmation, failed startup/restoration/rollback, save retry and detach recovery.
Run actual native-driver cases with the device/server available; report physical,
virtual and mocked evidence separately. Keep final-head results and review evidence
on the implementation PR, and retain unrelated parity capability gates.

## Acceptance coverage

| Criterion | Production coverage |
| --- | --- |
| A1 confirmation | Host controller tests cover cancel, expiry, stale confirmation and changed-rate reconsent; CPAL mock preflight opens no streams. |
| A2 conflicts | Service capture recording predicate is shared with persistence; host confirmation test retains an active take. |
| A3 preservation | Native immutable-image tests compare public identities, media, roles, processor state, controls, selection and targeting. |
| A4 conversion | Shared conversion unit tests and session integration tests provide dependency/quantum, signed timing, exact MIDI and resampling oracles. |
| A5 rollback | Production service with injected controller covers startup, restoration, corrupted mapping and double failure. |
| A6 ownership | Confirmed detach/reattach test verifies retained result and a single driver start; protocol authority/generation tests fence old submissions. |
| A7 preferences | Save-failure retry test counts driver starts; process tests verify the owning host's sidecar; embedded settings tests preserve unrelated keys. |
| A8 delivery | Native workspace, feature-isolation, dependency, Wasm and platform CI gates accompany isolated direct-JACK/CPAL-JACK qualification. |
