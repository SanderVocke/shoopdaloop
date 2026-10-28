# Proposal: modular, transport-independent engine communication

## Status and scope

This is a proposal for communication between UI/control-side clients and an
engine hosting `rt_runtime`, the loop layer and other processing components. It
extends the native/browser service direction in
[PROPOSAL_RT_ENGINE.md](PROPOSAL_RT_ENGINE.md). The high-level stack is described
in [ARCHITECTURE.md](ARCHITECTURE.md).

The common framework described here is not implemented or adopted as a replacement
for the production application protocol. Existing runtime/loop host fixtures
provide useful examples, not a complete implementation of this proposal. Names
for new modules below describe responsibilities, not existing exported APIs.

The proposal covers native in-process, browser inter-instance, native IPC and
cross-machine control. It does not specify the rest of the future ShoopDaLoop
stack or establish feature parity, a migration plan, an audio-streaming protocol,
a plugin sandbox, or arbitrary dynamic processor loading.

## Motivation

The final application will combine loops with custom DSP, potentially including
processors supplied by crates outside the runtime/loop/application dependency
stack. An audio plugin host is one example. Higher application layers will also
need their own operations and results.

A single protocol implementation depending directly on every DSP implementation
would couple unrelated modules and make reuse difficult. Independent ad hoc
connections for every component would instead duplicate identity, backpressure,
cancellation and lifecycle machinery.

Use a shared delivery framework with separately defined domain vocabularies and
explicit application-level composition. The application chooses its capabilities;
the runtime does not become a catalog of application commands.

## Existing mechanisms and their limits

### Local runtime transport

`rt_runtime::transport` is a concrete bounded in-memory ownership channel. Its
Rust generic types allow application-defined commands and returned objects. It
moves values through typed queue storage; it does not serialize them.

An owning buffer or processor may contain pointers into a local Rust heap. Those
values cannot be transmitted unchanged to another process or independent Wasm
instance. Sharing packet storage is not the same as sharing an allocator and
compatible Rust heap ownership.

The local transport remains useful behind a remote bridge: it provides admission,
ordering, cancellation observation and reliable ownership returns to the local
control endpoint. Scheduled domain execution has its own terminal-result contract.

### Loop layer and current wire examples

The loop library defines operations/results and includes the LTP1 codec for
prepared timing topology. That codec is a reusable payload representation, not a
whole controller/AudioWorklet protocol.

The independent hosts under `tests/rt_runtime_host` and `tests/loop_engine_host`
define concrete wire protocols and bridges. The runtime fixture demonstrates
separate Wasm memories using MessagePort and transferable buffers. These hosts do
not automatically expose every core API or provide an extensible production
protocol for all application components.

The existing `shoop_audio_protocol` is an application-specific vocabulary. Its
useful concepts can inform or evolve into the proposed design; creating an
entirely separate package hierarchy is not a requirement.

## Architectural responsibilities

### 1. Common protocol infrastructure

A transport-neutral module defines:

- Envelope framing and size limits.
- Connection identity and request correlation.
- Destination addressing and domain routing.
- Protocol/capability compatibility rules.
- Common delivery statuses and error categories.
- Credit, result-retention and transfer lifecycle contracts.

It must not depend on loop semantics or plugin-host implementations.

### 2. Domain schemas

Each domain defines portable requests, results and events. Examples:

| Domain | Example operations |
| --- | --- |
| Loop | Transition, select retained media, query settled state, export |
| Hypothetical plugin host | Parameter change, stage state, activate prepared state |
| Future application/session | Requests coordinating a larger application change |

Schemas can live in lightweight companion crates or modules without pulling DSP,
device or UI implementations into clients. A component need not originally
support this framework: an application-owned adapter can define its schema.

A schema must specify identity fields, legal sizes/ranges, timing meaning,
completion meaning, failure behavior and version compatibility. Choosing an
encoding technology alone does not establish those semantics.

### 3. Domain adapters

