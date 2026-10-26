# Live topology/program activation — remaining Stage 6 design work

**General activation remains design work; a fixed-graph facade path is implemented.**
The coordinated facade, inter-loop reference scenario, flat metadata and receiver
are followed by typed staging and activation with unchanged logical IDs/kinds,
media-role bindings and signal graph. Its broad gates pass; complete stage-level
Miri remains pending under the background/stage-level cadence. The full Stage 6
contract and later host requirements remain unchanged.

## Existing limitations

- Flat metadata now has canonical LTP1 encoding and maximum-capacity Stager
  storage with scoped ordered chunks, incremental validation and owning reuse.
  See `topology-staging-stage6{,-provenance}.json`. Control compilation remains
  content-sized; receiver-originated owners retain capacity. Compiler scratch and
  all actual storage destruction remain off-render. Timing readiness does not
  establish graph/resource/role-binding activation compatibility.
- `Resolver::replace_topology` returns displaced/rejected owners, validates the
  expected epoch and generation watermarks, and requires removed loops stopped.
  It safely refuses changed running programs; it does not implement their required
  iteration-zero activation.
- The facade installs one validated initial graph and hides generic live graph
  mutation. `prepare_with_staging` enables fixed-graph timing changes and stopped
  program replacement. It checks authoritative graph/topology epochs and exact
  logical identities/kinds, preserving all media-role/resource bindings. General
  graph validation alone cannot establish replacement role/resource invariants.
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
Rust layouts. The fixed-graph integration associates ready identities with reliable facade
operation outcomes and authoritative graph/topology epochs. It prepares a
maximum-capacity initial owner and reserves a retirement slot before activation;
rejected candidates stay Ready and displaced owners await explicit recycling.
General graph/role-generation compatibility and iteration-zero activation remain.

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

## Implemented fixed-graph integration slice

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
pass is separate from the interrupted Miri invocation (171 completed of178;
seven uncompleted/unreported). This slice does not close Stage 6 or replace the
requirements below.

## Atomic boundary requirements

A candidate must carry/recheck expected graph/topology identity, loop generations,
role/view bindings and resource ownership. Failed/stale candidates preserve the
working configuration. There must be one unambiguous activation cut across graph,
resolver/program state and media selection; publishing one component before an
ordinary failure in another is not transactional activation.

Investigate an explicit generic prepared-graph activation capability or owning
bundle hook, rather than exposing unchecked facade access to `Control::submit`.
Any generic extension must remain independent of loop modes/program types and
preserve the existing NoPlanner path. Retired graph/topology/node owners must be
retained in prepared return/recycling storage and ultimately destroyed off-render.

Changing a logical loop generation must not leave consumers addressing the old
owner, inherit stale request targets, or invalidate external old-media exports.
If physical role slots persist, distinguish their runtime node identity from the
logical loop generation they currently serve. Required MIDI cleanup, input-state
continuity/reset, resource claims and initial media selection need explicit tests,
not an assumption that changing a decision ID is sufficient.

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
candidate/graph/resource compatibility. The future owning transaction still must
recheck inputs and coordinate candidate selection, capture preflight, cancellation,
retirement and one final publication. No replacement policy or acceptance scope
has been reduced by adding this prerequisite.

Four guarded tests cover private non-emission assertions,128-frame independent
regular/explicit clocks and full comparisons with non-probed execution across
partitions/orders/journals, eleven same-frame control cases, nested/upstream
capture refusals, stale inputs and phase/epoch handling. A production-type E0502
proof rejects retaining a decision borrow across probing. See the
[API guide](../../src/rust/shoop_loop_engine/docs/program-boundaries.md),
`program-probe-stage6{,-provenance}.json` and the Miri ledger. The19 scoped gates
pass (loop116 per native profile and Node/Chromium,workspace2095/four skips);
new/affected Miri coverage remains deferred to the consolidated Stage6 run.

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

## Verification before acceptance

First preserve compiler/resolver behavior through any representation refactor,
including public lookup behavior, declaration-order canonicalization and owning
returns. Then prove staged validation/reuse and boundary authority under heap
and destructor guards. Integrated tests must cover old/new graphs and media,
iteration-zero running changes, pending cancellation and stale generations,
failed candidates, stop/restart/removal, retained exports, disconnect/recovery,
and full off-render disposal. Native/release/Wasm, independent partition/order
references and selected Miri remain required. No Stage 6 completion follows from
this design note alone.
