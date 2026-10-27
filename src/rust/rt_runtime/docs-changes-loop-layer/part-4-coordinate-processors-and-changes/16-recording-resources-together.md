# Chapter 16: reserve a mixed-media take before dependent rendering

Destination currently empty. Reviewed the full archived capture-group guide, current
boundary capability, take lifecycle and first-interval reservation implementation;
`../media-baseline.json` records inputs and refreshed selected tests. Write a focused
public-API planner/consumer example, not copied internal harness code or a loop engine.

## Example structure and observable checks

Prepare `with_capture_groups` with a MIDI index pool, a stereo audio member and a
MIDI member. Bind explicit `CaptureMember` descriptors. Set maximum frames/event/
payload capacities and show the arithmetic: audio staging is `max_frames × channels
×4`, MIDI staging is `12 × midi_events + midi_bytes`; their sum must fit the group
region. MIDI bounds must include any reserved recovery traffic the producer can
actually emit. No member may borrow another member's unused promise.

A small example can use max_frames4, stereo audio32 bytes and MIDI2 events/6 payload
bytes (30 encoded bytes), fitting staging64. This only sizes staging; separately
budget data pages, old/pinned revisions, versions, checkpoint rows and final leases.
Use literal samples and complete packets with an independent expected export.

1. `prepare_capture_take` (or boundary `prepare`) reserves all member versions,
   initial MIDI rows and final playback-lease slots, leaving old active content
   unchanged. Supply initial states in MIDI-member declaration order.
2. In `plan`, use `capture().reserve_prepared(take, F, N)` before activating a new
   take. Assert `PreparedReserved`, unchanged old revisions and failure before
   publication when even one member's page/checkpoint requirement does not fit.
   Activation at exactly F carries the same interval token/credits into Reserved.
   Do not stage data before activation or fake a new reservation afterward.
3. Give consumers copied interval/member descriptors through validated decisions.
   Use `RenderIo::stage_audio/stage_midi` for exactly one complete contribution per
   member; MIDI offsets are in `[0,N)`. Borrow no caller input past the call. Stage
   data without publishing a prefix that another processor could observe.
4. Commit from `finish` at the reserved end. Assert common duration/cut, exact audio
   and packet data, and returned unused conservative MIDI page reservations. Repeat
   under whole/single/irregular callback partitions without changing music.
5. Freeze at the common boundary with one empty `Option<Snapshot>` slot per member.
   Assert all final leases publish despite saturation of unrelated optional snapshot
   credit/background work. Freeze performs no tail copy or index rebuild. The group
   becomes **Idle**, while frozen active resources and returned leases remain owned.
6. In a separate small-store branch, fail a subsequent preflight before dependent
   samples render. Preserve the prior common prefix, update the planner's decision
   at F, and keep unrelated audio running. Never roll back already-rendered samples
   or drop one member's data and call the result coordinated.
7. Cover interval cancellation before take cancellation, stale/wrong-frame token
   refusal, occupied freeze outputs, retained old prefixes and cleanup/reuse. After
   callbacks stop, recover every reservation, snapshot, group claim and resource,
   and assert pool recovery. Domain state must retain its cleanup ledger.

## Authority, failure and accounting rules

`PlannerResourceIo::capture()` lends frame/ceiling-stamped `CaptureBoundaryIo`.
Operation/finish windows have no future sample range; only plan can reserve positive
samples up to its external/host ceiling. Commit must be at the reserved end;
activation/freeze must match the stamped boundary. Direct standalone
`ResourceStore::planner_io(frame, ceiling)` callers are responsible for those values.
Ordinary processors cannot acquire this lifecycle authority through ResourceIo.

`CaptureGroupId`, `CaptureTakeId` and `CaptureIntervalId` represent different checked
scope/generation/sequence lifetimes. Cancel the first interval to return
PreparedReserved→Prepared without cancelling the take's promised final leases;
then cancel the take if abandoning it. A retried interval gets a new sequence.

Group claims persist while idle, prepared, recording and after freeze until explicit
group release. Legacy replacement/remove/start/stop/raw append cannot bypass them.
Only Idle permits release_group; it preserves frozen content and outside leases.
Take preparation reserves lifecycle metadata, not unlimited future data pages.
MIDI interval preflight reserves the full declared producer bound even for a short
interval; it can conservatively refuse before the eventual actual packet count
would have filled storage. This is explicit capacity policy, not a musical quota.

Missing/duplicate/malformed/over-bound contributions after reservation are interface
faults, not late ordinary storage exhaustion. Incomplete commit must not publish a
prefix and remains cancellable; no downstream audio rollback is promised. A domain
coordinating several groups must release provisional reservations and latch each new
structural failure before re-preview; termination comes from its finite prepared
failure set, not a fixed retry count. The generic store does not choose musical modes.

Required group work is independent of background allowances. The current safe copy
path counts input→staging→64-byte stack scratch→resource pages: three media copies,
not zero-copy. Checkpoint-state copies have separate diagnostics. Counters saturate;
frame/revision/token/storage exhaustion remains semantic and explicit.

## Teardown and verification mapping

`into_resources` drops the planner; it cannot discover copied leases/reservations
hidden there. Use `into_planner_and_resources` when cleanup needs its ledger, after
stopping callbacks. Disconnect does not invent Stop or a completed interval. The
loop facade's consumed-engine permanent quiescence is a separate domain cleanup
policy; do not expose it as a generic runtime guarantee or replay authorization.

Fresh debug/release checks include capture_groups, capture_group_intervals,
capture_group_planner, capture_prepared_interval, capture_boundary, the2048-action
ownership/content model and disconnect-at17-lifecycle-cuts case. They form part of
63 existing cases/profile, not a new book example. Stage5/6 loop adaptation is now
implemented and separately tested; replace obsolete archive “full loop integration
pending” wording with correct attribution, while retaining actual-loop-host and
final-delivery requirements as pending. The separate worker still must implement
and execute this chapter's example and verify all published excerpts/links.
