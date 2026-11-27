# Semantic command FIFO contract — hardware ABI 16

This is the frozen implementation contract for Stages 4–5 of
[`FPGA_CMD_FIFO.md`](../../FPGA_CMD_FIFO.md), under its approved scope and ceilings.
**It is not a claim that ABI 16 is implemented or deployed.** The current legacy
mailbox remains ABI 15 until the replacement and matching software pass their
integration gates. Historical ABI-15 staged patches are not this interface.
Indexed AXI-Lite plus low-range discovery/retained diagnostics/public recovery/
ABORT and qualified hardware fault sidebands are implemented as a verified subset,
joined to the actual semantic audio/media candidate. A configuration-only public
CORE/AUDIO/STORE service subset is now implemented. Complete preparation/media/
lifecycle/trace service, external physical audio/watchdog hookup, ABI selection
and matching software remain integration work.

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
invalidates old-source pulse plans when that operand differs from the last executed
source. Frame-absolute plans are not source-bound and remain intact, matching the
reference's `clear_mode_plan` behavior. Use existing record source fields as
execution-owned comparison state; no ARM read/patch, source registry or separate
scheduler is required. This retains
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
After a fresh runtime reset it copies empty revision 0 at F=0, before the first
processing opportunity, once the audio-owned quantum is known. Reset must latch
the new nonzero incarnation for all components; changing epoch alone cannot
release startup. Capture completion, not a reader or ACK, releases processing.
This packet has no graph members. Physical rows can retain arbitrary prior RAM
contents and must be ignored through ARM bindings before interpreting observations;
valid bits in unused rows do not create graph membership or authorize restoration.
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
| 0x174 | `FAULT_SEQUENCE_HIGH`; `FAULT_TAG` at 0x12c is the low word |
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

Bank numbers must fit their required low bits; nonzero upper bits are malformed
register accesses, not silently truncated selections. Rewind/ACK doorbells require
value one. Receipt values 0..5 retain accepted/full/epoch/fault/invalid/unavailable;
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

The indexed component permits one read and one write outstanding independently.
An unavailable/out-of-range packet data read returns zero with receipt 5 and does
not advance its cursor. A successful read records receipt 0. Read receipts after
operation completion and before issuing another operation in that receipt family;
an ACK completion has priority over a same-cycle packet-read status update.
Selecting/rewinding during an accepted packet read does not relabel its copied
word or advance the new cursor. Full-width cursor bounds prevent wrapping into a
header or another bank.

AXI AW and W are captured separately; neither half alone issues a local request.
Runtime reset invalidates their per-half authority, so pairing an old half with a
new half refuses with epoch receipt 2 rather than mutating the new runtime. It
clears private selection/cursors/ACK staging, not accepted bus response ownership.
Already copied RDATA/BRESP remain stable until consumed; abandoned pending local
callbacks complete as epoch refusals, not successful execution/fences. Global
reset alone clears AXI transport state. Local bank/snapshot services have their
existing one-cycle copied-response contract and do not depend on ARM readiness.

Standalone MMIO defaults to independent epoch tracking. Only the actual wrapper
selects static `EPOCH_RESET_COUPLED=1`: its existing always-on recovery controller
latches runtime epoch and changes it only with global or safe runtime reset, so
MMIO need not duplicate that 32-bit tracker. Never use this mode for an independently
changing epoch source. Runtime reset still poisons all old halves/callbacks, while
actual execution/capture retain reset-qualified startup gates and nonzero epoch
latches. A boot-seed change alone neither changes runtime authority nor releases
startup. Copied AXI R/B remain owned by global transport reset, not this refinement.

`semantic_registers.json` generates candidate SV/Rust definitions independently
of the unchanged ABI-15 legacy schema/operations. `fifo_mmio.sv` implements the
0x150–0x32c indexed range plus an optional lower subset; holes remain unmapped.
Generated `fifo_mmio_pipeline_axi.sv` enables that subset on the actual semantic
audio/media/recovery pipeline. Transport uses global reset; local authority uses
actual safe runtime reset/incarnation. Both fault-sequence words and all retained
fields use the qualified always-on record, never a live attempted command.

