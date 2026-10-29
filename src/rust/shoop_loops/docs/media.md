# Fixed media ownership ledger

`media::Ledger` supplies ownership for the [coordinated facade](runtime.md). The ledger alone does **not** execute resolver transitions, freeze capture groups, acquire an atomic live prefix or certify a settled musical cut. The facade coordinates those responsibilities; its public [lifecycle](../examples/coordinated_loop.rs) and [configuration](../examples/configuration_cut.rs) examples retain readable old exports.

## Preparation and identities

Prepare with `Ledger::prepare(loop_slots, media_slots, pin_slots, export_slots)`. Storage arithmetic uses the common native/Wasm ceiling. Slots, free lists and reference rows are initialized during preparation; no method subsequently grows them. Preparation allocates a checked instance scope. There is no claim of OS page locking or a universal deadline.

Register each primitive with `bind(LoopId)`. A row remembers the highest registered generation even after `unbind`; the same or an older identity cannot silently acquire that row again. Media, pending-pin and export IDs have distinct types and carry scope, slot and generation. `parts`/`from_parts` support plain-data transport, but constructing a value does not establish its validity. Every operation validates it against the owning ledger. Exhausted slot generations retire permanently instead of wrapping.

A `MediaId` is a **borrowed identity**, not an owning reference. It stays usable only while at least one ledger owner remains. Copying an ID, view or `Source` does not acquire ownership.

## Import and reserved publication

`Input` describes one logical revision and contains optional audio/MIDI `Source` values. Each source includes both snapshot metadata and its scoped `ReadIndex`. A successful `adopt` transfers those existing snapshot leases to a new ready media slot and creates one resident reference. The caller must not release the transferred leases separately.

Validation checks:

- A currently registered logical owner.
- Scoped index/lease identity and exact supplied snapshot metadata.
- Audio format/channel count and MIDI format/indexing.
- A shared declared duration for every supplied source.
- Required sources for nonempty media; no sources for disabled media kinds.
- No duplicate ownership of a lease already held by another slot in this ledger.
- Compatibility with the store containing all existing ledger-owned sources.

An empty description may omit sources; timing-only media can have nonzero duration without audio or MIDI. Domain revision assignment remains the resolver/facade's responsibility. The ledger neither advances a musical revision counter nor infers whether two inputs describe the same logical take.

`reserve` creates an unreadable metadata slot before capture admission; its declared duration must be zero. `publish` validates the complete input before making that slot ready. Owner, logical revision and media shape must match the reservation; only duration is finalized. It publishes once. `retire` cancels an unpublished reservation without touching any snapshot.

Adoption/publication failures return `Rejected { error, input }` unchanged. They do not consume the input's leases, publish one media kind without the other, or discard the reservation. Returned copied aliases do not acquire ownership they did not have before the call. Work counters can advance during validation.

## Owners and independent capacities

Each ready slot owns each underlying snapshot **once**, regardless of reference count:

| Owner | Acquisition/release |
| --- | --- |
| Resident/prepared content | Successful adopt/reserve; `retire` releases this reference once |
| Playback | `set_playback` replaces one fixed per-loop reference |
| Latest completed take | `set_latest` replaces another fixed per-loop reference |
| Pending request | `pin` / `release_pin`, using a separate checked pin pool |
| External export | `export` / `release_export`, using a separate checked export pool |

Repeated assignment of the same reference is idempotent. A reference cannot select another loop generation's content. Pending pins preserve an admitted replacement after resident retirement or reference changes. Pins and exports have distinct tokens, so double/stale release cannot decrement another owner's reference.

External export saturation does not consume pending-pin capacity or the fixed playback/latest references. Pin saturation does not consume export capacity. Neither pool is needed to reserve a capture metadata slot or change internal references. All pools are finite: retained media can legitimately exhaust metadata or underlying resource storage. The facade sizes pending pins for admitted request ownership and refuses insufficient admission before promising an operation.

