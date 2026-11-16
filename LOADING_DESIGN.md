# Live loading of state processor programs

## Purpose and scope

This document specifies the agreed loading design for the Rust FPGA reference,
Marlin RTL simulation, and a future ARM/FPGA implementation. It describes the
intended architecture; live loading is not implemented by this document.

The priorities are simplicity, low hardware resource usage, and safe switching
when the compiler and ARM-side loader are correctly implemented. The execution
side receives a compiled state processor (SP) program. Graph construction,
allocation, binding, validation, and lifetime bookkeeping remain on the control
side, called **ARM** below even when running the Rust reference on a host CPU.

The design has:

- Two instruction-memory regions, one current and one available for preparation.
- One persistent pool of processor states, shared across successive programs.
- ARM-side allocation and target lowering to concrete addresses.
- Direct writes to instruction memory and unused state slots.
- A small ordered program-switch command and ownership-handoff completion.

There is no FPGA configuration bank, graph interpreter, image unpacker, allocator,
relocation engine, or graph-level reference counter. Sync and mode-transition
processing remain instructions executed by the SP.

## Trust boundary and safety guarantees

The compiler and loader are trusted to generate a valid executable and obey the
ownership protocol. ARM is responsible for:

- Correct instruction encoding, operand types, address ranges, and program counts.
- Complete state references, including persistent resources reached indirectly.
- Sync ordering and supported dependency relationships, including cycle rules.
- Compatible state layouts when carrying an instance into the next program.
- Available instruction, state, scratch, history, and job-queue capacity.
- A processing budget suitable for the configured quantum and implementation.
- Writing only the inactive program region and ARM-owned state slots.
- Completing initialization and memory visibility before publishing a switch.
- Holding allocations until execution has relinquished all access to them.

The FPGA need not independently prove these properties. In particular, it need
not decode a program to discover its state usage, maintain per-slot ownership
bits, validate graph topology, or check a state generation on every instruction.
ARM maintains the complete ownership record. This is a correctness contract with
trusted firmware, not hardware isolation against arbitrary ARM memory writes.

Execution hardware must still implement the mechanisms on which that contract
depends: correct RAM-port arbitration, atomic selection of a program and its run
parameters, ordered command processing, and a truthful completion/fence signal.
Trusting the compiler does not permit an early ownership-handoff acknowledgement.
Existing datapath fault handling can remain; the loading design requires no
additional semantic-validation machinery in the FPGA.

## Ownership of processor state

The execution side has exclusive mutable ownership of every state used by the
current program. “Execution” includes SP, job processor (JP), and memory/cache
work that can still access those resources. ARM prepares states only while they
are unused by execution and not awaiting an ownership handoff.

Each logical processor instance has an ARM-side identity and a compatible state
layout. Its allocation may cover several typed slots: for example, loop state,
gain-ramp state, media references, and persistent DSP history.

| State category | Allowed loading action |
| --- | --- |
| Used by the current program and retained by the next | Reuse its addresses; emit no initialization writes. |
| New instance for the next program | Reserve unused slots and initialize them on ARM. |
| Used only by the current program | Keep allocated and unavailable to ARM preparation until handoff completes. |
| Reserved for an unpublished next program | ARM may initialize it or discard its reservation after writes have completed. |
| Included in a published switch | Freeze ARM access until the switch outcome and ownership are known. |

The loader API must express this distinction. Only an unused-slot lease permits
initialization. A continuing-instance binding provides addresses for encoding
instructions, with no initialization capability. The low-level memory writer is
kept behind this loader API.

A new image cannot request that an existing active state be initialized, reset,
cloned, or restored from a snapshot. Initial-value recipes apply exclusively to
new instances. Changing a live gain, mode, or other runtime value uses an SP
command. An incompatible state layout requires a new instance and allocation.

Carried state is the actual state at the switching boundary. Cursor advancement,
recording, gain ramps, planned transitions, and DSP history continue evolving
while the next program is prepared. Loading never copies an earlier observation
back over that state.

## Program representation and address binding

The shared compiler can use logical identities and logical resource references
internally. The final **program image** is an ARM-side object with concrete target
addresses, constructed against the current allocation and reserved free space.

Preparation has three steps:

