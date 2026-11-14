# Fabric control batches, ABI 3

The 746-bit ingress carries complete control batches. This replaces the old
per-quantum fence protocol; old marker opcode zero is rejected. The TP action
ABI remains 3. This transport currently serves prepared continuous/demo/board
producers. It does not yet replace the synchronous application transport or
provide its required retained admission/query replies.

| Bits | Header | Operation |
| --- | --- | --- |
| 745:682 | Earliest logical quantum | Same quantum tag as its header |
| 681:680 | 2 | 0: TP command; 1: configuration operation |
| 679:672 | 0xa3, including ABI 3 | TP or configuration opcode |
| 671:640 | Number of following operations, 1 through DEPTH−1 | TP index; or state-select bit 31 plus address bits 15:0 |
| 639:0 | Batch identity in low 64 bits; remainder reserved | TP payload; configuration uses its specified payload fields |

The producer must use a nonzero batch identity unique within its reset epoch,
reserve space for header plus all operations and keep all
operation tags equal to the header. The header remains private until the queue
contains that many operations. Partial batches can fill ingress and backpressure
service, but do not block audio. No next transfer or ARM response consumption is
needed after batch execution begins. Frame-zero prepared admissions are part of
boot setup so an unpaced diagnostic runner also starts with resident actions.

At an idle boundary, a complete due batch wins before the next PROCESS. Its
operations execute in order and all their responses complete before PROCESS can
start. This is atomic visibility to the audio/transition executor, not rollback:
each operation has its own success/refusal result. A valid batch
never waits for another ingress transfer once execution has begun. Future tags
wait without gating audio. A late tag executes at the next available boundary;
INSTALL uses the TP's actual frame clamp/rounding and retains the authoritative
effective frame. The tag is an earliest admission boundary, not a promise that
an ARM publication reached a deadline. Public queue acceptance remains distinct
from device admission.

At most one batch executes per quantum, even if every queued batch is overdue.
A read-only preflight checks the complete body before any operation executes.
It bounds conservative work against `MAX_BATCH_WORK` (default 12,000): TP
operations cost 80 cycles, image activation costs `10*STATE_DEPTH +
4*PROGRAM_DEPTH + 32`, ramp changes cost 128, resident-order COMMIT costs
`14*ACTION_SLOTS + 32`, and other configuration operations cost 16. An over-budget
batch returns a capacity refusal for every operation without dispatching any of
them; audio remains runnable. Preflight runs while audio progresses.

`MAX_BATCH_CYCLES` bounds device execution from header acceptance to the last
response (default 16,384 fabric cycles); expiry requests fault/silence. Queue
capacity bounds operation count. Incomplete publication does not start this
timer. Full publication observed concurrently with a run handshake may be
admitted at the following boundary. Malformed headers/tags/opcodes fault without
dispatching an unsupported operation. Setup/reset and diagnostic SETTLE/PROCESS/
FINISH opcodes cannot enter the live path. Allowed TP opcodes are 0, 4–6, 12, 13,
15 and 17–21. A direct host command already pending at
arm retains its response and prevents new batch admission until acknowledged;
it does not prevent PROCESS/FINISH or continuous audio. No old reply is consumed
as a new batch's acknowledgement.

## Retained replies

Before starting a batch, the fabric reserves space for every operation's reply
in the bounded result FIFO (`RESULT_DEPTH`, default equal to ingress depth).
Insufficient space defers the whole batch and leaves audio runnable. Once a
batch starts, neither response storage nor a stopped ARM consumer can block its
remaining operations. The result consumer acknowledges complete records with
ready/valid; held records remain unchanged. Ordinary TP/configuration refusals
are returned as results and do not request audio recovery. Malformed transport,
device timeout and processing/memory faults retain their fault/silence policy.
Accepted operations are not silently dropped when result credits are full.

The admission decision is registered after complete-body preflight. The logical
quantum increments at most once per clock; its low bit qualifies that the cached
decision belongs to the current boundary. A changed boundary waits one clock for
reevaluation, without a wide queue-header path into audio start. A positive credit
decision cannot become insufficient while idle: this controller is the sole reply
producer and no batch remains in flight. Ingress has one writer and cannot overwrite
occupied cells, so validated body tags/opcodes need no second per-dispatch check.
Reset discards the preflight and decision together with the queue.

Each 1,088-bit result has the following fields. The shared Rust decoder validates
the record before a transport associates it with an outstanding operation.

