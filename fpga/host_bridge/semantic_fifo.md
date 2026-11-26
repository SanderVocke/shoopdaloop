# Semantic command FIFO contract — hardware ABI 16

This is the frozen implementation contract for Stages 4–5 of
[`FPGA_CMD_FIFO.md`](../../FPGA_CMD_FIFO.md), under its approved scope and ceilings.
**It is not a claim that ABI 16 is implemented or deployed.** The current legacy
mailbox remains ABI 15 until the replacement and matching software pass their
integration gates. Historical ABI-15 staged patches are not this interface.

## Ownership, profile and pacing

ARM is the trusted compiler, allocator and immutable address binder described in
[`LOADING_DESIGN.md`](../../LOADING_DESIGN.md). It supplies no continuing-state
initializers, effective frame, resolved transition, replacement loop record or
snapshot-derived execution decision. The FPGA owns semantic execution.

The profile has four complete input/result bank pairs, at most 32 commands per
group, two independent snapshot banks, a physical loop allocation range
`0..128`, and the existing persistent-state capacity (512 on Z-turn V2). The
physical loop RAM can be larger; neither allocator overlap nor commands may use
addresses outside that enforced 128-row profile. Other geometries require an
explicit matching profile, not snapshot truncation.

Static regular/explicit composites, nested/shared schedules, authored supported
audio work, transport, repeat-sync and iteration/seek remain supported. A live
replacement refuses with `GraphCompositeUnsupported` if **either** the active or
candidate program contains composites, including introduction/removal, child
edits and unrelated edits while a composite is installed. Stopping transport is
not quiescence. No automatic stop, future cut, restart preview or retry exists.

Quiesced replacement is a separate, explicit runtime lifecycle: stop/retire the
old execution owner, obtain the real JP/cache/DMA ownership fence or safe runtime
reset, create a fresh incarnation and allocator, then perform initial installation.
It is not an in-place composite switch in a stopped transport. A timeout, SP idle,
new revision or snapshot is not that fence. Preserve uncertain resources until
safe recovery. Native CPU/default-browser graph editing is outside this profile;
the optional `fpga-reference` worklet shares it.

Only the processing driver/audio device produces quantum opportunities. Uploads,
publication, polling, interrupts, result reads and snapshot reads do not advance
frames. Startup enables safe empty/silent processing before waiting for the first
ordered installation; it must not wait for an installation reply before enabling
the only source of processing boundaries.

At each quantum opportunity F, latch the completed-publication cutoff. Attempt
**at most one** previously published group in FIFO order, then run SP and JP.
Publications completing on that edge are eligible only at the next opportunity.
A delayed group is checked against the actual eventual F, not its submission
frame. No SP visit or snapshot capture interleaves commands of a group. There is
no whole-group rollback: earlier successful commands remain applied after a later
command refusal; later commands still run unless a genuine hardware fault stops
execution. Repeated physical targets observe preceding commands' actual edits.

## Identity and publication

All words are little-endian u32; all MMIO accesses are aligned. Hardware runtime
incarnation is the existing nonzero, nonwrapping u32 recovery `EPOCH`. Graph and
topology are u64 and both denote the one program revision. Group sequence is u64,
starts at one per incarnation and advances only after admission. No identity wraps.

Each bank is `Free -> Filling -> Queued -> Executing -> [WaitingFence] -> ResultReady
-> Free`. Its reliable result capacity is reserved by that same admission: there
is no accepted group without a complete result slot. Unread results can exhaust
admission, never rendering. A fault may retain a bank until safe recovery.
The execution cursor advances after edits/selection, independently of result
release: a delayed ownership fence must not prevent later ordinary groups from
running or cause the earlier group to execute twice. Reliable visibility/release
is FIFO: the ready mask exposes only the oldest unreleased group when complete;
a later bank cannot be read or acknowledged around an earlier pending fence.