1. Compile the desired graph to logical SP operations and state requirements.
2. Bind continuing instances to their existing allocations and reserve unused
   slots for new instances.
3. Lower through the selected target using that allocation map, encoding final
   addresses directly into the program.

The target-lowering interface must accept this allocation map. It must support
arbitrary retained addresses rather than assume every compilation gets a fresh
contiguous state array. A general-purpose binary relocation pass is unnecessary:
target lowering can encode the selected addresses once.

A bound image contains:

| Contents | Use |
| --- | --- |
| Encoded instructions with concrete operands | ARM writes these directly to the inactive instruction region. |
| New-state initialization writes with unused-slot leases | ARM initializes only these reserved slots. |
| Used-instance/allocation set and control/snapshot bindings | ARM-only ownership, command addressing, and snapshot interpretation. |
| Execution parameters | Small values such as instruction count, sync-prefix count, and job/buffer counts. |
| Expected current revision, next revision, and selected region | Identify and order the intended switch. |
| Target format and state-layout compatibility information | ARM-side compatibility checking before loading. |

The image is valid while its reservations are held and its expected current
revision remains current. A stale bound image must be discarded or rebuilt.
Ordinary runtime state changes do not invalidate it because it contains no copy
of continuing mutable state.

Rust and RTL artifacts retain their separate instruction representations. They
share logical compilation and lifetime rules, not a binary format.

## The program contains graph wiring

Instruction order specifies the sync pass and the audio pass. Instruction
operands specify processor states, sync sources, routing, and buffers. A separate
graph configuration table is unnecessary.

In particular, the sync operation becomes conceptually:

```text
ProcessLoop(loop_state_address, sync_source_address_or_none)
```

Changing a sync relationship changes the next program's operand. It does not
require writing graph wiring into a continuing loop's state during preparation.
The current program keeps executing its own relationship until the switch.

The current RTL encoding has space for a source-slot operand within its 96-bit
instruction width. Binding validates source identity on ARM; lifetime ownership
keeps the referenced allocation valid throughout execution. There is no need to
add a source-generation lookup table to the fast path.

Control bindings also remain on ARM. Commands are lowered to the appropriate
physical targets for the committed program, or access fixed fields of a typed
state slot. Persistent DSP history has its own retained allocation; it must not
accidentally follow temporary audio-buffer numbering between programs.

This separation applies to every existing state format, not just loop sync
sources. Graph-dependent destination-buffer IDs or routing addresses currently
packed into state values must move into program operands when necessary. Mutable
state can retain its intrinsic media/history references, but the loader must
never need to rewrite a continuing state merely because a program's scratch
layout or wiring changed.

The instruction region has a small set of run parameters. Selecting these
register values together with the instruction region is sufficient; these values
do not constitute another bank of graph tables.

## Memory layout and direct loading

Use two fixed instruction regions initially. They can occupy two ranges of one
RAM where capacity permits. Equal-sized regions allow selection by an address
bank bit. Separate physical RAM instances are not required by the design.

ARM writes the inactive region directly through the target's memory-access
interface. It also writes the new states into reserved unused slots. The FPGA
does not receive a serialized image to unpack, relocate, or apply.

```text
ARM constructs a bound program image
    |
    +-- instruction writes ------> inactive instruction region
    +-- new-state writes --------> reserved unused state slots
    +-- run parameters ----------> pending switch descriptor
    |
    +-- SwitchProgram -----------> ordered command FIFO
```

Instruction uploads must leave the current region untouched. Access to shared
state RAM ports is arbitrated so processing takes priority. ARM writes can wait
for an available port or execution phase. Preparation can span many quanta and
has no fixed completion deadline. Keeping an ARM write request asserted must
not prevent the next scheduled processing quantum from starting.

ARM waits for all initialization and upload writes to complete, then establishes
the platform's required memory visibility before publishing the switch. For
mapped or DMA-backed memory this includes the appropriate cache maintenance and
memory barriers. For simulation it includes completion of the corresponding
configuration transactions. No FPGA checksum engine or hardware sealing pass is
required; optional readback is an ARM diagnostic.

## Switch protocol

There is one loader/allocator owner and at most one update in preparation or
awaiting completion. There are no independently queued future graphs.