| Bits | Field |
| --- | --- |
| 831:0 | TP reply, or the configuration reply below |
| 835:832 | Native error (0–10) |
| 837:836 | Operation kind 0/1 |
| 845:838 | Issued opcode |
| 877:846 | Zero-based operation ordinal within the batch |
| 941:878 | Batch identity |
| 1005:942 | Actual quantum at batch admission |
| 1037:1006 | Reset epoch at admission |
| 1087:1038 | Reserved zero |

Replies retain transport results; TP request rows separately retain transition
completion until generation-checked collection. The two lifetimes must not be
confused. Prepared demo/benchmark consumers explicitly drain these transport
replies without interpreting them. The application transport and its historical
query/media reservations remain unfinished work.

The conservative work costs and watchdog are containment bounds; actual deployment
deadline closure requires measured complete processing/publication times, including
the resident configuration prelude. Retained media/query observations and application
migration remain open. No M5 or physical qualification claim follows from this ABI.

Coordinated reset clears queue, partial publication, batch timer and per-quantum
quota together with the processing core. Existing descriptor/bank/epoch and AXI
drain rules are unchanged. The event ROM stops asserting valid after its final
record. Diagnostic stepping is separate from continuous fabric progression.
Legacy direct TP/configuration/sample/media ports are available with audio
disabled for setup and diagnostics. Their live admission/data handshakes are
blocked while enabled; normal control uses the bounded batch path. Finish all
setup operations before arming. Live media/query service requires the autonomous
cut/ownership transport described in the completion plan, which is still open.


## Staged signal images and live ramp changes

Configuration opcodes 0–8 use `audio_image`; replies occupy at most 129 bits.
The existing state-processor configuration port remains the only active-memory
writer. Ordered uploads target separate staging RAM and never overwrite live
mutable state. Capacities remain the configured program/state/job/buffer limits.

| Opcode | Payload / operation | Reply |
| --- | --- | --- |
| 0 WRITE | Low 129 bits; state-select and address in operation index | Zero |
| 1 BEGIN | Expected active version u64 at 0; instruction/state/job/buffer/copy counts u32 at 64/96/128/160/192 | Fresh stage nonce u64 |
| 2 PROGRAM | Ordered address, low 96-bit instruction, nonce u64 at 129 | Zero |
| 3 STATE | Ordered address, low 129-bit initial state, nonce at 129 | Zero |
| 4 COPY | Ordered copy index; source/destination u16 at 0/16, expected-ramp bit at 32, nonce at 129 | Zero |
| 5 ACTIVATE | Nonce u64 at 0 | New active version u64 |
| 6 RAMP | State address; target i32 at 0, duration u32 at 32, expected active version u64 at 129 | Actual new 129-bit ramp |
| 7 STATUS | No payload | Active version u64 at 0, nonce at 64, staging bit at 128 |
| 8 READ | State-select and active address | Active 129-bit value |

BEGIN checks capacities and expected version, invalidates an abandoned staging
nonce and resets ordered upload counts. ACTIVATE requires all declared uploads
and an unchanged base version. At the actual cut, it reads every retained live
value into the staged destination before writing any active program/state word.
This permits copy permutations and preserves advancing ramps. A copy kind mismatch
refuses activation before active writes. An unexpected error after active writes
began requests sticky fault/silence. Counts switch only after all active writes
succeed. The bound is `6*copies + 4*(instructions+states) + 32` cycles with the
configuration response timing of this pipeline.

RAMP reads the actual live value at execution and computes full-range Q8.24
interpolation before writing the new start/target/duration/elapsed. It never uses
an ARM snapshot. Successful program WRITE increments the checked active version
and invalidates staging; state WRITE does not. Nonces/versions never wrap.

## Resident configuration actions

Future exact-cut activation, parameter changes and reads use `audio_actions`,
not a far-future header in the ingress FIFO. Headers are admission transport;
production service submits current/overdue headers. Finite prepared ROMs may use
future headers as an explicit diagnostic producer. A resident far-future action
cannot hold an immediate command behind it.

The ARM owner keeps original commands, duplicate sequences, credits and generation
lifetimes. It constructs a dense order by effective frame then sequence in an
inactive publication bank. The device validates the publication at its actual
cut, clamps newly published actions there, then flips banks. If clamping changes
ordering, the whole publication is refused before any active action changes;
ARM can rebuild and retry while the old bank continues running. Ordering checks
skip terminal/stale nodes. Preservation of all still-pending actions, uniqueness
and complete membership are trusted construction properties of the shared owner.
Raw nodes are bounds checked and iteration is bounded by the declared capacity.