| Lower subset | Implemented meaning |
| --- | --- |
| 0x000–0x03c | Discovery/geometry, ID `0x53484f50`, VERSION 16; existing 512-job admission cap retained |
| 0x040, 0x044, 0x048–0x074, 0x078 | Request epoch/opcode/twelve payload words/PUSH tag; configuration-only subset below |
| 0x080, 0x084, 0x088–0x0b4, 0x0b8 | Copied response tag/code/twelve data words/POP matching tag |
| 0x104–0x10c | Saturating completed quanta/peak clocks and trusted hardware audio-xrun readback |
| 0x110 | Epoch-qualified ABORT; local terminal halt, not retirement/fence/reset |
| 0x114–0x120 | Recovery ID/status/request and safety submission receipt |
| 0x124–0x14c | Qualified retained epoch/cause/low sequence/op/cycles/detail/revision/F |

A recovery write requires complete full-strobe AW/W authority and data equal to
the current nonzero runtime epoch. It accepts in hardware states 0/3, refuses old
epoch/halves with OKAY/receipt 2, and busy/failed state with OKAY/receipt 5.
Malformed strobes use SLVERR without effects. Receipt 0 means accepted request,
not successful fence or application Ready; state 3 means hardware reset complete.
Query status after uncertain ACK instead of replaying recovery. Accepted safety
receipts survive runtime reset, as do copied R/B; old incomplete halves cannot
request another reset after epoch change. Runtime quantum counters clear without
advancing F and share the existing opportunity timer. IRQ reflects complete
result/snapshot availability or terminal fault, never reader readiness.

The bounded candidate advertises the approved 128 MiB arena; discovery rejects a
zero or unrepresentable u32 extent rather than truncating 4 GiB. This changes no
approved profile capacity. The configuration-only service below owns configuration
inputs privately; complete preparation/media/lifecycle/optional-trace service is
still required, not a completed public endpoint.

### Configuration-service subset

The actual wrapper enables `SERVICE_REGISTERS=1` on the same MMIO owner; standalone
indexed MMIO defaults to 0. One private request/result slot accepts these opcodes:

| Opcode | Existing endpoint | Control mask | Address / data width |
| --- | --- | --- | --- |
| 1 CORE_CONFIG | Program/state/loop and unused-lease observations | `0x103` | u16 address / 129 data bits |
| 2 AUDIO_CONFIG | JP configuration, including scratch and history invalidation | `0x103` | u32 address / 32 data bits |
| 3 STORE_CONFIG | Media metadata configuration, **not bulk media upload** | `0x107` | u32 address / 160 data bits |

Payload word 0 contains low space bits and write bit 8; all other control bits
are zero. Word 1 is address, words 2–6 little-endian data, words 7–11 reserved zero.
CORE's fifth data word has only bit 0; AUDIO's extra four data words are zero.
Validate full opcode, widths and reserved bits before truncating/granting. Actual
endpoint geometry/value checks remain in their existing owners. In the external-
I/O profile audio scratch space 2 is readable/writable; space 3 is write-only
history invalidation, not a history readback/restoration interface. Configuration
remains trusted inactive-program/unused-lease preparation, never an ARM live-state
planner. The Stage 5 trusted API must enforce those ownership restrictions.

Stage epoch at 0x040, full opcode at 0x044 and **all twelve** zero-padded payload
words sequentially, then make writes visible and publish once with a nonzero u32
tag at 0x078. Writing epoch restarts an unpublished fill. Wrong-order/duplicate
fields refuse receipt 7 without changing cursor/data; incomplete PUSH also refuses
7. Wrong staged incarnation refuses 2, zero tag 4. Receipt 0 reserves the private
paired slot and accepts validation, not completed backend effects or a fence.
This sequential staging is compatible with the inspected existing submit ordering;
it does not make that ABI-15 client a matching ABI-16 transport.

A full-width opcode outside this subset produces response code 1 without backend
effects; malformed payload gives 2. Code 0 is successful endpoint completion;
code 3 is backend error or fault/uncertainty, never proof of rollback, cancellation
or ownership retirement. The backend response is captured without ARM readiness,
then copied into private RAM over five clocks. Read twelve immutable response
words; unused/high bits and all error data are zero. Tag/code are immutable until
matching POP. Wrong tag refuses 6, no pending reply 5, terminal halt 3. An unread
reply reserves only this lane. STATUS bit 0 is request space, bit 1 response pending,
bit 2 existing terminal fault; IRQ adds pending service reply independently of
semantic results/snapshots. Requests/polls/POP do not advance F or pace rendering.

