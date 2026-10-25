# Resolver/runtime integration contract

## Status and dependency boundary

Stage-1 design for `PLAN_LOOP_LAYER.md`. Stage 3 has implemented metadata/compiler/receiver validation, planner views/observations, the internal interval lifecycle, separate external event credits, and saturating diagnostics. Typed planner/processor delivery and retained runtime receipts are also implemented. Loop-domain adaptation and new resource integration remain later-stage contracts, not claims about today's public API. Musical rules are in `SEMANTICS.md`; `rt_runtime` must not depend on that document's mode/program types or on `shoop_loop_engine`.

One `GraphRuntime` owns one prepared planner and all media processors. `NoPlanner` is the default for existing graph users. A planner is a first-class execution dependency, not a processor with fictitious audio/MIDI ports. Multiple independent planners and parallel traversal are outside the first implementation; multiple musical loops/composites share the single resolver.

## Generic API shape and lifetimes

`GraphRuntime<P, L = NoPlanner>`, `Processor<L = NoPlanner>`, and the generic `Planner` contract now implement the decision/observation lifecycle. `PlanContext` and `FinishContext` expose `ResourceIo`, including the new restricted `render()` capability for required data access. Existing direct methods retain shared quotas. Page-indexed byte positioning and opt-in MIDI event/state indexing are implemented; prepared group lifecycle operations remain pending Stage 4 work. `PreparedNode`, `Command`, and `Control` also carry the defaulted planner parameter. The production-trait borrowing experiment is `prototypes/runtime_view_lifetime.rs`; retaining decisions across mutation fails with E0502. The planner provides associated plain-data types `Decision`, `Observation`, `Operation`, and domain terminal `Result`; these are `Copy` and cannot own heap objects. All associated storage/stride arithmetic is checked at preparation, including wasm32 ceilings. Observations have a defined initialized value for pre-touching scratch.

The interface separates these responsibilities:

| Surface | Contract |
| --- | --- |
| Planner preparation metadata | Fixed decision-slot count, planner identity/generation, operation payload limit, and required scratch. Queried before rendering. |
| `Planner::operation` | Deliver a typed operation at an absolute frame with a borrowed payload, reserved receipt token, and restricted resource access. Return immediate terminal result or explicitly retain the receipt as pending. No arbitrary closure or owning return value. |
| `Planner::plan` | Settle/preview the current boundary, preflight resources, and select `at < end <= ceiling`; the ceiling includes the next external due action and host block end. Commit only a successful boundary preview. Complete any now-terminal request through the executor's receipt writer. |
| `Planner::decisions` | Borrow the immutable prepared decision slice for the selected interval. Consumers cannot modify it or outlive that interval. |
| `Planner::finish` | Consume compact observations and advance musical/media state through the rendered interval. It must not separately settle the next boundary or resnapshot external input. |
| `ProcessBlock<L>` | Existing borrowed audio/MIDI/resource fields plus an optional typed decision view and a restricted observation writer. A processor's preparation signature declares whether it requires a decision and how many observations it may produce per interval. |
| Processor operation hook | Receive typed immediate processor operations through the same scheduled-delivery machinery, with declared payload limits. Ordinary parameter/MIDI operations remain available. |
| `NoPlanner` | No domain operations or required decision bindings; ordinary DSP examples continue to use the default type parameters. |

The concrete lifetime experiment is [`prototypes/interval_lifecycle.rs`](prototypes/interval_lifecycle.rs). Its reduced methods are `apply`, `plan`, `decisions`, and `finish`; it deliberately omits real audio/resource/receipt types. It establishes the borrowing pattern before extending production APIs:

1. Mutably borrow the planner for planning.
2. Borrow its decision slice immutably while separately mutably borrowing processor slots, resource store, and observation storage.
3. Hand each consumer only its validated view and observation allowance.
4. End all decision/consumer borrows.
5. Mutably borrow the planner for observation consumption.

No `Arc<Mutex<_>>`, `RefCell` peer access, `Any`/unchecked casts, raw-pointer view publication, seqlock retries, or lifetime extension is needed. Exact production parameter structs and names may be refined with documented compiler/test evidence, but these ownership phases and type boundaries are settled.

