# Prepared topology, program and graph activation

The constrained facade paths below are implemented over prepared physical
templates. They do not construct arbitrary processors or resize ports on-render.
The [activation guide](../../src/rust/shoop_loop_engine/docs/topology-activation.md)
is the public usage path; this design preserves the implementation rationale and
chronological evidence. [Stage 6 readiness](STAGE6_READINESS.md) records its completed
consolidated interpreter run at immutable faec78df, not certification of later
dirty documentation/examples or a new full callback audit. No independent loop
host/benchmark or new platform/performance campaign is supplied here. Implementation
acceptance criteria and the governing inter-loop decision are unchanged.

## Implemented publication paths and limits

| Operation | Publication cut and constrained scope |
| --- | --- |
| TopologyActivate | Delivery-time fixed-ID/kind timing; changed programs already stopped, roles/media/routes unchanged |
| ProgramActivate | Owning selected-program/nominal-length transaction at stopped/actual zero; graph, roles and other timing fixed |
| GraphActivate | Immediate fixed-role route/binding change; may preserve a continuing take |
| TopologyGraphActivate | Immediate compatible timing/routes, fixed logical owners; changed programs already stopped |
| ConfigurationActivate | Already-stopped logical generation/media edits with matching timing/routes over prepared physical templates |
| ProgramGraphActivate | Pending selected-program/routes until actual zero/stopped entry; IDs/kinds/sync/triggers/other programs and roles fixed |

Ready validates timing or graph metadata, not all domain invariants; a copied/pending
graph ID acquires no owner or lock. At the actual cut recheck scope/generation,
validated/current/expected epochs, capacities/bindings, pending injected MIDI and
all media/capture preflight before infallible publication. A same-frame queued
Stop has not settled and cannot satisfy an immediate already-stopped precondition.
Role/source, timing, graph and external export owners recycle independently.
Successful RoleAppend consumes source IDs; rejected batches transfer none. Media
OwnerActivation uses currently free metadata, not future retirement credits.

### Storage and boundary prerequisites

- Flat metadata now has canonical LTP1 encoding and maximum-capacity Stager
  storage with scoped ordered chunks, incremental validation and owning reuse.
  See `topology-staging-stage6{,-provenance}.json`. Control compilation remains
  content-sized; receiver-originated owners retain capacity. Compiler scratch and
  all actual storage destruction remain off-render. Timing readiness does not
  establish graph/resource/role-binding activation compatibility.
- `Resolver::replace_topology` returns displaced/rejected owners, validates the
  expected epoch and generation watermarks, and requires removed loops stopped.
  It safely refuses changed running programs. The separate standalone
  `stage_program` transaction now implements a single-program zero/stopped cut;
  the facade integrates it through ProgramActivate. Separate configuration/paired
  graph paths supply their own compatibility checks; this standalone API has no
  graph authority.
- The facade installs one validated initial graph and hides unrestricted generic
  mutation. prepare_with_staging enables timing/program banks;
  prepare_with_configuration_staging additionally provisions role/source banks.
  prepare_graph_staging separately enables generic graph banks before rendering.
  TopologyActivate preserves exact logical IDs/kinds and current role/resource
  bindings; ConfigurationActivate permits stopped logical-owner edits over those
  prepared templates. Each checks authoritative epochs and domain compatibility;
  generic graph validation alone cannot establish replacement-role invariants.
- Planner operations/results are Copy values. Putting an owning compiled topology
  inside an operation, or transferring native Rust pointers between Wasm instances,
  is not an acceptable delivery design.

## Representation and receiver direction

Compiled output now has flat arrays/ranges for nodes, triggers, programs, target
timelines, segments and sync-component membership. Borrowed typed views preserve
indexed lookup and safe lifetimes without self-references, temporary vectors or
retained resource/callback borrows. Program-relative segment ranges preserve
program equality across unrelated global-table relocation. Reusable receiver
storage and its checked lifecycle are implemented; LTP1 encodes explicit
little-endian integer fields rather than transferring internal usize ranges or
Rust layouts. The facade associates ready identities with reliable
operation outcomes and authoritative graph/topology epochs. It prepares a
maximum-capacity initial owner and reserves a retirement slot before activation;
rejected candidates stay Ready and displaced owners await explicit recycling.
The standalone iteration-zero transaction is now integrated with the fixed-graph
facade. Configuration and paired program/graph compatibility are implemented
separately over prepared role templates, not arbitrary processor construction.