One selected RAM read/write and cursor serve fill/validation/copy. AR can defer
while POP verifies its tag on that shared read port. Already accepted RDATA uses
the existing global holding register and survives POP/reuse or safe runtime reset;
only global reset drops it. Per-half authority rejects reset-crossing service
writes. Safe runtime reset flushes private fill/reply/callback ownership without
replay; accepted backend work still requires real external retirement before reset.
Matched software must serialize this lane, check incarnation around publication/
response/POP, cancel stale software callbacks on reset and never replay uncertain
operations or reuse their tags while authority is unresolved.

Only 1/2/3 are implemented at this milestone. Other legitimate preparation/media/
lifecycle/trace operations still belong to the approved final scope; temporary
subset refusals do not retire them. Old CPU live-control/staged-patch operations
must not be imported to supply those missing services.

ABORT requires full-strobe, complete current-epoch AW/W authority and data equal
to that epoch. Neither half alone has effects; wrong epoch/old halves return
OKAY/receipt 2, malformed strobes SLVERR. Receipt 0 accepts a local sticky halt,
not backend retirement, successful fence, reset or application Ready. Explicit
recovery remains separate and waits for actual AXI ownership retirement.

Trusted hardware `audio_fault`/`watchdog_fault` sidebands carry flags 2/32, ABORT
carries 16, and genuine simultaneous generic pipeline faults retain 64. They use
the existing same-edge halt path, not new controllers or ARM source registers.
The shared qualified cause word is zero-compatible with legacy generic-64 callers;
internal/AXI fault qualification is preserved. Physical source/adapter integration
remains open, not established by simulation inputs.

Actual tests cover geometry/telemetry/IRQ, both reset-crossing ABORT/recovery
halves, all lower retained fields, held R/B, coherent 128-row readback, real program
fences, five source combinations and delayed external retirement before reset.
A genuine newer-epoch fault can replace the retained record without relabeling
old copied cause RDATA; epoch advance/reset alone cannot replace it.
Two additional actual tests cover all unpublished configuration prefixes with
continuing audio/nonmutation, all three endpoints/full 160-bit metadata readback,
held R across POP/reuse, and both old service-half orders across actual DDR drain/
reset. A write-only history-read fixture failure is preserved; readable scratch
and explicit history-read refusal remain tested without an RTL workaround.
The [configuration checkpoint](../../docs/fpga/command-fifo-implementation/reports/config-mmio-606ce161b/results.json)
measured 7,990 LUT / 6,943 FF after optimization. Exactly 16 configuration inputs/
32 probe lines moved to MMIO ownership; every other byte/parameter was unchanged.
[Fresh reply-mux sharing evidence](../../docs/fpga/command-fifo-implementation/reports/reply-mux-b21d0925a/results.json)
measures **7,943 LUT / 6,943 FF**, leaving **57 LUT / 57 FF** after optimization;
synthesis is 7,948 LUT / 6,943 FF. Only response-data selection moves into the
existing 160-bit capture/shift cone: no states/cycles/widths/handshakes or same-edge
fault masks change. Distinct raw 129/32/160-bit replies retain exact source/masking
assertions. Public probe bytes and complete profile metadata remain identical to
the configuration checkpoint. The measured saving is 47 LUT / no FF, not a larger
preflight estimate. Unrouted WNS **−12.579 ns synthesis / −12.115 ns optimized** is
effectively unchanged from −12.116 ns at that checkpoint.
The [latest readiness refinement](../../docs/fpga/command-fifo-implementation/reports/config-ready-b1c37e8fd/results.json)
removes requester valid only from two dual-region ready terms. Actual valid-and-
ready grants, processing/local priority, single-region/capture protection and
same-edge fault masks remain unchanged: ready without valid cannot cause effects.
Eight standalone/core × one/two-region × local/DDR variants check grant probes,
129/64-bit RAM nonmutation, no callback/backend/frame activity without valid, then
real accepted payload/response. Profile bytes, metadata and Tcl stay identical.
Frontend remains **7,943 LUT / 6,943 FF**, **57 LUT / 57 FF** margin; three saved
LUT are in excluded execution, not frontend. Worst unrouted WNS is **−10.218 ns
synthesis / −9.754 ns optimized**, improving 2.361 ns, but aggregate TNS/failing
endpoints worsen. The 35-level path now reaches real SP state-RAM write enable;
this is not timing closure, a delayed fault mask or routed/whole-design acceptance.
Complete service/matching software remain unfinished; no ABI-16 deployment,
complete frontend/whole-design or routed-timing acceptance is established.
Production remains ABI 15 and Stages 4–7 remain incomplete.

