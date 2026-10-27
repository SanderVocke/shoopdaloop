# Run a loop through bounded wire credits

[Fixture overview](README.md)

Run the same checked stereo/MIDI task through serialized requests and replies:

```sh
cargo run --locked --manifest-path tests/loop_engine_host/Cargo.toml --bin replay -- --wire
cargo run --locked --manifest-path tests/loop_engine_host/Cargo.toml --bin replay -- --two-thread
```

Each run compares four partitions with the direct scalar/packet-checked scenario:
13 stereo frames, 134 MIDI packets, three exact-frame transitions and complete
pool recovery. Native threaded transport uses capacity-one channels outside the
callback; it does not establish browser execution, live scheduling or deadlines.

## 1. Prepare the two endpoints

Construct `Client::new(session)` on the control side and `Bridge::new(session)`
on the render side, before rendering. The nonzero `u64` session is a host routing
identity. Use a new identity for a new connection; it is not authentication.
The native deterministic fixture uses one fixed identity per isolated test run.

The bridge prepares the actual loop facade, cancellation-flag base owners and five
fixed wire slots. Slots 0–3 are normal; slot 4 is independent recovery. Each slot
can retain one admission response and one terminal execution response. These are
external protocol capacities, not limits on internal loop wraps or intervals.

The bridge currently supports absolute-frame Record, Play, Stop and Seek for
logical loop IDs. It does not encode pointers, Rust enum layouts, resource leases,
scoped stage tokens or graph/program payload owners.

## 2. Reserve and submit one request

`Client::prepare(at, target, transition, recovery, cancelled)` returns a 64-byte
array and reserves a slot. The final boolean requests best-effort cancellation
before transport dequeue. Encode failures and Full/Busy credit refusals do not
consume a sequence. The last `u64` sequence is usable once; it never wraps.

Copy these bytes to `Bridge::ingest`. It validates the entire fixed record,
session, strictly increasing sequence and free slot before changing its watermark.
A malformed/foreign/duplicate request returns a protocol error without mutation.
A Busy request may retry the same bytes **before sending a later sequence**.
Keep sends ordered: reordering, corruption or a foreign connection is a protocol
failure, not permission to release an accepted request's credit.

Ingress uses the facade's prepared submission API under the heap guard. Supported
requests have empty owning payloads. A runtime submission refusal becomes an
admission rejection response; it is distinct from a malformed wire record.
Recovery accepts only Stop in slot 4, and the runtime rejects a future recovery
request rather than allowing it to bypass normal scheduling.

`Bridge::cancel(slot, sequence)` can request cancellation of an existing ticket.
It checks the wire identity before touching the flag. It is best effort before the
original dequeue, not cancellation of an already delivered domain operation.
The completion response decides the outcome. A stale slot/sequence cannot cancel
its replacement.

## 3. Render and collect without growing storage

Supply prepared audio/MIDI buffers to `Bridge::process`. Its guard includes actual
loop rendering and collection/recycling of the facade's owning completions and
terminal receipts. Collection examines at most the five external credits per
result stream, once per nonempty callback; it does not throttle musical work.
Zero callbacks do not collect or settle new work.

The cancellation flags retain their prepared base owner. Empty payloads own no
allocation. Unexpected owning returns are quarantined instead of being destroyed
on the callback. A callback/bridge error permanently faults this adapter: stop,
extract and explicitly clean up off-render. Do not replay frames or fabricate
results for abandoned requests.

## 4. Consume both result phases

`Bridge::next_response` returns one 96-byte array. Admission precedes execution
for a slot, even when both became ready in the same callback. Copy it to
`Client::receive`, which validates session, wire sequence, slot, requested frame,
phase order and the execution's runtime sequence/effective frame.

An admission rejection/cancellation releases the credit without an execution
response. Admission success retains it until terminal execution. Duplicate,
foreign and reordered replies leave the pending client credit unchanged. Unread
normal returns can fill all four slots without stopping already-admitted music;
the recovery slot remains independently usable but is also finite.

