# Compact actions, results and execution control

This contract evolves the indexed installation interface. Public application
requests and completions keep their encodings in the Rust adapter; the wire
representation is private to the service and the RTL.

## Field audit and compact records (current ABI 3)

PREPARE word 5 carries the ABI revision. A mismatch returns `invalid_config`
without preparing the device. STATUS bits 144–159 report the supported revision.
M2 used ABI 2; membership uses ABI 3. Old prepared ROMs must be regenerated.
Bus widths remain 640/832 bits; unused
high bits are zero. This does not claim a physical register map or bus saving.

| Field | Admission use | Resident use / reconstruction |
| --- | --- | --- |
| Slot/generation | Bounds and empty-row reuse guard | Retained; result identity and checked retirement |
| Operation, clear, retrigger, timing kind | Input validation | Retained for real-time execution |
| Target slot/generation | Validate against prepared specs | Slot retained; generation comes from Rust's original |
| Sequence u64 | Public duplicate checking in Rust | Retained in full for offer/protected arbitration and stop cancellation |
| Requested frame u64 | FPGA clamps/rounds against its actual frame | Original retained in Rust; no resident echo |
| Source slot/generation | Validate configured synchronization relationship | Prepared target spec supplies the immutable source; original retained in Rust |
| Original skip u64 | Initializes remaining ticks | Rust retains original; FPGA retains the mutable countdown |
| Position/revision u64 | Alignment validation where applicable | Retained for operation validation, events and the clear extension |
| Effective frame u64 | FPGA computes with checked rounding | Retained and returned authoritatively |
| Remaining ticks u64 | Initialized from skip | Updated only by delivered source ticks; returned for pending inspection |
| Outcome/reason/terminal frame | Initialized pending | Written by FPGA; frozen until collection |
| Membership | ARM compiles immutable generation-qualified links | Two publication banks; resolver never inserts or unlinks |

Loop specs cannot be reconfigured after PREPARE. Thus source identity and target
generation need not be repeated in every resident action. SET_REPEAT changes a
loop's repeat flag, not that immutable relationship. The resolver still checks
the prepared source at the real-time reaction point.

Compact INSTALL input fields are generation at bit 1 (32), timing at 33 (1),
operation at 34 (2), retrigger at 36 (1), clear at 44 (1), target ID at 45 (64),
sequence at 109 (64), requested frame at 173 (64), source ID at 237 (64),
original skip at 301 (64), and position/revision at 365 (64): 429 significant bits.
The service clears unused header/result bits rather than accepting caller state.

The resident/public-wire result cell has valid at 0, generation at 1 (32), timing
at 33, operation at 34 (2), retrigger at 36, outcome at 37 (3), reason at 40 (4),
clear at 44, target slot at 45 (32), sequence at 77 (64), position/revision at
141 (64), effective frame at 205 (64), remaining ticks at 269 (64), and terminal
frame at 333 (64): 397 bits. M2 appends two internal `H`-bit list pointers, with
`H=max(1,ceil(log2(REQUEST_DEPTH)))+1`, for 417 bits at 272 requests.

A single compact cell represents either a pending action or its retained terminal
record. Reserving one cell per admitted action guarantees result capacity without
duplicating action RAM or relying on a drained event FIFO. Collection captures
the cell into stable reply storage before releasing it and advancing generation.
Rust reconstructs the public 640-bit record from its retained original plus the
device's header, effective frame, countdown and terminal frame. It validates the
identity and immutable execution fields before releasing ownership. Empty-row
inspection preserves next-generation information without inventing a request.

## Reply and execution ownership

The synchronous host interface has one held reply cell and at most one outstanding
host operation. Its ready/valid payload remains stable until consumption. An
uncertain exchange faults the Rust owner; a transport retry does not guess whether
an action installed or a result was collected.

Fabric PROCESS/FINISH uses a separate bounded start/done/error path into the
same resolver FSM. It does not share or consume the host reply cell. Execution
gets priority at an idle resolver boundary; streaming loop reads remain available
to the audio engine in the settled phase even if a host reply is held. Host
commands are admitted only at existing legal mutation boundaries. The coordinator
must not use host `response_ready` or host command traffic as a prerequisite for
already-runnable audio. A host-settle response is captured when settlement ends;
the fabric phase transition does not wait for ARM to consume that response.

These rules remove reply-induced stalls. Existing run/fence prerequisites and
the application adapter's service-driven settlement pause remain. Tests supply
those prerequisites explicitly; neither fence-free autonomy nor an ARM deadline
is claimed here.

## Membership publication and admission

ARM owns an ordered membership forest: one AT and one source-tick root per loop,
and one generation-qualified node per included physical request slot. Links
strictly increase in physical slot order. Terminal cells may stay in the forest;
FPGA checks valid, generation, target, timing class and pending outcome before
acting. Empty reserved cells are harmless. The shared pool has no per-loop quota.

The device has two node banks and two root banks. Execution reads only the active
bank. At IDLE (between PROCESS/FINISH), the host may write the other bank one
bounded exchange at a time. PROCESS still runs between those exchanges, including
with a held publication reply. The resolver and host never run simultaneously,
so a successful COMMIT immediately retires the old bank without an additional
reader acknowledgement. A later BEGIN can reuse that retired bank. A second
BEGIN abandons an incomplete stage and issues a fresh nonce.

