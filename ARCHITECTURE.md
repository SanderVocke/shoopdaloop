<a id="architecture-overview"></a>

# ShoopDaLoop architecture and decisions

[Workstreams and acceptance](PLAN.md) · [Verification and evidence](VALIDATION.md) · [Historical rationale](docs/history/README.md)

<a id="purpose-and-scope"></a>
<a id="stack-and-communication-structure"></a>
<a id="other-processing-components"></a>
<a id="implementation-status-and-architectural-continuity"></a>

## Scope and implementation map

This is the maintained architecture/design entry point for all workstreams in
[PLAN.md](PLAN.md). It separates implemented mechanisms, accepted constraints and
proposals. It does not claim production migration, feature parity or final loop
acceptance. The pre-consolidation proposals, contracts, critique and experiments
are preserved in the [historical snapshot](docs/history/README.md); obsolete
snapshot-status paragraphs there do not describe the current tree.

| Layer | Implemented responsibility | Boundary |
| --- | --- | --- |
| `rt_runtime` | Prepared audio/MIDI DAG, one optional planner, typed scheduling/results, resource/index/capture pools, staged graph permits, ownership transport | Generic core; no production/build Shoop, UI, device or browser dependency; no internal host threads |
| `shoop_loops` | One resolver for primitive/composite timing, playback/capture consumers, media ledger and coordinated control/publication | Depends on runtime, not legacy backend; current complete integration is graph-owning facade-first |
| `shoop_example_dsp` | Independent prepared stereo/MIDI processor, typed preset bridge and fixed-delay fixture | No production loop/old-engine/backend/UI/device dependency |
| `shoop_engine_service` | Native composition of the real loop facade and independent DSP in one graph/render owner; typed controls, telemetry and media access | Narrow headless service, not the production application backend or general protocol framework |
| Independent test hosts | Native direct/wire/two-thread and actual non-isolated AudioWorklets for runtime/loop scenarios | Test fixtures, not production adapters or the proposed general service protocol |
| Existing application | Existing backends, devices, sessions, UI, scripting, files and production browser hosting | Not migrated to this stack by these milestones |
| Common modular protocol, service browser/IPC/network deployment | Proposed | Not implemented merely because test-host codecs or Wasm libraries exist |

The runtime [book/reference](src/rust/rt_runtime/docs/README.md), loop
[task guides](src/rust/shoop_loops/README.md), DSP
[README](src/rust/shoop_example_dsp/README.md), service
[README](src/rust/shoop_engine_service/README.md), and host
[READMEs](tests/loops_host/README.md) are the public usage paths. They remain
separate from planning and evidence, and are not rewritten by consolidation.

<a id="generic-rt-runtime"></a>

## Accepted architectural constraints

### Responsibilities and timing authority

The generic runtime owns authoritative engine frames and execution mechanics.
One render-owned resolver owns logical loop modes, positions, synchronization,
composite cursors and request precedence. Playback/capture processors consume
borrowed immutable decisions and return observations; they do not discover timing
by reading mutable peer state or running another musical clock.

Timing relations (sync, state-changing actions and composite containment), signal
routes and generic planner/consumer bindings are distinct structures. Do not use
dummy audio/MIDI edges to impose planning order. One logical loop may have separate
playback and capture nodes with intervening effects:

```text
host input + retained loop playback -> independent DSP -> output + loop capture
```

The current facade owns its `GraphRuntime`, resources, prepared physical roles
and lifecycle. The application still supplies supported routes/effects; this is
not another graph executor or a complete application backend. One runtime has one
planner. Independently planning domains need explicit composition, not installation
of competing clocks or nested engines.

### Callback safety, finite storage and musical cost

Prepare, validate, allocate and pre-touch storage before rendering. Callback paths
must not allocate/reallocate/deallocate, block, lock/wait, perform external I/O,
compile arbitrary owners or chase producer progress. Owning returns, retirement,
refusal and fault quarantine must preserve owners until non-RT disposal. Production
libraries retain safe Rust and `forbid(unsafe_code)`; allocator instrumentation is
dev-only. Arbitrary downstream processors/hosts must honor the same contract.