The command and completion are conceptually:

```text
SwitchProgram {
    expected_current_revision,
    next_revision,
    instruction_region,
    execution_parameters,
}

SwitchComplete {
    revision,
    first_frame,
}
```

The command carries only fixed, small execution values. The used-state set and
initialization plan do not cross in this message. The FPGA does not scan state
membership when accepting it.

The normal command FIFO is the ordering authority. Large instruction/state
uploads use memory writes, so they do not fill that FIFO or delay ordinary stop,
gain, and transition commands. The existing revision check can reject a stale
switch; it does not require a second configuration-validation protocol.

The normal sequence is:

1. ARM reserves and binds new allocations and constructs the bound image.
2. ARM initializes new states and writes the inactive instruction region.
3. ARM completes the writes and establishes memory visibility.
4. ARM publishes `SwitchProgram`. From publication onward, it freezes both the
   new allocations and the uploaded program. Current and next allocations are
   unavailable for further preparation while the outcome is pending.
5. At an eligible quantum boundary, execution selects the new instruction
   region, run parameters, and revision atomically.
6. Execution returns `SwitchComplete` only when old work can no longer access
   resources that ARM may reclaim.
7. ARM updates its current allocation map and releases old-only allocations.
   The old instruction region is now available for the following update.

The pending descriptor must be stable after publication. An implementation may
carry its small values in the FIFO command or capture previously written pending
registers; either way, execution latches them as one configuration.

Initial installation uses the same sequence, starting with revision zero and an
empty current allocation set. Preparation and FIFO delivery never advance the
processing clock. A paused execution engine does not manufacture a processing
boundary merely to acknowledge a switch.

## Boundary and completion semantics

For the initial implementation, quanta execute serially through SP and JP:

```text
old quantum [F-Q, F): SP -> JP -> required memory completion
                                |
                                +-- select the next program
new quantum [F, F+Q):                 SP -> JP
```

SP completion alone is not a switching boundary: JP can still use emitted jobs,
scratch buffers, and media references. Selecting the new program must wait for
completion of the old quantum's logical state changes and relevant JP work.
Required memory writes must be visible to new execution through the memory/cache
subsystem before that execution uses them. An atomic program switch must never
run part of a quantum under each program.

`first_frame = F` identifies the first quantum using the new program. The global
processing frame remains continuous. Carried cursors and media are neither
reinitialized nor advanced an extra time by the switch. Per-quantum pulse flags
and render temporaries are refreshed by normal SP execution.

The ownership-handoff completion is deliberately conservative. It guarantees
that no old work can later dereference old-only state or media allocations.
Queued prefetch hints must be drained or cancelled before issue, and issued
prefetch/cache/DMA operations must be covered by a completion fence. JP accepting
the final job, or a command being submitted to memory, is not sufficient proof.

If harmless speculative work outlives program selection, the new program may
run while that work drains, provided it cannot corrupt new logical state. Delay
`SwitchComplete` until the fence guarantees safe reuse. ARM retains the union
of old and new allocations until then. This needs a completion fence for old
work, not per-graph reference counting or per-state scans in hardware.

The first implementation has one handoff completion rather than separate
activation and retirement protocols. A completion can arrive after `first_frame`.
No third program is prepared while it is outstanding. A memory stall can delay
handoff; it must never be converted into an early success acknowledgement.

## Commands, plans, and snapshots

Execution takes a fixed command-publication cutoff at each eligible quantum
boundary and consumes complete groups in FIFO order. Commands before a switch
use the old revision; commands following the switch must use the new revision.
A stale command is refused even if its addressed loop happens to survive.

Normally the switch occupies its own complete command group. When explicit
state commands must occur at precisely the switch boundary, place those commands
and the switch in one complete group, with the switch last. The whole group uses
the expected old revision. It must be consumed at one boundary with no SP sync
pass between its commands. Existing complete-group publication and consumption
provide this ordering; no additional state-initialization mechanism is needed.
There is no promise of rollback across commands if a runtime fault occurs. ARM
must keep command addressing tied to the correct current or pending binding map
rather than whichever compilation most recently finished.

