# P11 replacement-backend parity investigation

Investigated on 2026-11-12. This is an implementation assessment, not a feature
implementation or a claim that P11 is complete.

## Agreed scope and behavior

The user narrowed this work to P17 definition duplication and occupied cross-kind
copying, deferring MIDI state/history to a separate session. Cross-kind copying
has the same behavior as explicitly clearing the target and then copying into the
resulting empty slot. This supersedes the earlier prepared atomic replacement
proposal and its estimate.

Same-kind copying keeps its existing behavior. Cross-kind copying uses the
existing clear operation, waits for its confirmed completion, and then uses the
existing duplication path. A successful clear is not rolled back if copying later
fails. Existing clear/copy admission rules and dependent-composite handling apply;
this change does not promise that every dependency configuration is accepted.

## Source and checkout status

- Replacement branch: `feat/backend-replacement`, commit
  `9aee74966fa66e896e7b6c26ce4893e8c532735b`.
- Production reference: `master`, commit
  `9134eb20b7d1ee34e71b5745927645c9c2bd8d2a`.
- The investigation used replacement sources pinned to that commit and the local
  production reference. The later implementation fetch succeeded and work is on
  `feat/p17-clear-then-copy`, based on the replacement commit above.
- This document records source inspection. Implementation and validation follow
  [the duplication plan](../../PLAN_LOOP_DUPLICATION.md); results belong in its PR.

## Implementation outcome

The duplication work is implemented on `feat/p17-clear-then-copy` using the
existing service job, authored-event validator and clear lifecycle. Cross-kind
copy waits for lifecycle success and the empty projection, then enters ordinary
copy with current mappings. The baseline Clear/Copy oracle also exposed a narrow
empty-slot conversion bug: historical capture frames remained nonzero after
Clear. Conversion now checks stopped mode and current content length; capture
resource preflight is retained. No protocol or file-format change was needed.
The parity ledger keeps the MIDI requirement open. See the
[execution plan](../../PLAN_LOOP_DUPLICATION.md) and its linked PR for validation.

## Finding

P11 contains duplication and MIDI history work. Only duplication is in the current
scope. P17 copying should reuse the current composite machinery. Occupied
cross-kind copying can compose clear and duplication in the service: the first
operation establishes the empty-slot precondition already required by the engine.
No new atomic replacement protocol or engine transaction is proposed.

The architectural fit is good if the service continues owning copy operations and
authored definitions, and the loop planner continues owning musical timing and
media publication. No legacy production dependency, host-specific semantics,
second musical clock or client-owned copy executor is needed.

## Requirements identified at the investigation base

At the inspected replacement commit, the [parity row][parity] left open:

1. Copying primitive content over a composite and a composite definition over an
   occupied primitive, on compatible same-track destinations.
2. Copying P17 definitions, including Replacing and dry/wet modes, processed-track
   containers and nested references, with fresh copied definition/event ownership.
3. Preserving complete MIDI packet order, initial/preroll/seek state and parameter
   history through capture, imports, exports, duplication and session round trips.

Already supported: WAV/loop-audio/exact-MIDI/SMF import and replacement, mapping and
rate consent, generated click insertion, primitive role-aware copying and P03
Direct-track definition copying. System/Master preview belongs to completed P15.
P17 save/load already works; this investigation does not propose reimplementing it.
General latency compensation and take-alignment controls remain P09/P10. P11 must
preserve existing offsets, preplay and role metadata without depending on those
controls being restored first.

## Production reference versus replacement

| Area | Evidence on master | Replacement behavior and implication |
| --- | --- | --- |
| Destination identity and transport | `duplicate_loop_into` / `finish_primitive_loop_duplicate` copy into the destination model and leave it stopped at zero | The service copy contract keeps public slot identity, selection and targeting. Preserve that established contract; engine slot generations may change internally. |
| Cross-kind copying | The reference replaces an existing backend composite or configures one for a primitive destination | Replacement service deliberately refuses occupied cross-kind copies. Adapter `validate_prepared` and engine `validate_composite` independently require an empty stopped primitive. The inverse reset produces an empty primitive. This is more than a UI gate. |
| Composite copying | Reference copies authored composition and recorded settings | Replacement already copies a `Versioned` document, remaps event instance IDs and retains frozen geometry. `copy_closure` still restricts all traversed tracks to Direct and authored modes to Play/Stop/Record, although the shared compiler accepts P17 modes. |
| MIDI initial state | Master folds packets through `MidiStateTracker`, then emits channel state | The reference is useful for ordinary controls, notes and preroll timing, but does not preserve arbitrary vendor state or parameter command history. P11's explicit preservation requirement exceeds this old projection; matching its information loss would not close the row. |
| MIDI timeline | Exact media carries packet bytes, frames and stable ordering | Replacement shared MIDI retains ordered originals and immutable cuts, including opaque timeline packets. The missing piece is retaining/restoring history at an initial or seek boundary, rather than merely retaining packets after it. |

