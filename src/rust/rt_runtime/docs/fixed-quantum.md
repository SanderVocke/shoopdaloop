# Fixed execution core

`rt_runtime::fixed` is the replacement execution foundation. It coexists with the
interval executor while application parity is completed. It is not selected by
the current application hosts yet.

Prepare `fixed::Config` with a validated `Quantum` and an independent
`max_host_frames`. Its resource configuration uses Q-sized node audio buffers.
Compile an ordinary `GraphDescription` with `Runtime::prepare`: node identities,
ports, topological order and state bindings are validated outside processing;
routes retain direct prepared slot indices. Graphs stay immutable for this
runtime's lifetime. Safe live graph publication is a remaining milestone.

Implement `fixed::State` for the render-owned state and `fixed::Processor<State>`
for nodes. State changes precede each complete graph tick. State advancement and
capture commits follow it. There is no decision-slot lookup or processor
observation acknowledgement. Implementations have the same no-heap/no-lock/no-I/O
RT obligations as ordinary processors; construction and destruction are off-render.

`Runtime::process` accepts complete multiples of Q without adapter latency. Its
resource and state callback hooks execute once per host callback. Invalid blocks
do not advance time. An execution failure clears the full callback output and
latches a permanent fault. A separate bounded MIDI reset sink remains available
through `drain_midi_recovery`; ordinary output never resumes after a fault.

For arbitrary partitions, prepare `BufferedHost` before the first callback. It
keeps one partially collected input tick and the preceding output tick, shifting
both audio and MIDI by exactly Q stream frames. This latency is constant even
when individual host callbacks happen to divide Q. It reports host time separately
from the core's processed tick time. No partial tick enters the graph. This audio
stream delay is not a guarantee of Q/sample-rate seconds of worker execution time.

`command_channel` prepares an SPSC producer and render-owned command queue.
Commands and results are Copy values, so terminal publication cannot drop an
owning payload on RT. Admission credits remain occupied through result consumption.
An independent cancellation credit works while the normal pool is full. Ingress
is snapshotted once per host callback, including callbacks that only stage input.
Future commands do not block earlier deadlines. Effective frames round up to Q;
overdue commands use the next eligible tick. Equal-tick handlers execute in
submission order. Receipts preserve requested, effective and actual times.

Cancellation precedes tick delivery. It returns the target's terminal Cancelled
receipt and a receipt for the cancellation. A request already delivered or
cancelled is refused; tickets from another queue cannot address this queue.
The handler owns domain-specific precedence, grouping and atomic preflight.

After callbacks have stopped, `CommandQueue::shutdown` borrows both endpoints,
excluding concurrent submissions. It preserves unread receipts and emits one
Disconnected receipt for every remaining normal or cancellation request, including
unadmitted work beyond the ingress budget. New submissions and dispatch are then
refused. Supply the final aligned core boundary, including the start of a failed
partial tick. Shutdown is idempotent and does not replay work.

Verification is in `tests/fixed.rs`: all supported quanta, an independently checked
DAG with reversed logical identifiers, exact MIDI offsets, arbitrary single-frame
and irregular partitions, heap guards, unread-result pressure, cancellation,
timestamp overflow, terminal failures and callback-wide resource budgets.
