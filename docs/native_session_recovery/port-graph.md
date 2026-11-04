# Provider-independent port graphs

The external port graph contract lives in `shoop_port_graph`. Its default build is
`no_std` plus `alloc`, with optional serialization and a standard-library control
channel. It depends on no driver, engine, application or transport crate. The
application API re-exports graph values and the common audio/MIDI directions/types.

## Ownership and layers

The application owns logical port lifetimes and desired publication/connection
intent. `ApplicationPortState.publication` maps an application port ID to a
`LocalPortKey`. This mapping is optional for providers without publication support.
The session service
validates mapping formats and rejects duplicate bindings. Runtime mappings are not
saved into session images.

The service coordinates those keys with prepared engine bindings and callback
handoff. Creation waits for both provider publication and engine binding; removal
waits for engine retirement and provider reclamation. Failed creation removes its
engine object and publications before reporting failure. A provider understands specifications and keys, without track/bus IDs or
application semantics. The same `Controller` interface is injected into the service
for every provider. Native hosts expose an optional graph control handle regardless
of driver build features. Drivers without this capability return none.

A controller has snapshot, submit and completion methods. Its optional host-private
`IoBinding` supplies an opaque engine routing token for a published key. These
bindings and reserved `@host-io/` slots do not appear in the connection UI or
persistence/wire model. Providers without I/O bindings retain discovery/control
without opting into automatic application publication. Common operations are
connect/disconnect, publish, rename and unpublish. Publication/removal are host
lifecycle operations; the frontend wire command currently exposes connection edits.
Existing session operation tracking and lease checks wrap that command. No new
transport, authentication, credentials or network restrictions are introduced.

## Identity and observations

- `Snapshot.epoch` identifies one provider instance.
- `EndpointId` is a runtime identity within that instance. Names are labels and
  need not be unique. Renaming preserves IDs; observed removal/recreation assigns
  a fresh ID, including when a name or native slot is reused.
- `LocalPortKey` is a caller-owned logical publication key, independent of runtime
  endpoint IDs and engine slot numbers. A publication maps it to an endpoint.
- `Port.locator` is an optional opaque provider-specific restoration selector.
  Clients do not parse it or use it as a current endpoint ID. Session images save
  selectors and connection intent, never runtime endpoint IDs or publication keys.
  Missing peers retain pending intent and reconnect when rediscovered. External
  patchbay changes become observed state and are included in subsequent captures.
- `Port.physical` identifies hardware endpoints for default capture/playback
  selection; software peers remain available for explicit connections.

Graph revisions order observed snapshots. A change to unrelated ports does not
invalidate a command. Commands identify endpoints and provider epoch explicitly;
a stale endpoint or provider instance is refused. Native providers still have
backend-specific mutation races: JACK supplies no atomic compare-and-connect.
Confirmation reports an observed state, not permanent connectivity.

Capabilities describe support for discovery, connection editing, publication/removal
and renaming. Status separately reports unsupported, starting, ready or unavailable.
A stopped provider retains its capability description while becoming unavailable.
Generic errors distinguish unsupported/unavailable, stale instance/endpoint,
invalid/incompatible requests, capacity/queue exhaustion and backend failure.

Publication records expose publishing, published, removing and failed states. A
published/removing record references a matching local endpoint. Publication specs
are caller specifications; the provider may qualify the actual displayed name.
The bounded channel retains completions, closes dequeued as well as queued work
when its worker disappears, and checks successful results against observed state
before delivering them.

## Implementations and scope

JACK implements discovery, connection editing, publication/removal and renaming. Its native names,
registration indices, registration generation counters and naming rules remain
inside the JACK provider. A narrow JACK metadata adapter reads the registration
index used by notifications; only provider-issued IDs cross the contract. Registration
notifications increment preallocated generation counters so replacement between
polls cannot silently retain an observed old identity. Rename notifications leave
registration generations intact. Queries and mutations run outside the audio
callback; notification callbacks use bounded atomic operations.

Track audio/MIDI, the sync track and audio-bus outputs are published automatically.
JACK starts without a fixed public port bank. Every registered endpoint belongs to
a session publication and is reclaimed with its owner. Master auto-connect and saved
connection intent target those publications directly.

The native host reserves bounded private I/O capacity from the configured track,
channel and bus pools. These slots do not register public JACK ports. Registration
and unregistration run on the control worker. A bounded queue transfers typed
owners into preallocated callback slots, and acknowledgements transfer retired
owners back. MIDI removal emits reset messages and allows them to cross the graph
before unregistration. No allocation or owner destruction occurs in that handoff
on the callback.

The deterministic fake provider supports the complete lifecycle, allows duplicate
labels without JACK namespaces, exposes failures and deferred transitions, and uses
the same bounded channel and conformance scenarios as JACK. It is test support,
not a production driver.

The existing connection dialog projects provider inventory and observed links
through `ConnectionViewState`; mixer routes retain their separate engine meaning.
No MIDI buses are introduced. Display-name changes preserve initial physical port
names, matching the reference application.

## Validation

Shared conformance scenarios run against JACK and the fake provider. Coverage includes
format/local ownership validation, idempotence, rename identity, stale instances,
unrelated hotplug, same-label replacement, publication/removal/failure states,
capacity, and worker loss. Session tests exercise fake-provider operations and logical
publication mappings through typed and encoded delivery after UI detachment. Strict
JACK acceptance also verifies dynamically created track/bus audio and MIDI routing,
session reload with missing peers, subsequent hotplug restoration, removal, MIDI
reset, pool exhaustion and routing beyond 64 slots over IPC and TCP. A real JACK
callback heap guard covers publication handoff and retirement; a non-JACK fake
provider exercises aggregate creation/removal and creation rollback.
