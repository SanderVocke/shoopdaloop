# Standalone graph processing

`rt_runtime::processing` exposes a standalone graph composition API for experiments
and benchmarks. Production hosts use `executor::GraphRuntime`, which preserves
the owning endpoint, graph transactions and planner integration. Both process
whole quanta; there is no adaptive graph execution option.

Prepare `processing::Config` with a validated `Quantum` and an independent
`max_host_frames`. Its resource configuration uses Q-sized node audio buffers.
Compile an ordinary `GraphDescription` with `Runtime::prepare`: node identities,
ports, topological order and state bindings are validated outside processing;
routes retain direct prepared slot indices. The processor catalog stays fixed;
prepared routing may change without replacing processors or their storage.

Implement `processing::State` for the render-owned state and `processing::Processor<State>`
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

## Live routing

Call `Runtime::prepare_graph_changes` before rendering to obtain an independent
single-credit `GraphControl`. Its compiler owns immutable catalog metadata and
builds `PreparedGraph`s outside rendering. Candidates may reorder, deactivate or
reactivate prepared nodes; processor state and Q-sized working buffers remain in
their stable slots. New processor catalogs require re-preparation.

Submit a candidate with a requested frame and expected graph epoch. Ingress is
snapshotted once per callback; publication occurs before loop state settlement
and graph execution at an aligned tick. The state must implement bounded
`validate_activation`; the default refuses live publication. The loop bank
requires both consumers of every bound loop. Foreign catalogs, stale epochs and
state refusals preserve the active graph and produce terminal receipts.

Successful receipts own the retired routing. Failed receipts own the candidate.
The one admission credit stays occupied until the receipt is consumed. Dropping
or recompiling that storage belongs to the control side. Routing publication
neither compiles nor allocates/frees on render. `Block::graph_epoch` exposes the
published epoch to processors, independently of host callback partitions.

After callbacks stop, call `shutdown_graph_changes` with the matching control
endpoint before extracting runtime parts. Pending/unadmitted candidates return
Disconnected receipts; unread terminal results survive. This also works after a
partial-tick fault. This route transport does not yet coordinate logical owner,
media and routing changes as one engine configuration transaction.

## Typed processor operations

`Processor<State, Operation>` and `Runtime<Processor, State, Operation>` accept a
copy-valued processor operation family, defaulting to `()`. Prepare its bounded
`OperationControl` before callbacks. Targets bind to physical slots at submission;
operations reach inactive prepared processors as well as active graph members.
Normal ingress is snapshotted once per callback and shares the configured command
budget with `State::begin_callback_with_budget`. The loop bank obeys the remaining
allowance. A per-channel ingress ceiling can reserve room for domain work.

Tick ordering is typed node operations, routing publication, state settlement,
graph execution, then state advancement. This allows a processor to retain a
configuration for the next epoch before a same-tick route publication. Operation
failures produce terminal command errors without faulting ordinary rendering;
processors remain responsible for atomic parameter preflight. Coordinated engine
configuration publication still needs joint preflight across all participants.

`Block::callback_frame` identifies the outer callback even when the graph is called
many times. It lets prepared processors retain a callback-wide allowance in
addition to the shared resource-copy budget. Stop callbacks and use
`shutdown_operations` to settle remaining node requests before extracting owners.

Verification is in `tests/processing.rs`: all supported quanta, an independently checked
DAG with reversed logical identifiers, exact MIDI offsets, arbitrary single-frame
and irregular partitions, heap guards, unread-result pressure, cancellation,
timestamp overflow, terminal failures and callback-wide resource budgets.
