# Live topology/program activation — remaining Stage 6 design work

**Live activation design; no live-activation API is implemented yet.**
The coordinated facade, first integrated inter-loop reference scenario, flat
compiled metadata and bounded staging/validation/recycling are verified. They do
not expose live facade topology/program/graph replacement. The full Stage 6 contract and later
receiver/host requirements remain unchanged.

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
- The facade currently installs one validated initial graph and intentionally
  hides generic live mutation. Generic graph validation alone cannot establish
  loop role/resource/program invariants.
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
Rust layouts. The next integration must associate ready identities with reliable
facade operation outcomes and graph/topology epochs, and reserve compatible
retirement/recycling storage before any activation.

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