### Planner identity and graph descriptors

**Implemented metadata milestone:** `planner::{PlannerSpec, ConsumerAccess, ConsumerSpec, PlanningStep, DecisionBinding, PlanningDescriptor}`, `PlannerId`, `Catalog::with_planner`, and `compile_planned` implement the structural contract. `PreparedPlan` owns planning metadata separately from its existing signal-only `PlanDescriptor`; reusable staging initializes binding capacity before rendering. The single planning step must be a prefix (`before_step == 0`), making backward planning dependencies unrepresentable as valid plans. Bindings are canonicalized by consumer identity, may share decision slots, and are validated incrementally. Aggregate observation reservations count only graph members. Existing signal-only catalogs reject unsolicited planning metadata; their descriptors and host wire format remain unchanged. The executor now constructs planned catalogs, validates candidates before activation, and preserves the prior graph/bindings on refusal. Ordinary `prepare`/`prepare_with_resources` retain the default no-planner path.

Extend the catalog with optional planner metadata separate from processor port signatures. A prepared descriptor contains an explicit planning step and `DecisionBinding { planner, consumer, decision_slot }` records. Consumers remain ordinary signal nodes; each may have at most one decision binding, with one decision able to describe an entire logical loop's channels. A plain FX node need not consume a decision.

The control compiler and incremental render-side validator check:

- Planner/consumer existence, generations, expected graph epoch, and matching prepared signatures.
- Every decision-required consumer has exactly one valid binding; no duplicate or out-of-range bindings.
- Planner execution precedes all bound consumers; no dependency on incidental node-ID/insertion ordering.
- Existing signal DAG ordering, edge validity, and no illegal aliasing remain independently checked.
- Declared observation reservations fit prepared storage; one producer cannot consume another's required observation space.

An observations-back-to-planner relationship is a lifecycle phase, not a signal feedback edge. Playback → FX → capture can use two consumers bound to the same loop decision. Descriptors and bindings are relocatable integers, not native addresses. Private incremental validation finishes before activation; invalid/stale candidates leave the old graph and bindings active.

## Callback versus interval lifecycle

```text
validate buffers/time; zero frames is a no-op
begin host callback once:
  advance/check outer clock, snapshot ingress and recycle returned credits
  service finite external command and background-maintenance allowances
while current frame < host end:
  deliver already admitted external operations due at current frame
  planner previews boundary; preflights resources; commits stable decisions
  validate positive interval end <= next external frame/host end
  run signal DAG with immutable decision views and reserved observation writers
  planner consumes observations and advances through the interval
finish host callback:
  bounded reclamation and state/diagnostic publication
  host drains its callback-wide MIDI recovery allowance
```

The loop is not bounded by `budgets.subranges`. Strict positive frame progress bounds time-advancing iterations by actual frames. Same-frame planner settlement is governed by its application-specific terminating contract, not a runtime propagation counter. A malicious/nonconforming planner can still violate the processor contract; safe Rust does not certify algorithmic termination. Reject returned zero/backward/out-of-range intervals as programming/interface errors, not as an invitation to busy-loop.

External commands arriving after the callback snapshot wait until the next callback. Returning scheduler credits, consuming multiple intervals, or repeatedly borrowing resource views must not replenish command, publication, background-copy/validation, or host-recovery allowances.

The generic runtime reserves enough event/result storage at external admission to execute admitted actions on time. It no longer uses the internal musical interval count as that reservation. At block end, finish observations but leave boundary settlement/due actions to the next nonempty callback. A propagated processor/planner programming error is not rollback: the current runtime's output clearing and monotonic clock/fault reporting remain explicit, and hosts must not replay the same frame.

## Typed delivery, preparation, and owning returns

Extend scheduled actions with typed planner/processor operations and typed plain-data results, defaulted for no-planner users. Keep ordinary parameter/MIDI behavior, source ordering, and reliable owning command returns. A browser codec encodes the typed values; neither library calls `MessagePort` or contains worklet bindings.

