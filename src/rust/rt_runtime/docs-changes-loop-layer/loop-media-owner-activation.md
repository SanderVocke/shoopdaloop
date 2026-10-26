# Change instructions — transactional media-owner generations

Primary destination: `src/rust/shoop_loop_engine/docs/media.md`, read completely
for this slice. Also reconcile the loop README/runtime guide and repository
facade/topology-activation design references during the full handoff review.
The baseline is `owner-activation-baseline.json`. This is a ledger prerequisite,
not a claim that general role-generation activation is exposed by the facade.

## Correct older status statements

The media guide's opening says coordinated capture/state integration is pending.
That description is historical: the coordinated facade shipped in Stage5 and
fixed-role routing/timing publication followed in Stage6. Preserve the distinction
that the **ledger itself** does not own time, stop loops, perform MIDI cleanup,
freeze groups or certify a musical cut. Link the implemented facade rather than
moving its timing responsibilities into this component.

Keep the existing adopt/reserve/publish/reference APIs and their contracts.
`adopt` still requires an already-bound logical owner. The new batch API below is
an explicit exception: its initial Input belongs to a prospective newer owner,
validated and bound only when the whole batch publishes.

## New public surface and ownership

- `OwnerChange::replace(previous: Option<LoopId>, input: Input)` owns initial
  mixed-media input for an added/replaced logical owner. None means the row must
  currently be unbound. A replacement stays within the same logical slot.
- `OwnerChange::remove(previous)` removes that owner's internal roles only.
- `input()` inspects an unapplied input; `into_input()` returns its ownership for
  caller disposal/retry when it has not transferred. Neither clones a lease.
- `is_applied()` distinguishes publication; `published()` identifies the new
  initial media after success, or None for removal. That identity is borrowed;
  successful publication creates a separate resident reference that must eventually
  be retired. Dropping a change descriptor does not implicitly release resource
  leases or residency. Explicit cleanup remains required.
- `Ledger::prepare_owner_activation(&mut ResourceIo, &mut [OwnerChange])` returns
  `OwnerActivation`. It exclusively borrows the ledger, resource access and edits.
  No input, role, free-list credit or generation watermark changes during preflight.
- Dropping an unused permit preserves every input and current owner. Consuming
  `commit()` publishes the complete validated batch with no ordinary failure result.
  The permit's fields are private; callers cannot forge preflight authority.

A successful replacement releases the previous playback/latest references, binds
the newer owner, consumes its initial Input into a ready media slot, and establishes
one resident plus one playback reference. Latest is initially None. Removals publish
no initial media. The descriptor is single-use after publication. An empty batch is
a valid no-op. Preflight failure, including failure in a later edit, leaves earlier
edits unpublished and all inputs retained.

Resident content, pending pins, external exports and unpublished reservations from
an old generation are **not** invalidated or implicitly retired. Their descriptions,
logical revisions, snapshot capture/cut metadata and requested export cuts remain
unchanged. Old-generation content cannot become the new owner's playback merely
because the logical slot number matches. Caller-owned resource handles/groups are
still separately owned. Existing release_all recovers retained orphaned residency
and reservations at quiescent teardown.

## Preconditions and bounded work

Each logical slot occurs once. The expected previous identity must match the bound
row exactly. A supplied new generation strictly exceeds that row's committed
watermark; removal preserves the watermark. Failed/prepared-but-unused candidates
do not consume generations. Generation u32::MAX can activate once, then prevents
any further reuse of that slot rather than wrapping.

All initial sources undergo existing scoped snapshot/index/format/duration/indexed-
MIDI validation. Duplicate lease ownership is rejected both against the ledger's
existing media and between inputs in the same batch. A copied invalid alias returned
on refusal is not a newly acquired ownership reference. Preflight checks every old
role's sources before any role reference can be released. Exclusive resource borrowing
prevents reclaiming/mutating the checked leases before commit.

For L prepared owner rows, M media slots, C edits and N initial Inputs:

- C <= L; storage is supplied by the caller as a bounded slice, with no new ledger
  allocation or staging pool. A future candidate pool must account for its actual
  `OwnerChange` storage/capacity separately.
- At least N **currently free** media slots are required. Do not count metadata that
  old-role retirement might free later. This conservative reservation condition is
  intentional and can refuse even when post-publication occupancy would fit.
- No new export, pin or snapshot lease is acquired at commit. Input snapshots are
  already owned and transfer on success; old references release once as usual.
- Preflight scans prepared tables and earlier edits: O(N*M + C²), plus bounded
  source/index checks. MIDI validation seeks zero, not through content history.
  Commit performs finite per-edit role/reference changes; underlying resource
  reclamation remains deferred/budgeted. No universal deadline claim follows.
- Resource diagnostics may advance during failed validation; semantic ownership and
  generation state do not. A later caller failure after preparing an unused permit
  can drop it, not roll back an already committed domain operation.

## Integrator ordering and remaining work

The integrator must separately validate stopped/compatible resolver state, logical
and physical role mappings, initial shape/revision/length, input projection policy,
MIDI cleanup and capture-resource claims. These are not inferred from ledger IDs.
Finish other fallible preflight, hold compatible publication authorities, perform
any final fallible resolver publication, then consume the preflighted owner/graph
permits without adding an ordinary failure after publication. This API does not
supply a wire materializer, resource uploader, or authorization to move Rust pointers
between independent Wasm instances. General facade integration remains pending.

## Executable evidence and documentation example requirements

`docs/loop_layer/prototypes/media_owner_activation_lifetime.rs` is an executable
production-type positive example. Five expected compile failures prove that a held
permit blocks ledger access, mutable resource access, input inspection and owning
input extraction, and that private fields prevent forgery. It is a lifetime/authority
proof, not a musical or AudioWorklet demonstration.

Three guarded tests in `tests/media.rs` add:

1. Two-owner mixed-media publication with all export/pin credits occupied; old
   playback/latest references, old immutable audio/MIDI/cuts, unpublished reservations,
   exact initial selection and complete pool recovery.
2. Later-input shape failures, stale/missing/wrong-slot owners, duplicate edits and
   cross-input duplicate leases, unused-permit retry and conservative metadata refusal.
3. A512-edit independent reference model (seed0x38d1_e45f_0bc2_9a67), eight action
   classes64 times each, predicting every live role/lease reference and free-list
   count after every edit, reading full audio/MIDI throughout, plus actual maximum
   logical-generation exhaustion and empty-batch/idempotent cleanup behavior.

See `docs/loop_layer/evidence/owner-activation-stage6{,-provenance}.json` for current
verification. The three new Miri cases are registered, not interpreted in this
slice. Do not copy the older guide's "all workloads ran under Miri" onto these tests.
A teaching example should show retained old exports, a refused whole batch, dropped-
permit retry, successful publication and explicit disposal of both old/new ownership.
Do not present test-harness helpers as public library methods.
