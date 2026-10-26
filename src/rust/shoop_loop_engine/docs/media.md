# Fixed media ownership ledger

`media::Ledger` implements the ownership component of the planned coordinated facade. It does **not** execute resolver transitions, freeze capture groups, acquire an atomic live capture prefix, or establish a coherent public musical-state cut. Those integration responsibilities remain pending.

## Preparation and identities

Prepare with `Ledger::prepare(loop_slots, media_slots, pin_slots, export_slots)`. Storage arithmetic uses the common native/Wasm ceiling. Slots, free lists and role rows are initialized during preparation; no method subsequently grows them. Preparation allocates a checked instance scope. There is no claim of OS page locking or a universal deadline.

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

Repeated assignment of the same role is idempotent. A role cannot select another loop generation's content. Pending pins preserve an admitted replacement after resident retirement or role changes. Pins and exports have distinct tokens, so double/stale release cannot decrement another owner's reference.

External export saturation does not consume pending-pin capacity or the fixed playback/latest roles. Pin saturation does not consume export capacity. Neither pool is needed to reserve a capture metadata slot or change internal roles. All pools are finite: retained media can legitimately exhaust metadata or underlying resource storage. The future facade must size pending pins for its admitted request ownership and refuse insufficient admission before promising an operation.

Exports retain media metadata and leases without acquiring another snapshot, copying content or rebuilding indexes. `exported` returns the immutable media view and the caller-supplied `requested_cut`. That frame is **not independently certified as a settled musical-state boundary**; snapshot capture/cut metadata retain their own meaning. The coordinated planner must provide the authoritative cut. Existing generic resource cursors/owning read buffers can read exported snapshots.

When the final reference disappears, the ledger releases both underlying leases exactly once and invalidates the media identity. Generic resource reclamation remains incremental. Removed resources remain readable through still-pinned immutable snapshots.

## Teardown and costs

`unbind` clears internal roles, but resident content, pending pins and exports remain owned and readable. It does not stop a loop. `release_all` explicitly releases all roles/tokens/residents, including unpublished reservations, and is idempotent. Call it only after the surrounding engine is quiescent; it cannot send device MIDI cleanup. The caller still owns resource handles/capture groups and must remove/release them and drive bounded reclamation.

All callback-capable operations use fixed storage, with no locks, waits or I/O. Adoption/publication check the fixed media table for existing owners; quiescent cleanup scans prepared tables. MIDI validation seeks frame zero to validate indexing, not through content history. Reference retention is not snapshot duplication. Resource metadata work uses scoped resource access, while snapshot acquisition/tail copies and ordinary exports remain subject to the generic resource accounting contracts. This component has no subdivision or autonomous progression quota.

## Evidence scope

Four guarded integration cases cover mixed-media shared ownership, saturated independent pools, scoped/malformed/duplicate/unindexed inputs, reservation cancellation/publication, stale generations, source removal and full cleanup. An independent **2,048-action** model (seed `0x87a4439cdd3517ab`) predicts live media, resident/role/pin/export references and snapshot credits after every action, checking complete samples/packets throughout. All ten action classes execute more than twenty times. A guarded private case exercises actual `u32::MAX` media/pin/export generation retirement.

All five workloads run unchanged on native, Wasm and Miri. The allocator implementation is shared through the dev-only test-support crate; production retains `#![forbid(unsafe_code)]`. These ownership proofs are not the required coordinated record-to-play/FX/export example or independent loop AudioWorklet demonstration.