## Retained first-fault diagnostics

The always-on recorder retains the first event per nonzero incarnation across
runtime reset. Same/older-epoch events cannot replace it; advancing epoch alone
is not an event. A genuine newer-epoch fault may replace it. Global reset clears
the record. Read its epoch before/after the fields to reject replacement races.
Semantic sequence is the full u64 (`FAULT_TAG` low plus `FAULT_SEQUENCE_HIGH`),
not a truncated correlation ID. The ABI-15 mailbox retains its existing u32 tag.
This candidate ABI-16 extension is implemented on the semantic wrapper; deployed
registers/schema remain ABI 15.

Operation is the current decoded semantic kind, 5 for qualified pending-fence
context, or `0xffffffff` when no command is decoded. A header refusal cannot
inherit a previous command's opcode. A delayed fence error or blocked selected
fence-result writer keeps its original group/attempt F rather than borrowing a
competing group's identity. Wrong epoch/sequence callbacks cannot create a fault.
Revision is active execution revision at capture. Without a group, F is the
active audio opportunity's latched F, or the next opportunity when idle, never
SP's advanced next-frame counter while the current opportunity still runs.

Existing cause bits remain: memory 1, audio 2, control timeout 8, ABORT 16,
watchdog 32, pipeline 64, recovery 128. These are flags, not an exclusive enumeration.
Optional trace overflow does not set an audio fault. Accepted recovery captures
`ABORT|RECOVERY` only if no earlier same-epoch event exists. Quiesce/reset-induced
abort events cannot overwrite/relabel the old record as a new-incarnation fault.
Cycle count is saturating active-opportunity clocks (zero when idle); control
cycles are the active group count, zero for fence-owned context. Neither is a
successful-service bound. Detail packs error code [7:0], error stage [9:8],
command phase [10], capture [11], observation/snapshot/revision mismatch [14:12],
and fault reasons [25:18]; other bits are zero. Reasons low-to-high are pipeline,
bank protocol, packet response, local RAM response, kernel uncertain write,
command watchdog, qualified fence context, and blocked result write.

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

Bulk media service obeys the same ownership rule: commit a complete private upload
chunk before acquiring the shared store, and reserve/copy the complete read chunk
before exposing it to an ARM consumer. The current staging component uses at most
32 words; larger preparation transfers require state-independent chunk lowering
outside semantic groups, not a reduced public transfer capability. Route streams
and completions by the accepted operation owner, not current processing-busy state.
Latch fence epoch/sequence when its actual store request is accepted and return
that stored identity after real cache/DMA completion; do not echo live pending
identities or infer completion from selection, SP idle or snapshot revision.

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

The semantic AXI component now exposes terminal master quiesce: stop new internal
burst commands, cancel unpresented headers, preserve active AW/W/AR and drain real
R/B without upstream consumer readiness. Suppress callbacks to the abandoned
runtime; never pad missing write payload or report this as its successful fence.
Unsafe response accounting permanently forbids quiescence. Generated
`fifo_recovery_pipeline_axi` keeps recovery outside runtime reset, requires actual
AXI quiescence/zero credits/two quiet clocks, then advances epoch and resets clients.
A held old-epoch request cannot replay; B/R during reset or late completion after
timeout cannot yield Ready. Hardware reset-complete is not application/backend
Ready. Raw RAM survives only as nonmembers under the fresh initial revision 0.
The shared first-fault recorder now also stays outside runtime reset and retains
qualified semantic identity through recovery. ABI-16 MMIO/ARM selection remains
integration work; the legacy deployed interface is still 15. This implemented
subset does not replace remaining lifecycle, service-envelope or resource/timing
acceptance tests.

Required integrated tests include every packet-word stall, unread/full streams,
held AXI channels and inactive uploads, FIFO cutoff/deadline order, 16/32-loop
skip-0/1 arming, mixed/repeated targets, all retained controls, source changes over
128 slots, delayed real fences, static-composite refusal/continuity, reset/ABA and
worst-case cycle/resource assertions. Physical work remains separately gated.