Reference entry points:
[copy implementation][master-copy], [logical MIDI export][master-export],
[canonical initial state][master-state].

## Duplication implementation

### Reuse the service operation

Extend [service duplication][duplication], which already executes
Capture -> Rows -> Stop -> Stopped -> Apply -> Controls -> Done and survives UI
detach. It does not require a file provider. Same-kind copying keeps its current
failure semantics: before payload commit, old destination content survives,
although Stop and confirmed row additions may remain. For cross-kind copying,
that protection applies to the state after the clear, not the previous occupied
content. After payload commit, report remaining control/metadata failures without
replaying or pretending to roll back committed content. Unknown engine outcomes
stay unknown.

For P17, replace copy-specific Direct/mode allowlists with shared validation of the
whole authored document and its dependency closure. Validate authored events as
well as compiled spans: an unsupported event can be shadowed out of the compiled
program. Existing tests deliberately cover that distinction.

Preserve explicit modes versus dynamic defaults, roles, recorded processor state,
timing basis, frozen event endpoints, repeat and sync. Allocate fresh event IDs and
prepared versions for the copied definition. Preserve child references: copying a
definition does not recursively clone shared child loops or nested definitions.
Source and destination definition edits must remain independent.

Audit `copy_destination_ready` with P17 ownership: active dry playback, wet
rerecord, Replacing, future writes, pending candidates and retained working plans
must all receive the same protections as their ordinary edit/persistence routes.
Do not stop an affected recording to make a refused copy proceed. Preserve the
existing policy for conflicting active parents; closure does not require enabling
otherwise unsafe concurrent edits.

### Compose clear and duplicate for cross-kind targets

Keep one service-owned operation with explicit clear and copy phases. Validate
basic request identity, including self-copy, before destructive work. Apply the
existing admission rules without adding a special conversion policy.

1. If the occupied target's kind matches the source, use the existing copy path.
2. Otherwise invoke the same clear path as a normal user Clear. Reuse
   [composite structure lifecycle][structure] for its existing affected-parent
   handling and wait for the complete operation to confirm, not just command
   admission.
3. Re-read the confirmed target state and run the normal copy path into that empty
   slot. A primitive source loads ordinary media; a composite source uses the
   existing empty-slot composite conversion and prepared definition activation.
4. Report the terminal result through the original service operation. Keep the
   service job across UI detach so the two phases cannot be reordered or replayed
   by the client.

If clearing fails or its outcome is unknown, do not start copying. If clearing
succeeds and copying later fails, retain the state left by the normal copy path
and report that the target was cleared. A failure before copied-payload commit
therefore leaves the target cleared; a later failure may leave copied content with
incomplete controls/metadata. Cancellation does not undo an acknowledged clear.
Do not automatically restore old content or retry an unknown mutation outcome.

The current persistence contract's old-destination-preservation wording must be
qualified for cross-kind copies when implementing this user-approved behavior.
Same-kind guarantees remain unchanged. Existing adapter and engine conversion
preconditions remain useful and need not be relaxed:
[adapter validation][adapter-composites], [engine validation][engine-composites].

No engine/protocol changes are anticipated for this composition. Implementation
must verify service operation nesting, receipt ownership and confirmed clear
completion; factor shared lifecycle helpers where needed instead of inventing a
second clear implementation or a new RT transaction.

## Deferred MIDI investigation

The following notes preserve the original investigation for a separate session.
They are not part of the current duplication implementation proposal.

### Where the information is lost or refused

