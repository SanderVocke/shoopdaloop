# Change instructions — chapter 17: graph metadata capacity and work

Destination: the matching chapter under `docs/`. Baseline: empty; archive sources
`performance.md`, `planning.md`, `graph-activation.md`. The initial graph-bank slice
below is retained; the later backfill section supplies the remaining Stage3–6
resource, MIDI, observation and accounting requirements. Publication remains pending.

Add the new graph-bank storage contract:

- `PreparedPlan::storage_bytes()` counts its descriptor object and every vector's
  capacity, not only populated rows. Allocator overhead is excluded. A short plan
  with excessive spare capacity can be refused; row count alone is not sufficient.
- Stage preparation calculates a conservative per-owner allowance from configured
  maximum nodes/edges and the sizes of NodeStep, Connection, Membership and
  DecisionBinding rows, plus PreparedPlan itself. Checked aggregate arithmetic
  covers `count + 1` compatible plan owners and `count` inline slot descriptors,
  refusing totals above i32::MAX. This deliberately overcounts some inline metadata;
  do not present it as a measured exact heap footprint or a portable numeric constant.
- Candidate admission checks the actual retained capacities. Activation also checks
  the displaced active owner, because a legacy path may have supplied oversized
  metadata. Refusal preserves ownership and current output. Staging does not grow
  descriptor capacity; returned owners keep their original capacities.
- Command payloads, compiler temporaries, validator scratch, buffers, resources and
  the rest of runtime preparation have separate costs. Do not imply that this
  graph-bank formula is an entire-application memory budget.

Separate work classes: incremental validation shares the callback-wide allowance;
internal intervals do not refresh it. Activation preflight may scan prepared
node/input-port state for pending MIDI compatibility. The final owner swap is
constant work and does not free the displaced plan. This is finite configured
work, not a universal deadline guarantee.

Recovery preemption adds one fixed inline held-normal slot, not another admission
credit or dynamically allocated owner. Existing operation/publication budgets still
apply; cancelled recovery consumes work and return allowances. Validator progress
must survive unrelated completions. Normal work can still be starved if higher-
priority recovery consumes every configured allowance indefinitely; do not promise
unconditional normal-lane fairness.

Evidence/source: `PreparedPlan::storage_bytes`, `GraphStages::prepare/accepts`,
`GraphRuntime::prepare_graph_staging`, graph capacity/exhaustion tests, and the
new guarded lifecycle preemption test. Use platform-native sizes in worked examples,
check arithmetic against current source, and keep observed benchmark timings and
Miri interpreter durations in evidence rather than turning them into deadlines.

## Stage3–6 capacity/accounting backfill

`../operation-baseline.json` records the still-empty destination, reviewed sources,
archive integrity and fresh checks. Preserve the chapter7 example-led style. Start
with one concrete prepared workload and show its capacity/refusal assertions; move
exhaustive Config/error tables to the planned reference pages. Do not copy historic
benchmark maxima as performance promises.

### Worked accounting and example requirements

- Validate `Config`, then prepare runtime/processor/host storage separately. Config
  checks representability, not available OS memory; actual typed node/planner/queue/
  staging allocations introduce further checked products. Prepared limits cannot
  grow during rendering. `max_nodes` is slot-index space, not automatic installation
  of undeclared processors; `max_ports` sums host and prepared-node signature ports,
  including inactive prepared nodes.
- Normal scheduled action/result capacity is `min(scheduled_events, budgets.events)`.
  A planned runtime adds one independent recovery receipt. Payload storage uses
  `(normal + recovery) × min(max(midi_bytes_per_port, declared operation_bytes),
  budgets.transfer_bytes)`. Larger operation storage does not enlarge allowed MIDI
  packets. Unread/pending results retain credits; a terminal read returns scheduling
  credit through the next callback recycle, not instantly through a new interval.
- Command admission and prepared cancellation-flag availability are different pools.
  Show Full for occupied normal credits, Busy after receiving a completion while
  retaining its ticket/result flag owner, and independently available recovery.
  Chapter18 owns the detailed guarded submission example. Recovery credits are
  finite: an unread recovery result can prevent another recovery request.