Keep four things distinct:

- **Prepared storage/admission:** finite nodes, media/pages/indexes, operations,
  results, observations, descriptors and retirement credits.
- **Callback-wide optional work:** bounded ingress, upload/export, validation,
  reclamation, telemetry and recovery snapshots.
- **Required work:** admitted rendering, indexed data access, lifecycle cuts,
  finite musical transitions and cascades. Optional work budgets cannot truncate it.
- **Diagnostics:** saturating counts with overflow indication, not musical quotas.
  Semantic frames, generations, revisions and sequences use checked exhaustion.

The engine owns correctness and termination; the application owns musical cost.
An expensive valid workload may xrun. Do not impose an interaction quantum,
mandatory block/per-hop delay, wrap/subdivision limit or propagation-wave counter
to hide that cost. Finite storage and safe Rust do not certify arbitrary user
planner termination, browser scheduling or callback deadlines.

### One callback envelope and borrowed authority

`GraphRuntime<P, L>` defaults to `NoPlanner`. `Planner`/`PlannerSpec`,
`DecisionBinding`, `ConsumerAccess` and prepared associated `Copy` types give one
planner decision storage and independently reserved producer observation slots.
Validate generations, binding completeness/order and observation capacity before
activation. Consumers cannot retain decision/context/resource borrows across
planner mutation, finish or graph publication.

For one host callback: validate the block/clock; snapshot admission/recycling and
background/recovery allowances once; deliver due actions and settle a boundary;
select a positive interval up to the external ceiling; render the signal DAG;
finish observations/capture and advance. Repeat internal intervals without
refreshing any callback allowance or polling producers. `callback_start` remains
the enclosing callback origin; `graph_epoch` is context-entry identity.

Intervals are half-open `[F,E)`: changes at F affect sample/event F. Callback-end
settlement belongs to the next nonempty callback. Zero callbacks do nothing.
Finish reports observations/advances positions but does not settle the next
boundary. Final position may equal length with a wrap pending. Public State/Export
cuts are post-settlement, pre-render; `AfterInterval` is conceptual, not a public
Cut phase. Sample-rate changes require stopping and re-preparing; no implicit
resampling exists.

### Admission, scheduling, cancellation and results

Queue admission, owning completion, scheduled delivery, domain terminal outcome
and publication are distinct. Preserve requested, effective, delivered and terminal
frames; pending-domain wait is not transport lateness. Runtime success can contain
a domain refusal. Reserve reliable outcome/return storage before promises; unread
results retain credits but cannot halt admitted autonomous music.

Normal external event capacity is `min(scheduled_events, budgets.events)`, plus
independent declared recovery capacity; legacy `budgets.subranges` does not limit
internal musical intervals. Scheduler order is effective frame, target/category/
subkey, sequence. Planner targets precede processor actions; per processor,
parameters precede MIDI then typed operations. Recovery priority at transport
admission does not rewrite that scheduler order.

Eligible callback-snapshotted recovery can park one held normal owner inline,
preserving validation progress, original dequeue/cancellation observation and FIFO
resumption before later normal work. It does not create another credit/dequeue or
cancellation opportunity. Finite service/publication limits and cancelled-recovery
costs still apply; continuous recovery need not guarantee normal fairness.

Ticket cancellation before dequeue, queued typed cancellation and delivered-domain
cancellation are different cuts. Standalone `RequestId` requires correct caller
routing; the facade cancels by originating runtime `Sequence`. The facade's exact
payload-free recovery-safe set is Cancel, TopologyCancel, RoleCancel,
RoleSourceRelease and nonfuture `Timing::At` Stop. Recovery terminates during
planning at its delivery frame before positive samples. Queries, validation,
exports and activation remain normal work; recovery cannot activate graphs.

<a id="loop-execution-layer"></a>

## Loop semantics

### Primitive operations and media identity