INSTALL stores a **private** row; its ACK is not admission and promises no future
effect. Successful order COMMIT is admission for new rows. No ARM continuation,
publication, observation or result draining is needed after that ACK. Private
installation may be abandoned with ABORT. Uncertain exchanges quarantine the
owner and cannot free or guess a generation. The lowest free nonretired slot is
reused only after validated collection/abort. Cancelled/terminal entries retain
duplicate protection until collection.

| Configuration opcode | Payload / operation | Reply |
| --- | --- | --- |
| 0x80, 0x85, 0x86, 0x88 INSTALL | Operation address is action slot; native payload 224 bits at 0, requested frame u64 at 384, sequence u64 at 448, generation u32 at 512, native address u16 at 544, native state bit at 560 | Private row |
| 16 BEGIN_ORDER | Expected active version u64 at 0, count u32 at 64 | Fresh nonce u64 |
| 17 ORDER_NODE | Ordered operation address; action slot u16 at 0, generation u32 at 16, nonce u64 at 64; cancellation target slot u16 at 128, generation u32 at 144 | Zero |
| 18 COMMIT_ORDER | Nonce u64 at 0 | New version u64 at 0, actual publication frame u64 at 64 |
| 19 READ_ACTION | Action slot address, generation u32 at 0 | Retained row |
| 20 COLLECT_ACTION | Same; requires terminal | Retained row, then advances generation |
| 21 ORDER_STATUS | No payload | Count/version/nonce/frame u64 at 0/64/128/192; staging bit at 256 |
| 22 ABORT_PRIVATE | Slot address and generation; refuses published rows | Private row, then advances generation |
| 23 CANCEL_ACTION | Slot address and generation; retains terminal | Terminal row |
| 24 PAYLOAD_PRIVATE | Slot address; generation u32 at 0, native payload 512 bits at 64 | Private row |

A 768-bit resident row contains valid at 0, generation u32 at 1, effective frame
u64 at 33, sequence u64 at 97, native opcode u8 at 161, state bit at 169,
address/flags u16 at 170, terminal bit at 186, native error u4 at 187, terminal
frame u64 at 191, payload 512 bits at 255 and published bit at 767. Legacy
configuration actions retain zero padding above their native input/reply. Their
cancellation uses terminal error 11; transport errors remain 0–10. Native public
results instead use the payload's tag byte at bit 504. ARM retains the original
request/frame for reconstruction. Terminal payloads remain until collection.

With `NATIVE_ACTIONS=1`, INSTALL opcodes `0x89..0x94` reserve public rows for
transition, clear, cancellation, query, export, release, repeat, parameter, initial
activation, graph activation, cursor and retirement. INSTALL writes only the header; PAYLOAD_PRIVATE
must complete before publication. Publication rejects an incomplete native row
before changing the active bank. General prepared/demo/board parents currently
leave this profile disabled; the native public delivery integration remains M5
work. Do not infer support for those operations in a deployed parent from the
component's private reservation whitelist.

Each ordered node is 96 bits: its row slot/generation occupies the low 48 bits;
the cancellation target slot/generation occupies the high 48. The shared ARM
owner rebuilds these references when publishing a later-admitted target. An
unknown target has generation zero. Lookup/writeback checks generation and
sequence, so collected/reused targets cannot alias. The private write guard is
single-use and acknowledges only after synchronous RAM commit. It cannot change
row validity, generation or publication identity. No public sequence CAM is
required in fabric.

The native delivery component uses three device phases: mode 0 delivers before
PROCESS; mode 1 resolves native outcomes/queries after PROCESS and capture cuts;
mode 2 collects native execution cells after FINISH. Public results remain
retained through that native collection. Transition outcomes carry their actual
TP terminal frame. Future public rows consume native execution cells only at
delivery, preserving lowest-slot allocation and stop-cancellation behavior.
Family flags at address bits 1:0 distinguish planner, processor and ordinary
commands; bit 2 identifies recovery-lane commands. Planner cancellation uses
public references and distinguishes queued, delivered, terminal, recovery and
collected targets. Repeated cancellation of a still-pending native request in
one delivery phase remains idempotent. Public row collection is a separate
user-receipt lifetime; an ARM copy of a result does not authorize row reuse.