Requested, effective, delivered and terminal frames are distinct. For a request
for frame 0 admitted at frame 1, effective/delivered/terminal can all be 1 while
requested stays 0. For a future request admitted at frame 0, admission can be
observed at 0 long before its effective/delivery frame. `effective` is meaningful
for admission success and execution; a refusal is not a scheduling promise.

## Byte layout, version 1

All integers are unsigned little-endian. Lengths must match exactly; reserved
bytes and unused arguments must be zero. Keep all 64 frame/sequence/session bits
when adapting to JavaScript—do not convert them through a Number.

### Request: 64 bytes (`LLC1`)

| Offset | Bytes | Meaning |
| --- | --- | --- |
| 0 | 4 | ASCII `LLC1` |
| 4 | 2 | Version 1 |
| 6 | 1 | 1 Record, 2 Play, 3 Stop, 4 Seek |
| 7 | 1 | Bit 0 monitor-previous/retrigger; bit 1 cancel before dequeue; bit 2 recovery; other bits zero |
| 8 | 8 | Nonzero session |
| 16 | 8 | Nonzero wire sequence |
| 24 | 8 | Requested absolute frame |
| 32 | 4 | Wire slot (recovery iff slot 4) |
| 36 | 4 | Logical loop slot |
| 40 | 4 | Nonzero logical generation |
| 44 | 4 | Reserved zero |
| 48 | 8 | Play/Seek position; zero for Record/Stop |
| 56 | 8 | Reserved zero |

Stop requires option bit 0 clear. Recovery requires Stop. Actual target validity
is checked by the local domain; decoding a well-formed ID is not ownership or
proof that it names a current loop.

### Response: 96 bytes (`LLR1`)

| Offset | Bytes | Meaning |
| --- | --- | --- |
| 0 | 4 | ASCII `LLR1` |
| 4 | 2 | Version 1 |
| 6 | 1 | Phase: 1 admission, 2 execution |
| 7 | 1 | 0 no delivery, 1 delivered frame present |
| 8 | 8 | Session |
| 16 | 8 | Original wire sequence |
| 24 | 4 | Wire slot |
| 28 | 4 | Status below |
| 32 | 8 | Runtime sequence; zero only for local pre-admission refusal |
| 40 | 8 | Requested frame |
| 48 | 8 | Effective frame |
| 56 | 8 | Delivered frame, zero when absent |
| 64 | 8 | Admission completion or terminal execution frame, according to phase |
| 72 | 4 | Error code; zero for successful/cancelled/superseded outcomes |
| 76 | 20 | Reserved zero |

Statuses are 1 Admitted, 2 Cancelled, 3 Rejected, 4 Applied, 5 Superseded and
6 Failed. Admission permits only 1–3. Execution permits 2–6, not Admitted.
Rejected means runtime admission/delivery failed. Failed means successful runtime
delivery produced a domain failure, including an early `Reply::Failed` before a
resolver request was installed. Do not flatten those outcomes into one success bit.

Runtime errors 1–20, in order: InvalidConfig, Capacity, Exhausted, InvalidShape,
InvalidFrame, InvalidId, InvalidEvent, Full, Disconnected, InvalidState, InvalidGraph,
Cycle, PortType, StaleEpoch, Busy, InvalidParameter, InvalidOffset, Incomplete,
Budget, StaleRevision. Codes 513–532 are those same errors returned as a domain
`Failure::Runtime`, rather than a runtime delivery error.

Timeline errors 257–268, in order: InvalidConfig, Capacity, InvalidId, DuplicateId,
InvalidState, InvalidFrame, InvalidProgram, MissingSync,
InstantaneousDependencyCycle, RecursiveProgram, Exhausted, EmptyContent.