| Layer | Current restriction | Required change |
| --- | --- | --- |
| Adapter preparation | `initial_message` accepts short channel messages/System Reset; validation rejects untracked CC6/38/96/97 history in initial/preroll state | Validate a richer ordered representation against explicit byte/event limits; remove refusals only after downstream support exists. |
| Engine upload/runtime initial state | `ContentInitial` is reduced to a maximum three-byte resource initial packet; runtime initial state is a fixed `MidiState` projection | Stage original initial/history packets in bounded immutable storage, independently of the channel projection. |
| Engine capture/bulk | `MidiInitial` reconstructs channel state with `restore_channel` and refuses `parameter_history_untracked` | Export retained history plus projected state from the same immutable cut. Current one-byte packet lengths and short-packet assembly cannot carry arbitrary initial SysEx. |
| Adapter capture/read | Initial reads cap each channel result at 16 KiB and parse packet lengths <=3 | Add bounded chunk assembly for the negotiated history shape and packet lengths; charge aggregate bytes and retained ownership. |
| Runtime shared sources/edits | Shared takes carry initial projected state; rolling history eviction advances that state | Carry history ownership/coverage across record, grab, import and Replace. Retain enough history or report unavailable coverage; never quietly discard state when old input ages out. |
| Loop restoration | `midi_restore` correctly restores channel/key/pedal/program state, but intentionally does not recover vendor or parameter state | Add a prepared restoration plan with explicit history/baseline semantics; keep the existing fast projection and cleanup. |
| Codec/export/load | Logical export folds preroll into `MidiState`; `validate_initials` refuses vendor/system and untracked history | Preserve ordered history through exact media and archives; project only where the format contract permits it. Share validation rules with adapter preparation to prevent drift. |

Sources: [adapter preparation][prepared], [adapter validation][validation],
[upload handling][engine-content], [bulk initial export][bulk],
[adapter initial assembly][reads], [shared import][shared-import],
[channel projection][state], [loop restoration][restore], [codec][codec].

### Recommended representation and ownership

Keep the indexed channel/key projection for fast notes, pedals, controller and
program queries. Add a separately owned immutable ordered history/replay component
for information that cannot be compressed into that projection. Reuse the shared
MIDI packet slabs, ordered indexes, immutable version leases and global credit
admission instead of another unbounded event store.

Retain original packet boundaries and equal-frame order. Represent an initial
prefix and its baseline/coverage explicitly; do not insert its packets as ordinary
frame-zero timeline events, since start, wrap and seek have different restoration
rules. Exact files can already carry ordered `start_state` packet vectors, but a
versioned extension may be needed for timed preroll, coverage or baseline metadata.
Prove compatibility before deciding whether a document version change is needed.

RPN/NRPN need selector-aware history: banked parameter identity, Data Entry MSB/LSB,
selector changes/nulling, relative increment/decrement and reset effects. A latest
CC array cannot represent writes to several parameters. Relative commands cannot
be replayed repeatedly against an arbitrary receiver baseline and called
idempotent restoration. A bounded parameter index could compress known absolute
values, but must preserve unresolved history and original command order.

Vendor/system packets must remain opaque to the generic runtime. The engine cannot
infer arbitrary synthesizer state, deduplicate SysEx safely, or reconstruct history
it never observed. Define and test exact recorded-packet replay separately from
device-state reconstruction. Specify which baseline makes replay valid, what is
done on repeated seeks/wraps, and how stateful vendor traffic, resets and transient
system/realtime packets are handled. Unknown-baseline cases must remain explicit;
they cannot be advertised as full restoration by suppressing the warning.

### RT constraints and protocol consequences

- No filesystem/codec logic enters the service's engine commands. Clients own
  dialogs and codecs; the service owns admitted mutations; the adapter maps IDs
  and protocol; loop parts own frame cuts and restoration.
- Account history packets/bytes, COW indexes, retained bases and retirement in
  prepared finite pools. Live capture reservations must survive competing imports
  and long-held exports. Finite capacity refusal is compatible with the
  architecture; silent prefix/history truncation is not exactness.
- Precompute/validate restoration descriptions using bounded preparation. A
  required seek/start cannot scan an arbitrary prefix in the callback or spread
  musical restoration over later callbacks. Reserve worst-case event/byte output
  capacity, including simultaneous starts and host MIDI egress. Refuse before
  admission when that capacity is unavailable.