ARM selects a free bank and begins filling with the expected incarnation. It
writes the complete header and body **sequentially**, with full-word strobes, then
establishes platform memory visibility and publishes once. A small per-bank word
cursor proves completeness; no byte-valid bitmap or per-command shadow record is
required. Data implicitly appends at that cursor; random packet-word writes are
not exposed. Re-beginning an unpublished fill discards that private fill.
Partial-strobe or excess writes refuse; publication cannot expose unwritten words.
Writes to an admitted bank refuse without changing it. Published data is immutable
until its matching result is acknowledged.

Publication checks incarnation, exact next sequence, header format/count/flags,
exact written length, bank ownership and reserved result space. It does **not**
check expected program revision/deadline early: preceding queued switches can
change the configuration. A refusal admits nothing and does not consume sequence.
A duplicate/old sequence never executes again. Retained bank state, sequence and
last-admitted watermark support reconciliation after a lost acknowledgement;
absence after reset is not permission to replay an uncertain workload.

### Group header: 16 words / 64 bytes

| Words | Meaning |
| --- | --- |
| 0 | Magic `0x53434631` |
| 1 | ABI 16 in low 16 bits; command count 1..32 in high 16 bits |
| 2 | Expected runtime incarnation |
| 3 | Flags: bit 0 deadline present; all other bits zero |
| 4–5 | Group sequence |
| 6–7 | Expected graph revision |
| 8–9 | Expected topology revision |
| 10–11 | Inclusive latest attempt frame; zero when absent |
| 12–15 | Reserved zero |

At the attempt, header refusal priority is stale graph/topology, then deadline
`F > deadline`, then malformed switch placement or latched execution fault.
Corresponding codes are `StaleConfiguration`, `DeadlineMissed`, `CannotApply`.
A switch must be last and occur at most once. Header refusal executes no commands
and gives every command that header code. Header `Applied` means the header was
accepted, not that all commands succeeded.

### Bound command: 12 words / 48 bytes

| Word | Meaning for ordinary commands |
| --- | --- |
| 0 | Kind: 0 transition, 1 clear, 2 repeat, 3 gain, 4 iteration, 5 switch, 6 stateless refusal |
| 1 | Physical loop slot in low 16 bits; gain-state address in high 16 bits |
| 2 | Physical loop generation |
| 3 | Immutable binding flags: bit 0 self/no external sync; other bits zero |
| 4 | Stateless refusal code, or zero to execute |
| 5 | Original finite f32 gain bits for gain; otherwise zero |
| 6–10 | Five semantic argument words |
| 11 | Reserved zero |

A normal record is bound to the expected program, not the most recently compiled
candidate. Generation is checked against the live physical record. A valid gain
address is required only for gain. No binding registry or graph lookup is added
to FPGA execution. Trusted stateless refusals preserve one result position for
invalid values, unavailable logical targets, unsupported operations or cancelled
unpublished preparation; they must never forge `Applied`/`Scheduled`. Validate
original command values before address lookup as the Rust oracle does. Never drop
an invalid gain, silently split a group or change the remaining command order.
Unknown kinds refuse `Unsupported`; malformed ordinary reserved fields refuse
`InvalidValue` without writes. Stateless refusal codes are 4..9 and 11..29,
retaining their protocol meanings; zero, successful outcomes, attempt-time
header codes and unknown codes are invalid. Only state-independent binding or
preparation facts may produce them. A refused switch keeps kind 5 so placement
checks and earlier ordinary command effects are preserved.
Gain metadata must be finite (an exponent-bit check suffices); the trusted binder
is responsible for matching that original float to its fixed-point conversion.

Argument words, in increasing order:

- Transition: now mode, first mode, first skip, second mode, second skip. Modes
  are Stop=0, Play=1, Record=2; absent is `0xffffffff`, with absent skip zero.
- Clear: mask 1/2/3 (first/second/both), then zeroes.
- Repeat-sync: boolean 0/1, then zeroes.
- Gain: saturated Q8.24 constant as in `Fixed::from_f32`, then zeroes. Nonfinite
  input is an `InvalidValue` record, not a float conversion or dropped command.
- Iteration: u64 iteration in the first two words; retrigger bit 0 and seek bit 1
  in the third; remaining words/bits zero. Checked multiplication and live-mode,
  length and frame validation execute in the FPGA.
