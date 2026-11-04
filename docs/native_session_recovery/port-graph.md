# Provider-independent port graphs

The external port graph contract lives in `shoop_port_graph`. Its default build is
`no_std` plus `alloc`, with optional serialization and a standard-library control
channel. It depends on no driver, engine, application or transport crate. The
application API re-exports graph values and the common audio/MIDI directions/types.

## Ownership and layers

The application owns logical port lifetimes and desired publication/connection
intent. `ApplicationPortState.publication` maps an application port ID to a
`LocalPortKey`. This mapping is optional: a shared engine bank route does not mean
that an individual track has a dedicated published endpoint. The session service
validates mapping formats and rejects duplicate bindings. Runtime mappings are not
saved into session images.

The host will coordinate those keys with prepared engine bindings and callback
handoff. A provider understands specifications and keys, without track/bus IDs or
application semantics. The same `Controller` interface is injected into the service
for every provider. Native hosts expose an optional graph control handle regardless
of driver build features. Drivers without this capability return none.

A controller has snapshot, submit and completion methods. Common operations are
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
  Clients do not parse it or use it as a current endpoint ID. Persistence and
  automatic restoration of external graph intent remain future work.

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

JACK implements discovery, connection editing and renaming. Its native names,
registration indices, registration generation counters and naming rules remain
inside the JACK provider. A narrow JACK metadata adapter reads the registration
index used by notifications; only provider-issued IDs cross the contract. Registration
notifications increment preallocated generation counters so replacement between
polls cannot silently retain an observed old identity. Rename notifications leave
registration generations intact. Queries and mutations run outside the audio
callback; notification callbacks use bounded atomic operations.

The configured JACK bank is represented by published local keys. JACK explicitly
reports dynamic publication/removal as unsupported until engine slot allocation
and callback handoff are implemented. Registering unbound audio ports would not
satisfy that capability. The callback and its fixed bank remain unchanged here.

The deterministic fake provider supports the complete lifecycle, allows duplicate
labels without JACK namespaces, exposes failures and deferred transitions, and uses
the same bounded channel and conformance scenarios as JACK. It is test support,
not a production driver.

The next implementation step is automatic publication of track audio/MIDI and
audio-bus ports through this interface, with aggregate engine/provider completion
and rollback. No MIDI buses are introduced.

## Validation

Shared conformance scenarios run against JACK and the fake provider. Coverage includes
format/local ownership validation, idempotence, rename identity, stale instances,
unrelated hotplug, same-label replacement, publication/removal/failure states,
capacity, and worker loss. Session tests exercise fake-provider operations and logical
publication mappings through typed and encoded delivery after UI detachment. Strict
JACK acceptance also verifies audio/MIDI routing over IPC and TCP.