The browser transport copies these records between independent Wasm instances;
only local binding code uses memory offsets. It returns request buffers after the
copy, while the Rust wire credit stays retained until the terminal response.
The response-buffer acknowledgement is separate from both phases above.

The primitive protocol remains unchanged. Opt-in native graph commands use the
separate envelopes below and the prepared descriptor payload. Graph Wasm binding/
MessagePort transport, program staging and capture export still remain open;
this is not the complete Stage7 bridge.

## Prepared graph descriptor payload (`LGP1`)

`graph_transfer::{Record, Receiver}` adds a separate fixed96-byte payload format.
It does **not** extend `LLC1`/`LLR1` or by itself submit/admit/activate a runtime
graph. The opt-in native graph bridge connects its owning handoff to the command/
result protocol below. `BrowserGraphReceiver` separately checks the materializer
through public Node/Chromium bindings; the worklet transport does not yet carry
graph payloads or commands.

### 1. Prepare receiver-local ownership

Create `Receiver::new(session, config)` off-render. It prepares two full-capacity,
pre-touched `PreparedPlan` owners. The nonzero session routes records to this
receiver; copied slot/generation IDs are not authentication or cross-instance
ownership. Receiver-local bank slots0/1 start at generation1. These are **not**
the runtime's scoped graph stage IDs.

The receiver retains actual vector capacity and a maximum per-owner byte envelope.
An externally retired plan may be smaller than the initial full-capacity owner.
`capacities(id)` advertises the usable node/edge/binding counts of the current owner,
capped by configuration. Begin refuses larger declarations before changing state;
it never reallocates to restore earlier capacities. Oversized or foreign-config
recycled owners are returned to the caller unchanged for off-render quarantine.

### 2. Send ordered records

Every record has this header. All unlisted bytes are zero; size must be exactly96.

| Offset | Bytes | Meaning |
| --- | --- | --- |
| 0 | 4 | ASCII `LGP1` |
| 4 | 2 | Version1 |
| 6 | 1 | Operation1–7 below |
| 7 | 1 | Reserved zero |
| 8 | 8 | Nonzero session |
| 16 | 8 | Nonzero, strictly increasing receiver-wide sequence |
| 24 | 4 | Receiver bank slot |
| 28 | 4 | Nonzero bank generation |
| 32 | 64 | Operation fields; unused bytes zero |

| Operation | Fields at absolute byte offsets |
| --- | --- |
| 1 Begin | u64 expected graph epoch32; u32 step count40, edge count44, binding count48; byte52 planned flag0/1 |
| 2 Step | u32 offset32; node slot36/generation40; u64 input-end44 |
| 3 Edge | u32 offset32; source endpoint36, destination endpoint52 |
| 4 Planning | planner slot32/generation36; u64 decision slots40, observation capacity48, operation bytes56, before-step64 |
| 5 Binding | u32 offset32; planner slot36/generation40; consumer slot44/generation48; u64 decision slot52 |
| 6 Seal | No fields |
| 7 Abort | No fields |

An endpoint is16 bytes: u32 node slot at0/generation4, byte8 kind0 Audio/1 Midi,
three reserved bytes9–11, u32 port12. Generation0 denotes the host and requires
slot0. Node/planner generations otherwise must be nonzero. All integer values are
unsigned little-endian. `usize`-shaped descriptor fields travel as u64: a value
not representable by the receiving target is rejected, never truncated on Wasm32.
Session, sequence and epoch retain all64 bits.

Begin is legal only in Idle. Append all declared steps, then edges, then the
single Planning record if planned, then bindings, then Seal. Step/edge/binding
offsets must be exactly the next index. Unplanned descriptors require zero bindings.
Zero counts are valid payload declarations, not proof of compatibility with the
loop facade. Each call consumes at most one external record; there is no retry or
musical-work loop. Any malformed/stale/foreign/out-of-order/Busy refusal leaves all
banks and the sequence watermark unchanged. Retry rejected bytes before sending
a later sequence. Sequence `u64::MAX` is usable once; no subsequent record can pass.