- Switch: u64 next revision; region in low 16 bits and instruction count in high
  16 bits; sync count in low 16 and state count in high 16; buffer count in low 16
  and candidate-has-composites in bit 16. Remaining bits zero. Words 1–5 and 11
  are zero except word 4 may carry a stateless refusal. The trusted compiler's
  immutable composite marker is retained with the selected run parameters.

The shared sequential kernel validates each command before its first live write.
Its ordinary outcomes retain `shoop_rt_protocol::immediate::Code` values:
`Applied=1`, `TargetUnavailable=4`, `InvalidValue=5`, `Unsupported=6`,
`CannotApply=7`, `CaptureUnavailable=8`, `Scheduled=10`. Switch/profile refusals
also use `StaleConfiguration=3`, `GraphCompositeUnsupported=23`, `Cancelled=29`.
Actual read/write/pipeline faults remain faults, not safe semantic refusals.
Uncertain mid-write failure fails closed; there is no inferred rollback or safe
reclamation from an ordinary refusal code.

## Program selection and ownership completion

Uploads and initialization of unused allocations remain a separate low-priority
loader lane. Complete their writes/barriers before publishing a switch. Neither
selection nor the reliable result waits for an ARM release or readback.

A switch checks expected revision, increasing next revision, region/run parameters
and the reduced composite profile before mutation. Initial installation at
revision zero can contain composites. Primitive-only replacement preserves actual
continuing state and selects all run parameters together at F. Keep the existing
real hint/JP/cache/DMA fence. The result slot stays reserved until that fence can
truthfully report ownership completion; a read-only selected/pending status is
not success. A delayed fence does not make a second consumer execute the group.
Only one prepared/pending program exists; no third image may reuse old resources.

**Source-change rule:** the first SP visit under the new inline source operand
invalidates old-source plans when that operand differs from the last executed
source. Use existing record source fields as execution-owned comparison state;
no ARM read/patch, source registry or separate scheduler is required. This retains
the prepared reference's current `Engine::switch` behavior, which clears
`changed_sources`, without expanding a switch into up to 128 kernel invocations.
An explicit clear command remains supported. All commands preceding a last switch
use the old binding; the SP source-change check occurs before that quantum's
scheduled-plan/pulse processing. The reference must agree on that ordering.

This refines the earlier loading document's explicit-clear implementation rule:
128 clears at the measured 65 cycles each would alone take 8,320 cycles and cannot
fit a 32-command/4,096-cycle group. It changes neither ordinary switch semantics
nor the approved group capacity/budgets. Verify broad source changes, repeated
targets and explicit clear-plus-switch together in the integrated tests.

### Reliable result: 16-word header plus one u32 code per command

Header words: magic `0x53465231`; ABI/count as above; incarnation; flags;
sequence (2 words); attempt F (2); initial revision (2); selected/current revision
(2); header code; first frame (2); switch outcome. Flag bit 0 denotes an actual
hardware fault/uncertain outcome; other bits are reserved. Switch outcome is
0 none, 1 refused, 2 fenced success, 3 fault/uncertain. Never publish fenced
success before the real fence. No result contains `Unprocessed` as a successful
command outcome. The result packet is immutable until acknowledged by matching
incarnation, bank and sequence. Duplicate/stale acknowledgements cannot free a
new occupant. Revision fields are captured at the attempt, not recomputed when a
delayed fence completes. `first_frame` is F for a selected switch and zero when
none was selected; it never implies ownership completion on a fault. ARM retains
original records solely for API correlation/formatting.

## Local observations and coherent snapshots

SP pulse/countdown/slot execution remains local. Accumulate pulse frame/validity
and saturating scheduled-failure count locally, including while ARM is halted.
Optional event traces can drop with observable saturating loss; their overflow
cannot become an audio fault. Mandatory local effects never wait for a trace reader.