Exports retain media metadata and leases without acquiring another snapshot, copying content or rebuilding indexes. `exported` returns the immutable media view and the caller-supplied `requested_cut`. That frame is **not independently certified as a settled musical-state boundary**; snapshot capture/cut metadata retain their own meaning. The coordinated planner must provide the authoritative cut. Existing generic resource cursors/owning read buffers can read exported snapshots.

When the final reference disappears, the ledger releases both underlying leases exactly once and invalidates the media identity. Generic resource reclamation remains incremental. Removed resources remain readable through still-pinned immutable snapshots.

## Atomic logical-owner batches

Use `OwnerChange::replace(previous: Option<LoopId>, input)` to add (`None`) or replace (`Some`) one row, or `OwnerChange::remove(previous)` to remove it. A replacement stays in the same logical slot and strictly exceeds its committed generation watermark. Ordinary adopt still requires an already-bound owner; it is not a generation-replacement API.

Pass the entire mutable edit slice to `ledger.prepare_owner_activation(&mut io, &mut changes)`. It validates every edit before publication: unique slots, exact previous owners, generation watermarks, old selected leases, new source shape/duration/index/scope, and duplicate leases both within the batch and against ledger ownership. New initial inputs must fit **currently free** metadata; it does not count credits that old-owner retirement might later recover. A later bad input refuses the whole batch, not an applied prefix.

The returned `OwnerActivation` exclusively borrows ledger, resource access and edits. Dropping it unused changes neither ownership nor generation watermarks; retained input leases still belong to the caller. `commit(self)` is the preflighted infallible publication step. It clears old playback/latest references, binds the new generations and publishes each new initial input with resident plus playback references, no latest take. Old residency, pins and exports remain independent owners and readable through their original identities.

After commit, inspect `is_applied()` and `published()` on each change. Before application, `input()`/`into_input()` expose or recover the unapplied input; explicitly release its leases if abandoning it. Neither dropping an OwnerChange nor copying its raw handles releases a lease. Reapplying a consumed edit is InvalidState. This API does not stop music, reconcile MIDI, freeze capture, update the resolver/graph or acquire capture groups. The facade's ConfigurationActivate holds this permit with a graph permit across final fallible timing publication after checking stopped owners and clean rows; use that integrated transaction for musical configuration cuts.

## Teardown and costs

`unbind` clears internal references, but resident content, pending pins and exports remain owned and readable. It does not stop a loop. `release_all` explicitly releases all references/tokens/residents, including unpublished reservations, and is idempotent. Call it only after the surrounding engine is quiescent; it cannot send device MIDI cleanup. The caller still owns resource handles/capture groups and must remove/release them and drive bounded reclamation.

All callback-capable operations use fixed storage, with no locks, waits or I/O. Adoption/publication check the fixed media table for existing owners; quiescent cleanup scans prepared tables. MIDI validation seeks frame zero to validate indexing, not through content history. Reference retention is not snapshot duplication. Resource metadata work uses scoped resource access, while snapshot acquisition/tail copies and ordinary exports remain subject to the generic resource accounting contracts. This component has no subdivision or autonomous progression quota.

## Evidence scope

Four guarded integration cases cover mixed-media shared ownership, saturated independent pools, scoped/malformed/duplicate/unindexed inputs, reservation cancellation/publication, stale generations, source removal and full cleanup. An independent **2,048-action** model (seed `0x87a4439cdd3517ab`) predicts live media, resident/playback/latest/pin/export references and snapshot credits after every action, checking complete samples/packets throughout. All ten action classes execute more than twenty times. A guarded private case exercises actual `u32::MAX` media/pin/export generation retirement.

All five workloads run unchanged on native, Wasm and Miri. The allocator implementation is shared through the dev-only test-support crate; production retains `#![forbid(unsafe_code)]`. These foundation ownership proofs retain their original scope. The separate coordinated examples demonstrate record/play/FX/export and stopped-owner publication; neither those examples nor package-Wasm tests are an independent loop AudioWorklet or new performance/deadline evidence.