### 3. Hand off, validate, return

Seal proves only framing and declared-count completeness. It does not check live
catalog generations, signal ordering/cycles, planner signatures or loop-role
compatibility. `take(id)` changes Sealed→Loaned and returns local owning
`Loan { id, epoch, plan }`. Submit the plan with `Controller::stage_graph`; process
real callbacks and inspect its owning completion. Only receiver-side runtime
validation can produce a Ready scoped graph ID. Ready is not active.

Rejected/cancelled/locally refused submissions retain their owning candidate.
Successful stage/release or activation/release returns an owned plan through the
runtime's existing return path. After matching the completion to the original
handoff, call `recycle(id, plan)`. Invalid return IDs, phases, configurations or
storage envelopes return `(error, plan)` without destroying the input. Successful
recycle returns Idle and advances the generation. Abort does the same for a
Receiving/Sealed bank without handing its owner away. Generation exhaustion
retires the bank at0, never wraps.

Abort cannot reclaim a Loaned owner or cancel a runtime command. Do not synthesize
owning returns on disconnect: stop callbacks and dispose the runtime/receiver
owners off-render. The native tests separately prove queued/Ready abandonment;
those banks remain logically Loaned, not falsely credited as returned.

`BrowserGraphReceiver` stores loans only in its own instance. `record_ptr` is a
local96-byte input address; `take`/`recycle` are local owner-exchange checks, not
runtime admission. Phase codes are0 Idle,1 Receiving,2 Sealed,3 Loaned; invalid
queries return the high bit plus a runtime error. Ingest/take/recycle use the
runtime error codes1–20 above. Only stop/free the object outside callbacks.

## Native graph commands/results (`LLG1` / `LLS1`)

### 1. Prepare the opt-in bridge and client

Use `Bridge::with_graphs(session)` instead of `Bridge::new(session)` off-render.
It enables two runtime graph candidates and two prepared descriptor owners, plus
exactly two graph command/result slots. `graph_wire::Client::new(session)` tracks
those two credits separately from the primitive client's four normal/one recovery
credits. Graph work uses the normal lane; it cannot bypass recovery or consume its
independent wire slot. Default primitive bridges remain unchanged.

Send descriptor records through `Bridge::graph_payload`. Then call
`graph_wire::Client::prepare(Operation::Stage { bank, slot, epoch }, cancelled)`
and pass the96-byte envelope to `Bridge::graph_ingest`. Payload, graph-command and
primitive-command sequences are separate monotonic namespaces. Do not interleave
or retry a rejected sequence behind a later accepted one within its namespace.

The bank must be Sealed and its Begin epoch must match the Stage envelope before
any owner is taken. A malformed/foreign/duplicate/Busy/invalid-bank envelope leaves
the watermark and all owners unchanged. The runtime still validates against its
actual current epoch/catalog. A local submission refusal, runtime rejection or
cancellation returns the candidate to its prepared bank and reports its **next**
generation/capacities. These are outcomes, not malformed-envelope refusals.

### 2. Consume Ready, activate, release

`Bridge::next_graph_response` yields a fixed128-byte record. Validate it with
`graph_wire::Client::receive` before releasing credit. Stage yields one completion:
Staged means Ready, **not active**. It returns the runtime's scoped graph ID and
the still-loaned bank identity. The graph client keeps a bounded ID→bank mirror;
that metadata is not an owning plan or transferable Rust capability.

Activate supplies the Ready graph ID, absolute frame and expected graph/topology
epochs. Its admission success retains command credit until the distinct execution
record. A successful execution includes actual delivery/terminal frames and the
new graph epoch. Runtime delivery failure remains Rejected; successful delivery
with a domain refusal is Failed. Do not infer graph publication merely from admission.

Release yields one owning completion. On success the runtime's returned candidate
or retired old graph moves into its original prepared bank. The result carries
the next generation and actual usable capacities. The client validates the scoped
ID, original bank, next nonwrapping generation and phase before changing its mirror.
A returned initial graph can be smaller than the full-capacity candidate it replaced.