Storage must be derived from total declared limits, not an accidental
programs × targets × maximum-segments expansion. Allocation/pre-touch belongs to
preparation. Control-side compilation may allocate; receiver-side staging and
reuse may not. Canonical ordering, sparse/billion-tick programs, valid pure-sync
cycles, rejected action cycles and recursive programs must retain their existing
semantics and independent tests.

Receiver staging now checks scopes/generations, finite bounded ordered chunks,
complete timing references, sparse invariants, recursive/action dependencies and
canonical strong sync components. A partially filled or malformed image is not
readable as a topology. Media-role shapes and runtime graph/resource compatibility
still need activation preflight. Materialization/validation is background work;
no held call waits for another chunk. The adapter must debit one callback's
remaining work allowance rather than refresh it at each internal musical interval;
on validation/append error, conservatively charge the offered allowance because
there is no successful progress tuple.

Preloading only the particular future fixture variants is not a substitute for
a usable bounded staging protocol. Nor may later MessagePort handlers compile or
allocate replacement owners on the worklet thread outside the process guard as
a workaround. The actual receiver/host integration must meet the same prepared
ownership contract.

## Historical fixed-graph integration slice

`runtime::TopologySettings` provisions candidate banks and whole-callback record/
validation allowances; generic contexts now carry graph epoch and callback origin.
Begin/append/validate/status/cancel/activate/recycle use reliable typed operations
and owning scheduled payload returns. Validation is explicit optional preparation;
it does not wait for future ingress or refresh budgets at internal intervals.
Recovery can cancel unpublished candidates without normal request/result credits.

Activation occurs at operation delivery before musical preview. It preserves
exact logical IDs/kinds, signal graph, physical roles, resources and selections;
changed programs must already be stopped. A same-frame queued Stop has not yet
settled and is not enough for that precondition. Post-settlement state/export cuts
carry both epochs. Rejected candidates and retired owners remain held in prepared
storage; stopped cleanup recovers every candidate phase without callback disposal.

See the [public guide](../../src/rust/shoop_loop_engine/docs/topology-activation.md)
and `facade-topology-stage6{,-provenance}.json` for six integrated independent
sample/MIDI/journal/state/capture/export and ownership tests. The broad 25-gate
pass was separate from the interrupted Miri invocation (171 completed of178;
seven uncompleted/unreported). That partial evidence stays partial; later
consolidated Stage 6 completion is linked above. This slice alone did not close
the stage or replace its requirements.

## Atomic boundary requirements

A candidate must carry/recheck expected graph/topology identity, loop generations,
role/view bindings and resource ownership. Failed/stale candidates preserve the
working configuration. There must be one unambiguous activation cut across graph,
resolver/program state and media selection; publishing one component before an
ordinary failure in another is not transactional activation.

The generic prepared-graph capability below now supplies the graph half of this
contract, rather than exposing unchecked facade access to `Control::submit`.
It remains independent of loop modes/program types and preserves NoPlanner.
The loop adapter must still preflight and coordinate its own topology/media/role
changes. Retired graph/topology/node owners must remain in prepared return storage
and ultimately be destroyed off-render.

Changing a logical loop generation must not leave consumers addressing the old
owner, inherit stale request targets, or invalidate external old-media exports.
If physical role slots persist, distinguish their runtime node identity from the
logical loop generation they currently serve. Required MIDI cleanup, input-state
continuity/reset, resource claims and initial media selection need explicit tests,
not an assumption that changing a decision ID is sufficient.

## Implemented generic graph-bank and permit prerequisite

`GraphRuntime::prepare_graph_staging` opts into fixed receiver-owner slots before
rendering. Normal-lane `StageGraph` validates a prepared descriptor incrementally
at an expected epoch, then returns a scoped StageId in its owning completion.
Ready is not active. `ReleaseGraph` returns rejected/unneeded or displaced owners
through existing transport credits; slot generations never wrap.

Planner operation/planning contexts lend `GraphAccess`. Its exclusive permit
rechecks readiness/current and validated epochs, next-epoch availability and
retirement capacity before any domain publication. The domain completes every
fallible preflight first, publishes its prepared state, and consumes the permit
for an infallible graph/epoch swap. Dropping it leaves graph owners unchanged,
not a rollback of arbitrary domain mutation. Actual vector capacities are counted;
checked aggregate stage storage includes one compatible active owner. Returned
plans retain their capacities; this API is not a graph wire materializer.

Review identified another boundary precondition: a planning-time swap must not
remove a node with a delivered but unconsumed MIDI injection. Otherwise the old
buffer could survive until a future graph and replay at the wrong frame. The
permit now refuses Busy before domain mutation; the old graph consumes that
frame. Operation-time activation runs before MIDI delivery under existing
scheduler order, so a later missing target fails InvalidId. A dedicated guarded
case verifies both lifecycle orders and no replay when the node returns.