A primitive starts Stopped at position/cycle zero, with zero length if no prepared
content exists. Its checked logical identity is not its runtime physical node ID,
resource revision, media/take revision, graph/topology epoch or stage generation.
Raw IDs have correct-store/caller-routing preconditions; copied IDs acquire no
lease. Scoped access/staging/export handles add their own validation.

| Operation | Winning boundary behavior |
| --- | --- |
| Play | Select valid prepared content/position; already-playing non-retrigger Play is idempotent, without reset/start pulse |
| Retrigger | Restart at a valid position and emit a start edge; reset playback cycle count |
| Stop | Freeze capture at the common prefix if needed, stop media, reset position and cancel older pending target transitions |
| Record | Activate a prepared distinct revision; retain old playable content. Optional `monitor_previous` publishes separate authoritative playback/capture positions |
| Finish/play | Freeze `[start,F)` and make indexed new media playable before sample/event F; no extra silent block |
| Seek | Playing-only valid offset; retain mode/content, cleanup/restore MIDI and apply audio discontinuity policy. No pulse unless retriggering |
| Select prepared content | Validate a complete resident revision, reset/preserve position as requested, retain old leases; stale/refused candidates change nothing |

Zero-length Play yields `EmptyContent` rather than a zero-duration wrap loop.
Empty record-to-play refuses transactionally and leaves capture running; explicit
Stop can finish an empty take. An empty composite is a successful stopped no-op.
Fresh capture grows only through successfully committed duration; no padding/gap
reconstruction or reuse of a previously reached take serial. Repeated Record is
a new take, not in-place replacing/overdub.

Playback wraps at its own length; sync clocks do not stretch media or reset a
running playhead. Start/retrigger resets cycle count; ordinary wrap increments it
once; forwarded pulses do neither. Fresh Record emits a start edge but recording
alone creates no periodic wrap clock. Monitored old playback keeps its own supplied
position and edges while a distinct take captures; exhaustion retains old playback
if monitoring, otherwise stops, and reports an incomplete new prefix.

`Timing::At` uses the separately supplied requested frame. `AfterTicks { source,
skip }` arms at its effective frame and applies on the `(skip+1)`th source edge at
or after it. Source must match the target's prepared sync relation. A valid idle
source leaves work pending; stale/missing identity fails explicitly. No UI timeout,
nominal duration or unsynchronized fallback invents a pulse. Revalidate pending
work on topology changes. Facade Select pins validated media; raw
`Transition::Replace`/`Program` tags are not direct owning facade APIs.

External terminal results are exactly Applied, Superseded, Cancelled or Failed at
an actual frame. Automatic edges/actions use prepared state/journals, not external
receipt slots. An already terminal action cannot be undone by cancellation.

### Coincident decisions, pulses and termination

Desired-state intents do not require intermediate zero-duration audio. Settle
already-eligible decisions, then those made eligible by the resulting clock edge;
choose one complete winning transition per target in each phase. Losing external
contenders receive Superseded. A reactive transition may follow an earlier applied
one at F, but cannot retract its pulse or rewrite an earlier terminal result.
Carry protected earlier precedence into the reactive phase.

Total precedence:

1. Direct external intents, including armed eligible requests: highest accepted sequence.
2. Explicit-mode composite/trigger actions.
3. Regular-composite inherited playback.
4. Natural continuation/wrap.

Within a non-direct class, lower stable source identity (including generation),
then higher compiled action ordinal wins. Duplicate source ordinals are invalid;
canonicalize contiguous same-target/mode continuation without redundant restart.
Results must not depend on declaration or signal traversal order.

A pre-clock winning Stop masks coincident natural/start pulses and forwarding.
A winning restart replaces the old wrap. A Stop made eligible *by* a pulse cannot
retract that already delivered edge. Stopped loops may otherwise relay source
edges without starting media. Edge identity is `(absolute_frame, logical_loop_id)`:
at most one edge per loop/sample, whether start, wrap or forwarding. This is
semantic coalescing, not a CPU cap; requests still arbitrate and MIDI is not coalesced.