A receipt token identifies a generation-checked runtime-owned terminal slot. Delivery may be immediate or pending in the planner (for example, waiting for a sync source). A pending operation is removed from the timestamped due queue but retains its terminal reservation. The planner later completes the token through an executor-owned writer; this reuses scheduler/result transport rather than introducing a second domain command bus. Exactly one terminal outcome is accepted per token. Requested, effective/delivery, and musical completion frames remain distinguishable. Lossy telemetry is never evidence of successful execution.

**Implemented receipt refinement:** `Action<Operation>` distinguishes planner and processor targets. Processor operations are immediate; planner operations return `Disposition::Complete` or retain a runtime-scoped token with `Pending`. `Execution` records requested, optional delivered, and terminal frames separately; `operation_result` carries domain success/refusal/cancellation rather than conflating it with generic delivery status. `ReceiptWriter::cancel_queued` cancels a normal typed request before delivery; delivered pending requests complete their retained token. Slots remain reserved until terminal receipt and callback-boundary recycling. Token scope is instance-local, not a cross-Wasm pointer/capability.

Recovery uses one extra scheduler/result slot, independent of normal event capacity and the owning transport's reserved lane. Only classified payload-free planner recovery operations at or before the current frame are admitted. They must complete immediately or during planning at the delivery frame; advancing with an unresolved recovery receipt is an interface error. Admission sequences are monotonic per lane so priority recovery does not incorrectly reject older normal traffic; the transport still assigns globally unique sequences. These refinements implement the specified independent recovery guarantee without introducing a second command bus or a musical-work quota.

Pending receipt capacity is independent of autonomous wraps. If the consumer stalls, refuse additional external work but continue automatic progression. Recovery admission must support cancellation/stop of pending work without first acquiring another normal scheduled-result slot; permit only explicitly classified recovery-safe operations, not structural plan edits. Existing held normal validation may delay recovery dispatch until its finite self-contained validation finishes; reserved recovery credit does not promise preemption.

Prepared program data uses fixed receiver-side staging slots allocated at startup. Native preparation may allocate its own compiled representation, but live transfer uses typed bounded chunks into those slots, also usable across independent Wasm memories. Completed candidates are incrementally validated before activation. No held command may wait for later upload commands to provide missing bytes. Program slot capacity bounds resident metadata, not musical transition count. Materialization/validation uses background allowances; musical activation occurs only after the candidate is ready.

Payload buffers use existing owning completion/retirement mechanisms and prepared submission pools. Staging metadata contains only prepared plain-data arrays and resource/lease identities; it is not a competing media store. Replaced candidate metadata is made inactive and recycled with bounded maintenance, not freed on the callback. Old graph/program/resource references are invalidated or retained consistently before slots can be reused. Stop/join callbacks before final planner/processor/resource/transport destruction.

## Resource access and group capture

### Work classes

| Class | Accounting and refusal |
| --- | --- |
| Fixed storage/owners | Prepared bounds on pages, versions, snapshots, indexes, program metadata, pending receipts and MIDI arenas; explicit exhaustion |
| External admission | Finite queue snapshot and admitted outstanding event/result capacity; bounded payloads and owning returns |
| Background work | Per-callback command dispatch, uploads/exports, descriptor/program validation, optional publication and incremental reclamation allowances |
| Required rendering | Indexed reads, capture writes/checks, prepared activation/freeze and internal musical transitions complete under declared per-operation/storage bounds; counted, not throttled by background quotas |
| Diagnostics | Saturating counters plus overflow flags where appropriate; never wrap or alter execution because observation counters fill |

Frame, identity, revision, and request-sequence exhaustion remain semantic errors, not saturating identities. Preserve old configuration/API behavior only where it does not impose a hidden musical-work cap; document intentional migration of resource-I/O/subrange fields and update tests explicitly. Background quota zero/exhaustion cannot be treated as a capture-content failure in the new render path.

Render-side access is a restricted capability. Consumers may use prepared read/capture handles and publish observations, but cannot reset accounting, mutate graph structure, allocate leases unexpectedly, or access arbitrary mutable resources. Planner access adds only the prepared lifecycle/reservation operations needed at a boundary. Generic resource methods remain free of loop semantics.