- Prepared decision/observation storage includes the fixed typed element sizes.
  Consumers have explicit per-producer observation reservations; unused capacity
  cannot be borrowed by another producer. Planner/internal boundaries consume no
  normal event credit and `Budgets::subranges` is ignored, including zero. Remove
  any inherited claim of subdivision credits, fixed musical quanta or a universal
  bound on arbitrary same-frame musical work.
- Processor audio scratch is ports × max_frames ×4 bytes; MIDI arenas also include
  separately reserved32 reset events/96 bytes. Node inputs have routed and injected
  arenas. Host buffers/recovery sinks are extra. Config's audio_samples/midi_bytes/
  resource_bytes are not allocator/RSS totals or total runtime preparation memory.
- Resource payload is resource_pages × page_bytes. Identity, version and snapshot
  pools each have resource_slots entries. Count simultaneously active, uploaded,
  reserved, old pinned, private-tail and retiring content, not only current media.
  Release does not immediately recover every page; reclamation shares remaining
  validation work. Required RenderIo reads/capture bypass optional byte/step quotas,
  but not physical capacity, shape, timestamp or ownership checks.
- For a legacy stereo128-frame read+write path,1024+1024=2048 bytes of processor
  headroom is needed before optional snapshot copies. With4096-byte pages,
  transfer6144/resource_io_bytes2048 leaves a page for control work. This is an
  illustrative legacy budget, not a required-render throughput cap. Reservations
  are inside totals: leave at least one page of unreserved copy and one unreserved
  validation step. Upload MIDI validation charges bytes; arena capture checks
  charge events. Do not label these different units as identical work.
- MIDI index rows reserve roughly versions × (1+C) × sizeof(MidiState), plus metadata,
  with C×K−1 complete packets per version. Each version's row is private capacity,
  not a global flexible checkpoint quota. Group staging sums max_frames×channels×4
  for audio and12×events+payload_bytes for MIDI; prepared take versions/checkpoint
  rows/final leases and per-interval worst-case pages are additional. MIDI reserves
  the declared producer bound even for short intervals; unused page credit returns
  on commit/cancel. Current group data traverses three counted copies, not zero-copy.

Exercise at least one retained-lease/resource-capacity refusal independently from
background Budget refusal, and assert admitted unrelated output still proceeds.
Use the chapter9/16 examples for those focused proofs rather than making one huge
benchmark serve as the whole book. Full descriptor/vector capacities and the graph-
bank formula above still apply; domain topology/role/media banks have additional
loop-library-specific costs, not hidden generic-runtime guarantees.

### Work, diagnostics and measurement

All admission, recycle, validation, optional I/O, maintenance, recovery and telemetry
allowances share one host-callback envelope. More internal intervals or newly read
results cannot refresh it. Required data operations/index lookup/state copies have
separate saturating counters. Semantic generation/frame/revision exhaustion remains
an error; diagnostic overflow does not refuse rendering. Read live counters from
the render owner, and distinguish lossy telemetry from reliable owning outcomes.

Explain processor calls/route visits as N×R/E×R for N nodes, E edges and R intervals,
plus total sample and MIDI work. Dense stable MIDI insertion may require quadratic
descriptor work; expensive valid programs can miss a deadline. A held normal command
can delay normal FIFO traffic, but **eligible snapshotted recovery can park it**.
Do not copy the archive's obsolete claim that recovery cannot preempt held validation.
This does not promise normal fairness under endlessly consumed recovery budgets.

The existing `bounded_runtime --iterations 1024` is a useful bounded documentation
smoke: seven workloads, samples/events/ownership checks and render heap guards.
It was executed in debug/release here, with allocation/reallocation/deallocation
canaries each producing the expected guard failure in both profiles. The full
native rt_runtime suite passed188 cases/profile and warning-denied rustdoc passed.
This smoke is **not** the full required benchmark campaign, loop benchmark or actual
AudioWorklet evidence. Its capture path intentionally uses legacy quota-based I/O;
do not describe it as measuring new required/group/loop rendering throughput.

Record distributions, setup, guard/wrapper overhead, shared-host contention and
limitations outside processing. Current smoke runs share the machine with immutable
Stage6 Miri, so neither their timing nor Miri wall/simulated time establishes
callback deadlines, CI margin or cold-cache behavior. Guard canaries check heap
operations, not all locks, I/O, host JavaScript or arbitrary third-party DSP.