Unread Stage/Release results retain their graph command slot; unread activation
admission/execution retains it through the last record. Already admitted music and
primitive recovery still progress. Completion/execution collection uses one fixed
seven-credit bound per stream per nonempty callback (five primitive plus two graph),
not a renewed allowance per musical interval. Zero callbacks do not admit/collect.

`graph_cancel(slot, sequence)` and the request cancellation flag are best effort
before runtime dequeue. They do not cancel an already applied graph or roll back
published state. A cancelled Release keeps the runtime owner retained; retry with
a new request rather than pretending the bank is returned.

### 3. Stop only after ownership is accounted for

`Bridge::outstanding()` includes both protocols' command slots;
`Bridge::retained_graphs()` counts staged/taken graph owners still awaiting release.
The graph client separately exposes both counts. Successful native tests require
all command counts and retained graph counts zero before stopped cleanup. Invalid,
foreign, stale, duplicate and out-of-order responses preserve client credits and
known graph/bank mappings.

Unexpected owning completions are returned intact to a prepared quarantine and
permanently fault the outer bridge. Unexpected local owning refusals are retained
in a separate prepared slot. Neither path destroys an owner on the callback.
Intentional disconnect/abandonment stops callbacks before consuming the engine;
it disposes queued/Ready owners off-render without fabricating returns or results.

These native methods compile for Wasm, but no public graph command bindings or
MessagePort dispatch exist yet. The current worklet exercises only primitives;
`BrowserGraphReceiver` still demonstrates only payload/local-owner behavior.

### Request: 96 bytes

All integers are unsigned little-endian; absent/unlisted fields are zero.

| Offset | Bytes | Meaning |
| --- | --- | --- |
| 0 | 4 | ASCII `LLG1` |
| 4 | 2 | Version1 |
| 6 | 1 | 1 Stage,2 Activate,3 Release |
| 7 | 1 | 0 normal,1 request cancellation before dequeue |
| 8 | 8 | Nonzero session |
| 16 | 8 | Nonzero graph-command sequence |
| 24 | 4 | Graph credit slot0/1 |
| 28 | 4 | Stage: descriptor bank slot |
| 32 | 4 | Stage: nonzero bank generation |
| 36 | 4 | Stage: runtime candidate slot |
| 40 | 8 | Stage/Activate: expected graph epoch |
| 48 | 8 | Activate: expected topology epoch |
| 56 | 8 | Activate: requested absolute frame |
| 64/68/72 | 4 each | Activate/Release: scoped runtime graph ID (scope/slot/generation) |
| 76 | 20 | Reserved zero |

Stage requires all Activate/Release-only fields zero. Activate requires Stage-only
bank/candidate fields zero. Release permits only the scoped ID and common header.
A syntactically valid ID does not prove that it belongs to this runtime or is Ready.

### Response: 128 bytes

| Offset | Bytes | Meaning |
| --- | --- | --- |
| 0 | 4 | ASCII `LLS1` |
| 4 | 2 | Version1 |
| 6 | 1 | 1 owning admission/completion,2 execution |
| 7 | 1 | Presence bits:0 delivered frame,1 scoped graph ID,2 bank/capacities; other bits zero |
| 8/16 | 8 each | Session/original graph-command sequence |
| 24 | 4 | Original graph credit slot |
| 28 | 4 | Status below |
| 32 | 8 | Runtime sequence; zero only for a local pre-admission rejection |
| 40/48 | 8 each | Requested/effective frames; both zero for Stage/Release |
| 56 | 8 | Delivered frame or zero when absent |
| 64 | 8 | Owning completion or terminal execution frame |
| 72 | 4 | Runtime/domain error code, same ranges as `LLR1` |
| 76/80/84 | 4 each | Scoped graph ID, or all zero when absent |
| 88/96 | 8 each | Staged validated graph epoch / Activated published graph+topology epochs; otherwise zero |
| 104/108 | 4 each | Descriptor bank slot/generation, or zero when absent |
| 112/116/120 | 4 each | Actual usable step/edge/binding capacities, or zero when absent |
| 124 | 1 | Original command kind1–3 |
| 125 | 3 | Reserved zero |