Future transition plans and pulse countdowns carry over with continuing state.
If changing a sync source requires cancelling those plans, control must issue an
explicit SP clear-plans command before the switch, ordered at the same boundary.
The clear and switch use the complete group described above. Snapshot
observations alone cannot prove that plans will remain empty.
There is no initialization write or implicit state-reset prologue for carried
processors. Other explicit live-state changes use the same command discipline.

Snapshots contain copied values, instance generations, and the executing
configuration revision. Announce the new configuration before publishing its
snapshots; do not combine chunks from different configurations. ARM keeps the
small metadata needed to interpret old and pending snapshots during a switch.
An old snapshot does not pin execution slots or authorize slot reuse.

A snapshot showing the new revision can precede `SwitchComplete` if an old
memory fence is still pending. Observing the new revision is not an ownership
handoff. Reliable completion capacity must be reserved so the completion cannot
be dropped under backpressure.

## Lifetime bookkeeping on ARM

ARM tracks instance allocations and the used-allocation sets for the current
and pending programs. No per-instruction retain/release operations are needed.

After a successful handoff:

```text
continuing = old_used intersection next_used
released   = old_used minus next_used
introduced = next_used minus old_used
```

Continuing allocations remain execution-owned throughout. Introduced allocations
are frozen for execution when the switch is published and become part of the
current program when selected. Released allocations return to ARM only after
`SwitchComplete`. The set calculations and any substantial destruction/reclamation
work run outside real-time execution.

For example:

| Instance | Current allocation | Next allocation | Result |
| --- | --- | --- | --- |
| X | slot 3 | absent | Slot 3 returns to ARM after handoff. |
| Y | slot 8 | slot 8 | One state object continues without copying or initialization. |
| Z | absent | slot 11 | ARM initializes slot 11 before publishing the switch. |

The sets include all persistent state resources, not just loop rows. Temporary
audio scratch can be reused once the old JP use completes. Persistent history
must retain its own allocation identity when scratch-buffer assignments change.

Media ownership remains subject to the media subsystem's existing reference and
fence rules. Removing a loop from execution stops further processing of that
loop after its last old-program quantum; it does not by itself permit freeing
data retained by a media export or other reader. ARM can finalize or reclaim a
removed instance only after obtaining ownership, retaining reader-owned media
as necessary. Independent media readers do not require retaining the old SP
instruction region once all execution accesses to that region have ended.

Capacity admission must allow both instruction regions and the union of old and
new persistent allocations. If that union does not fit, preparation fails while
the current program continues. This design does not evict a live state to make
space for its replacement. Future recording/media-allocation limits continue to
use the media subsystem's normal resource semantics.

## Revisions, reset, and failure

Use one authoritative monotonically increasing program revision for this loading
profile. Each switch gets a new revision; ordinary state commands do not. Existing
command/snapshot graph and topology fields can both carry this revision in the
FPGA profile, avoiding additional independent hardware epoch counters.

Processor instance generations change only when slots are repurposed for new
instances. Reordering or replacing a program does not change the generation of
a continuing state. A content revision remains separate: new recorded contents
do not constitute a newly allocated processor instance.

ARM checks target program-format and state-layout compatibility. Execution uses
physical addresses whose lifetimes have already been established. No hardware
ABI parser or generation lookup is introduced for every memory operand.

A runtime incarnation distinguishes traffic and allocations across reset. Reset
clears or invalidates old command, response, snapshot, and pending-switch state.
ARM establishes a new incarnation before reusing reset execution storage. Neither
revisions nor generations silently wrap and make an old identity current again.

Preparation can be cancelled before publication. ARM must finish or cancel
outstanding upload writes before returning its reservations to the free pool.
Allocation or compilation failure changes no active state or program.

After publication, ARM retains both allocation sets until it has a definitive
switch result and ownership status. A refused switch returns new reservations
to ARM only once execution has definitively rejected access to them. A missing
response never authorizes reclamation. There is no automatic rollback after
activation, retry loop, or destruction of the old allocations based on a timeout.

The loader can reconcile an outstanding operation from retained switch status
or its reliable completion. Such status must distinguish selection of the new
program from completion of the ownership handoff. The FPGA needs at most the
active revision/region and the single pending or last switch result, not a graph
registry or an unbounded operation log.