Implemented accounting foundation: `RenderIo` provides sequential read and active single-resource audio/MIDI append using the same validation/storage paths with independent saturating `RenderWork` counters. It never consumes or resets background credits. It cannot create leases or change capture lifecycle. Background reservations and old quota-based APIs remain effective. Append-maintained page indexes and opt-in MIDI checkpoint rows now supply byte/event/state positioning. The coordinated capture contract below is still required.

### Indexed access and recording readiness

Implemented page-index decision: use an append-only balanced search tree embedded in physical page metadata, with a root/count per version, instead of a dense version-by-page table. This keeps total metadata O(prepared pages) and guarantees logarithmic insertion/seek without a whole-resource scan or render-time allocation. Only increasing ordinals are inserted, so right-heavy single rotations suffice. Leases retain the version tree; partial snapshot tails keep their private copied page. Index nodes recycle with pages, not through a separate destruction walk. Checked opaque scopes prevent cross-store index/cursor reuse; legacy raw IDs retain their same-store precondition. Diagnostic metadata-step counters saturate and reset with the host callback. Upload maintenance is bounded overhead of admitted pages, not an extra quota or scan task. This is a representation choice preserving the prepared version-scoped indexing contract, not a change to timing or acceptance criteria. MIDI positioning uses event checkpoints and initial/note/controller state with a declared maximum local replay span. Validate index/lease identity on every access. Index construction occurs with uploads or incrementally with capture; freezing a recording must not trigger a whole-content scan. Snapshot leases retain both content and the index/initial-state metadata needed to interpret that revision.

The fixed `midi_state::MidiState` projection is now implemented as a foundation for initial/checkpoint metadata. It retains channel/key/controller/program/pitch/pressure state, held versus pedal-latched keys, and reset baseline markers without allocation or clock authority. Its protocol-state policy is documented in the runtime's MIDI-state guide; packets remain distinct, and opaque device/SysEx effects are not synthesizer snapshots. Opt-in fixed checkpoint rows now reserve initial/checkpoint state per indexed version, build during upload/capture, and support bounded local replay with scoped event/packet access. Restoration emission remains separate renderer work; it is not implied by the state value or seek API.

The remaining concrete pool, token, staging, and lifecycle design is recorded in [CAPTURE_TRANSACTIONS_DESIGN.md](CAPTURE_TRANSACTIONS_DESIGN.md); those APIs are not yet implemented. A capture group owns exclusive writer claims on its resource identities while managed. External remove/replace/start-capture operations that bypass the group must fail explicitly rather than invalidate one channel mid-take. Old read leases remain immutable. Beginning a take reserves version/lifecycle/initial-state/index/final-playback-lease capacity; data capacity is finite and its exhaustion remains explicit. Freeze makes the common captured prefix immediately readable without asynchronous control-side construction.

Before rendering an interval, reserve all required audio and MIDI append resources for the group. Known audio bytes are exact; downstream MIDI needs a safe bound derived from prepared producer/event/byte capacities, or a staged strategy with an equally explicit observation boundary. Initially use conservative per-interval MIDI reservation and return unused credit after the group commits. It may refuse earlier than an exact-content reservation; document that storage consequence rather than pretending it is a CPU budget. Never append one channel and only then discover that another cannot be reserved.

Boundary planning uses a prepared shadow and provisional reservations. If preflight fails, latch the failing group/activation, release provisional reservations without changing published IDs/generations/state, and re-preview from the same committed state and input batch. Every retry adds a previously unlatched failure; at most the finite prepared group/activation set can fail. No arbitrary retry limit is used. Successful preview commits once, including request results and clock occurrences; diagnostic/test journals of musical events contain committed decisions only.

A failure starting a new take rejects that transition and preserves prior playback. Exhaustion of an already-running take stops capture at the interval start with the previous common complete prefix and an incomplete reason; explicitly monitored previous content keeps playing, otherwise the affected loop enters Stopped. If changed state affects other loop/composite decisions at that frame, re-preview them before any dependent samples render. Unexpected failure after a successfully reserved append is an invariant/programming fault, not a normal retroactive capacity observation.

### Failure/observation table