The client adapter exposes useful typed APIs and encodes requests. The engine
adapter validates/decodes requests, routes preparation, invokes local control
APIs and encodes outcomes. It retains mappings between wire requests and local
runtime identities, receipts and prepared owners.

An adapter handles only its declared domain and destinations. It cannot bypass
runtime admission, directly mutate live processors, or invent a successful
musical outcome from transport admission.

### 4. Application composition

The final executable selects schemas, adapters and implementations. Initial
composition can be static, for example a root request enum containing loop and
plugin-domain variants, or a fixed dispatch table keyed by domain identifiers.

This is not a requirement for dynamic plugin discovery, runtime reflection or
serialization of arbitrary Rust objects. Both ends must support a compatible
schema, and the engine binary must contain the selected processor code.

The application owns policy for authorization, endpoint lifecycle, disconnects
and coordination between domains. The full session layer is outside this proposal.

### 5. Carrier adapters

A carrier adapter transports envelopes through a local connection, MessagePort,
a shared-memory queue, IPC or a network connection. It implements the common
contracts without interpreting musical or plugin semantics.

The framework should permit separate physical channels for control, telemetry and
bulk traffic. A unified protocol need not mean one FIFO or one socket for all data.

## Example: an independently supplied audio plugin host

An application could register loop and plugin-host domains while linking the
plugin DSP implementation only into the engine executable. A client parameter
request names the plugin instance, parameter, value and requested engine frame.
The plugin adapter validates those fields and submits the corresponding prepared
local operation. Its reply distinguishes scheduling admission from application
at the reported engine frame. The loop handler need not understand that message.

A larger plugin-state replacement uses the bulk-transfer path to stage a blob,
then returns a checked prepared-state handle. A later activation request references
that handle. This is supported only if the plugin implementation supplies a safe
preparation/activation lifecycle; arbitrary state loading is not assumed cheap.

The same domain messages can travel in-process or over browser/IPC/network
carriers. An in-process adapter may replace the encoded payload with typed local
values, without changing the declared outcome or ownership semantics. General
processor-operation composition still requires the local API work described below.

## Message and identity model

The exact binary layout is a later design decision. The conceptual envelope needs:

| Field | Purpose |
| --- | --- |
| Envelope version and message kind | Decode framing and distinguish request/result/event traffic |
| Connection/session identity | Reject obsolete traffic after restart or reconnect |
| Request identity | Correlate admission, terminal results and cancellation |
| Domain identifier and schema version | Select a supported vocabulary |
| Destination instance identity/generation | Address a service or processor without stale-slot aliasing |
| Optional requested engine frame | Express supported sample-clock scheduling |
| Payload length and bounded payload | Validate framing before interpreting content |

Identifiers and fixed-width numeric encodings must be portable. Do not transmit
Rust struct layouts, pointers, `usize`, trait objects or local receipt tokens.
Version-specific domain codecs may reuse existing canonical representations such
as LTP1 rather than encode the same data twice.

An instance-local handle is not automatically a remote capability. Adapters must
explicitly validate or map IDs into the receiving engine's scope and maintain the
associated lifetime. Copying a media ID does not acquire an export lease.

At connection setup, exchange supported envelope/domain versions, capabilities
and prepared limits. Unsupported domains/operations are refused without execution.
Negotiation cannot enlarge prepared RT storage while running. Domain identifiers
must be unambiguous within the negotiated connection; stable names may map to
compact connection-local numbers. The final assignment convention remains open.

## Execution and result semantics

Keep distinct phases visible:

1. **Received/staged:** the bridge owns the incoming frame, not necessarily an
   admitted engine operation.
2. **Admitted/scheduled:** the appropriate local machinery accepted the request
   and reserved its required result capacity.
3. **Terminal:** the requested operation applied, failed, was superseded or was
   cancelled according to its domain's rules.