Prepared relations distinguish Sync, state-changing OnPulse, composite parent/child
actions, and separate signal/binding edges. Compute strong components of sync/
action relations. A cyclic component is valid only if **all internal edges are
Sync**; reject self-action/instantaneous state-changing cycles and recursive
program containment before activation. This is a constrained language, not a
termination test for arbitrary stateful callbacks.

Collapse pure-sync components to a stable DAG. At F:

1. Take the finite admitted operations/cancellations/prior observations, without resampling producers.
2. Walk components in topological order; upstream incoming actions/edges are now known.
3. Resolve pre-clock intents and surviving natural/start seeds; propagate through Sync with one visited mark/member, respecting Stop masks.
4. Make due requests eligible once per source edge, arbitrate reactive winners and propagate newly seeded members. A higher-priority reactive start may lift a mask; a reactive Stop cannot retract delivery. Idle cyclic waiting cannot create its own first seed.
5. Advance composite tick cursors once per actual edge and stream due actions downstream. Start/seek iteration-zero authority must not also advance on its own start edge.
6. Keep results/state unpublished, preflight the whole boundary, then commit once.

Finite prepared components/edges/program actions/requests and once-only edge
visits establish same-frame termination. Positive intervals advance toward a finite
host end; one-sample loops may process every sample. Preflight retries start from
unchanged committed state and must latch an additional previously unlatched finite
capture/activation failure. No pulse, receipt, revision or generation publishes
twice. There is no wave/retry/subdivision allowance changing musical results.

For an authoritative request family, `Refused` atomically rejects the family.
Autonomous failed capture start/restart uses `CaptureUnavailable`, preserving prior
state/content and not stopping siblings; existing append failure uses
`CaptureExhausted`. Both may coexist after a refused restart falls back to an
exhausted old take. Family refusal takes precedence independently of observation
order. Timing-relevant failure reaches the resolver before dependent samples, not
as retroactive rollback after rendering.

### Prepared composites

Programs have stable targets, positive nominal units, sequential sections with
parallel entries, nonnegative delays and positive durations/repetitions. Check all
arithmetic, descriptor capacities and targets. Reject zero-duration entries,
unsupported/mixed implicit-explicit modes, stale identities and recursive nesting.

Regular programs inherit Playing and repeat a finite pass; empty children reserve
time but do not implicitly record. Explicit-mode programs use supported modes and
end after one finite pass; recording still needs prepared storage. Nested iteration-
zero actions execute at the parent's activation frame without propagation delay.
Only actual sync edges advance iteration. A no-sync composite remains at its
selected iteration; nominal units supply derived durations/seek offsets, not a
fallback clock. Derived duration is `max(1, ceil(child_length/nominal_unit))` unless
explicitly overridden.

Start/seek supplies an authoritative desired-state snapshot: stop no-longer-desired
descendants, then arbitrate selected states. Prepared sparse lookup avoids replay
from iteration zero. PlayIteration starts/retriggers; SeekIteration requires Playing
and retains passes unless retriggering. Non-retrigger seek repositions continuing
children without new edges; newly entered playback/recording retains normal start
semantics. Playback offsets derive from elapsed nominal units modulo nonzero length.
Seeking into Record creates a new prepared take at the seek frame, never history;
missing preparation refuses the whole family.

Stopped program changes can activate at the requested boundary. A running change
waits for a true next iteration-zero/stopped entry. Probe the old unpublished cut
without executing old zero actions, then select the new candidate for real preview.
Same-frame Stop/seek/restart and clock/refusal precedence still apply. Preserve
contiguous continuations; stop removed descendants, including after component
reordering. Installing a candidate is not another Play: an ended explicit program
stays stopped absent a winning restart. Recheck identities and preserve displaced/
rejected owners. A cached informational probe alone authorizes no publication.

## Resources, capture and media

### Resource ownership and indexed access