Keep small observational metadata per physical loop: original f32 control gain,
cumulative failed-plan count, last pulse frame and pulse-valid bit. Initialize
only new-slot leases; continuing values persist. ABI 16 extends `CORE_CONFIG`
with space 3 for these new-lease observations: address `slot * 3 + word`, with
three u64 words in the same order as the snapshot metadata below. Use the existing
configuration payload (`0x103` for write, address, data); unused data bits are
zero. Initialization permits finite original gain with zero failure count, zero
pulse and zero validity. The trusted loader API accepts only unused-slot leases,
never continuing-state restoration. Complete these writes before publishing the
installation. Update gain only after a successful
gain command, and pulse/failure fields from the local SP outcomes. This is copied
observation storage, not a second execution record or source of planning decisions.
It preserves the reference's original f32 gain metadata: a Q8.24 value alone cannot
recover the original float, especially after saturation. No ARM result/event reader
is needed to keep these observations current.

For this profile, target lowering emits the complete sync prefix before audio
instructions. Standalone components can retain their independent-work interleaver;
that encoding must not be selected for the semantic profile. This creates an
unambiguous capture gate without a second scheduler or ARM involvement.

Normal capture is **after the entire SP sync prefix has settled, before JP/audio
work changes persistent values for that quantum**. Drain mandatory local observation
updates before capture. Header F is the render quantum's first frame; raw word 8
is its render cursor and word 9 its processed stamp. Raw word 2 is the next cursor,
not an alternative public playhead. Use the packed-record render interpretation
for capture lengths/plans. A snapshot may show a selected revision before a delayed
ownership completion; it never authorizes reclamation.

Snapshot header, 16 words: magic `0x53465331`; ABI in low 16 bits and phase in high
16 (0 initial, 1 settled); incarnation; quantum; sequence (2); render F (2);
revision (2); physical row extent; row size in u64 words (19); cumulative skipped
publications (2); reserved zero (2). Graph and topology both equal that revision.
Initial capture is explicitly marked and does not manufacture a processing frame.
Each row contains the 16 raw u64 loop words, then gain-f32/failure-count as low/high
u32, last pulse u64, and pulse-valid bit 0 with all remaining bits zero. Membership
and logical names remain ARM bindings; unused physical rows are not current graph
members. The 128-row maximum packet is `64 + 128 * 19 * 8 = 19,520` bytes.

Two banks publish complete immutable packets. With neither free, skip the entire
capture rather than wait. Sequence advances per attempt, skipped count saturates,
and readers can identify gaps. On snapshot sequence exhaustion, disable further
publication and report exhaustion; rendering continues without reusing identity.
A reader can select the latest complete packet and discard an older one. Matching
incarnation/sequence acknowledgement releases a bank; no reader acknowledgement is
needed to execute commands or start another quantum. Reset invalidates both banks.

## MMIO allocation within the existing 4 KiB aperture

Discovery/geometry, retained fault record, recovery and service registers through
0x14c retain their addresses, with `VERSION=16`. Add the following indexed ports;
there is no widened PS aperture. Stage 4 updates the generator schema from this
contract; Stage 5 removes legacy live-control/batch/CPU-capture operations.

| Offset(s) | Registers / meaning |
| --- | --- |
| 0x150, 0x154 | `SEMANTIC_ID=0x53434631`, `SEMANTIC_CAPABILITIES=0x7f` |
| 0x158, 0x15c | `GROUP_BANKS=4`, `COMMANDS_PER_GROUP=32` |
| 0x160, 0x164 | `SNAPSHOT_LOOP_LIMIT=128`, `OBSERVATION_ROW_WORDS=19` |
| 0x168, 0x16c, 0x170 | Command/snapshot/total cycle ceilings: 4096/8192/60000 |
| 0x178–0x180 | Next publication sequence low/high; input free-bank mask |
| 0x200, 0x204 | Selected command bank; its state |
| 0x208, 0x20c | Begin/restart unpublished fill (write expected epoch); sequential command data |
| 0x210, 0x214, 0x218 | Publish (write expected epoch); receipt; written-word cursor |
| 0x21c–0x228 | Last admitted sequence low/high; last quantum cutoff sequence low/high |
| 0x22c–0x238 | Groups attempted low/high; peak command cycles; peak snapshot cycles |
| 0x280, 0x284, 0x288, 0x28c | Result-ready mask; selected result bank; rewind; sequential result data |
| 0x290–0x2a0 | Result ACK sequence low/high, ACK epoch, ACK doorbell, receipt |
| 0x300, 0x304, 0x308, 0x30c | Snapshot-ready mask; selected snapshot bank; rewind; sequential snapshot data |
| 0x310–0x320 | Snapshot ACK sequence low/high, ACK epoch, ACK doorbell, receipt |
| 0x324–0x32c | Skipped snapshots low/high; snapshot status (bit 0 sequence exhausted) |