Not every operation needs all three notifications, but omitting a notification
must not collapse the underlying meanings. A successful local owning completion
can precede a deferred musical terminal result. Ownership-return bookkeeping and
remote user-visible success are different responsibilities.

For scheduled operations, preserve requested, effective/delivered and terminal
engine frames where applicable. A countdown completing later is not simply
transport lateness. Domain errors remain distinguishable from protocol errors,
admission refusal and runtime faults.

Cancellation is an explicit request with a separate acknowledgement. It may race
execution; only the authoritative terminal outcome determines what happened.
Remote cancellation maps to queued or already-delivered local work as appropriate,
not merely to a flag in the client's memory.

Define ordering per destination/stream, including the existing scheduler's
same-frame rules. Do not promise one global FIFO across recovery, telemetry,
bulk transfers and domain scheduling. Cross-domain order or atomicity requires an
explicit coordination contract, not an incidental socket arrival order.

## Flow control, ownership and RT safety

### Reliable work and credits

Bound outstanding requests before admission. Reserve capacity for all mandatory
results and retained local owners of admitted work. Reliable results cannot be
discarded because a client or network writer is slow.

Wire credits and local runtime credits are separate resources. The bridge may
receive a local completion only if it can safely retain/recycle its owners and
retain any required remote result. It must not drain a bounded runtime queue into
an unbounded outgoing queue. Define exactly when each credit is returned; an
initial staging acknowledgement does not release a credit still needed to retain
the terminal result.

Constrain the whole path: ingress, in-flight work, terminal records, outgoing
traffic and transfer leases. A disconnected client cannot force RT-side object
destruction or overwrite retained results. Bounded retention ends through an
explicit disconnect/teardown policy, not indefinite resource growth.

### Traffic classes

- **Reliable control:** bounded admission and retained terminal results.
- **Telemetry:** explicitly lossy/coalescing updates or latest-value mirrors;
  a stalled consumer must not obstruct reliable completions.
- **Bulk transfer:** chunked payloads with independent capacity, offsets and
  cancellation/lease lifecycle.
- **Recovery:** independently reserved resources for an explicitly approved set
  of cheap operations. Arbitrary handlers cannot self-declare priority or safety.

Apply callback-wide global allowances as well as any per-domain limits. Otherwise
many individually bounded handlers can collectively consume an audio deadline.
Finite required musical execution remains subject to its existing semantics;
communication budgets must not silently truncate loop boundaries or events.

### Decoding and prepared owners

Validate envelopes before payload access. Bound byte lengths, record counts,
materialization and validation work. RT-side decoding uses borrowed views, fixed
records and prepared pools; general deserialization that allocates strings,
vectors or arbitrary objects is not an acceptable callback path.

A serialization library may be used outside rendering, or on-render only where
its chosen representation and implementation meet the same tested contracts.
No universal arbitrary-type serializer is required.

Prepare state before activation, and return/recycle displaced ownership locally.
A remote reply can describe completion or resource release; it cannot contain an
owning Rust processor. Browser worklet adapters may need to retain retired objects
in prepared banks until safe reuse or stopped disposal, because another Wasm
instance cannot destroy their local heap allocations.

All callback-reachable bridge work obeys the runtime's no-allocation/freeing,
no-locks/no-waits/no-I/O contract. This does not certify browser scheduling/GC,
third-party DSP or total callback deadlines.

## Deployment modes

### Native in-process

```text
UI/control thread -> domain adapter -> local typed runtime transport -> RT thread
```

Use the same logical domain operations, identities and outcome semantics. Encoding
all messages into byte packets is optional: the local adapter can pass typed
prepared commands and ownership directly. Compatibility tests should prevent the
in-process fast path from acquiring different application semantics.

Multiple clients aggregate outside rendering because a local runtime control
endpoint has one producer. Preparation and disposal run on suitable non-RT
contexts, including any component-specific main-thread affinity requirements.

### Browser: independent Wasm instances

