# Fixed-tick history and private replacement candidates

`LoopPlanner::prepare_shared` installs prepared history stores before the planner
transfers to a runtime. `shoop_rt_engine::prepare_with_shared` connects them to
strip input capture feeds in the production graph. The ordinary engine preparation
uses no histories. These internal library operations add no application capability
flag, host wire command or continuous Replacing recording mode.

## Sources and complete cuts

`HistorySettings::source` identifies a prepared input tap (the engine uses a strip
slot). Configure audio, MIDI or both, with explicit horizons, pages, owner slots,
event/packet limits and transaction credits. Audio scratch covers the core quantum
Q; maximum host callback size is a separate bound. The tap appends once per source
and tick, even when every loop is stopped. Independent loops share these pages.

The history MIDI tracker observes input once before storage admission. A loop's
own recording tracker is a separate observer. Storage refusal leaves this tracker
advanced. The next accepted input starts a new segment from the observed state;
missing audio is never represented by fabricated silence or callback replay.
`audio_history_discontinuity` retires the old history root while retained takes and
exports keep their own roots. Old retirement can delay admission of a new segment.

`HistoryStatus` reports the intersection of audio/MIDI coverage, capped by the last
successfully finished core tick. A tap can write before a later processor faults;
that tick never advances the common completion watermark. Routing changes fence
old source IDs immediately. No coverage in the new graph is eligible until a tick
finishes in that graph. Input changes, resets and gaps also change the source
generation. Unchanged routing masks do not reset the source's MIDI projection.
Audio-only routing changes start a common segment while preserving the unchanged
MIDI source's observed projection. MIDI-source changes and full input resets use
a separate observation token and reset that projection.

Schedule `HistoryGrab` through the ordinary domain lane. Its source ID includes
planner scope, graph epoch and source generation. The positive logical duration
must be divisible by Q; stored start/end coordinates keep frame precision. The
range must be retained and complete by the effective operation tick. A partial
host callback may complete no tick; a newly submitted operation cannot act on an
earlier tick whose host input has already started accumulating. Execution receipts
retain requested and delivered frames. Buffering has the runtime's declared output
delay; history time is core input time.

Paired audio/MIDI admission rolls back the first resource if the second refuses.
`SharedTake` contains ordinary resource IDs, duration and prepared store IDs.
Copying it acquires nothing. The caller may remove each resource after transferring
immutable snapshots to playback or exports. The coordinator tracks these resources
for permanent shutdown, including successful results the controller never reads.
Generic independently acquired snapshot leases retain their ordinary release rule.

## Preparation and publication

`CandidateBegin` binds a private take to a primitive loop's current content,
length, slot generation, graph and topology. Its sources must match the loop's
audio/MIDI shape and fixed logical duration. Shared sources fork immutable roots.
Loaded PCM retains an immutable base. Loaded indexed MIDI imports descriptors and
checkpoints incrementally without copying packet bodies; it reports `Building`
until complete. Loaded media may exceed the history horizon, within prepared
descriptor/page/owner capacities.

`CandidateAudio` copies at most Q input frames from an immutable snapshot into
prepared interleaved scratch and applies one wrapping private edit. `CandidateMidi`
owns a bounded snapshot interval, its complete packet bytes and its pre-input
projection before starting an edit. Stored coordinates are frame-precise even
though live loop periods and seeks are Q-aligned. Replacing logical zero refreshes
initial state; outside originals retain their bytes and order. Stored splices add
no restoration messages. An empty positive MIDI interval erases originals.

The prepared candidate table rotates fairly through pending edits/imports. One
step advances at most one event under the store's limits. The total allowance is
`edit_steps_per_callback`, shared across every tick in that host callback. Scanning
is bounded by that allowance times the candidate table size; idle tables perform
no repeated scan. Input capture and required playback use render accounting and
reserved stores independently of ordinary background transfers.

Query `CandidateStatus`. `Building`, `Ready` and `Failed(error)` are explicit.
There is no next-tick readiness promise. A patch must finish before another patch
uses that candidate. Zero-length patches change nothing; publishing an entirely
unchanged candidate refuses. Cancellation invalidates its ID and retires private
owners/imports/edits without changing the live loop.

`CandidatePublish` refuses an unfinished candidate at the requested cut. The
controller must explicitly choose a later cut; no deadline is silently relabelled.
Publication rechecks source resource revisions/writer conflicts, target generation,
content revision, duration, graph/topology and active/prepared capture. It snapshots
the complete private pair and preflights ordinary media activation before changing
the resolver revision and playback selection in one tick. Same-length replacement
preserves playhead and current controls, including primitive loops used by programs.
Actual period/family changes retain the separate family-selection contract.

Source-writer validation scans the prepared ordinary version table (at most twice
per mixed candidate), charges that finite scan to render diagnostics and refuses
pending uploads or capture writers. It does not consume background validation
credits. Permanent shared-owner teardown also cancels unfinished ordinary uploads
before returning their resource and reserved-page credits.

Ordinary MIDI cleanup, seek and restoration occur on the playback discontinuity.
They are output policy, separate from stored replacement packet data. Old exports
and sibling takes retain their original complete revisions after selection, source
removal, a superseding upload or whole-configuration replacement.

## Capacity, cleanup and evidence

`SharedCapacity` reports coordinator tables/scratch, store metadata and the MIDI
state subset separately. Reserved payload bytes are a subset of the ordinary
arena, not another allocation. Figures exclude allocator overhead and common
resource/runtime/graph metadata. Page reservations, owner/index credits, candidate
slots and queued retirement remain finite; free raw bytes alone do not guarantee
admission. `SharedWork` reports callback-local history ticks/refusals, preparation
steps and scans. Resource render/background and MIDI-store counters supply the
remaining copy, state, index and retirement diagnostics.

Permanent stopped cleanup cancels private work, releases tracked returned owners
and retires histories. Bounded reclamation must finish before destruction. A
terminal fault never authorizes replaying consumed input. Ordinary external
snapshot/upload ownership remains explicit.

The engine fixtures use the production graph and domain with allocator guards,
independent audio/packet/state oracles and Q=1/16/32/64/128 across host partitions
1/7/31/128. Scalar library fixtures are fixed Q=1 references. The mixed persistence
fixture retains exports through publication, session/exact/WAV codecs, indexed
reload/edit, family selection and whole-configuration replacement. Codec imports
in the engine are development dependencies only; production dependency boundaries
and file formats are unchanged.

The release CI measurement retains every full guarded `Render::process` duration,
pool/work reading and deadline miss, plus work maxima, callback-end pool extrema,
prepared capacity, aggregate MIDI work,
source SHA and toolchain/runner metadata. It includes stopped capture, two playing
loops, private audio edits, MIDI preparation and publication with old exports alive.
Admitted maximum-density and refused MIDI ticks check truthful recovery. Pool
extrema are sampled after callback maintenance; prepared reservations bound
intra-callback storage separately.
Means, tails and maxima describe that CI workload, not a physical-device guarantee.
[Recorded measurements and functional evidence](../../../../docs/native_session_recovery/evidence/shared-fixed-media.md)
include every observed timing miss and output overflow. Very short callbacks are
not established as deadline-safe, and prepared MIDI state can dominate memory.
Application P04/P05 admission, selection/sync policy and UI remain open.