Each wire command has an 80-byte payload and at most one 104-byte reply. Fields
below are 32-bit words; a u64 occupies words 0–1, little endian. Unused input words
must be zero in trusted producers; replies are zero except the specified result.

| Opcode | Payload / index | Successful reply |
| --- | --- | --- |
| 18 BEGIN | words 0–1: expected active version; word 2: declared node count | fresh u64 stage nonce |
| 19 NODE | words 0–1: nonce; word 2: generation; word 3: successor or `0xffffffff`; index: physical slot | zero |
| 20 ROOT | words 0–1: nonce; word 2: AT root or sentinel; word 4: tick root or sentinel; index: loop slot | zero |
| 21 COMMIT | words 0–1: nonce | committed u64 version |
| 4 INSTALL | compact action plus publication at bit 429 (64); index: reserved slot | slot/generation |

BEGIN checks the expected active version and capacity, clears the inactive bank's
written mask/counters, and increments a separate u64 stage counter. Abandoned
stages never reuse a nonce. Exhaustion returns `exhausted` without changing the
active view. NODE checks slot bounds, current row generation, uniqueness and a
strictly greater in-range successor (or sentinel). ROOT follows all declared node
writes, in increasing loop order, and checks that each root is written/in range.
COMMIT requires every declared node and every loop root, then flips banks and
publishes the nonce atomically. Stale tokens, writes without an open stage, early
commits and invalid identities are refused without changing action state.

Global construction properties (complete/disjoint per-loop forests and correct
membership classes) are checked by the trusted Rust compiler. Raw malformed
traffic cannot create a backwards/cyclic walk or access a row out of bounds.
A link to an unwritten slot faults finitely if reached; hardware does not run an
extra full forest validator or recover malformed trusted input silently.

A public admission reserves one ARM credit, publishes the forest including its
still-empty device cell, then performs INSTALL. INSTALL checks current row
identity, emptiness, the committed publication and matching membership generation.
**Successful INSTALL is the admission point**: FPGA clamps and rounds against
its current frame then, even if audio advanced during staging. There is no
accepted action absent from membership and no staged action exposed early.
Publication never writes effective frame, countdown, outcome or terminal data.

A definite refusal releases only the unadmitted ARM reservation. A forest already
committed before a refused INSTALL may contain an empty node; it remains harmless
and the next publication replaces it. An uncertain/malformed reply faults the
owner and quarantines further mutations until coordinated device reset. No retry
assumes whether COMMIT, INSTALL or COLLECT took effect. Reset drains/invalidates
the transport together with both banks, versions, cells and Rust metadata; host
metadata must never be reset alone.

## Terminal retention, reuse and optional compaction

Cancel, application, supersession and stop cancellation change authoritative
cells and the absolute-action bitmap; they never edit roots or neighbours.
Collection first captures the terminal reply, clears the cell and advances its
generation with checked retirement. Old membership still names the old generation
and cannot execute a reused cell. Before installing the new generation, ARM
publishes the replacement forest. A held collection reply cannot gate execution.
The Rust owner releases the credit only after validating that reply.

ARM can omit a terminal only after an authoritative cancel/read/collect result;
it does not predict outcomes. The virtual service operation 22 publishes a compact
forest using those known terminals, without releasing credits or sequences. It is
not an RTL opcode. Omitting cleanup leaves finite stale entries and preserves
all admitted behavior. Static preparation reserves all actual credits once,
publishes a single boot forest (fresh-reset nonce 1), and emits INSTALL at each
original delivery boundary. It does not predict completion or recycle credits.

## Storage and traversal bounds

For loop capacity L, request capacity R, and
`H=max(1,ceil(log2(R)))+1`, action/result RAM is `397R` bits. Immutable membership
adds `2R(32+H)` node bits, `4LH` root bits and two R-bit presence masks, plus
fixed counters/version/reply state. At L=64/R=272/H=10 this is 107,984 action bits,
22,848 node bits, 2,560 root bits and 544 mask bits. Loop/spec/order/event storage
is unchanged. M2's temporary action rows were `397+2H` bits. Physical BRAM totals
include bank duplication and mapping fragmentation; see the measurements.

A single list walk visits at most R monotonically increasing slots, including
stale entries. Trusted forests are disjoint: at most R tick nodes across all
source reactions in a quantum, because each loop receives its source tick once.
The absolute bitmap visits at most R slots. A loop can apply an absolute and a
tick action; each successful stop scans both lists for that target. Conservatively
there are at most two cancellation scans per loop, or 2R node visits across all
targets. Thus request traversal is at most 4R visits per PROCESS under valid
publication, plus bounded per-offer/application work and O(L) loop phases. A raw
malformed shared forest can raise this to O(LR), but not an infinite walk. The
per-list bound also applies while ARM is absent; no compaction deadline is needed.
These are visit bounds, not full-core or board deadline qualification.

Admission sends at most R+L+3 transfers (BEGIN, nodes, roots, COMMIT, INSTALL).
Compaction sends at most R+L+2. ARM preparation uses preallocated O(R+L) buffers
and builds physical-slot order in O(R+L); one synchronous public operation may
therefore involve multiple bounded wire exchanges. Keep this command service off
the audio callback. Host preparation times are native simulation measurements,
not an ARM performance or memory-interference qualification.