Seven guarded integration cases, one guarded transport case and one private
exhaustion/sizing case pass. Recovery stress reproduced a held normal validation
request hiding the next callback's recovery lane. One fixed parking slot now
preserves that owner while servicing eligible recovery; resumption retains its
original cancellation observation and FIFO order. Unrelated completions no longer
reset validator progress. One-step staging and legacy validation both finish under
continuous recovery, with publication limits and owning returns intact.

`graph-activation-stage6{,-provenance}.json` records runtime188/loop131 native,
runtime184/loop131 Node/Chromium,workspace2119/four existing skips,25 broad gates,
five new capability compile refusals and refreshed existing lifetime proofs.
Generic native/actual-worklet regressions and canaries pass with unchanged golden
semantics and fixed1,703,936-byte memory. Nine selected Miri executions were added
for consolidated verification, not run separately at this milestone; the later
collected run has its own immutable source/inventory record.

This supplies generic graph publication, not loop policy. The configuration and
paired facade paths add logical/physical identities, old-role MIDI policy,
resource claims, retained exports, media/capture preflight and publication;
see the public guide and configuration/program-graph evidence in VALIDATION.md.

## Implemented unpublished program-cut prerequisite

`Resolver::probe_program_boundary` now shares normal input validation, shadow
initialization, control/clock settlement and cursor advancement, but stops before
the selected composite's child or trigger actions. Its `ProgramBoundary` reports
the old configuration's prospective frame/epoch/state and distinguishes a true
playing entry to zero from an idle cursor still sitting there. No receipt, event,
epoch or committed state is published; probing invalidates an earlier uncommitted
preview and cannot itself be committed. A committed interval remains protected.

This split avoids evaluating the old zero-iteration actions merely to discover an
activation cut. It does not authorize activation from a cached probe or establish
candidate/graph/resource compatibility. An owning transaction must still
recheck inputs and coordinate candidate selection, capture preflight, cancellation,
retirement and one final publication; the standalone slice below now does so. No replacement policy or acceptance scope
has been reduced by adding this prerequisite.

Four guarded tests cover private non-emission assertions,128-frame independent
regular/explicit clocks and full comparisons with non-probed execution across
partitions/orders/journals, eleven same-frame control cases, nested/upstream
capture refusals, stale inputs and phase/epoch handling. A production-type E0502
proof rejects retaining a decision borrow across probing. See the
[API guide](../../src/rust/shoop_loop_engine/docs/program-boundaries.md),
`program-probe-stage6{,-provenance}.json` and the Miri ledger. The19 scoped gates
pass (loop116 per native profile and Node/Chromium,workspace2095/four skips);
new/affected Miri coverage was deferred here and later included in the
consolidated Stage 6 record.

## Implemented standalone owning program transaction

`Resolver::stage_program` retains one candidate with a normal request-family slot
and a reserved displaced-owner location. It accepts exactly one program change
with fixed logical IDs, kinds, sync/trigger metadata and other programs. The
compiled nominal length of that composite may change. Its source/identity checks
and aggregate resident metadata sizing are finite and allocation-free; incoming
owners are already compiled/validated. Busy/rejected admission returns the owner.

At the requested ready boundary, the old unpublished probe decides whether the
program is stopped or entering zero. Selection then uses the new candidate DAG
for the real preview, without the old program's zero actions. Removed active
children are offered Stop before traversal, including when their component moves
before the former parent; ordinary precedence and nested settlement apply.
Natural same-mode continuations are preserved. Authoritative seek/restart/stop
cuts also stop removed declared targets. Empty new programs do not invent playback;
an old explicit program ending remains stopped unless an explicit restart wins.
These clarify cut behavior rather than changing the Stage1 precedence contract.

Configuration effects carry the retained request as their direct/nested origin,
so `Observation::Refused` can reject the whole capture/resource family before
publication. Ordinary autonomous OnPulse attribution is unchanged. The old
working configuration then settles normally, including its legitimate zero
actions. Commit swaps owners, publishes state/receipts/events and increments the
epoch together, with no ordinary post-publication failure. The old owner remains
retained until `take_program_change`; taking only its completion is refused.
Cancellation, input changes and intervening topology replacement cannot publish a
speculative candidate. A separate `Transition::Program` tag identifies the owning
receipt/event; raw direct submission of that tag is invalid.

