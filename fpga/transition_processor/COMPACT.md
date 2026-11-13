# Compact actions, results and execution control

This contract evolves the indexed installation interface. Public application
requests and completions keep their encodings in the Rust adapter; the wire
representation is private to the service and the RTL.

## Field audit and compact-record checkpoint (ABI 2)

PREPARE word 5 carries the ABI revision. A mismatch returns `invalid_config`
without preparing the device. STATUS bits 144–159 report the supported revision.
Old prepared ROMs must be regenerated. Bus widths remain 640/832 bits; unused
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
| Next/previous pointers | M2 list insertion | Temporarily retained in M2; removed by membership cutover |

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

## Membership publication

The membership representation and its measured choice are recorded at the M3
decision checkpoint. Publication must not write execution-owned countdowns,
outcomes or terminal cells. A partially written inactive view cannot block the
active resolver view. Admission is acknowledged only when a newly installed
action is visible in committed membership, using the actual FPGA admission frame.
Every reference has a generation/lifetime guard, and all walks and version counters
have explicit bounds and checked exhaustion. Terminal entries may remain until
ARM compacts them; no real-time continuation depends on that cleanup.