Status1 Staged and2 Released complete Stage/Release respectively. Staged carries
the still-loaned bank ID; Released carries its next generation. Status3 Admitted
is Activate admission success;4 Activated is its delivered terminal success.
Status5 Cancelled and6 Rejected are terminal, with bank return metadata iff the
command is Stage. Status7 Failed is a delivered Activate domain refusal. Execution
records are only for Activate and follow admission success. Bank generation0 means
permanently exhausted and requires all three capacities zero.

Staged/Released/Admitted/Activated carry a scoped graph ID. Failed, Rejected and
Cancelled do not. Only Staged/Activated carry epoch metadata; zero elsewhere means
absent metadata, not a statement that the live graph epoch is zero. Preserve all64
bits of sessions, sequences, frames and epochs when adding future JS adapters.

## Browser report (`LLH2`)

The runner's JSON report has `format: "LLH2"`. Historical `LLH1` reports lack the
semantic record stream and are not upgraded retrospectively. Each entry in `reports` identifies
`offline` or `live` and retains:

- `requests`: all three complete 64-byte wire records, as arrays of byte values.
- `responses`: all six complete 96-byte admission/execution records. Opaque wire
  identities remain encoded as bytes; do not convert their `u64` fields to JS
  numbers. The browser Rust client validates each record before releasing credit.
- `audio`: two arrays of 256 downstream Web Audio samples. `offlineAudio` additionally
  contains both complete 512-frame OfflineAudioContext output channels; it is null
  for live mode. The live probe is upstream of the final mute gain.
- `midiBytes`: the exact used prefix of a fixed 16,384-byte observation buffer.
  Each record is an 8-byte little-endian engine frame, 4-byte little-endian packet
  length and that many complete MIDI bytes. No padding, pointers or synthesizer
  state is implied. Validate every length and consume the complete blob. This is
  an observation format, not a media snapshot/export codec.
- `semanticBytes`: the complete used `LSJ1` prefix, extracted from the stopped
  observer into prepared local storage. `semanticSummary` is the runner's validated
  boundary/count/loss summary. All bytes must match native reference execution.
- `summary`: decimal-string engine frames/callback count and browser `firstFrame`,
  initial/final memory bytes, overflow/heap witnesses, response-buffer count,
  instance marker and recovered `[pages, versions, snapshots, media references]`.
  `ready` records worklet startup memory, non-shared state and decoder fallback;
  `controlBytes` records the prepared main instance's memory size.

The normal scenario requires zero journal overflow. If diagnostic storage fills,
rendering still proceeds and the overflow counter saturates rather than changing
music. Binding checks deliberately exhaust this buffer while checking every audio
sample and collecting all command receipts; a truncated journal is never reported
as complete output evidence.

The report also retains Node/Chromium check names, tool versions, native/Wasm
profile, Wasm SHA-256, checkout head/dirty scope and failure-gate results. A dirty
head identifies the base, not an assertion that the changes were already committed;
the milestone evidence identifies their containing source commit.

Failure reports are separate unsuccessful sessions. `workletFailure` retains the
actual render result, processor fault witness and allocation/reallocation/
deallocation counts. The supervisor requires the matching diagnostic **and**
these fields plus a nonzero exit. Matching text, an unrelated exception, a missing
binding or a timeout alone is not a passing canary.

## Semantic boundary journal (`LSJ1`)