- Continuous Replace must update history/index roots without rebuilding unchanged
  suffixes. Rolling-history eviction must advance a truthful restoration-coverage
  boundary. This is a core feasibility test for the proposed representation.
- Evolve versioned app/RT schemas and both typed and encoded paths together where
  necessary. Transfer arbitrary packets using existing bounded chunk lanes, not
  a huge inline `MidiMessage`, and retain generation/cancellation fencing.
- Exact export/session storage must preserve packet order and history. SMF uses
  quantized time and cannot necessarily preserve the initial-prefix distinction or
  every system packet; explicitly report conversion loss or refuse unsupported
  conversion. Its current result primarily reports maximum timing error, so audit
  this reporting as part of P11.

## Suggested delivery for the agreed scope

- [ ] Enable P17 definition copying through shared authored validation. Verify
  processed/nested modes, fresh event IDs, shared child references and frozen
  geometry using the existing production copy fixtures.
- [ ] Add a cross-kind clear phase to the service copy job, reusing existing clear
  and empty-slot copy operations. Verify both directions, confirmed phase ordering,
  parent handling and failure/cancellation before and after the clear.
- [ ] Validate public UI/Lua behavior, accepted-operation detach/recovery, native
  typed/encoded delivery, applicable actual AudioWorklet behavior, and save/load of
  the copied result. Update the persistence contract and duplication portion of
  the parity row with evidence.

The earlier estimates included a new atomic replacement mechanism and MIDI history
work; they do not apply to this smaller service-composition change. P11 remains
open for deferred MIDI history even after the duplication portion is delivered.

During implementation, keep the accepted criteria fixed, update checked work as it
lands, commit meaningful milestones, and revise the design only with documented
evidence. If implementation is commissioned, create a branch based on the
replacement commit, use a PR against `feat/backend-replacement`, run CI and address
automated review; merging is not part of this investigation.

## Acceptance for the agreed duplication scope

- [ ] Same-kind and both cross-kind occupied copies preserve the public slot,
  selection/targeting, copied metadata/settings and stopped-zero result. Source
  data and unrelated active recordings remain usable.
- [ ] P17 regular/explicit/nested definitions copy on eligible Direct, External,
  built-in FX and OxiSynth tracks, preserving roles, explicit/default semantics,
  timing and shared child references while obtaining independent copied ownership.
- [ ] Existing clear/copy admission rules handle affected recordings/future writes,
  unsupported topology/modes, cycles, stale revisions and capacity exhaustion.
  Same-kind failures retain their existing guarantees. Cross-kind clear failures
  do not start copying; a successful clear is not rolled back after copy failure
  or cancellation. Later failures and unknown outcomes are reported without replay.
- [ ] A cross-kind copy matches the observable result of normal Clear followed by
  normal Copy, including dependent-definition handling and any existing refusal.
  Copy starts only after complete clear confirmation and uses confirmed state.
- [ ] The service retains the compound operation across UI disconnects, including
  between clear and copy. Neither phase is replayed by reattachment.
- [ ] Save/load preserves the resulting P17 definitions and supported role media;
  copying works without a file provider. Existing same-kind behavior is unchanged.
- [ ] The persistence contract and parity row document cross-kind clear-then-copy
  semantics and the delivered duplication scope; deferred MIDI remains open.

## Deferred criteria for full P11 closure

These remain outside this session's agreed implementation scope.

- [ ] Initial, preroll, ordinary timeline and seek-boundary packets preserve full
  byte content and equal-frame ordering through live capture, grab, Replace,
  copy, import, exact export and session save/load/store. Logical cuts neither
  duplicate nor omit frame-boundary originals.
- [ ] Restoration tests cover multiple RPN/NRPN parameters, relative operations,
  resets, program/pending banks, sustain/sostenuto/Hold2, initial held notes,
  SysEx across chunks/pages, repeated seeks/wraps and history eviction. An
  independent linear oracle checks retained history; meaningful receiver models
  check only the device semantics the contract actually guarantees.
- [ ] Native typed/socket and applicable shared-memory routes, real browser
  AudioWorklet paths, public UI and Lua routes, accepted-operation detach/recovery,
  failure injection, finite storage and allocator guards pass. Firmware flags and
  budgets remain truthful where the feature is offered.