An application client disconnect does not discard the active program or the
allocator owner's bookkeeping. If the ARM allocator loses that bookkeeping,
it must recover the exact allocations and outstanding handoff, or quiesce/reset
execution before starting a fresh allocator. It must not infer free slots from
an old snapshot. Uncertain memory faults retain ownership until safe recovery.

## Target implementations and first implementation limits

| Target | Preparation and transfer | Execution |
| --- | --- | --- |
| Rust FPGA reference | ARM-side compiler/allocator builds a typed bound artifact and transfers preallocated ownership through a bounded handoff. | Persistent state store plus selection of the current typed program. |
| Marlin RTL simulation | ARM-side code writes the packed program and new states through simulator memory/configuration transactions. | The same switch and ownership-completion semantics as hardware. |
| ARM/FPGA | ARM writes mapped memories directly or uses DMA, then submits the small ordered switch command. | Existing SP/JP datapath with program selection and completion/fence coordination. |

`carrier::Endpoint` remains the transport boundary. Program-memory transfer is
a target-specific memory operation, and lifecycle/runtime messages use the
supported command/response queues and snapshots. No new adapter combines those
interfaces with processing-clock ownership. Processing pacing stays with the
driver and is independent of the controlling client.

Initial live loading keeps quantum size, sample rate, media chunk geometry,
physical I/O configuration, and hardware capacities fixed. Programs may change
their use of those resources within the configured limits. There is one active
program and one update at a time. Crossfades between concurrently executing
graphs, implicit state migration, and compaction of live allocations are outside
this design. State-continuous switching does not promise a click-free change
when the requested routing itself changes the audio waveform discontinuously.

The current Rust implementation must separate program/plan ownership from its
persistent state store. Replacing an entire prepared processor or rerunning its
initializers on retained instances would violate this design. Both targets must
accept externally selected state addresses, and graph-specific sync sources must
move into program operands. RTL configuration arbitration must allow inactive
instruction writes and unused-state preparation without delaying scheduled work.

## Fit with the resource and simplicity goals

The FPGA additions are limited to selecting an instruction region and run
parameters, retaining a small switch descriptor, arbitrating memory access, and
reporting a safe handoff. There is one SP and one JP, with one copy of every
continuing state. No graph-aware loader, allocation search, ownership bitmap,
binding table, state-copy engine, or hardware graph-validation pass is required.

The principal storage costs are space for a second program and headroom for
genuinely new states. A second program region need not consume another RAM block
if both programs fit in existing storage; increasing instruction capacity can
increase BRAM usage. New states and persistent history can dominate the temporary
overlap cost. Exact FPGA usage must be measured after implementation.

Preparation and reclamation can take multiple quanta without adding regular
graph-management passes to SP execution. Carry-over is address reuse, and switching
requires no scan or copy of all live state. Memory arbitration/fence work remains
real hardware integration work; safe handoff cannot be reduced to observing SP
idle or flipping a program pointer early.

This is a good fit for the goals under the stated trust assumption. It moves
variable-sized bookkeeping to ARM, keeps the FPGA mechanism small, and makes
ownership transfer explicit. The tradeoffs are overlap capacity, slower staging
when memory ports are busy, and reliance on a correct compiler/loader rather
than hardware containment of their bugs.

## Validation criteria for implementation

- Switch during playback and recording while retained loops preserve cursor,
  content, gains, plans, counters, and persistent DSP history.
- Reorder instructions and scratch assignments without changing retained state
  addresses or accidentally transferring history to another instance.
- Add and remove processors repeatedly, including reuse of released slots with
  new generations. Verify no initialization write targets a continuing instance.
- Keep processing during slow uploads and under command/response backpressure.
  Freeze all pending-image writes at publication.
- Delay JP, queued hints, cache activity, and DMA. Verify that ownership is never
  returned early, including when snapshots already show the new revision.
- Exercise command cutoffs, old/new revisions, sync-source changes with planned
  transitions, cancelled preparation, refused switches, disconnect, and reset.
- Demonstrate exact Rust/RTL agreement with different physical allocations for
  the same logical program and target-specific instruction representations.
- Measure resource usage and switch/staging timing; verify that execution has
  priority over preparation and that processing performs no allocation or
  substantial destruction work.
