# Fabric control batches, ABI 1

The 746-bit ingress carries complete control batches. This replaces the old
per-quantum fence protocol; old marker opcode zero is rejected. The TP action
ABI remains 3. This transport currently serves prepared continuous/demo/board
producers. It does not yet replace the synchronous application transport or
provide its required retained admission/query replies.

| Bits | Header | Operation |
| --- | --- | --- |
| 745:682 | Earliest logical quantum | Same quantum tag as its header |
| 681:680 | 2 | 0: TP command; 1: state/program write |
| 679:672 | 0xa1, including ABI 1 | TP opcode, or ignored for configuration |
| 671:640 | Number of following operations, 1 through DEPTH−1 | TP index; or state-select bit 31 plus address bits 15:0 |
| 639:0 | Batch identity in low 64 bits; remainder reserved | TP payload; configuration uses low 129 bits |

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

Each 1,088-bit result has the following fields. The shared Rust decoder validates
the record before a transport associates it with an outstanding operation.

| Bits | Field |
| --- | --- |
| 831:0 | TP reply, or zero for configuration |
| 835:832 | TP error, or configuration error 0/1 |
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

The largest valid batch must fit the control/deadline budget of its deployment.
The watchdog is a containment bound, not evidence that a physical deadline is
met. Program/state writes are visible together within one batch; a larger graph
update needs an inactive image/publication mechanism before live application
migration. Device-cut ramp continuity and retained media/query observations also
remain required. No M5 or physical qualification claim follows from this ABI.

Coordinated reset clears queue, partial publication, batch timer and per-quantum
quota together with the processing core. Existing descriptor/bank/epoch and AXI
drain rules are unchanged. The event ROM stops asserting valid after its final
record. Diagnostic stepping is separate from continuous fabric progression.
Legacy direct TP/configuration/sample/media ports are available with audio
disabled for setup and diagnostics. Their live admission/data handshakes are
blocked while enabled; normal control uses the bounded batch path. Finish all
setup operations before arming. Live media/query service requires the autonomous
cut/ownership transport described in the completion plan, which is still open.