The continuous coordinator reserves a configuration cut before PROCESS. It scans
at most `ACTION_SLOTS` generation-qualified nodes in published order and executes
due actions, then permits the existing TP PROCESS/audio/FINISH sequence. It does
not resolve loop wraps, source ticks or transitions. The fixed profile's logical
frame is `process_sequence * MAX_FRAMES`; run-quantum mismatch and unsuccessful
PROCESS/audio/FINISH fault before I/O sequence advancement. Held legacy TP replies
and transport reply credits cannot gate resident effects. A scan has its own
watchdog and conservative per-cut work budget (8,000 cycles by default). The
admission batch and resident prelude together allow at most
`12,000 + 8,000 + 7*ACTION_SLOTS + 32` conservative control cycles per quantum;
this combined bound must be included in each deployment's deadline budget.
Admission and resident work have separate reservations, so command floods cannot
consume the resident allowance. Work beyond the resident budget receives a
retained capacity outcome without dispatch or audio fault. The normal producer
must account for that budget before promising application effects.

Reset clears resident rows/publications together with the processing epoch.
Generation exhaustion retires a slot; nonce/version exhaustion refuses publication.
The image and resident-action component tests cover full-size copy activation,
1,050 unserviced quanta with future image/ramp effects, private/partial/unsorted
publication, exact late cuts, cancellation, full credits, generation reuse,
ordinary work-budget refusal, stale identities and uncertain ownership.

## Historical public observations (resident application profile)

`RESIDENT_ACTIONS=1` connects native public delivery to device phases before
PROCESS, after PROCESS/capture cuts and after FINISH. The application profile has
one loop and mono/stereo managed capture. General prepared/demo/board defaults
retain their raw kernel profile. The existing Rust application/mono consumers
have not migrated yet.

Native actions 12/13 retain State/Export results at the post-PROCESS cut. Each
public row reserves its corresponding retained-record cell before admission.
The fabric holds the coherent source through a bounded 31-clock copy; neither
ARM readiness nor response delivery participates. A public snapshot has 30 u64
payload words in a 32-word stride; word 31 stores its generation/session epoch.
The smaller standalone component's default remains 29 payload words.

The payload is the 1,728-bit media cut, followed by gain/balance f32 bits at
1728/1760, smoothing frames u32 at 1792, LinearRestart flag at 1824, topology
epoch u64 at 1825 and 31 reserved zero bits. LinearRestart(0) remains distinct
from None. The transport session epoch, media cut graph epoch and topology epoch
are separate. The current connected fixture has initial gain/balance/smoothing
and epoch values; mutable parameter/graph integration remains open.

Snapshot result tag 5 uses payload slot u16 at 0, generation u32 at 16, session
epoch u32 at 48, word count u8 at 80 (30), export flag at 88, actual cut frame
u64 at 96, graph epoch u64 at 160 and topology epoch u64 at 224. All remaining
payload bits below the tag byte are zero. The shared reader verifies this
identity against the retained public row and checks every returned word's
metadata before reconstructing public values. Global application frames use a
fixed session-origin offset; initial activation must not reset that offset.

Configuration operation **48 READ_RECORD_WORD** takes the public slot as its
address, generation u32 at payload 0, session epoch u32 at 32 and word index u8
at 64. It returns 160 bits: data u64 at 0, word u16 at 64, slot u16 at 80,
generation u32 at 96 and session epoch u32 at 128. The full index is checked
before narrowing. Reads retain ownership. COLLECT_ACTION (20) validates and
acknowledges snapshot RAM before freeing its public row; it never releases an
export lease. Old-generation/session reads return InvalidId without changing
storage. Reset invalidates both partial and complete records.

Native **14 RELEASE**, **19 CURSOR** and **20 RETIRE** use generation-qualified
export/content identities. ARM validates immutable scopes and defers ordinary
errors to delivery; the device validates the current lease/content. Cursor
results use tag 8 with a content slot/generation in the low 64 bits, and borrow
that content. Releasing an export does not invalidate a cursor while its content
is still latest/playback or otherwise retained; retirement invalidates subsequent
reads. Empty-export cursors return InvalidShape. MIDI cursor requests retain
their unsupported-operation error precedence.

Managed live batches return bounded Unsupported replies for raw TP install,
cancel, collect and membership/repeat mutation, and for raw active image/ramp
writes or legacy resident actions. They cannot bypass the private resident owner.
Status, staging uploads, native public commands and record reads remain service
operations. Full replies, held records and a stopped reader leave audio runnable.

## Canonical configuration profile

The managed profile has two inactive image banks; the diagnostic profile keeps
one. BEGIN/STATUS select a bank by address. Upload tokens and activation nonces
include the bank in their low bit and advance monotonically without reuse.
Consuming one bank leaves the other staged image intact. Ordinary activation
still checks the active image version and copy map. Private canonical activation
requires a complete map-free image and independent stage preflight.