One generic resource store owns prepared page/version/lease pools; the loop ledger
is not another store. Fixed media metadata retains each underlying snapshot lease
once, with separate playback/latest, pending pin and export references. Reserve
currently free metadata before new promises; anticipated retirement is not credit.
Imported resources remain caller-owned. Validate common channel duration, format,
scope and indexed MIDI off-render. Any short-channel audio padding/MIDI trailing
duration must be explicitly materialized by the caller; no implicit render padding.

Uploads are page-bounded contiguous chunks, with Finish distinct from expected-
revision Commit. Validate parser/checkpoint needs before changing the accepted
prefix. Old leases remain immutable/readable through append, replacement/removal
and partial-tail snapshots. Snapshot refusal is not publication. Scoped cursors/
event tokens own no lease; a too-small packet copy retries the same event token,
not an already advanced iterator. Release/cancel/reclamation remain explicit.

Page indexes are maintained as data arrives. MIDI opt-in indexed versions reserve
C checkpoint rows including initial state and replay span K: capacity `C*K-1`
packets; local seek replay `<K`, with page/state cost `O(log C + K log P)` plus
fixed-shape state work. Every packet counts, including opaque SysEx. Select a
checkpoint strictly before target F, reconstruct state before F, and leave all
original F packets for playback. EOF includes final state; snapshot prefixes bound
lookup. Row/page exhaustion refuses atomically, not an unbounded fallback scan.
Stop/freeze/seek/wrap never rebuild whole history. Generational metadata recycles
without clearing/scanning old owners; pages retire incrementally.

### Coordinated capture

Store-owned opt-in groups hold fixed member/staging rows and exclusive writer
claims throughout their managed lifetime. Validate all resources/formats/duplicates
before acquiring any claim. Legacy mutations that bypass the claim must refuse;
read/snapshot access retains its lease/optional-work rules.

Preparing a take reserves all version/index/initial-state/final-snapshot slots
before success; pages remain finite interval reservations, not unlimited recording.
Reserved final leases are unpublished and unavailable to ordinary snapshot callers.
Checked take/transaction identities prevent cancelled tokens retargeting a later
reuse. Cancellation returns promises without changing old active content.

Reserve a new take's **first interval before activation** (`PreparedReserved`).
Validate exact frame/window, audio shape, revision arithmetic and MIDI producer
promise (`12*events + payload_bytes`, including possible recovery) across all
members before changing global reservations. Conservative MIDI bounds may refuse
earlier than exact future-output knowledge; this is storage admission, not a
musical CPU cap. Activation at F retains its interval token and installs all new
zero-duration versions together.

Consumers stage complete contributions once per member in prepared storage.
Publish nothing while collecting. Only after all are ready does an exclusive
store commit append/index every member and publish the common duration, releasing
unused promises. Over-bound, duplicate or missing staging is an interface/fault
condition, not ordinary post-publication capacity refusal. Staging→fixed stack→
pages is deliberately safe and accounts for three media copies including input→
staging; it is not zero-copy. `free_pages` includes promises; immediate availability
subtracts `reserved_pages`.

Freeze requires the exact common next frame and no in-flight interval. It publishes
reserved immutable leases for `[start,F)` without new acquisition/history build.
Generic empty freeze is possible; empty loop record-to-play has its separate refusal.
Failure preserves a truthful common incomplete prefix and changes timing before
crossing affected samples. `PlannerResourceIo`/`CaptureBoundaryIo` lend stamped
frame/ceiling lifecycle authority; consumers get restricted `RenderIo` staging/
reads, not uploads, arbitrary leases, reclamation or budget resets.

### Audio and MIDI policy

Stored audio is interleaved little-endian f32; host ports are planar. Use prepared
scratch/direct-port adapters, not callback vectors of borrows. Gain is per sample.
Default smoothing is None. `LinearRestart { frames: N }` weights each new gained
sample `(i+1)/N` from the last actual output, continues across callbacks and restarts
from that output on another discontinuity. N=0 is None; Stop is immediate silence,
not a release tail. Start/retrigger/seek/wrap/replacement discontinuity is an
authoritative decision, independent of optional journals; losing/refused previews
cannot leak a restart, and unchanged-position explicit seek is still discontinuous.