```text
Main/Worker Wasm                     AudioWorklet Wasm
client + preparation                 bounded domain bridge
       |                                     |
       +-- MessagePort/shared packet queue --+
                                             |
                                  local control endpoint -> runtime
```

With independent ordinary memories, use portable bytes and explicit transferable
buffer ownership. A shared-memory packet queue is an optional carrier; it can
reduce browser messaging/copy overhead but does not turn separate Rust heaps into
one ownership environment. A genuinely shared Wasm heap/threading build would
need additional allocator, initialization and synchronization design.

The worklet's message handling and rendering share its rendering thread. The
local control endpoint does not create a background thread. Receiving, bounded
adaptation, submission and result recycling therefore count against that thread's
work. Moving expensive work into `onmessage` does not make it non-RT work.

Submit staged commands before the runtime snapshots callback ingress when they
are intended for that callback; no extra callback delay is inherent in the local
queue. Message arrival is still nondeterministic. Main/Worker contexts perform
heavy preparation where possible; worklet-local construction must be prepared,
pooled or incrementally staged rather than assumed transferable from another heap.

MessagePort scheduling, wrapper object creation and JavaScript GC are not hard-RT.
Credit bounds apply to cooperating producers, not to an attacker flooding the
browser's own message queue.

### Native cross-process IPC

```text
UI process -- IPC -- engine non-RT bridge/preparation thread
                                  |
                         local runtime transport
                                  |
                             RT audio thread
```

Use local sockets, pipes or explicitly managed shared-memory queues. Socket
reads/writes and general decoding occur outside the audio callback. The engine
process can prepare objects on its own non-RT threads and activate them through
local ownership transfer.

Large transfers may use shared-memory regions referenced by checked handles and
offsets. Define access rights, immutable publication, bounds, lifetime and crash
cleanup; never assume process virtual addresses are portable. The audio callback
must not wait for another process.

This arrangement separates the whole engine from the UI. It does not automatically
isolate each plugin, restart a failed engine or provide RT-safe IPC scheduling.

### Cross-machine sockets/network

Use the same logical protocol over a framed stream such as TCP or WebSocket, with
appropriate security. Stream adapters handle fragmentation, partial reads/writes
and maximum frame sizes. A datagram carrier would additionally need explicit
ordering, loss, duplication and reliability rules; it is not interchangeable
without those rules.

Network I/O and buffering remain outside rendering. Commands can execute at exact
engine frames only when admitted before their deadline. Clients need engine-clock
observations, uncertainty estimates and sufficient lookahead; "execute now" has no
sample-accurate remote-delivery guarantee. Report lateness and apply a documented
late-command policy.

A recovery reservation protects local admission, not the connection itself. A Stop
cannot bypass a broken link or necessarily overtake bulk bytes already queued in
one reliable stream. Separate channels or bounded transfer windows can reduce
head-of-line blocking, but cannot guarantee delivery.

Remote engine control is distinct from live audio streaming. Returning audio to a
remote UI requires additional buffering, clock-drift and latency design outside
this proposal.

## Bulk media and component state

A transfer protocol should support begin/reserve, ordered chunks, validation,
ready status, activation and explicit cancel/release. Describe byte length, format,
revision and offsets without transmitting local pointers. Domain rules determine
whether an active recording prefix, frozen loop or plugin-state blob is meaningful.

Separate preparation from activation. For example, a plugin host can stage and
validate new state before a bounded activation, but the protocol does not make an
arbitrary plugin's state-loading implementation RT-safe. Components must declare
what they can prepare and what their activation costs are.

Retaining a snapshot is not the same as delivering all its bytes. The current
single-owner resource store uses budgeted running-engine read/upload copies.
A shared outer queue alone does not make these zero-copy. Off-thread readable
media or shared ownership would require a separate resource-access design.

Bulk traffic must not monopolize control/result credits or mandatory rendering
work. Incremental waveform/meter summaries should be preferred over repeatedly
transferring full recordings for display.

## Telemetry