This is a fixed, little-endian diagnostic stream, not a Rust layout, a lease,
a media export or a live telemetry interface. The observer runs after settlement
and before rendering. It records boundaries with events, playback discontinuities,
or changes in resolver journal loss. Ordinary callback cuts are omitted. An
eventless same-position seek still produces a decision record; no transition or
pulse is invented to represent it.

Each recorded boundary is atomic: one boundary record, all decision snapshots,
then all events in observed order. If the whole batch does not fit, none of it is
written and saturating loss counters increase. Observation loss cannot change
music, receipts or cleanup. A complete report requires all three loss fields zero.
Recorded committed decisions survive a later processor fault; journal completeness
alone is **not** proof that the following audio interval rendered successfully.

### Header: 40 bytes

| Offset | Bytes | Meaning |
| --- | --- | --- |
| 0 | 4 | ASCII `LSJ1` |
| 4 | 2 | Version1 |
| 6 | 2 | Record stride160 |
| 8 | 4 | Used record count, at most128 |
| 12 | 4 | Reserved zero |
| 16 | 8 | Observer-dropped whole boundaries |
| 24 | 8 | Observer-dropped records |
| 32 | 8 | High-water mark of resolver-reported cumulative lost events |

The total length is exactly `40 + count * 160`. The resolver loss high-water mark
stays nonzero even if a later counter resets; it is not an invented aggregate of
loss across resets. The capacity is128 records, not a musical transition quota.
The normal fixture uses15 records/2440 bytes: four boundaries, four decisions and
seven events. Observer and stopped-export storage are both prepared before render.

### Records: 160 bytes

Common fields: byte0 type, byte1 code, byte2 flags, bytes3–7 zero, bytes8–15 absolute
engine frame. Boundary frames strictly increase. All child records have that same
frame. Every unused byte is zero.

- **Type1 boundary:** code/flags zero; u32 decision count at16, u32 event count at20,
  u64 resolver loss counter observed at this boundary at24; bytes32–159 zero.
- **Type2 decision:** code0 Stopped,1 Playing,2 Recording. Logical loop slot/generation
  are u32 at16/20. Consecutive u64 fields at24,32,40,48,56,64,72,80,88,96 are content
  length, content revision, position, cycles, capture frames, capture revision,
  last pulse, composite iteration, composite passes and composite entered-at.
  Bytes104–159 are zero. Flags: bit0 playback discontinuity, bit1 capture incomplete,
  bit2 monitor previous, bit3 last pulse present, bit4 composite present,
  bit5 composite entered-at present, bit6 request origin present. Other bits zero.
  Absent optional values are encoded as zero; present frame zero is distinct.
  Opaque origin RequestId values and the private revision watermark are deliberately
  excluded. Routing/graph epochs, gain and resource metadata are not this projection.
- **Type3 event:** logical loop slot/generation at16/20; u64 arguments A/B at24/32;
  bytes40–159 zero. Codes and allowed option flag bit0 are below. Other flags are zero.

| Code | Event | A | B | Option bit0 |
| --- | --- | --- | --- | --- |
| 1 | Record | 0 | 0 | monitor previous |
| 2 | Play | position | 0 | retrigger |
| 3 | Stop | 0 | 0 | zero |
| 4 | Seek | position | 0 | retrigger |
| 5 | Replace | length | revision | preserve position |
| 6 | PlayIteration | iteration | 0 | retrigger |
| 7 | SeekIteration | iteration | 0 | retrigger |
| 8 | Program | 0 | 0 | zero |
| 9 | Wrap | 0 | 0 | zero |
| 10 | Pulse | 0 | 0 | zero |
| 11 | CaptureExhausted | 0 | 0 | zero |
| 12 | Iteration | iteration | passes | zero |
| 13 | ProgramEnd | 0 | 0 | zero |
| 14 | Refused | timeline error257–268 | 0 | zero |

The codec supports these public event/decision fields; that does not imply the
primitive browser script has executed composites, replacement or capture failures.
The native codec tests cover full-width values and optional-field encoding;
broader actual host scenarios remain separate work.
