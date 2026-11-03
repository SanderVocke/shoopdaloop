# Shared MIDI history, editable takes and persistence

## Status, baseline and delivery

Implementation in progress on `feat/shared-midi-persistence`. Written under
[write_plan.md](.agents/prompts/write_plan.md).

- Baseline: `feat/shared-audio-persistence` at `2f0d1a88`, including the upstream
  merge through `3aabf437`; audio foundation delivery is PR #909.
- Completion target: the MIDI equivalent of the branch's
  [shared audio foundation](src/rust/rt_runtime/docs/reference/shared-audio.md),
  including resource, loop-library and persistence integration.
- Implementation branch: `feat/shared-midi-persistence`. Refresh the integration
  base when work starts. If the audio work has not landed, branch from its current
  head and record that dependency; reconcile again after it lands.
- Final implementation PR target: `feat/backend-replacement`. Keep the final diff
  scoped to MIDI and necessary shared infrastructure. Leave merging to the user.

## Goals and scope

Deliver prepared MIDI history that supports retained overlapping grabs,
independently editable takes, immutable exports, master-compatible interval
replacement, playback through existing loop operations, and loading/saving through
the existing media paths. Support complete, bounded variable-length MIDI packets,
all 16 channels, explicit duration, and correct projected initial state.

This is a library/storage foundation, matching the audio work's black-box scope.
It includes deterministic runtime/loop fixtures and existing full-loop adoption
and configuration-replacement paths. It does not restore application P04/P05
capabilities, global grab controls, musical admission/transition policy, a complete
Replacing recording mode, dry/wet workflows, latency compensation, or new public
host/service commands. It establishes no physical-device deadline or arbitrary
synthesizer-state reconstruction guarantee. Keep capability flags and parity gaps
honest throughout implementation.

## Reference behavior

Use master `92079fb6` as the pinned legacy semantic reference:

- `MidiRingbuffer` retains timestamped input; `MidiPort` tracks current state and
  state carried forward as old events leave the history.
- Ordinary MIDI grab copies/rebases selected events and currently installs them
  with no initial-state argument. Correct state at the selected cut is an explicit
  improvement in this work; the existing grab path is not its state oracle.
- `MidiChannel::process_replace` and `MidiStorage::replace_range` replace events
  in `[start, end)`, preserve outside events and duration, and erase the interval
  when replacement input is empty. Loop wrapping is split into ordered intervals.
  Replacement touching logical zero refreshes the take's initial state.
- Edits change subsequent stream state naturally. An inserted controller value
  persists until changed; deleting a note-off may extend a note. There is no
  restoration of the old suffix state at an edit boundary. Ordinary Replacing
  mode writes without ordinary playback of that channel.
- Legacy snapshot publication keeps replacement work private until committed,
  preserving previous immutable exports.