Domain telemetry may use a common lossy event format, latest-value publication or
host-specific shared mirrors. Native/shared-memory meters need not pass through
reliable request/reply queues. Across processes or machines, adapters may sample
local mirrors and publish rate-limited updates.

Define accumulation and freshness semantics: peak-since-read differs from the last
callback's peak. Related state fields require a coherent publication policy;
separate atomics do not automatically produce a coherent snapshot. Preserve source
frames/generations where useful and report dropped/coalesced data honestly.

## Lifecycle, reconnects and security

A connection lifecycle must include compatibility setup, bounded running state,
shutdown/disconnect handling and owner reclamation. Define whether playback
continues, stops through an engine-local watchdog, or terminates after client loss.
The transport must not silently choose musical policy.

Request identity does not by itself guarantee exactly-once execution. If a reply
is lost after an operation executes, a client must query/resynchronize or use a
specified bounded deduplication/resumption mechanism. Do not blindly retry
non-idempotent operations. Engine restarts invalidate old identities and require
explicit state resynchronization; they do not recover recordings automatically.

A remotely accessible engine needs authentication, authorization and encrypted
transport where appropriate. Validate all lengths, counts, identities and allowed
operations; enforce admission/rate/resource limits before expensive work. Local
IPC also needs an access-control policy. Connection failure and malformed traffic
must not become unbounded RT work or callback destruction.

## Cross-domain coordination and local API composition

Common framing does not make operations across loops, plugins and routing atomic.
A future higher-level coordinator must stage candidates, check compatibility and
publish them at an authoritative cut. Each participant needs preparation,
activation and retirement contracts that permit that coordination. This document
does not define the complete session transaction system or assume that sequential
fallible calls can be rolled back after rendering.

There is also a concrete local composition gap to resolve:

- A `GraphRuntime<P, L>` has one planner type.
- Typed processor operations/results are associated with that planner type.
- The loop facade fixes `L` to `LoopPlanner` and provides predefined loop nodes
  plus custom effects.
- Its exposed effect-control path is not a general independently typed operation
  interface for arbitrary third-party processors.

A modular wire router cannot solve this by itself. Before implementing the full
application adapter, choose an execution-side composition approach: for example,
a higher-level composed planner/facade with suitable delegation, or an independent
processor-control extension. Preserve existing loop authority, receipt ownership,
role validation and RT contracts. This is an open design requirement, not a claim
that wrapping the existing facade trivially implements planner composition.

## Validation and unresolved decisions

Validate common delivery contracts across native typed, encoded browser and IPC
paths using the same semantic scenarios. Compare terminal outcomes and reported
engine frames for commands admitted before equivalent deadlines, not wall-clock
arrival timings. Exercise unknown versions/domains, malformed framing, stale IDs,
cancellation races, result stalls, transfer exhaustion, disconnects and full owner
recovery. Add network fragmentation/reconnect/authorization cases where supported.

Guard all callback-reachable decode, materialization, submit and recycle paths
against heap activity. Measure idle bridge overhead, command bursts, telemetry and
simultaneous bulk transfers separately from DSP. Record callback tails, throughput,
latency and memory/owner peaks; bounded storage is not proof of deadline safety.

Before implementation, decide:

- Exact envelope encoding, domain assignment and schema-version compatibility.
- Static enum versus dispatch-table composition and adapter interfaces.
- Local operation/planner composition for independently controlled processors.
- Credit accounting, queue sizes and global/per-domain work allowances.
- Capability negotiation, authorization and reconnect/result-retention policy.
- Bulk/shared-memory handle contracts and any resource-access extensions.
- Which deployment modes are initial deliverables and which remain future work.

The recommended starting point is static application composition over a small
common envelope and a few domain schemas, preserving the local native fast path.
Generalize only demonstrated common delivery machinery. Do not make `rt_runtime`
know Shoop commands, make the loop library depend on unrelated DSP implementations,
or require a universal dynamic RPC/serialization framework.