Nine guarded tests cover scalar positions/cycles/cursors and complete ordered
journals under partition/order/journal variations; capture refusal/retry with no
revision gap or old-action phantom pulse; stalled clocks; Stop/seek/restart and
speculative cancellation; empty/stopped/explicit endings; removed nested children
and direct override; malformed/stale/busy owner returns; and128 one-sample
intervals with all request slots occupied. Native/release and Node/Chromium each
pass125 loop tests; workspace2104/four skips and19 scoped gates pass. Production
E0505/E0502/E0505 proofs protect candidate moves, decision borrows and retired-owner
borrows. Nine Miri cases were queued, not run at this milestone; later consolidated
completion retains its own source/inventory scope.
See `program-change-stage6{,-provenance}.json` and the
[owning API guide](../../src/rust/shoop_loop_engine/docs/program-changes.md).

That milestone was standalone resolver support. The following adapter connects
it to the facade. Subsequent configuration, paired-program/graph and nested
milestones supply the constrained additional paths and Stage 6 verification;
no independent loop host or newer audit/performance evidence is implied.

## Implemented fixed-graph program-bank adapter

`ProgramActivate` transfers a Ready receiver owner into `stage_program`, associating
its StageId/RequestId with the reserved runtime execution token. Admission failure
returns the same owner to Ready. The callback's actual graph epoch is checked on
delivery and rechecked while pending; fixed logical identities/sync/triggers/other
programs preserve all resource and physical-role compatibility. Candidate decisions
pass through the existing real capture preparation/first-interval reservation and
monotonic family-refusal loop. No rollback follows publication.

At commit, coherent queries and `ProgramChanged` expose the configuration cut.
After interval finish, collection returns a rejected/cancelled candidate to Ready
or stores the displaced owner in the slot's retirement bank. It does not wait for
external receipt consumption. A delivered sequence cancellation retains Ready;
recovery `TopologyCancel` additionally requests recycling after finish, reporting
Taken until that safe collection point. Applied changes cannot be cancelled.

A render fault can occur after program publication and before interval finish.
Previously the core collection precondition would strand that owner during stopped
cleanup. The explicit solution is permanent `Resolver::quiesce`, not fake interval
completion or rollback: retain published state/time/results, cancel outstanding
nonterminal requests, invalidate decisions, forbid future execution, and permit
owner collection. The facade invokes it only after consuming the running engine.
Resource cleanup then uses its own actual committed capture prefix. This changes
teardown capability, not musical precedence or acceptance scope.

Five new guarded facade cases cover full media/state/journal references, real
capture versus family refusal, request/owner pressure, cancellation/generations,
stale replacement, disconnect and post-publication fault recovery. A standalone
five-phase quiescence case and E0502 borrow refusal protect terminal extraction.
`facade-program-stage6{,-provenance}.json` records131 loop tests per native/Wasm
profile,2110 workspace passes/four existing skips and19 scoped gates. At that milestone the
197-case inventory was registered, not yet interpreted, and its controller could
not exercise intervening live signal changes. Subsequent live/configuration/paired
scenarios and consolidated Miri have separate records; do not relabel the earlier
registration as a pass.

## Running programs and settlement

Stopped/pending program replacement can activate at its requested ready boundary.
A changed running program must stage until its next iteration-zero boundary.
Determine eligibility and preflight using unpublished state: do not execute old
iteration-zero actions and then retroactively replace them. Same-frame Stop,
seek/restart, source pulses, nested actions and capture refusal must obey the
existing precedence rules. Preserve defined contiguous child continuations and
stop removed descendants at the correct sample.

Pending activation outcomes must distinguish admission, readiness, cancellation,
supersession, stale rejection and actual musical activation. Unread external
results must not stall already admitted automatic music. State cuts must identify
the active topology/program as well as frame and musical cursor. Configuration
readiness is not a reason to add a fixed interaction quantum or arbitrary
propagation quota.

## Preserved acceptance obligations

First preserve compiler/resolver behavior through any representation refactor,
including public lookup behavior, declaration-order canonicalization and owning
returns. Then prove staged validation/reuse and boundary authority under heap
and destructor guards. Integrated tests must cover old/new graphs and media,
iteration-zero running changes, pending cancellation and stale generations,
failed candidates, stop/restart/removal, retained exports, disconnect/recovery,
and full off-render disposal. Native/release/Wasm, independent partition/order
references and selected Miri remain required. No Stage 6 completion follows from
this design note alone; its completed implementation/evidence mapping is in
STAGE6_READINESS.md. Missing independent hosts/newer audits/performance remain
explicit evidence limits, not dependencies of documentation authoring.