Configuration **49** uploads canonical keys and immutable binding maps. Its
payload is opcode u8 at 0, key u16 at 8, bank bit 24, nonce u64 at 32, count/index
u32 at 96, state address u16 at 128, value 129 bits at 144, target i32 at 273
and duration u32 at 305. Host operations are seed (0, before initial load),
begin map (1), append ramp (2), read key (7), validate map (8), and append
smoothing word (10). Other suboperations return Unsupported. Active retiming,
snapshotting and activation are private fabric operations. Ordered map uploads
are bounded by STATE_DEPTH and checked before addresses are narrowed.

Canonical keys preserve inactive ramps. Retiming reads the first active binding
at the actual cut, evaluates full-range Q8.24 progress, and writes all bindings.
Graph activation snapshots old bindings in order (the last duplicate wins),
patches the staged image from canonical state, installs it, then rebuilds active
bindings. Smoothing entries combine the new compiled buffer base with current
smoothing frames. The distinct None/LinearRestart(0) state remains observable.

Configuration **50** installs/releases immutable stage guards; its suboperation
is the byte at 632. Graph stage/release are 0/1, pair ready/cancel/recycle 2/3/4,
topology ready/cancel/recycle 5/6/7. Shared constructors encode complete scoped
identities. The device retains generation, readiness and validated graph epoch;
ARM construction does not authorize a future activation after release/reuse.

Native **17 INITIAL** and **18 GRAPH** preflight epochs, identities and readiness
in application error order. Their executor checks the complete image/map and,
for INITIAL, the actual empty TP and supported primitive specification before
changing active state. Graph activation snapshots old bindings; initial
activation installs canonical fields without snapshotting an earlier program's
elapsed progress. Patch/install/map-swap and INITIAL_LOOP then finish without
another ARM transfer. Unexpected failure after checked mutation
faults the session. Successful activation advances the actual graph epoch;
INITIAL also advances topology epoch and preserves the running TP frame.
Results retain tag 7/6 and the committed identities/epochs. Native **15 REPEAT**
checks the actual loop identity and applies its repeat bit before PROCESS.

Canonical activation work is conservatively charged as
`48*STATE_DEPTH + 6*PROGRAM_DEPTH + 4352`; the managed action controller uses a
32,000-cycle work allowance and 65,536-cycle watchdog. These bounds include map
and state-copy work. Final workload admission/deadline qualification and routed
timing remain open; the watchdog alone is not an audio deadline guarantee.

## Resident signal controls and monitoring

Native **16 PARAMETER** carries the node/loop identity in low 64 bits and the
operation byte at 64. Operations are gain/balance/mute/presence/crossfade/monitor
(0–5), loop gain/balance/smoothing (6–8), input-source/discard/configure (9–11),
and reader start/stop (12–13). ARM encodes immutable float validation and Q8.24
targets; the device checks the actual node/loop and mutable state in application
error order. Original f32 loop values remain in historical observations.
Reader controls validate delivery; ARM bulk stream servicing remains separate.

Configuration **51** installs immutable node metadata before loading (suboperation
0), updates a generation-qualified monitoring policy (1), or reads status (2).
The suboperation is at bit 632. Metadata contains node generation, signal kind,
channel/ramp limits, manual monitoring, policy generation and pending configuration.
The current managed profile reserves 128 metadata rows and seven canonical keys
per node. Configure and discard retain distinct processor-pending and canonical
configuration ownership, matching the application executor. Initial activation
applies canonical configuration; a later graph commit consumes only matching
pending processor epochs.

After PROCESS, before query/audio execution, the fabric scans native requests
for the final tick of a pending RECORD and updates strip monitoring from actual
recording/pre-recording state, manual demand and policy generation. An unchanged
target does not restart its ramp. The core holds its coherent cut throughout this
work; no ARM polling or copied per-quantum state image is required.

Immutable binding upload **49/12** describes a conditional balance binding.
Its base word packs program address u16 and destination buffer u16. At activation
the device reads the node's canonical stereo flag. A disabled balance instruction
becomes a zero constant in inactive program memory and its ramp is excluded from
active heads/snapshots. The frozen parameter still accepts commands. Enabling it
in a newly uploaded graph restores the original gain instruction and retained
progress. This preserves inactive-ramp behavior across channel-shape changes.
Private image operation 13 patches only a fully uploaded inactive program.