| Situation | Boundary and committed state | Outcome and unrelated processing |
| --- | --- | --- |
| No staging/receipt/required preparation capacity | Before accepting a promise | Explicit refusal; caller retains ownership; active program/music unchanged |
| Stale graph/program/resource generation | Validation and recheck at activation | Reject candidate/transition, retain prior active configuration and leases; report reliable failure |
| Begin-capture group preparation invalid | Requested boundary, before commit | Reject complete group, preserve old playable take/mode; no channel begins alone |
| Active capture append reservation fails | Start of proposed interval F | All channels freeze at existing common prefix ending F, incomplete reason recorded; resolver re-previews dependent decisions before sample F |
| Freeze then play | Exact requested boundary F | Freeze `[start,F)`, activate reserved playback lease/index before F, complete external transition once; no silent block |
| Reserved append succeeds with less MIDI than reserved | End of interval | Commit one common duration/prefix; return unused credits; no change to already-rendered timing |
| Reserved append violates its preconditions | Detected during interval | Programming/interface fault with truthful runtime error; no claim of rollback or successful complete take |
| Playback/content positioning invalid | Before content activation or interval use | Reject invalid prepared activation and retain old playable content where possible; stale externally corrupted handles are explicit faults |
| Ordinary MIDI writer full | Actual write boundary | Report/reset through existing recovery protocol; maintain logical progress and unrelated audio; no lossless-output claim under insufficient event storage |
| Optional snapshot/export/telemetry capacity full | Requested cut/publication point | Refuse/coalesce according to channel contract; recording/playback/trigger progression continues |
| Consumer disconnected or results unread | Callback admission/publication | Retain bounded owners/terminal credits; refuse additional admission as needed; existing musical work continues until host stops |
| Semantic counter exhaustion | Before the mutation that would wrap | Explicit exhaustion and defined stopped/faulted affected state; no ABA or invented timestamp |

## State publication and browser integration

A coherent state cut combines resolver mode/position/program identity with media revision/capture outcomes after the appropriate phase. Tag it with absolute frame and cut phase. Use prepared state buffers/leases or fixed plain-data snapshots; chunks of a larger state snapshot share one cut/version. Readers cannot observe independently sampled fields as one coherent state. Snapshot ownership and request acknowledgements use runtime mechanisms; the UI never locks the resolver.

Control-side program/graph/media preparation may run on a main thread or Worker. The authoritative resolver, renderers, resource store, and executor run together under the worklet owner. `tests/loop_engine_host` owns browser serialization, transferable credits, Wasm instance setup, and lifecycle. Native and browser adapters share loop semantics but not memory addresses. Main-side state is an observation, not a competing clock. Production adapters remain a later milestone.

## Prototype verification and limitations

The standalone prototype verifies immutable decisions across two independently mutable consumers, compact observations consumed after traversal, four equivalent partition/density/order executions, a rejected binding, and observation-storage refusal. It executes 128 internal intervals in one callback without resnapshotting callback state. A compile-fail configuration deliberately retains a decision borrow across planner mutation and must fail with Rust `E0502`.

Commands (repository Nix environment):

```sh
rustfmt --edition 2021 --check docs/loop_layer/prototypes/interval_lifecycle.rs
rustc --edition 2021 -D warnings docs/loop_layer/prototypes/interval_lifecycle.rs -o target/loop-layer-contracts/prototype-debug
target/loop-layer-contracts/prototype-debug
rustc --edition 2021 -D warnings -O docs/loop_layer/prototypes/interval_lifecycle.rs -o target/loop-layer-contracts/prototype-release
target/loop-layer-contracts/prototype-release
rustc --edition 2021 -D warnings --target wasm32-unknown-unknown docs/loop_layer/prototypes/interval_lifecycle.rs -o target/loop-layer-contracts/prototype.wasm
# Expected nonzero exit with E0502, not a missing-file/compiler error:
rustc --edition 2021 --cfg compile_fail_reuse docs/loop_layer/prototypes/interval_lifecycle.rs
```

Create `target/loop-layer-contracts` before running these commands. Successful Wasm compilation here is not Wasm execution or AudioWorklet evidence. The prototype does not implement graph receiver validation, musical semantics, resource transactions, typed receipt transport, or real-time instrumentation; those require the staged implementation and tests, not an extrapolation from this type/lifetime experiment.