Reference implementations/tests:
[replacement](https://github.com/SanderVocke/shoopdaloop/blob/92079fb6/src/rust/shoop_engine/src/midi_channel.rs#L1059),
[replacement cases](https://github.com/SanderVocke/shoopdaloop/blob/92079fb6/src/rust/shoop_engine/src/midi_channel.rs#L1700),
[grab](https://github.com/SanderVocke/shoopdaloop/blob/92079fb6/src/rust/shoop_engine/src/app_backend.rs#L5252),
and [snapshot publication](https://github.com/SanderVocke/shoopdaloop/blob/92079fb6/src/rust/shoop_engine/src/content_snapshot/midi.rs#L491).

## Immutable acceptance criteria

These criteria and the goals above require explicit user approval to change.

1. **Retained grabs:** overlapping windows share immutable event payload storage.
   Their events, ordering, duration and state survive later input, history
   eviction, sibling edits and removal of originating resources. Grabbing does
   not copy the whole event window or replay its entire preceding history.
2. **Correct cuts:** a window contains originals in `[start, end)` with rebased
   frame positions and projected state strictly before `start`. Originals exactly
   at the start remain originals at local zero, in input order. Positive empty
   intervals advance time; held state can survive an event-free interval. EOF
   positioning includes all earlier events. Zero-frame operations do not advance
   time or state.
3. **Master-compatible replacement:** replace/erase a fixed-length interval,
   including wrap crossings, without changing outside event bytes/order or adding
   edit-boundary restoration packets. Initial state changes when logical zero is
   overwritten, using input state strictly before the corresponding input portion.
   Subsequent state is computed from the edited prefix. Forks and previously
   acquired exports remain unchanged.
4. **Complete publication:** event root, initial state, duration, revision and
   state index publish together. Every committed revision is immediately usable
   by normal enumeration, bounded seek and rendering. Pending work is explicit;
   an accepted preparation request is not an applied edit. Refusal/cancellation
   before publication preserves committed contents and readers.
5. **Bounded prepared execution:** capture, reads, grabs, edits, index-building
   steps, commit and retirement allocate/free nothing and never block. Each RT
   operation has configured work and storage bounds. No edit/seek/commit performs
   an implicit whole-history replay or suffix rebuild in one callback. Required
   capture/playback and ready commits work with ordinary background allowances
   exhausted; admitted index preparation has its own bounded progress contract.
6. **Existing resource interface:** shared MIDI works through ordinary resource
   IDs, snapshots, read indexes, byte/MIDI cursors, initial-state access and seeks.
   Existing `NodePairSnapshot`, `Adopt`/`Select`, media exports, bulk reads and
   `ConfigurationReplace` accept it. Loaded indexed MIDI, including loops longer
   than the history horizon, can become independently editable without copying
   the whole payload at fork time.
7. **Persistence:** exact-MIDI and session round trips preserve original packet
   bytes, ordering, logical positions, duration and the existing supported
   initial-state/restoration contract. Standard MIDI retains its current conversion
   and loss-reporting contract. No page IDs, edit roots or pending work enter file
   formats. Old exports remain stable during edits, uploads and session replacement.
8. **Black-box execution proof:** real resolver, capture and renderer operations
   produce the independent packet/state oracle across whole, single-frame and
   irregular callback partitions, with identical explicit publication cuts.
   Cover simultaneous events, held notes, pedals, controllers, resets, multi-page
   SysEx, seeks, wraps, stop/resume and output-exhaustion recovery. Include shared
   audio and MIDI captured/published at a common cut and complete teardown.
9. **Capacity and delivery:** configured payload, event, index, state, transaction,
   lease and output limits fail explicitly without partial committed publication.
   Released handles invalidate immediately; bounded maintenance eventually returns
   all credits. Native/Wasm behavior, portable builds, documentation, CI and any
   arriving automated review are verified on the final implementation head.

## Design rules

- Reuse the prepared resource arena, scoped/generation-checked ownership and
  bounded retirement model. Event ordering is `(absolute frame, arrival order)`;
  physical page boundaries never alter MIDI semantics. Use checked frame/count
  arithmetic and avoid periodically rewriting every timestamp.
- Preserve `no_std + alloc` support and the existing runtime/loop/host dependency
  boundaries. Keep file codecs and application policy outside the render layer.
- Keep event payload ownership separate from state-index ownership. Immutable
  event ranges can be reused after a splice; absolute checkpoints depending on an
  edited prefix cannot be reused unchanged. Storage spans must not introduce
  independent entry-state anchors or synthetic cleanup/restoration at joins.
- Use the current `rt_runtime::MidiState` projection and `shoop_loops` restoration
  policy. Preserve complete valid packets within configured limits, including
  SysEx. Retain existing diagnostics for untracked parameter history; do not claim
  reconstruction of arbitrary device/SysEx effects predating the cut.
- A history cut owns its matching state and event view. Tail-state advancement
  must handle time eviction, event-density pressure and callbacks longer than the
  horizon. A gap/overflow must never be advertised as complete retained history.
  Respect the existing distinction between observed input state and successfully
  committed storage; define and test the failure/recovery contract.
- Baseline replacement strategy: build a private event revision, retain valid
  prefix checkpoints, and rebuild affected state checkpoints in bounded steps.
  Retain the old committed head while work is pending; atomically publish only
  when the new revision has a complete index. Total preparation may scale with
  the affected suffix, but per-step work and memory remain bounded and reported.
  Do not disguise this total cost as a touched-page-only edit.
- Preparation readiness and the musical publication cut are separate. Provide
  explicit prepare/progress/ready/commit/cancel states and revision checks as
  needed. Preparation must progress during ongoing capture and playback without
  relying on spare ordinary transfer credits or requiring the engine to stop.
  Do not promise readiness at an arbitrary next callback. Stage M3 must prove
  this contract before live replacement integration proceeds.
- Count state-copy and checkpoint storage costs explicitly; MIDI payload size
  alone is not the memory budget. Bound edit fragmentation, retained bases and
  concurrent transactions, including repeated replacement passes and slow exports.
- Keep existing resource and loop publication authorities. Extend private backing
  dispatch rather than exposing a parallel public media-source or file format.
  Logical byte export must rebase timestamp headers correctly even when reads
  split headers or packets; shared physical bytes remain immutable.

## Implementation stages

Stages are ordered; M3 is a blocking design/verification gate for M4. M5 and M6
must complete before claiming the same finished functionality as the audio work.

### M0 — Establish the implementation baseline

- [x] Refresh upstream/audio status; create `feat/shared-midi-persistence` from
  the required audio foundation and record base/dependency SHAs in this plan.
- [x] Map the acceptance criteria to runtime, loop and codec fixtures. Pin the
  master edit cases and the newer runtime state/restoration rules separately.
  Include grab's initial-state improvement and controller/note-off persistence.
- [x] Record prepared limits and the initial operation/readiness contract.

Verification: baseline focused suites pass; planned behavior and excluded product
work are unambiguous. Commit the baseline/contract milestone.

M0 record: upstream remains `3aabf43789708a5640a8f5b368f737651ef1d29d`;
audio PR #909 is open at `2f0d1a885c97196613f3bd4bf0451610ad0c6b5a`.
The implementation branch starts at that exact audio head. Baseline runtime,
loop and codec package tests pass.

Implementation contract: use immutable event chunks with at most eight events
per checkpoint span, packet-byte blocks backed by the common resource arena,
and a persistent chunk index. Prepared limits separately bound chunks, views,
transactions, pages, incoming event/byte counts and packet size. Long packets
can span physical pages. Shared byte blocks and checkpoint chunks have separate
ownership, so editing metadata does not copy retained packet bodies. History
uses monotonic chunk identities and frame positions; density eviction advances
coverage past an entire discarded frame. A replacement transaction owns its
inputs, builds a private indexed revision in explicit bounded steps, then offers
ready/commit/cancel with expected-revision checks. A single edit spans at most
one loop duration; resolver-cut callbacks can issue successive edits.

Acceptance evidence map: criteria 1–5 and 9 use guarded runtime shared-MIDI
fixtures; criterion 6 adds ordinary snapshot/upload/cursor and loop-configuration
fixtures; criteria 7–8 add codec round trips and resolver-driven mixed audio/MIDI
oracles. Master cases determine raw splice contents; the current MIDI state and
restoration contract determines boundary-state/output expectations.

### M1 — Immutable MIDI pages and retained windows

- [x] Implement prepared complete-packet storage, immutable owners and borrowed
  cursors; keep event payloads shared across clipped/rebased views and forks.
- [x] Give each view matching initial state, duration and revision. Implement
  strict-pre-target positioning using bounded checkpoints, preserving all
  originals at the target and stable equal-frame order.
- [x] Add a small guarded proof with two overlapping windows, a note held across
  the cut, events exactly on boundaries, an event-free span and multi-page SysEx.

Verification: compare packets and state to an independent linear oracle; show no
whole-window payload copy, stale/foreign-handle refusal, retained-reader isolation
and complete credit recovery. Commit the immutable-window milestone.

M1 evidence: the heap-guarded overlapping-cut fixture checks every seek against a
linear packet/state oracle, including held state, simultaneous boundary originals,
empty spans, three-page SysEx, immutable forks/exports and full reclamation. Grabs
and forks leave the physical-page count unchanged.

### M2 — Rolling history and truthful coverage

- [x] Append explicit frame intervals, including empty input, while advancing the
  current projection and retaining sufficient state for later cuts and eviction.
- [x] Add bounded time/event/byte/checkpoint capacity admission and retirement;
  preserve old views through repeated wraps. Define gap/exhaustion recovery.
- [x] Test time eviction and event-pressure eviction separately, arbitrary cuts,
  callbacks longer than the horizon and frame positions crossing the old u32 limit.

Verification: a seeded lifecycle oracle checks coverage, events, state and pool
usage through long runs; retained windows remain unchanged. Exhaust optional
background allowances and demonstrate required capture/read progress. Record
representative payload, metadata and state-memory sizing. Commit this milestone.

M2 evidence: seeded time/density oracles pass across 128 irregular callbacks per
configuration, including callbacks longer than the horizon and crossing u32 time.
A retained window remains byte/state-identical. Required appends and reads succeed
with both ordinary background allowances exhausted. Failure leaves committed
history untouched; explicit `discontinuity` adopts the caller's observed state and
starts a new complete coverage segment. Old history is retired incrementally;
new admission can report capacity until those credits return. For the eight-chunk
fixture, the projection is 15,120 bytes, checkpoint/initial/current state reserves
2,086,560 bytes, and packet reservation is 64,000 bytes. This deliberately
conservative ownership budget favors predictable admission over dense storage;
metadata reporting also includes packet leases and reclamation stacks.

### M3 — Prove edit-index maintenance and publication readiness

- [x] Implement the smallest private splice/index-builder transaction using
  fixed scratch, checkpoint credits and an explicit per-step work quantum.
- [x] Prove that changing an early event invalidates the appropriate later state,
  that retained payload ranges stay shared, and that affected checkpoints are
  rebuilt without a callback-wide suffix scan.
- [x] Define and test admission, progress, cancellation, readiness, stale-revision
  rejection and atomic commit. Old snapshots and committed seeks remain usable
  throughout; no half-indexed revision can escape.

Verification gate: use a long suffix, a changed controller, a deleted note-off,
and simultaneous events at a checkpoint boundary. Assert bounded work per step,
finite progress with reserved preparation work while ordinary background credits
are exhausted, and exact post-commit seek state. Run capture/playback concurrently
with preparation. If the approach fails this gate, revise the representation or
index algorithm and document the evidence; do not weaken acceptance or substitute
state-reset semantics. Commit the proven maintenance contract before M4.

M3 gate passed: guarded long-suffix replacement rebuilds with at most three
consumed records per step while ordinary byte/validation credits are exhausted.
Original seeks/readers and continuing nonempty history capture remain usable.
Preparation changes no committed take; progress allocates no packet pages; ready
commit changes the complete root/state/index/revision together. A changed early
controller persists through the suffix, and deleting a note-off leaves the note
held. Cancellation and competing ready transactions exercise immediate handle
invalidation and stale-revision rejection. Source pins and staged revisions have
separate conservative owner credits; transaction credits return after bounded
retirement. The prototype already handles circular interval ordering; M4 adds the
independent wrap/repeated-pass admission oracle before integrating it further.

### M4 — Independently editable takes and wrapping replacement

- [x] Implement raw event replacement/erase on forked takes, using M3's private
  transaction and complete publication. Retain original and intermediate exports.
- [x] Handle wrap crossings as one logical transaction. When logical zero is
  overwritten, capture input state at that point, including earlier input from
  the same host callback. Keep duration and untouched original events intact.
- [x] Bound concurrent writers, transaction storage and fragmentation; exercise
  repeated passes, empty edits, sparse/dense replacement and all admission failures.

Verification: port master-derived edit cases and compare against an independent
splice-plus-state oracle. Inserting a controller must affect later state; removing
a note-off must not manufacture a compensating event at the edit boundary.
Failure preserves committed root, initial state, index and revision. Commit M4.

M4 evidence: 128 seeded wrapping replacement passes match an independent raw
splice/state oracle after every commit and at every seek position. The fixture
retains original and intermediate exports, covers erasure, full passes, zero work,
pedals/controller resets and initial-state refresh using pre-zero callback input.
Separate guarded admission cases saturate packet, byte, event, chunk, view and
transaction limits; refusals preserve the entire committed revision and pool use.
Fragmentation is bounded by the configured chunk ceiling; affected suffixes pack
eight descriptors per checkpoint and a partial preceding chunk is rebuilt when
needed to avoid accumulating append-only fragments.

### M5 — Ordinary resources, loaded MIDI and lifecycle integration

- [x] Register the prepared MIDI backing in `ResourceStore`; connect ordinary
  snapshots, byte access, MIDI cursors/seeks and initial-state exports. Preserve
  the existing audio and linear MIDI paths and their work accounting.
- [x] Fork production indexed MIDI uploads as immutable bases, including content
  longer than the history horizon. Implement byte-header rebasing and multi-page
  packet reads without eager materialization of the entire take.
- [x] Integrate source removal, superseding uploads, cancellation, fair bounded
  reclamation and stopped cleanup. Update resource requests, documentation and
  tracing inventory where required.

Verification: generic readers consume grabbed/imported/edited sources with odd
chunk sizes, partial headers and EOF. Read old exports after source removal and
full-loop upload replacement; saturate leases/transactions and recover every
credit. No special public playback/export path is required. Commit M5.

M5 evidence: ordinary resource snapshots retain immutable exports; every byte
starting offset is checked with odd transfer sizes, including split/rebased
headers and multi-page packets. Generic MIDI enumeration and every logical seek
agree with the packet/state oracle. Loaded indexed MIDI imports descriptor/index
metadata with explicit bounded progress, retains packet bodies as a linear base,
and remains independently editable beyond the history horizon. Source removal,
superseding uploads, partial-import refusal/cancellation, stale pending resource
commits and three-way bounded reclamation all pass guarded fixtures. Focused
runtime, loop and codec suites pass. The public contract is documented in
`rt_runtime/docs/reference/shared-midi.md`; application parity flags stay unchanged.

### M6 — Loop, mixed-media and persistence black-box proof

- [x] Exercise real resolver/MIDI capture/render operations with two overlapping
  grabs, independent edits, original/intermediate exports and continuing history.
  Compare whole, single-frame and irregular host partitions at matching cuts.
- [x] Exercise `NodePairSnapshot`, `Adopt`/`Select`, playback/export and complete
  `ConfigurationReplace` with both grabbed and imported shared MIDI. Include
  shared audio and MIDI at one cut, with initial state matching those exact roots.
- [x] Round-trip original and edited exports through session and exact-MIDI
  codecs, reload, fork and edit again. Check existing Standard MIDI conversion
  behavior separately. Preserve source originals exactly; use the established
  initial-state representation rather than inventing normalization to hide errors.
- [x] Cover output exhaustion/recovery, stopped/resumed playback, control/upload
  traffic, saturation and complete teardown under heap guards. Capture failures
  must not undo already observed input or authorize callback replay.

Verification: independent packet/state and audio oracles agree on events, timing,
restoration order and revision isolation. Mixed exports stay coherent across
replacement, removal and reload. Commit the end-to-end functionality milestone.

M6 evidence: a guarded resolver fixture compares complete packet/restoration and
audio output against independent linear/splice oracles across whole, one-frame and
irregular callbacks at fixed preparation/publication cuts. It covers overlapping
grabs, delayed edits, ongoing capture, sixteen channels, pedals/controllers,
resets, simultaneous originals, multi-page SysEx, wraps and stop/resume seeks.
Audio/MIDI roots share the same cut; original/intermediate exports survive.
Production node-pair validation, Adopt/Select, MIDI-initial queries, exports and
ConfigurationReplace now run with both loaded and grabbed shared MIDI alongside
shared audio. The graph overflow fixture repeats with shared MIDI and verifies
required audio, downstream reset and media-state advancement. Capture refusal
retains observed input and forces a truthful history segment.

Mixed session and exact-MIDI round trips preserve packet bytes/order/time,
duration and supported initial-state restoration, then reload into indexed bases,
fork and edit again without changing the base. Original/intermediate/final exports
remain pinned through source removal. SMF conversion is checked separately: its
existing quantization warning remains, and its existing conversion drops realtime
system-reset packets; exact/session formats retain those resets. Focused runtime,
loop and codec suites pass, with complete guarded teardown.

### M7 — Final validation, documentation and delivery

Validation routing update: the user requested relying on CI for the checks.
The remaining native/Wasm/browser/bare-metal matrix and final-head validation
therefore run through the PR's existing CI, rather than duplicating them locally.
Local focused runtime/loop/codec suites, warning-denying workspace builds,
formatting, test attributes, tracing, parity and documentation checks have passed.
A local full run on the refreshed base passed 2,570 tests; two existing LAN
management/shutdown fixtures timed out in this environment (four tests skipped).
The attempted Node runner stopped at sandbox read-only tool setup before executing
its suites. Neither result is represented as a passing final gate; CI is authoritative.

CI follow-up: coverage reproduced the LAN management/shutdown timeouts and a
shutdown timeout after recovery stress. Reclamation was scanning all configured
shared-store slots on every turn, including unused slots, across the stopped
engine's page-bound cleanup loop. Registries now reserve their slot capacity at
preparation and expose only prepared entries to maintenance, preserving fair
rotation while avoiding the unused-slot multiplier. Validation follows CI per
the user's instruction.

Integration refresh: upstream advanced to `64a4ad0634de1664b1ff723b36c635cd243d819b`
(click-track generation, PR #923). This branch merges that head, preserving the
new codec-provider click metadata alongside shared-MIDI tests. Audio PR #909
remains open at `2f0d1a88`; its required foundation remains an explicit dependency.


- [x] Document public contracts, readiness/work bounds, measured memory, capacity
  tradeoffs and remaining P04/P05 gaps; keep this plan and parity evidence current.
- [ ] Run focused runtime/loop/codec suites, then the complete Rust workspace
  suite using the repository [build](.agents/info/build.md) and
  [test](.agents/info/test.md) guidance. Run the warning-denying workspace
  build, formatting, test-attribute, tracing, parity, documentation and dependency
  boundary checks. Keep test execution separate from builds that overwrite its
  companion executables.
- [ ] Verify the relevant package Node/browser Wasm suites, production Wasm
  builds, existing mixed-media browser workflows and portable bare-metal targets.
  Use the existing CI matrix for Linux/macOS/Windows and browser coverage; report
  skips, unsupported facilities and evidence scope accurately.
- [ ] Push milestone commits and create the implementation PR targeting
  `feat/backend-replacement`. Reconcile the audio dependency and refreshed upstream
  before final validation. Verify all required checks on the final head are green.
- [ ] Inspect automated reviews after publishing and after fixes. Address
  applicable feedback, repeat validation and continue until arriving automated
  reviewers approve. If none arrive, record that fact without claiming approval.
  Leave the PR unmerged.

Final end-to-end gate: demonstrate M6 through the ordinary resource/loop/media
interfaces on the exact final tree, with complete cleanup, passing final-head CI
and resolved applicable automated review. A standalone storage test, pending
index builder, or manual-only MIDI export adapter is not completion.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