Capability bits 0..6 respectively require complete groups, reserved reliable
results, settled snapshots, fenced primitive switches, reduced static-composite
profile, local event effects, and one-group-per-quantum service. Matching software
requires all seven plus matching geometry/row format. Public `LIVE_PROGRAMS`
means supported primitive replacement, not arbitrary composite live editing;
`CANCEL_PREPARED_COMPOSITES` permits cancelling unpublished preparation, not a
future-cut switch engine. Retain initial static preparation/automatic monitoring.
Reject an ABI/profile mismatch before attachment or enabling the backend.

Bank numbers use only their required low bits; rewind/ACK doorbells require value
one. Receipt values 0..5 retain accepted/full/epoch/fault/invalid/unavailable;
6 is old/duplicate sequence, 7 incomplete fill, 8 publication sequence exhausted.
Aligned semantic refusals return AXI `OKAY` plus receipt, not an ARM abort.
Unmapped/unaligned or malformed register-protocol accesses retain `SLVERR`;
partial strobes on packet data are incomplete-fill refusals with `OKAY`.
Selecting a result/snapshot bank or rewinding resets its read cursor to zero.
Data reads advance that cursor, not packet ownership; held AXI responses retain
their copied value without holding RAM or execution. Epoch-qualified ACKs are
idempotent. ACKs for a non-head reliable result refuse without freeing it. Read live u64
counters with high/low/high consistency plus epoch bracketing; immutable packet
headers need no retry until the owner releases their bank.

## Arbitration, bounds, reset and verification

Local RAM accepts a granted request with a one-cycle response; production RAM
arbitration must establish that bound for the edit and capture phases independently
of ARM readiness. Packet reads are pipelined; count decode, turnover and metadata
writes in the command budget. Use one kernel/working record and shared arithmetic.

Priority is scheduled processing and its bounded semantic/capture phases before
loader transactions. A partial AXI write never asserts a RAM/config request.
Reserve response holding space before a loader access, latch its response, and
release the RAM port regardless of an unread ARM reply. Inactive uploads may wait
across quanta but cannot prevent a due quantum from starting. An unread result,
snapshot, trace, held bus response or pending upload never holds an execution gate.

Enforce 4,096 command cycles/group, 8,192 snapshot cycles/128 rows and 60,000 total
cycles at 100 MHz/Q32/48 kHz, including SP/JP and required memory service. Define
and test the admitted graph and DDR/cache service envelope; arbitrary DDR stalls
have no finite successful-render bound. Preserve real watchdog/fault behavior
outside that envelope. Measure routed setup/hold and net whole-design resources
against the approved plan; storage arithmetic and controller size are not proof.

Runtime recovery first obtains the actual safe drain/fence, then flushes fills,
queued/executing packets, replies, observations, snapshots and pending switches.
Always-on recovery/MMIO bookkeeping and first-fault records survive runtime reset.
Advance incarnation without wrap before storage reuse. Epoch-qualified publication
and ACKs cannot cross that boundary. Lost replies/disconnects never authorize
replay, premature freeing, forced task deletion or restoration from snapshots.

Required integrated tests include every packet-word stall, unread/full streams,
held AXI channels and inactive uploads, FIFO cutoff/deadline order, 16/32-loop
skip-0/1 arming, mixed/repeated targets, all retained controls, source changes over
128 slots, delayed real fences, static-composite refusal/continuity, reset/ABA and
worst-case cycle/resource assertions. Physical work remains separately gated.