MIDI stores complete original packets, equal-time order and trailing duration.
Capture start takes strict-preboundary input state; connected input tracking
continues while stopped. Note-on velocity zero tracks as release but keeps bytes.
Cleanup → strict-pretarget projected restoration → original target-time events is
the output order. Prepared state covers notes/velocities, pedal/controller/program/
pitch/pressure, bank/program separation and supported RPN/NRPN selection. Preserve
sostenuto's latched subset and do not restore pedal-released notes as held keys.
Projection is not synth voice/vendor SysEx/parameter-history reconstruction;
untracked histories are reported, not silently certified.

Loop cleanup cannot distinguish unrelated owners on the same physical channel/key;
hosts must route/channelize them. Runtime overflow reserves CC120 then CC123 on all
16 channels (32 events/96 bytes), acknowledges delivery rather than dropping resets,
and retries a full sink without restarting unfinished recovery. Logical processing
and unrelated audio continue on refusal. Lossless comparisons require sufficient
prepared output capacity; unlimited MIDI/recording is not promised.

## Prepared publication and teardown

| Operation | Implemented cut / scope |
| --- | --- |
| TopologyActivate | Delivery-time fixed IDs/kinds; changed programs already stopped; roles/media/routes unchanged |
| ProgramActivate | One owning selected-program/nominal-length change at stopped/true zero; other timing/graph/roles fixed |
| GraphActivate | Immediate compatible fixed-role route/binding change, including continued capture |
| TopologyGraphActivate | Paired compatible immediate timing/routes; logical owners fixed, changed programs stopped |
| ConfigurationActivate | Already-stopped logical generation/media edits with matching topology/routes over prepared physical roles |
| ProgramGraphActivate | Pending selected program/routes at actual zero/stopped entry; IDs/kinds/sync/triggers/other programs and physical roles fixed |

LTP1 encodes flat prepared timing tables, ordered bounded chunks and scoped
receiver-bank lifecycle, not Rust layouts/pointers or a whole control protocol.
Staging validates complete canonical references, sparse metadata, dependency/SCC/
recursion rules and checked aggregate capacities. No partial image is readable;
receiver validation/materialization is callback-wide background work and cannot
hold a call waiting for later input or allocate owners in a MessagePort handler.

Generic `StageGraph` makes a validated owning candidate Ready, not active or locked.
`GraphAccess` lends an exclusive permit that rechecks scoped generation, current/
validated/expected epochs, next epoch, actual vector capacities, retirement space,
bindings and pending MIDI. Complete every fallible domain/capture preflight before
consuming it for infallible publication. An unused permit moves no graph owner and
is not rollback of arbitrary domain mutation. Planning-time removal of a node with
delivered unconsumed MIDI refuses Busy so old graph consumes it once; operation-time
removal precedes injection and a later missing target refuses InvalidId.

Generic structural graph validation cannot establish loop playback/capture pair,
decision binding, input-tracking and media-shape correctness. The facade enforces
these over prepared physical templates, not arbitrary processor construction/port
resizing. A queued same-frame Stop has not settled and cannot satisfy an immediate
already-stopped configuration precondition. Successful RoleAppend consumes source
IDs; rejection transfers none. Role/source, timing/program, graph retirement and
external exports recycle independently; old-generation exports remain valid.

`prepare_with_initial_graph` installs validated routes/bindings off-render at epoch
one/frame zero, with no warm-up callback or ingress receipt. A post-publication
fault may leave changed state/epochs and advanced time. Never replay samples,
fake finish or infer rollback from cleared output. Stopped extraction must retain
the planner ledger (`into_planner_and_resources`); `into_resources` alone drops it.
Permanent quiescence cancels outstanding work, invalidates decisions, prevents
future execution and permits real-owner cleanup after a fault. It is repeatable
cleanup, not resumable Pause or device MIDI cleanup after callbacks have stopped.

<a id="native-engine-service-slice"></a>

## Native service composition