- [ ] Exact-format guarantees and SMF losses are stated separately. Old supported
  files load coherently; unsupported new requirements fail before mutation.
- [ ] Update the persistence contract, capability descriptions and P11 row with
  implementation/validation evidence. Mark P11 complete only after both
  workstreams and their persistence requirements pass.

For the current scope, start with
`shoop_app/tests/support/duplication_cases.rs`, existing composite lifecycle tests
and `shoop_rt_backend/tests/content_copy.rs`. Add production coverage for the
master duplication expectations and the agreed clear-then-copy behavior. Broaden
to the branch's documented build, native suite and actual browser acceptance at
integration milestones. MIDI restoration/index oracles belong to the deferred
workstream, not a new acceptance requirement for this service-only change.

[parity]: https://github.com/SanderVocke/shoopdaloop/blob/9aee74966fa66e896e7b6c26ce4893e8c532735b/docs/native_session_recovery/feature-parity.md
[master-copy]: https://github.com/SanderVocke/shoopdaloop/blob/9134eb20b7d1ee34e71b5745927645c9c2bd8d2a/src/rust/shoop_app/src/lib.rs#L6367
[master-export]: https://github.com/SanderVocke/shoopdaloop/blob/9134eb20b7d1ee34e71b5745927645c9c2bd8d2a/src/rust/shoop_app/src/lib.rs#L12462
[master-state]: https://github.com/SanderVocke/shoopdaloop/blob/9134eb20b7d1ee34e71b5745927645c9c2bd8d2a/src/rust/shoop_backend/src/lib.rs#L5379
[duplication]: https://github.com/SanderVocke/shoopdaloop/blob/9aee74966fa66e896e7b6c26ce4893e8c532735b/src/rust/shoop_app/src/service/duplication.rs
[structure]: https://github.com/SanderVocke/shoopdaloop/blob/9aee74966fa66e896e7b6c26ce4893e8c532735b/src/rust/shoop_app/src/service/composites/structure.rs
[adapter-composites]: https://github.com/SanderVocke/shoopdaloop/blob/9aee74966fa66e896e7b6c26ce4893e8c532735b/src/rust/shoop_rt_backend/src/composites.rs#L170
[engine-composites]: https://github.com/SanderVocke/shoopdaloop/blob/9aee74966fa66e896e7b6c26ce4893e8c532735b/src/rust/shoop_rt_engine/src/composite.rs#L55
[prepared]: https://github.com/SanderVocke/shoopdaloop/blob/9aee74966fa66e896e7b6c26ce4893e8c532735b/src/rust/shoop_rt_backend/src/prepared.rs
[validation]: https://github.com/SanderVocke/shoopdaloop/blob/9aee74966fa66e896e7b6c26ce4893e8c532735b/src/rust/shoop_rt_backend/src/prepared/validation.rs
[engine-content]: https://github.com/SanderVocke/shoopdaloop/blob/9aee74966fa66e896e7b6c26ce4893e8c532735b/src/rust/shoop_rt_engine/src/content.rs#L352
[bulk]: https://github.com/SanderVocke/shoopdaloop/blob/9aee74966fa66e896e7b6c26ce4893e8c532735b/src/rust/shoop_rt_engine/src/bulk.rs#L538
[reads]: https://github.com/SanderVocke/shoopdaloop/blob/9aee74966fa66e896e7b6c26ce4893e8c532735b/src/rust/shoop_rt_backend/src/reads.rs#L420
[shared-import]: https://github.com/SanderVocke/shoopdaloop/blob/9aee74966fa66e896e7b6c26ce4893e8c532735b/src/rust/rt_runtime/src/resource/shared_midi/import.rs
[state]: https://github.com/SanderVocke/shoopdaloop/blob/9aee74966fa66e896e7b6c26ce4893e8c532735b/src/rust/rt_runtime/src/midi_state.rs
[restore]: https://github.com/SanderVocke/shoopdaloop/blob/9aee74966fa66e896e7b6c26ce4893e8c532735b/src/rust/shoop_loops/src/midi_restore.rs
[codec]: https://github.com/SanderVocke/shoopdaloop/blob/9aee74966fa66e896e7b6c26ce4893e8c532735b/src/rust/shoop_persistence_codec/src/codec.rs
