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

This protocol slice still has no graph/program staging, capture export or semantic
boundary journal. Those formats and ownership paths must be implemented and tested
before claiming the complete Stage 7 bridge.

## Browser report (`LLH1`)

The runner's JSON report has `format: "LLH1"`. Each entry in `reports` identifies
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