The service uses the current loop facade with an independent DSP in **one graph
and render owner**. The DSP snapshots a finite prepared component-owned request
bridge once per host callback. Its processor changes private prepared state at the
requested frame within the supplied interval and reports actual frame/revision
through reserved reliable returns. No magic float IDs, extra `Engine::process`
calls, second generic scheduler or mutable access to the resolver is involved.

Preset changes are not atomic transactions with arbitrary graph/loop changes.
Peak-since-read atomics are independent telemetry, not a coherent cross-domain
snapshot. Loop cuts are coherent resolver observations. Live exports retain exact
immutable audio/MIDI prefixes while capture continues; owning reads must finish
before release. Stop/cancel recovery must remain available with normal receipts
full. The D=3 probe retains raw physical data and exposes absent dry history, wet
postroll and independent logical playback windows rather than shifting media to
fake latency parity.

The native composition is implemented and its narrower consolidation is delivered.
Service browser/IPC deployment and production UI/backend integration are not.
Measurement regressions and validation limits belong in [VALIDATION.md](VALIDATION.md#integration-evidence),
not architectural claims of performance parity.

## Open composability decision

The current `runtime::Engine<P>` owns `GraphRuntime<Node<P>, LoopPlanner>`.
Although the resolver and low-level renderers are public, the production planner
and role processors lack a complete public standalone preparation/install path.
Applications cannot simply install those production components in their own
runtime without reproducing coordination. One planner per runtime and the facade's
operation vocabulary constrain independently controlled/planning domains.

Graph ownership helps prevent bypasses but is not intrinsically required to own
unrelated DSP/routes. Any alternative must preserve:

1. Required playback/capture consumers, decision bindings and input tracking on every relevant graph activation.
2. Paired program/routing publication at the real cut after full preflight.
3. Coherent logical-generation/media/physical-role reassignment and retained exports.
4. Capture reservation/freeze and failure feedback before affected musical samples.
5. Reliable receipts, bounded owners, cancellation and post-fault reclamation.

The critique recommends a preparable coordinated subsystem in an application-owned
runtime, explicit protected-role/activation contracts, and an optional turnkey
Engine facade. **That is a proposal, not an adopted refactor or existing API.**
It cannot relax the approved musical/media guarantees or be silently implemented
under the loop's remaining verification tasks.

## Compatibility and integration direction

The native slice is deliberately not full legacy parity. Current backend tests,
`shoop_latency`, session/archive/media modules, UI waveform code and scripting
are reference/reuse candidates, not dependencies to pull into the portable core.

| Capability | Intended owner / reuse | Current scope and remaining gap |
| --- | --- | --- |
| Record/play/stop/wrap/seek/select | Existing resolver, ledger and public facade; future session mapping | Included native slice; destructive editing/full legacy modes not established |
| Sync/composites/countdowns | Existing resolver/programs | Implemented loop capability; M1's one-loop scenario is not inter-loop parity proof |
| Replacing/overdub/dry-wet policy | Application/session routing plus required loop/media mechanisms | In-place modes/full policy excluded; no implied additive-overdub contract |
| Prerecord/preplay/latency correction | Reuse checked `shoop_latency` math; explicit retained history/tail/windows needed | Probe findings identify gaps, not a completed mechanism |
| Routes/processors/presets | Runtime validates signal DAG; loop coordinates roles; service/component owns DSP control | Prepared route/preset slice works; real plugin hosting/dynamic construction deferred |
| Device/driver switching | Existing host/device layer adapted later | Not supplied by portable libraries/service |
| State/meters/waveforms | Settled loop cuts, separate peak telemetry, off-render analysis over immutable media | Slice has state/peaks/live exports; full waveform publication/UI freshness deferred |
| MIDI initial state and cleanup | Loop projection/capture plus runtime reset recovery | Supported projection/packets, not arbitrary synthesizer state |
| Media import/replacement | Off-render preparation and explicit leases | Startup imports/retained selection; arbitrary service live adoption deferred |
| Persistence/session replacement | Existing session format/archive/resampling, future service adapter | No new archive format or portability of old engine resource IDs assumed |
| Undo/history | Future application policy over retained immutable revisions | Retained leases alone do not implement general undo |
| Scripting/MIDI rules | Existing off-render app/Lua policy, future service adapter | Not loop planner execution |
| Browser/remote service | Proposed domain/carrier adapters | Test hosts/Wasm compilation are not the full service deployment |

Select an eventual old/new backend at a clear lifecycle boundary; do not silently
route individual unsupported operations into another live engine. Start with a
restricted real UI workflow after a credible composition/protocol slice, keep the
old backend as fallback/reference, and let the parity map determine the next gaps.
The complete session/device/plugin/persistence/undo/scripting/migration architecture
remains undescribed at implementation depth.

<a id="proposed-protocol-architecture"></a>

## Proposed modular protocol

Use common transport-neutral delivery infrastructure with separately selected
domain schemas and adapters, rather than a protocol depending on every DSP crate:

```text
UI / component clients
  -> selected loop / DSP / future application schemas
  -> common framing, routing, correlation, flow control and lifecycle
  -> typed native | browser inter-instance | IPC | network carrier
  -> engine-side domain adapters
  -> local component control APIs
  -> one runtime / graph / render owner
```

The in-memory runtime transport moves compatible typed Rust owners; it is not
serialization between independent heaps. The LTP1 payload and test-host codecs
are useful concrete examples, not the proposed universal framework. The existing
application protocol may evolve; a parallel package hierarchy is not required.

Common infrastructure would define versions/capabilities, bounded envelopes,
connection/request/destination identity, routing, delivery/error categories and
credit/transfer lifecycle. Domain schemas define portable requests/results and
exact timing/completion/failure semantics; adapters validate/decode and retain
mappings to local sequences, stage owners, receipts and leases. Static enums or
routing tables are sufficient; no reflection, arbitrary Rust serialization,
dynamic code loading or plugin discovery is implied.

Keep reliable requests/results, disposable telemetry and chunked bulk transfer
separate in retention/capacity semantics. Transport admission is not musical success.
Reserve return/terminal capacity before admission, use prepared bounded decoding/
materialization and explicit buffer recycling; transferred bytes are not Rust
ownership credits. Carrier-specific backpressure, disconnect, cancellation,
ordering, fragmentation/size limits, security and reconnection identity must be
specified rather than inferred from local SPSC behavior. No render callback may
block on an adapter/client or perform filesystem/network I/O.

Native typed calls may avoid serialization. Actual AudioWorklets require prepared
bounded receive/adapt/render work with independent ordinary memories; heavy
preparation belongs off-render. IPC/network need framing and explicit lifecycle;
network control is not audio streaming or a guarantee of deadline delivery.
Authorization and endpoint policy belong to application composition. Optional
atomic/shared telemetry is not a substitute for reliable results, and arbitrary
cross-domain atomicity requires a local composition contract first.

A future bounded milestone should run **one equivalent admitted musical scenario**
natively, in an actual independent-memory worklet and over local native IPC, with
samples/MIDI/outcomes, throughput/freshness, stalled clients and full ownership
recovery compared. Start with one client/process and defined disconnect; general
resumption, cross-machine deployment, production plugins and full session parity
are separate work requiring approval. No such new deployment is claimed here.

## Planning and execution tools

The reviewed direction is one authoritative Markdown plan, native execution state
and an independent completion auditor. The plan owns outcomes, constraints,
ordered checkboxes and verification/delivery obligations. The executor tracks
human-selected focus, lifecycle, activity and evidence references; a parsed view
is derived, not a second editable objective/task hierarchy.

Checked, skipped, current activity, waiting and verified acceptance are different.
Audit against approved criteria, current source/configuration, project rules,
actual evidence and required CI/review; executor claims are untrusted. Report
satisfied/unsatisfied/unverifiable criteria and reopen invalidated evidence rather
than weakening goals. User-owned focus/pause/resume and exceptional approvals stay
explicit. A new extension is only a reviewed proposal, not implemented by this
migration; `.pi/` storage and native lifecycle remain untouched.
