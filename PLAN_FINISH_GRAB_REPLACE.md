# Finish application Grab and Replacing parity

## Status and delivery

Current execution phase: Phase A implementation and local acceptance are complete.
Phase B is active: the target is reconciled and the final iterative CI correction
loop is running. CI was not inspected or corrected during Phase A.

Status refreshed on 2026-11-05. Work is on `feat/finish-grab-replace`; ready-for-review
[PR #940](https://github.com/SanderVocke/shoopdaloop/pull/940) targets
`feat/backend-replacement`. Phase A implementation/acceptance is committed as
`b53ed797`, following FX/composite reconciliation `e5b87190`. The Phase B target
refresh found `273853aa` (connection drag cancellation) and `bb5b5598` (callback
profiling budgets), now reconciled on top of target `78d39677`.


| Stage | Current status |
| --- | --- |
| F0 | Complete: delivery branch, parent reconciliation, PR and gate inventory. |
| F1 | Complete behavior, ownership and capacity contracts; public production tests and measured limits are linked in the acceptance evidence. |
| F2 | Complete normal host histories, protected global MIDI credits, source/generation lifecycle and default/wide preparation measurements. |
| F3 | Complete continuous fixed/growing writers, long-recording preparation, reserved exits, fault cleanup and render-owned finishes. |
| F4 | Complete negotiated public transactions, atomic settlement, cancellation, adapter ownership and coherent pending/completion projection. |
| F5 | Complete service gesture/exact-list policy, default Grab, solo, lifecycle fencing and persistence/recovery ownership. |
| F6 | Complete normal UI, keyboard and Lua routes, Replacing menu, capabilities and incomplete-capture projection. |
| F7 | Complete: lifecycle, exact file round trips, protected resource pressure, native stack/reset corrections and complete pool/callback measurements pass local acceptance. |
| F8 | Local acceptance complete: packaged native carriers, real JACK, Chromium/Firefox and enabled firmware on both board memory profiles pass. Final CI correction/review and parity closure remain. |

Grab and Replacing are enabled through negotiated application support. P04/P05
remain OPEN until the complete acceptance and final CI gates pass. No Phase A CI
inspection or push occurred.

Execution originally started from fixed-runtime head
`2e19210bdbb5e32510322f8b8344715ce7cbefe5`. Written using
[write_plan.md](.agents/prompts/write_plan.md). This plan finishes **P04 and P05
in the normal application**, including capability advertisement and parity-row
closure. Completing another internal engine demonstration does not finish it.

Authoring checkout: `f9075999e996ca56cfd30ff561cf85626c0df85d`.
[Foundation PR #927](https://github.com/SanderVocke/shoopdaloop/pull/927) is merged
into `feat/fixed-quantum-runtime` at `bfea527b9ed12798c0f05a17606a2c81ad45b5db`.
Its parent [PR #930](https://github.com/SanderVocke/shoopdaloop/pull/930) merged
into `feat/backend-replacement` at `0ef7581b`. This delivery has been rebased onto
that merged result and its new PR retargeted; closed #927 is not reused.

This plan succeeds [PLAN_RECONCILE_GRAB.md](PLAN_RECONCILE_GRAB.md) and
[PLAN_SHARED_MIDI.md](PLAN_SHARED_MIDI.md) for application delivery. Retain their
shared ownership, exact media, fixed-clock and bounded-work contracts. Their green
checks validate the foundation, not completion of this application work.

## Execution record

F0: initially refreshed the parent on 2026-11-04, when PR #930 was open; the merged
foundation and BufferedProcessor delivery (#937) are present in target
`2e19210b`. The buffering layer is additive and leaves the production loop hooks
unchanged; request/effective-time tests will include its declared timing when used.
The new branch preserves the untracked plan from the prior checkout. Draft
[PR #940](https://github.com/SanderVocke/shoopdaloop/pull/940) initially targeted
the refreshed fixed branch. [The application contract](docs/native_session_recovery/grab-replace.md)
records the gate inventory and reference behavior. A checked portable musical
window resolver pins the cycle/phase, prefix-continuation, explicit padding and
u64 range semantics before engine settlement uses them. It does not enable a
capability or publish resources. F1 remains open until production routes and the
capacity/admission policy are verified. Initially builds and tests ran only in CI.
The user's 2026-11-05 testing-strategy update supersedes that routing: use limited,
targeted native checks during development. The later 2026-11-05 strategy update
below defers all CI inspection/correction until implementation and local acceptance
are complete. P04/P05 remain OPEN during implementation.

The production sizing investigation exposed checkpoint arrays multiplied by view
slots that the page budget could never admit. MIDI preparation now estimates its
complete layout without allocation and sizes chunk/index tables by reachable
owner credits. A pressure scenario retains all credits through repeated live
history wraps, then prepares an edit and verifies the immutable old export.
This preserves conservative admission and live workspace; it is an F2 sizing
increment, not resolution of global pooling or the continuous-writer design.

The first fresh focused CI exposed duplicate test global allocators where the
new parent BufferedProcessor fixture and shared-media fixture were combined.
The engine test allocator is now defined once at crate scope so both sets retain
their heap guards. This repairs a target-integration compile failure; fresh CI
must confirm it before either milestone is called validated.

Audio grabs now clip persistent roots at the requested page boundaries, sharing
interior branches without copying samples. Owner credits cover the visible pages,
including partial boundaries, rather than pinning the whole history ring. A
heap-guarded scenario admits several short users within less than one additional
ring's budget, edits them independently across wrap, retains an old export and
keeps live capture running at the owner limit. Production global metadata limits
and writer integration are still open.

Audio index preparation is now bounded by retained page credits and per-owner
boundary paths rather than a full history tree per view. A 30-second stereo
512-view sizing scenario asserts a sub-8-MiB metadata budget and acquires/releases
every handle without a sample copy. CI must validate that assertion and the
updated lifetime tests; ordinary loop-memory capacity is unchanged.

MIDI grabs now clip chunk roots and charge their covering chunks rather than a
whole source history per owner. Private edits reserve growth plus a source pin,
trim shrinking results before publication, and transfer/refund the correct credits
through retirement. Metadata follows the total retained-chunk budget; unreachable
handle/initial-state slots are not allocated. New heap-guarded packet/state oracles
check growth, full erasure, old exports and exact credit recovery. These sizing
changes retain the library checkpoint representation and do not implement the
continuous MIDI writer or enable application capabilities.

Musical gesture settlement now has a checked portable policy resolver as well as
window arithmetic: authoritative target cycle/period, apply-N clamping, synced
post-mode, unsynced prefix Recording and completion delay (including pending
target transitions). Unit scenarios cover both play-after-record settings,
non-integral target periods, absent sync and overflowing musical intent. Service
target selection and actual RT settlement still need wiring. A reference audit
also found that legacy Control Grab expands selected targets while raw
AdoptRingbuffers uses the exact list; the initial reference description conflated
them. After discussing the example, the user identified selection-dependent
explicit scripts as undesirable. Delivery keeps the plan's exact-list behavior
for Control/Lua Grab as an intentional correction; UI gestures stay selection-aware.

The MIDI projection representation now packs per-note flags and unknown-pressure
state and uses the existing controller-known mask for byte values. All public
getters and packet semantics are preserved. A portable size gate bounds the full
projection to 9 KiB (the prior native shape was 15,120 bytes), with edge-state and
canonical-reset tests alongside the existing independent pedal/state suites.
This reduces ordinary as well as shared prepared checkpoint arrays without
lowering an event/byte limit or seek bound. Global production limits and the
continuous-writer mechanism remain unresolved and no application flag is enabled.

Fresh Linux coverage on `e63acb0f` exposed two test failures: the new pressure
fixture used the default maintenance budget against a one-step drain helper, and
root clipping added work to the final packet-preparation call. The fixture now
prepares the one-step budget it asserts. MIDI edit readiness now includes a
separate bounded index-finalization step; tests retain the packet-step limit and
separately assert no record/state replay and logarithmic index/reclamation work
in finalization. The remaining suite reached these cases after compilation; no
full green application run is claimed.

The continuous-writer investigation now has a concrete
[semantic-index design note](docs/native_session_recovery/midi-writer-index.md).
It addresses the suffix dependency using ordered category queries and fixed-size
pedal transfer summaries, retaining COW bodies/roots and complete-cut publication.
The initial heap-guarded summary tests compare composition with protocol note
release and unchanged sostenuto suffixes after edits. This is a de-risking proof
for one part of F3, not a completed writer, production density policy or state
index. The complete projection/query and ownership tests remain required.

Complete projection reconstruction is now implemented against an ordered-category
query interface, using the pedal summaries for release/latch intervals. New
heap-guarded scenarios compare every opaque channel field with linear packet
application across initial latches, program-bank snapshots, resets/channel modes,
equal-frame order above u32, erasure/wrap and changed logical-zero state. The test
timeline is deliberately linear; the bounded persistent production store and
callback work accounting are not implemented by that oracle. CI must validate
the semantic reconstruction before the writer uses it.

The audio kernel now prepares multiple independent source histories in one shared
page/index/owner pool. Scoped source handles preserve separate coverage and reset
lifetimes; ordinary resource/snapshot interfaces retain the resulting takes. New
tests saturate global owner and unrelated upload credits while both sources keep
capturing, reset one source, verify old/current exports and recover all credits.
Allocation-free sizing covers sixteen 30-second stereo histories and 512 views
with one retained-page budget, plus refusal of the largest combined shape beyond
the portable arena limit. This is kernel pooling only; normal host policy and the
loop coordinator still need integration and production measurement.

Focused shared-media CI passed on `beb092e498fd3b6340098013bd21eecab956873a`:
[run 41925068783](https://github.com/SanderVocke/shoopdaloop/actions/runs/41925068783),
job 126908066169. That head includes the compact projection, clipped ownership,
separate MIDI finalization and interval-summary tests. Full matrix jobs were still
running at inspection. It does not validate the later complete semantic query or
multi-source audio pool, and is not application acceptance evidence.

The loop coordinator can now install a common audio pool while keeping source
slots, complete-cut coverage, generation fences and resets independent. Shared
capacity reporting and shutdown account for that pool once. New engine scenarios
capture two differently scaled input routes into ordinary resources at the same
fixed cut for Q=1/32/128 and partial/whole host callbacks, then reclaim all owners.
Existing explicit per-source fixtures retain their preparation policy. The normal
host constructor is still unchanged pending production MIDI/writer integration.

A prepared persistent AVL index now implements the ordered-tree layer for the
semantic MIDI representation. It copies edited paths, retains snapshot roots,
preflights metadata, composes interval summaries and retires one node per granted
maintenance step. Value retain/release hooks support shared immutable packet
records. Heap-guarded scenarios check balance, every root/value reference, old
versions through edits, logarithmic range work, pressure and complete reclamation.
Sostenuto summaries can keep 32-byte compact record-marker tables and resolve
only query-result markers. This tree layer still needs CI and integration into
the complete category store, payload storage and live writer.

The continuous MIDI core now stages each input packet once, shares immutable
bodies between users and commits a complete semantic root after each successful
fixed or growing write. Whole-tick metadata preflight precedes root changes;
positive empty ticks erase originals, logical-zero state follows the input cut,
and exit consumes the complete view without suffix repair. Bounded admission
checks existing quantum density/bytes. Two focused Q=32 native tests cover
48 ticks/twelve wraps, two replacing users plus a growing prefix, full/equal-time
packets, old/intermediate exports, complete state, pressure refusal and exit
without write space. Required index work is accounted. All sixteen focused
writer/packet/index tests pass with warnings denied. Source-history/coordinator
publication, reserved production credits and the application stack remain open;
these core tests do not finish F3 or enable P04/P05.

The first targeted native pass after adopting this strategy passed four packet
slab tests and ten complete-query/tree/category tests, with warnings denied.
Tracing inventory and test-attribute checks passed. It includes the type-annotation
fix for the category oracle found by CI on `2707c466`; fresh broad CI is pending.
These are library ownership/state proofs, not application acceptance. The new
packed packet bodies/records are not yet connected to a live writer or host.

The shared MIDI core now also captures independent rolling sources, advances
truthful density/time coverage with strict-before-start projection, and retains
immutable grabbed windows across source reset. Packet-root clipping and lazy
category clipping preserve explicit empty padding through later continuation.
One representative native scenario covers two sources, padded grab, continuation
beyond the horizon and source reset; the existing tree ownership oracle now
includes clipped snapshots. All seventeen focused native writer/packet/index
tests pass. Ordinary resource access, production quota guarantees, cross-media
publication and application integration remain open.

The latest implementation adds immutable uploaded MIDI
bases: semantic records pin existing packet ranges rather than copying loaded
payloads into history pages. Byte-offset tree queries support ordinary serialized
reads and seek offsets. The existing continuous-writer oracle now starts from a
real uploaded resource, removes its original owner, and checks old/current bytes
and state through repeated writes. All three focused writer/history tests pass
with warnings denied on this working tree. Broader CI is pending; resource/host
integration and P04/P05 application completion are still outstanding.

The continuous MIDI pool is now exposed through ordinary resource and snapshot
leases. Bounded imports retain uploaded packet bases, source ticks can feed
multiple private fixed/growing writers, and exit publishes a complete resource
revision without acquiring a new view, snapshot or version credit. Generic byte
cursors and MIDI seek read the retained semantic version. Source resets, duplicate
or skipped ticks, competing uploads and resource generations fence stale jobs;
imports and writers have explicit cancellation. Allocation-free pool sizing covers
the prepared owners, packet/index arrays and staging buffers. Three new guarded
resource scenarios cover repeated wraps, simultaneous writers, continuation beyond
the history horizon, uploaded SysEx/equal-frame packets, old exports, exhausted
view/transfer capacity at exit, reset, competing uploads and exact teardown credit
recovery. All 54 native resource tests and 16 loop-library unit tests pass with warnings
denied, as do the affected library build, formatting, tracing inventory and test
attribute checks. Fresh broad CI remains pending.
This is the resource boundary for F2/F3, not production quota reservation, loop
mode/coordinator integration, or application acceptance. P04/P05 remain OPEN.

Audio COW views now support a separately reserved growing tail after a retained
prefix, including prefixes that cross the physical history-ring boundary. The
tail uses disjoint linear page coordinates, so continuation can exceed the source
horizon without sample relocation or overwriting the prefix. Empty takes and
immutable uploaded PCM bases also grow. Ordinary resource/snapshot descriptions
track complete lengths/revisions, and growth completion refunds unused credits
without acquiring exit resources. Fixed replacement of a continued take preserves
its period across the prefix, ring and loop boundaries. Preparation accounts for
three retained intervals and additional transaction scratch. Pressure fixtures
reserve the extra scratch explicitly while preserving their owner-limit assertions.
The guarded stereo resource oracle covers continuation, subsequent repeated
replacement, old/current exports, zero-length startup, uploaded bases, capacity
refusal and exact teardown. All 55 resource tests passed; the extended loaded-base
case and warning-denying build, formatting, tracing and attribute checks also pass.
Engine mode/coordinator integration, global production preparation/reservations,
protocol, service and application delivery remain outstanding.

The target advanced during this delivery through profiling, P12 timing and Zynq
host support. Reconciliation with `a068ce72` retains both tracing inventories and
adds the new parameter-history query to indexed MIDI resources. The query uses
the complete semantic state and preserves strict-before-frame behavior. Its
resource scenario distinguishes selector/data history at frames zero and one.
The PR was observed unmergeable before this reconciliation; no fresh workflow run
was available for the earlier pushed milestone. Combined-tree validation passed 76 runtime, 17 loop and 27 engine unit tests
with warnings denied; the CI release measurement was intentionally ignored locally.
A missing tracing-inventory entry for the new FreeRTOS build script is reconciled.
Fresh broad CI and application acceptance remain pending.

The loop coordinator can now use one indexed MIDI pool for every configured MIDI
source, alongside its shared audio pool. It observes stopped input once, fences
source resets/routing generations, intersects cross-media coverage, grabs ordinary
resources at a completed cut and accounts/cleans up the common pool once. A new
Q=32/irregular-callback engine scenario captures two independent sources, checks
mixed packets/SysEx against generated input, adopts the take through ordinary
media/playback validation, retains old exports and reclaims every source/owner.
The 22-test engine shared-media group and three indexed-resource tests pass with
warnings denied; the CI-only release measurement is intentionally ignored locally.
Affected builds, formatting and test-attribute checks pass. This does not yet
install production history defaults or implement loop writer modes. Indexed
continuous media intentionally cannot enter the checkpoint-based private-patch
candidate API; continuous writer integration is a separate path.

CI on `f95c37a559030a6c7f98cebdf1748247952a1886` passed the fixed shared media
foundation job (including release callback measurements) in run `41999646740`.
Other platform jobs were running/queued at inspection. That job validates the
preceding resource/audio milestones, not this later coordinator integration or
application parity.

F1 now has a complete compact behavior-to-test matrix in the application
contract, with stable G/R/X case IDs. It covers explicit versus gesture/default
membership, sync/target/apply-N/solo, warmup/padding/gaps, trigger tracks,
monitoring, writer-to-writer transitions, pressure, persistence and reconnect.
Reference expectations and existing lower-layer proofs are distinguished from
still-pending public-route tests. Only the matrix extraction item is checked off;
F1 production limits and policy-test porting remain open.

Indexed source preparation now includes an explicit serialized-byte limit as well
as the event/time limits. Aggregate byte/event rank queries select a truthful
whole-tick eviction cut. Small evictions retain bounded packet-state application;
large byte evictions clip the persistent prefix and reconstruct its complete
boundary projection without replaying the discarded suffix. Repeated clipped
source windows stay narrow. A guarded SysEx pressure case evicts more than the
old two-tick removal buffer, preserves sustain/sostenuto state and old/current
exports through repeated bulk and small evictions, then recovers every credit.
Idle indexed sources now avoid repeated full projection copies; a guarded
silent-tick/writer-zero case verifies both required work and pre-input state.
Five indexed resource tests, ten neighboring semantic index tests and the mixed
coordinator scenario pass with warnings denied, as do affected builds, formatting,
tracing and test-attribute checks. Global admission reservations, normal host
preparation and application delivery are not yet established by these results.

Raw musical lookback windows can now be resolved at a current completed
coordinator cut and returned as paired ordinary resources with explicit requested
duration, destination position and padding. Audio represents the empty tail
without allocating/copying silence; its sparse reads, later growth and wrapping
edits preserve declared zeros and old exports. Indexed MIDI uses its clipped
window and retained boundary state over the same duration. Empty unsynced prefixes
are supported. Unknown/gapped source ranges and noncurrent coverage are refused,
not padded. The checkpoint fixture path retains its explicit padding refusal.
A guarded resource oracle checks silence, edits, continuation and gap refusal;
an engine oracle checks a short mixed history against exact input packets/samples,
excludes later source data from padding, adopts/exports ordinary media and checks
empty prefix acquisition and cleanup. This raw helper does not publish targets,
settle service musical policy or enable an application capability; F4 still needs
RT-authoritative batch intent and owned completion/publication resources.
All 59 native resource tests and 23 engine shared-media tests pass with warnings
denied (one CI release measurement intentionally ignored), along with the affected
builds, formatting, tracing, test-attribute and parity checks.

Publication reservations now own the final generic snapshot slot and a future
shared backing view before a writer enters. Growing audio reserves the maximum
future view credits as well as the initial immutable pin; MIDI reserves an empty
semantic owner whose lease is retargeted to the complete final root. Publication
uses no new snapshot/view/version/payload allocation and consumes no optional
transfer/maintenance credits. Version/resource/generation fences preserve stale
reservations for explicit cancellation, and generic snapshot release also reclaims
abandoned future owners. A guarded paired audio/MIDI scenario fills every generic
snapshot/version/resource slot and both backing view pools before capturing and
publishing; old exports retain exact bytes and all credits recover. A companion
scenario covers cancellation, foreign scope, competing upload and resource removal.
These are exit-resource primitives for the remaining coordinator writer lifecycle,
not a completed Replacing mode or application feature.

Clipped semantic MIDI views now materialize hidden category bounds through the
bounded retirement scheduler. Shared leases preserve identical packet/state
queries while out-of-window packet bodies become reclaimable. Source and writer
ticks receive explicitly configured required cleanup steps, independently of
optional transfer credits; pool usage exposes remaining packet/body and index
ownership. New guarded scenarios retain a short view and peer, reset capture and
recover all hidden packets while silent live input continues with optional budgets
at zero. The ten index tests, four writer tests, two publication reservation tests
and focused live-cleanup test pass locally with warnings denied; affected library
builds, formatting, tracing and test-attribute checks pass. This closes a retained
window leak risk before production sizing; global admission and normal app routing
remain open. CI is deferred under Phase A.

The coordinator now owns continuous paired writers, prepared before resolver entry
and finalized through reserved resource/snapshot/media owners. A staged audio root
commits only after the MIDI tick succeeds, so a failed paired tick keeps the prior
complete cut. Fixed Replacing advances/wraps without loop playback; prefix Recording
grows beyond live history. Stop/Playing publish together, active exports retain
complete immutable views, and growing Recording can become Replacing at the exact
boundary without another suffix admission scan. Density already verified by the
same MIDI pool travels with immutable media; loaded imports still undergo bounded
pre-entry checking. Source reset fences future input but cannot prevent publication
of the last complete private MIDI cut. Failed growing ticks cannot inflate the
reported complete capture length.

Local guarded scenarios prove many wraps, prefix growth, active/old exports,
Recording-to-Replacing, scheduled exits, a long loaded base with more events than
one preparation allowance, refusal before readiness, and exit after one tick.
Source-reset coverage exposed a graph-operation ordering bug: source notifications
now accept the just-activated epoch as well as its prepared predecessor. The
padding scenario explicitly warms its history after configuration now that resets
are actually delivered; exact padding/sample/packet assertions are retained.
Protocol version 12 can represent Replacing in confirmed telemetry; application
capabilities and backend entry remain disabled. The scalar single-frame oracle
now includes both new transitions in seeded partition comparisons.

All 64 resource tests, 18 loop-library unit tests, affected scalar/capture/interloop
and control integration groups, protocol codec tests and the affected shared-media
scenarios pass locally with warnings denied. The opt-in release measurement remains
unrun; it is not acceptance evidence for these changes. Affected builds, formatting,
tracing inventory and test-attribute checks pass. This advances F3 without closing
it: ordinary Recording-to-Replacing, non-default channel windows, globally reserved
live quotas and normal application routes remain. CI stays deferred in Phase A.

Normal engine preparation now builds one shared audio/indexed MIDI history pool
for the configured source slots. Host setup version 7 and worklet configuration
version 4 carry retention, shared audio pages, MIDI pages/records and writer slots.
The existing ordinary loop-memory allowance is unchanged: preparation adds the
exact shared reservation to the arena. An allocation-free public sizing API reports
retention, payload/metadata, source quotas and the portable arena refusal before
allocation. At application defaults (48 kHz, sixteen stereo sources, 128 loops,
256 MiB ordinary media), the history rings reserve 256 MiB, shared audio owners
128 MiB plus 20 KiB transaction scratch, and MIDI payload 16 MiB. Density limits
can shorten actual MIDI coverage; the default per-source quotas are 2,048 events
and 512 KiB serialized bytes. These are explicit storage limits, not a claim that
all simultaneous writers can exhaust every density limit indefinitely.

The normal-constructor guarded scenario captures stopped inputs, verifies real
samples/packets, retains thirty seconds at the configured rate, wraps with an old
export held and recovers the complete expanded arena. Dynamic source removal
resets its history through bounded retirement; slot reuse cannot retarget old
exports or generation handles. Required idle MIDI maintenance continues after
sources disappear, independently of optional transfers. Local checks pass for all
29 shared-media scenarios (one opt-in measurement ignored), engine control and
structure, the eight native dummy-host cases, host/worklet configuration codecs,
and backend contracts through real socket and shared-memory child processes.
Affected warning-denying builds, formatting, tracing and test-attribute checks
pass. Global per-owner indexed MIDI credit guarantees and actual default callback
measurements remain open; F2 and application P04/P05 are not complete. CI remains
deferred until the complete local implementation and acceptance pass.

Fixed Replacing now maps each channel into its retained raw window while keeping
the manual logical period. Audio stages one atomic root across independent channel
spans; MIDI supports a clipped, possibly offset raw interval for each complete
source tick, including a raw duration between quantum boundaries. Outside samples,
packets, gains/preplay and old exports are preserved. Raw-zero MIDI projection
includes source packets strictly before the mapped input offset. An offset bucket
can combine two admitted input quanta; a bounded density preflight refuses excess
before mutation and retains the preceding complete cut. Windowed audio preparation
reserves the larger disjoint-channel transaction shape explicitly; stereo default
workspace is now 32 KiB (eight pages), preserving the same 128 MiB owner allowance.
The public sizing API includes this change.

A guarded normal-constructor scenario exercises independent positive/negative
channel offsets, a shorter manual period, repeated wraps, phase-preserving entry,
suppressed loop playback, natural MIDI state and exact raw tails. A resource oracle
covers non-quantized raw MIDI duration, SysEx/order, partial input, raw-zero state,
density refusal and reserved exit without optional credits. Malformed preparation
also returns every owner. Replacing-to-Playing now preserves cursor/cycles when not
retriggered, and entry at zero emits the reference clock pulse; the scalar seeded
oracle and fixed-domain test cover these mode edges. All 65 resource tests, affected
loop/runtime/shared-audio and engine shared/timing groups pass locally, as do the
affected builds, formatting, tracing and test-attribute checks. This advances F3;
ordinary long Recording-to-Replacing preparation, global live MIDI reservations
and application batch/service/UI delivery remain outstanding. CI stays deferred.

2026-11-05 reconciliation and application integration in progress: the merge of
`78d39677` is resolved locally and preserved WIP is retained in stash
`1e70974f`. Protocol 22 / host setup 8 / worklet configuration 7 and Replacing
mode tag 5 coexist with target FX/composite contracts. Shared takes now preserve
primary/auxiliary/MIDI role indices, metadata-only wet-edit snapshot reservations
coexist with writer backing reservations, and normal processor startup installs
histories without taking the wet-edit budget. Edit-only audio scratch now covers
blocks wider than its absent history ring. The wet refusal fixture budgets retained
pages independently from window scratch, reflecting clipped owner accounting.

Automatic gesture finishes now belong to resolver requests, use a candidate pending
clock query and become eligible at A+Q. Pending mode/delay publish with the same
checked LoopSnapshot as confirmed state. Explicit clock requests can settle across
unrelated topology components through bounded candidate retries. Local guarded
oracles cover prefix/sample/packet continuity, both finish modes without controller
polling, and rollback during active composite playback including children, cursors,
pulses and existing pending clocks.

The service now routes button/default/control Grab and explicit/triggered Replacing
through one adapter-owned input mutation, with all-target preflight and atomic
solo peers. Control Grab preserves its explicit list; UI selection expansion follows
the reference policy. Negotiated service capabilities expose implemented routes.
Media/save/import/family/session guards include Replacing; default-Grab settings
survive application-image and file conversion. A real service-wire scenario passed
selected Grab, exact lists, stale-member refusal without solo effects, Replacing,
save/load and another edit. The combined processor-host test passed direct Grab and
Replacing while a composite runs, rejected a mixed FX input list atomically and
retained hosted state and role-aware archives. These results are local; P04/P05
remain OPEN pending the remaining acceptance gates.

Remaining work includes final indexed MIDI owner/workspace admission and maintenance,
combined default MIDI output/processor memory and callback measurements, additional
UI/keyboard/Lua and recovery acceptance, packaged native/browser execution, then the
final-only CI/review correction loop. No CI inspection or push has occurred during
this reconciliation.

The combined tree now builds with warnings denied across the workspace, including
native FX/reference features. Local core verification passes 90 runtime, loop
resolver/runtime and 52 engine unit scenarios (one opt-in measurement is separate).
CPAL callback and device-fault subprocess shutdown checks pass after stopping idle
reclamation promptly; all real JACK scenarios have passed locally across the focused
runs with a dummy JACK server. No backend-unavailable skip is counted as execution.
The Replacing context-menu action and unsupported-capability menu tests pass.

Global MIDI admission now protects live source quotas and edit workspace before
charging retained owners. Ownership includes lazily retained outside-window data;
credits remain charged through deferred retirement and shrink only after bounds are
materialized. Required maintenance scales with configured event/path work. Page-run
summaries account for sparse interleaved SysEx slabs, and source density eviction
reports actual coverage. The small resource fixture now reserves 256 category pages for its protected
live sources, workspace and owners. Its 1,024-byte source quota still exceeds the
896 serialized bytes possible over its entire four-tick horizon; record and payload
page counts remain unchanged. A new allocator-guarded test reproduces the former tick-36
starvation, then proves 200 dense ticks on two sources with held immutable exports,
explicit owner refusals, zero optional maintenance and complete recovery. The
existing integration fixture budgets and functional assertions are preserved.
Production index sizing now adds explicit live-category and source-edit workspace
reservations to the existing owner arrays; ordinary loop payload and the configured
MIDI record/page budgets are unchanged. Metadata remains under its existing gate.

A full normal engine/host-buffer measurement at 16 sources, 128 loop slots, stereo,
Q=32 and 256 events per source tick prepares 3,157,467,205 requested bytes at peak
in a debug build, and recovers every page after two simultaneous continuous writers.
All raw callback timings and work counts are retained in the local measurement log;
this deliberately extreme debug run misses every device-time budget. Release and
representative-density measurements, largest channel shape, complete hosted-bank
accounting and packaged acceptance remain required. No timing certification is
inferred from a debug or CI run.

The final application acceptance adds packaged companion/socket/shared-memory and
Chromium input canaries. The socket canary uses actual default preparation, drops
the UI/Lua VM during growing capture, reattaches to the render-owned finish, then
round-trips exact stereo/MIDI files and edits loaded owners again. The browser
canary stalls its main thread during the finish and continues the existing hosted
FX, bus, timing and composite workflows. Shared-memory reattachment remains the
previously accepted recovery exclusion, while its editing/file sequence is covered.

F7 corrections now cancel owned finish records when capture aborts/exhausts,
cover the target's pending clock delay, and distinguish musical startup padding
from density eviction/faults. Explicit raw adoption preserves its intentional
requested padding. Finishing MIDI writers gives back future tick reservations;
physical page-run boundary slack remains charged until actual retirement. The
sustained protected-live-credit oracle includes packed events, fragmented SysEx,
finished immutable writers and zero optional maintenance.

Complete release preparation and callback distributions are recorded in
[application evidence](docs/native_session_recovery/evidence/grab-replace.md) and
[raw measurements](docs/native_session_recovery/evidence/grab-replace-measurements.json).
The default 16-source/128-loop stereo host with optional FX banks peaks at
3,458,138,693 bytes; eight channels with explicitly configured 512-MiB processor
budget peaks at 4,080,206,525 bytes. The unchanged 256-MiB processor default refuses
that wider processor shape. Extreme 256-events-per-source/Q32 input is finite and
exact but misses every wall-clock deadline here; all tails/misses are preserved.
The representative 256-frame/eight-event measurements also retain their misses
under concurrent build/test load. This is not hardware deadline certification.

Full regression exposed native Q1 output queues whose whole-period sizing exceeded
the portable allocation bound. Dummy/JACK now select a smaller deliverable render
block when MIDI layout preflight requires it, preserving the physical callback
period, control lead, input frames and complete event capacities. Dummy explicitly
renders every sub-block. Host integration cases share a bounded nextest group so
multiple default prepared hosts and JACK graph mutation tests do not contend with
each other. Original failures and corrected local acceptance are recorded in the
final evidence rather than being labelled green retroactively.

A default-pool JACK canary additionally found callback-stack exhaustion during
Grab. The debug backtrace showed the general-operation dispatch closure retained
above the MIDI source query. Input/shared operations now bypass that frame,
gesture preparation no longer recurses, and source Grab reuses its complete media
value. The complete default JACK sequence passes on the existing 512-KiB callback
stack with no increased storage or stack setting. The strict JACK acceptance
script now runs this exact packaged workflow; extracted native archives also run
the input canary over socket and shared memory.

Phase B refreshed the target to `273853aa0b3b433d02c0373ec646925b96443681`.
Its profiling change adds serialized wrapper budget statistics; its connection
change caches semantic connection views so unrelated loop/meter updates do not
cancel a drag. Preserve both alongside input mode/capture projection. Both branches
independently used application protocol30 for different schema additions, so the
combined schema is version31 with updated canonical codec bytes. RT protocol23,
host setup8 and worklet configuration7 remain unchanged. Affected verification
covers profiling/wrapping, service connection identity, public input workflows,
codec/client projection and UI drag/graph behavior before the first Phase B push.

The first final-source CI round (Build/Test run42085353425, head `2ec4abcb`)
found four correction causes. The release shared-media matrix counted two idle
live-queue scans in addition to its sixteen optional candidate scans. A tracked
preparing-live-job count now avoids scanning an empty admission queue; the
original eight-edit/sixteen-scan bounds remain unchanged. All twenty Q/partition
measurement combinations and 39 focused shared-engine tests pass locally.
The packaged input canary now loads the external keyboard catalog rather than
compiling an application Lua payload. The raw JS worklet fixture now encodes
configuration7's five history fields and RT protocol23; its profiling/record/MIDI
assertions and all five trace transport cases pass unchanged. The strict JACK
launcher resolves Cargo's actual target directory so coverage instrumentation
runs its newly built executable instead of a nonexistent default-target binary.
These corrections do not waive any CI gates. Fresh source/base CI remains required.

The next CI round exposed firmware memory regressions. The initial correction
hard-disabled firmware histories; the user clarified that a configuration policy
may disable a feature, but every target must have a working way to enable it.
The firmware now consumes explicit engine pool/history settings, with a checked
`--engine-config` build path and resolved policy in the image manifest. Its shipped
policy can retain the existing eight-track/thirty-two-loop allocation; the enabled
example selects three tracks/twenty loops, thirty-second history and explicit
shared audio/MIDI/writer reservations within the same board arenas. Core input
preparation and capability negotiation are shared across targets. No target-specific
feature exclusion is permitted.

Core preparation also stops reserving processor-send, global-controller and dry-
playback MIDI banks when neither prepared processors nor external dry/wet support
can use them. Hosts with those features retain complete original capacities.
Default-policy QEMU recovery passes upload-loss/reattachment/exact download and
ownerless shutdown at 355,649,560 peak bytes with zero Rust owners after teardown.
Default-policy ZedBoard boot/restart passes at 248,796,872 peak bytes with the same
zero-owner result. Enabled-policy acceptance now passes real Grab, two synchronized Replacing
writers with scheduled exit, detached prefix completion, reload/re-edit and cleanup
failure/retry on both board memory profiles. Peaks are 335,593,044 bytes on the
1 GiB profile and 233,298,772 on ZedBoard; both return Rust heap usage to zero.
The [firmware acceptance record](docs/native_session_recovery/evidence/grab-replace-firmware.json)
binds resolved policies, board contracts and memory/stack evidence to source hashes.
The default eight-track/thirty-two-loop disabled policy also passes fresh boot,
restart, unavailable-capability negotiation and zero-owner shutdown on both profiles.

This acceptance exposed a shared repeat-sync bug: a Replacing loop tried to
advance beyond its completed period while waiting for the sync source. The fixed
writer now remains admitted at the end position, pauses audio edits and observes
zero-width MIDI ticks to preserve its input-frame fence. The next sync pulse
resumes the same owners at zero. The new production-engine regression compares
all audio samples, MIDI packets and retained exports for two simultaneous writers
through the wait and subsequent wraps; it failed before the correction.
All 361 affected loop/engine tests pass (the existing opt-in release measurement
is excluded from this debug run). The warnings-denied workspace build, attributes,
formatting, tracing, parity and dependency checks pass. Freshly staged native
socket/shared-memory input canaries also pass on this corrected tree.

The prior Build/Test round at `ed179738` passed native/platform checks but Firefox
waited for an input-stall signal its harness never generated. Firefox now induces
the same two-second UI stall as Chromium and verifies the input test reaches
`Passed`. Its complete local workflow against the untouched packaged web release
passes at Q128/44.1 kHz with zero xruns. This harness correction and the firmware
policy/sync fixes still require fresh final-source CI.

The next main matrix (`efcda1ac`, Build/Test run42118770631) exposed a macOS
composite-canary setup error. Its fixed 600 ms recording sleep produced a sync
source of 17,408 frames, exactly the frozen arrangement's 17,408-frame period.
After phase alignment no repeat-wait interval existed for recovery to observe.
The probe now waits until actual recorded source frames exceed twice the frozen
arrangement period and confirms the stopped source before proceeding. The original
repeat-wait, source-release, accepted-edit and recovery assertions remain intact.
Local socket/shared-memory canaries and repeated single-CPU socket runs validate
the correction; it changes probe setup, not production composite semantics.
Fresh CI is required for this final correction.

## Findings that determine the work

The semantic index now has a shared category/version store with compact record
IDs, sparse category pages and preflight across all affected indexes before any
mutation. Immutable versions retain one handle; edits fork shared directories,
category pages and ordered paths. Tests compare full projection and original
packet order through repeated edits and retained versions, exercise each
metadata limit before mutation, and recover every prepared owner. Payload slabs,
continuous paired writers and application integration are complete; final evidence
and CI pins are recorded in the delivery section and application acceptance report.

On 2026-11-05, the parent #930 was observed merged at `0ef7581b` into
`feat/backend-replacement`. The delivery branch is rebased onto that
actual merged head and PR #940 now targets `feat/backend-replacement`. CI on `b6ac9443` exposed duplicate test
allocators in GitHub's combination with the updated parent; consolidation removes
the duplicate before fresh validation. Focused shared-media and native jobs on
`0d2f525f` passed, validating the complete query and pooled audio kernel but not
the later tree/category/coordinator increments.

| Layer | Delivered support |
| --- | --- |
| `rt_runtime` / `shoop_loops` | Shared audio/MIDI histories, immutable grabs, indexed imports, continuous fixed/growing paired writers, ordinary Recording entry and protected global live admission. One-shot CandidateMidi remains a separate edit operation. |
| Production preparation | Common thirty-second source pools with a separate arena allowance, matching native/worklet settings, protected live MIDI credits and measured default/wide callback and memory costs. |
| Engine protocol / adapter | Negotiated RT protocol23 input batches, retained outcomes, cancellation, authoritative cuts and adapter-owned mutation settlement/cache invalidation. |
| Service / clients | Normal Grab, AdoptRingbuffers, selected GrabClicked/default Grab and exact-list Lua/Control. Primitive Replacing is discoverable in the UI; active/scheduled writer guards and incomplete-capture state are projected consistently. |
| Documentation | Application behavior, exact media/state oracles, measured costs and final validation are linked in the acceptance report; P04/P05 closure follows the final CI gates. |

Reference behavior is preserved in `shoop_app_reference`, the test-only legacy
backend and `shoop_engine`. Important facts to preserve:

- `grab_targets_inner` requires a sync-loop length even for its unsynced policy.
  Synced gestures can include selection and use targeted length/cycle phase or
  apply-N-cycles. Unsynced gestures operate on the initiator, adopt a prefix,
  **continue Recording**, and schedule completion to Playing/Stopped. Solo peers
  and play-after-record are part of the operation.
- Explicit `ControlOperation::Grab` / `AdoptRingbuffers` and Lua commands have
  distinct targeting semantics from selection-aware UI gestures.
- The old backend's nominal input retention is **30 seconds**, shared per track
  input rather than per loop. Startup, sparse MIDI, missing input and insufficient
  coverage need explicit compatibility tests; legacy audio adoption can construct
  a requested-length zero-filled result around the available data.
- Replacing overwrites existing content without ordinary playback of that loop:
  channel flags are REPLACE, not PLAYBACK. Its cursor advances/repeats like a
  playing mode; Playing -> Replacing preserves position. Input monitoring is a
  separate route. Audio duration does not grow; positive empty MIDI input erases.

## Target update impact — 2026-11-05

Read-only impact analysis compared the reconciled base `a068ce72` with target
`78d39677171c1ccd4583e4d29db415a40847168a`. The target adds composite restoration
[#941](https://github.com/SanderVocke/shoopdaloop/pull/941), master reconciliation
`2cec7c8e`, and built-in FX/OxiSynth/External dry-wet support
[#946](https://github.com/SanderVocke/shoopdaloop/pull/946). It changes 479 files,
including vendored DSP dependencies. Fifty-eight paths overlap our committed work.
Object-only merge simulations against local head `d0451596` and against a
snapshot including the unfinished gesture work each report 35 conflicted paths.
Neither simulation changed the index, checkout or branch. The target has been
fetched but is **not reconciled**. Existing local results validate the earlier
base, not a combined implementation.

This is a material integration dependency, not a replacement of the shared COW
writer design. Reconcile it in Phase A before continuing service/UI integration;
CI remains deferred until all implementation and local acceptance are complete.

- **Protocol:** target engine version 21, worklet configuration 6 and host version
  7 must be combined with our engine 13/configuration 4/host 7 changes. Host version
  7 denotes different layouts on the two branches. Target request tags 38–45
  overlap our 38–44, target dry-play uses loop-mode tag 3, target timing outcome
  uses tag 5, and target processor/global-FX capabilities occupy our new bits 8/9.
  Allocate fresh combined versions/tags/bits and update every carrier, configuration
  conversion and malformed/old-peer test together; retaining the larger version
  alone does not repair these collisions.
- **Production preparation:** native and worklet startup now call
  `prepare_with_processors`; its default empty SharedSettings would bypass our
  normal histories. Apply the same separately budgeted history policy there and
  preserve the explicit-settings constructor. Reconcile the edit-only wet store
  with shared audio pools without silently consuming or changing loop-memory
  capacity. Recalculate total preparation, including both processor banks and
  the documented 256 MiB processor ceiling, histories, MIDI and output workspaces.
- **Modes and scheduling:** Mode moves to the shared `shoop_loop_plan` crate.
  Preserve dry-play/rerecord, frame-driven composite progression, repeat alignment,
  retiming, child independence and coherent snapshots when adding Replacing,
  atomic batches and musical finishes. Reuse the target's PendingTransition query
  and internal mutation ownership seam instead of retaining duplicate mechanisms.
- **Capture and routes:** media now has primary/wet, auxiliary/dry and MIDI roles;
  capture resource banks and loop ports changed shape. Preserve those roles and
  source-aligned buffered wet taps, processor demand/sleep, automatic monitoring,
  recorded processor settings and fault isolation. Direct histories still observe
  the direct capture feed once. Unsupported processed/composite input actions must
  refuse the entire batch before media or solo peers change.
- **Storage/API compatibility:** both branches independently add a different
  `reserve_snapshot` API; a textual auto-merge of resource/mod.rs would still leave
  duplicate incompatible methods. Keep metadata reservation and complete final
  backing reservation distinct. Port edit-only/channel audio writes into our
  pooled layout and preserve compact MIDI projection defaults. New initial-channel
  MIDI reads must handle indexed-MIDI leases as well as flat/shared MIDI.
- **Adapter, persistence and UI:** combine input mutation ownership/cancellation
  with pending composite conversion and processor jobs, optional primitive strip
  ownership, role-aware caches, recorded FX state, family/session imports and
  telemetry. Preserve newly working default playback/editor/Lua capabilities and
  whole-document refusals; global Grab/Replacing support must not accidentally
  advertise dependent composite support.

The existing Grab/Replacing scope stays direct audio/MIDI/trigger tracks. This
update does not add FX-track Grab/Replacing or dependent composite event modes to
that scope. The target explicitly preserves the latter in OPEN P17 under its
user-approved split. Newly delivered composite and FX/dry-wet workflows are now
regression requirements, rather than capabilities to keep globally disabled.
The target's processor evidence documents remaining validation failures and a
saturated-synth deadline limitation; these are provenance, not fresh CI results
or an inherited waiver of this plan's final gates.

- [x] R0: complete the ordered reconciliation checklist below against target
  `78d39677`, preserving both deliveries. Check off R0 only after every R0 item
  and the R1 combined-tree verification checkpoints pass; resolving Git conflicts
  alone does not complete this step. R0/R1 precede further F4/F5 service/UI work.
- [x] R1: run focused warnings-denied codec/configuration, shared writer/resource,
  resolver/composite, normal processor-host startup and adapter checks before
  resuming F4/F5 integration. Verify direct Grab/Replacing alongside unrelated FX
  and composite playback, and atomic refusal of unsupported mixed batches.
- [ ] R2: include role-aware persistence, processor fault/retirement and combined
  memory/MIDI output accounting in F7/F8. Retain final-only CI correction, review
  and PR handoff; no autonomous PR merge.

### R0 — Reconcile the target before continuing F4/F5

The initial impact assessment identified collisions and risks. The deeper source
review below fixes the intended disposition and ordering; it is not a claim that
a merged tree compiles or passes. The execution record below records the completed local checks.

**Use the target's application/graph/composite structure, retaining our shared
storage and writer algorithms.** Do not resolve whole integration files by taking
either branch wholesale. Checkpoint the unfinished gesture patch separately,
reconcile committed `d0451596` first, and then reapply the gesture changes against
the combined interfaces. Preserve the existing branch/PR history with a local
merge. Keep the earlier base/head and WIP snapshot as provenance. Execute R0.1–R0.8
in order; later integration depends on the vocabulary, ownership and lifecycle
decisions established by the preceding items. Commit the reconciled milestone
locally after R1 passes, then resume application implementation. Do not push or
start CI inspection/correction at this checkpoint.

- [x] **R0.1 — Preserve work and establish the reconciliation base.** Checkpoint
   unfinished gesture changes separately, record source/base/WIP identities, and
   merge target `78d39677` into committed `d0451596` locally. Use the target's
   application/graph/composite structure and retain our shared storage/writer
   algorithms. Keep unfinished gesture changes separate until the combined
   interfaces in R0.2–R0.8 are established; reapply their required behavior through
   those interfaces before R1. Preserve the feature branch and existing PR.

- [x] **R0.2 — Unify vocabulary and wire formats first.** Add Replacing to the shared
   `shoop_loop_plan::Mode` instead of restoring a private mode definition in
   `shoop_loops::program`. Keep the target's dry/wet enum order and encodings.
   For the pinned target, assign Replacing tag 5; input request tags 46–52 in
   begin/append/prepare/commit/cancel/status/capacity order; input progress and
   capacity outcome tags 6/7; capability bits 12/13. Retain target timing bit 11
   and processor/global bits 8/9. Use engine protocol 23, host setup 8 and worklet
   configuration 7 for the combined layouts. Application protocol 30 also carries
   the incomplete-capture projection and normal Replace input action. Update host conversions,
   worklet configuration fields, codec fixtures and old-version refusals together.
   Private batch records must also distinguish every mode canonically and reject
   unsupported dry/wet input intents without translating them into a direct mode.

- [x] **R0.3 — Reconcile resource ownership before planner integration.** Preserve target
   `SnapshotReservation` and its no-argument `reserve_snapshot` metadata API for
   wet edits. Rename our unpublished backing-reservation entry point to
   `reserve_writer_snapshot(resource, cut, maximum_frames)`; keep the complete
   future audio/MIDI backing and publication/cancellation guarantees. These credits
   solve different problems and neither substitutes for the other. Extend our
   allocation-free shared-audio Layout with the target's edit-only, zero-history
   case and channel writes. Keep multi-source history reservations, clipped COW
   roots, owner credits, window transactions and growing tails. Implement target
   MIDI initial-channel reads for indexed leases as well as flat/shared leases;
   retain its byte-boundary cursor API. Preserve packed note/controller state and
   the pressure-unknown sentinel when porting the target's const initial state.

- [x] **R0.4 — Port continuous takes onto role-aware banks.** Retain target primary/wet,
   auxiliary/dry and MIDI media fields, independent widths, sixteen audio window
   slots and `resources[bank][roles][member]`. In that physical layout primary is
   member 0, auxiliary 1 and MIDI 2, including MIDI-only direct loops. Replace
   arithmetic based on channel presence with explicit role selection. Direct
   continuous writers still operate on their audio/MIDI pair; they must not
   flatten, replace or drop auxiliary dry leases in existing FX operations.
   Combine the target's Take token/edit/snapshot-credit fields with our writer,
   prefix and reserved-publication fields, with cleanup dispatched by actual take
   kind. Retain separate active-take and monitored-playback windows; apply our
   fresh direct-capture origin reset without overwriting wet alignment policy.
   Preserve the target's source-tagged wet completion tap, recorded processor
   settings and Q node/host delay accounting.

- [x] **R0.5 — Layer atomic settlement onto the frame resolver.** Keep the target's frame
   arrangement progression, natural-boundary/react ordering, repeat waiting,
   frozen retiming and child arbitration. Port our batch IDs, failure retry,
   all-member cancellation, Adopt/RecordPrefix/ReplaceInput transitions and
   receipt cleanup into that resolver. A failed batch must roll back any effects
   on composite cursors, pulses, children and monitoring demand as well as media.
   Audit writing, growing, playback and cursor advancement separately: Replacing
   writes and advances a fixed period without playing its own old media or
   accumulating a new growing capture length. Keep P17's explicit event-mode and
   whole-document gates even after primitive Replacing becomes a valid Mode.
   Preserve target CaptureAborted rollback for unpublished wet captures; an
   admitted continuous writer instead closes at its last complete paired cut.

- [x] **R0.6 — Consolidate finish ownership and coherent state.** Reuse the target's
   `PendingTransition`, observer `settled(resolver)` hook and publication-checked
   LoopSnapshot. Replace the unfinished gesture patch's separate
   Decision.planned/auto-mode cells with a bounded pending-plan projection through
   that observer and snapshot. Musical resolution needs candidate-preview pending
   delays, not just the target's committed-record iterator, so add a corresponding
   bounded preview query. Reserve initial transitions and finish requests together;
   failed initial settlement cancels and reclaims every finish. Start finish
   eligibility at A+Q so the entry boundary cannot immediately consume its clock.
   New actions, removal, fault and shutdown must cancel/collect owned follow-ups.
   Requested/effective cuts and request identity stay attached to the operation.

- [x] **R0.7 — Unify normal startup and capacity calculation.** Keep the processor-aware
   constructor chain and effective compiled-host policy. Both `prepare` and
   `prepare_with_processors` must install default history policy before entering
   the common constructor; explicit shared-settings constructors retain caller
   limits. Use target node/edge/port counts and resource-slot shape, adding our
   reservations rather than replacing those counts with the old direct-only
   formula. Keep the default wet-edit reservation independent of retrospective
   histories: merely making histories present must not silently redirect existing
   FX rerecords into the Grab owner budget or remove their previous reservation.
   Account for these reservations once, preserving the target's ordinary-media
   accounting and processor memory ceiling.

   MIDI sizing must cover the new route, not merely strip output. Strip-input
   ports now separately receive host capture, dry-loop playback and global controls;
   processor sends combine live input, restored playback and cleanup. Introduce
   preparation-time per-port capacity overrides so capture admission stays E/B
   while playback/merge/send buffers use their derived bounds. Keep strip-output
   input forwarding. Thread the appropriate send capacity through hosted buffered
   input and GlobalMidi preparation and its processor-budget calculation. Uniformly
   applying the worst merge bound to all three inputs/four outputs would multiply
   memory needlessly. The final admitted fan-in and combined default memory/work
   envelope still require F2/F7 measurement and admission closure; no fit or
   realtime guarantee is established by this source analysis.

- [x] **R0.8 — Combine controller and adapter ownership before service work.** Keep target
   composite conversion/reset deferrals, processor jobs, optional strip ownership,
   role-aware reads, recorded-state caches and resync. Use its
   reserve_mutation/push_internal sequence-zero seam for our input job instead of
   keeping duplicate sequence allocators. Extend ordering/cancellation fences to
   clear, conversion, removal and session replacement; a deferred composite reset
   must not be overtaken by an input job addressing the old slot generation.
   Revalidate all targets before preparation and again at the cut. Grab/Replacing
   require supported direct primitive targets; preserve legal existing mode actions
   and witness roles without a blanket rejection of unrelated FX/composite state.
   Whole-batch refusal must precede any target/solo mutation. Keep normal input
   jobs independent of unrelated processor controls and playback.

### R1 — Verify the combined tree locally

R1 is the reconciliation exit gate. Run these three focused checkpoints after the
relevant code compiles, rather than a build/test cycle for every conflict hunk.
Record actual results and leave failed or unexecuted checkpoints unchecked.

- [x] **R1.1 — Vocabulary/resources:** warnings-denied affected builds, protocol/host/worklet
  round trips and malformed/old-version cases; shared-audio edit-only/history
  layouts, compact MIDI state, initial-channel reads from all backing kinds,
  metadata-slot versus final-backing pressure and exact reclamation.
- [x] **R1.2 — Resolver/planner:** existing batch refusal/delay oracle and seeded resolver
  reference, target frame/nested/repeat/retime tests and dry/wet capture tests.
  Add one failing batch during active composite playback to prove cursors,
  children, clocks and old pending plans roll back together. Keep allocation
  guards and immediate-exit/old-export assertions.
- [x] **R1.3 — Normal hosts/adapter:** our public_input scenarios and
  public_grab_and_replacing_have_one_owned_mutation_and_preserve_old_exports,
  plus target tests for exact hosted delay, wet bypass versus dry processing,
  recorded-take state, isolated processor faults, composite resync/conversion and
  automatic finish without polling. Add a compact mixed direct/FX/composite
  scenario that uses normal processor startup, checks history capability/coverage,
  performs direct Grab/Replacing, refuses an unsupported mixed input batch
  atomically, and retains unrelated FX/composite data and controls. Keep strict
  zero-overflow diagnostics for admitted routed MIDI, with a distinct deliberate
  overflow-recovery case. Reuse role-aware archive fixtures in F7.

After these checks pass, resume remaining F4/F5 service and gesture work on the
combined tree. Final-only CI/review correction remains unchanged. Existing known
MIDI owner/workspace admission and sustained-capacity work remains open; target
reconciliation does not turn those earlier partial stages into completed ones.

## Goals, scope and immutable acceptance criteria

Goals and these criteria require explicit user approval to change.

User clarification: features may be disabled by an explicit resource/configuration
policy. Every supported target must retain a documented and validated configuration
that enables Grab and Replacing; target-specific permanent exclusions are forbidden.

1. **Usable by default:** normal native and browser application startup prepares
   the required resources and advertises supported Grab/Replacing capabilities.
   Real controls, keyboard/default actions and Lua work without a test-only
   constructor, hidden feature flag, alternate backend or manual configuration.
2. **Complete P04 policy:** restore explicit and gesture-based grabs, targeting,
   selection, global sync, apply-N-cycles, solo, play-after-record and default
   recording policy. Resolve a coherent target set and one authoritative cut.
   Include synced and unsynced prefix-plus-live-recording behavior, audio-only,
   MIDI-only and mixed tracks; preserve meaningful trigger-track behavior.
3. **Continuous P05:** actual Replacing mode records every admitted input tick
   over arbitrarily many loop iterations within prepared limits. Preserve period,
   phase, controls, outside data, empty-interval erasure and correct mode-dependent
   output. Immediate/scheduled entry, Stop, Playing, Recording and cancellation
   follow the reference behavior under the fixed runtime's timing contract.
4. **Exact media and ownership:** preserve multichannel audio and complete MIDI
   packets/SysEx, equal-frame order and all supported channel-state projections.
   Handle wrap and logical-zero state correctly without stored splice-restoration
   packets. Siblings, saved owners and old exports remain immutable while another
   loop replaces or continues recording. Loaded media remains editable.
5. **Truthful timing and atomic outcomes:** compute windows/phase from RT state at
   the effective cut, not stale UI positions. Preserve fixed Q, partial callback
   eligibility, due-operation ordering and output latency. No partial target batch,
   mixed audio/MIDI revision, optimistic success or silently relabelled deadline.
   Startup padding, where required by the reference, is explicit logical content;
   it never certifies unavailable or faulted source time as recorded history.
6. **Bounded live operation:** no allocation/free, blocking, callback replay,
   unbounded patch queue or complete-prefix copying in rendering. Admitted ongoing
   capture, required playback and finalization have reserved work and ownership
   credits independent of ordinary exports/uploads. MIDI suffix/index work cannot
   silently drop input, force a gap every wrap or make an accepted exit cut late.
7. **Production capacity:** retain the nominal 30-second history behavior under
   documented production limits. Bound payload, MIDI metadata/state, pending input,
   writers, snapshots and retirement globally. Preparation and operations refuse
   explicitly when those limits cannot be met. Do not quietly multiply fixture
   reservations by all configured tracks, shrink retention or reduce existing
   recording capacity to make the default app start.
8. **Service-owned lifecycle:** reliable request identity, authoritative pending
   and confirmed state, cancellation and stale-ID/session/source fences survive
   UI detach/reconnect. Admitted work belongs to the service/host; Lua stays
   client-side and is never restarted/replayed to recover it. True host loss is
   diagnosed under the existing recovery contract.
9. **Existing workflows:** grabbed/replaced media supports ordinary waveform/MIDI
   views, complete-loop get/set, immutable exports, save/load, file replacement and
   family/configuration replacement. Active/scheduled writer conflicts are handled
   consistently. Save supported default-Grab settings; load loops stopped without
   persisting histories, pending writers or transient Replacing mode.
10. **Parity closure:** P04 and P05 are COMPLETE only after public-route production
    tests, normal UI/Lua controls and negotiated flags agree, evidence is linked,
    and applicable fresh CI passes on the delivered head. Foundation tests or
    toggling flags alone are insufficient. Leave the implementation PR unmerged.

Supported scope is the current production direct audio/MIDI/trigger core on native
typed/socket/shared-memory carriers and browser MessagePort/worklets. Preserve
capabilities already delivered by the current target, including ordinary
composites, FX/dry-wet and timing editing. Preserve remaining limits/refusals and
the separate P17 dependent-composite boundary, duplication and unsupported MIDI
initial/preroll rules. Do not expand this delivery's feature scope as a side effect. This plan adds neither
host-crash durability nor recovery of a Lua VM, UI drafts or shared-memory
reattachment. Physical-device deadline certification is not implied by CI timing.

## Design rules

- Keep fixed execution as the sole architecture. Reuse shared resource IDs,
  snapshots, capture feeds, media admission and retirement. Keep policies in the
  service, timing/data mutation in RT, and files/conversion in client codecs.
- Give production hosts one common, versioned preparation policy. Prepare source
  slots and global pools off-render; activate/reclaim their leases with track
  lifetime. Input histories belong to sources, independently of loop lifetime and
  monitoring. Never allocate a 30-second buffer for every loop.
- Separate history and loop-media accounting explicitly. Preserve the meaning of
  the existing loop-memory setting. At 48 kHz, sixteen stereo 30-second histories
  alone require 184,320,000 payload bytes before MIDI, metadata or retained users.
  The foundation's full MIDI checkpoint projections also need a production scaling
  solution. Choose and document finite history/metadata/writer budgets and defaults
  after measuring this real host shape; introduce explicit settings/negotiated
  limits if needed. History density eviction must advertise truthful coverage.
- Carry musical intent and generation/revision/source fences to the engine.
  Service resolution selects authoritative loop IDs and policy; the engine settles
  sync/target phase and a common completed cut. Use bounded batch preparation and
  commit with owned credits. Never implement multi-loop grab as independently
  successful single-loop commands or use SelectFamily for unrelated loops.
- Extend the take writer for both growing capture seeded from a grabbed immutable
  prefix and fixed-period replacement. Prefix and live tail join exactly once at
  the cut; the prefix may outlive the source horizon. Ordinary Record must not
  discard the adopted prefix, and Replacing must not reset/grow its period.
- Add a distinct continuous writer rather than polling CandidateMidi once per
  tick. It owns incoming bytes immediately, a private COW root and working MIDI
  state/index metadata. Reuse the final complete-view publication machinery.
  Maintain bounded dirty-range/checkpoint repair with reserved progress and a
  proven maximum outstanding debt. Admission must account for the complete write
  rate and exit/publication cost. If the current checkpoint representation cannot
  maintain that invariant, extend it before enabling P05; a larger queue or a
  whole-suffix rebuild every tick is not a solution.
- Never publish an incomplete index or silently defer an accepted cut. A requested
  change that needs preparation has truthful pending/admission/results and a
  defined effective tick. Accepted Stop/exit must remain possible using reserved
  credits. Any visible finalization state must preserve reference control/output
  semantics, not hide lost intervals or indefinite Replacing behind Playing.
- Treat Replacing consistently as a writer wherever active/scheduled Recording
  matters: mode codecs/projection, media caches, persistence, import/family/full
  replacement, busy states, cancellation and shutdown. Preserve its separate
  cursor/output semantics and existing content-change revision distinctions.
- Add negotiated protocol capabilities and bounded request/result/state schemas;
  update versions, codecs, worklet configuration encoding and all carriers together.
  Do not send private page addresses or raw callback buffers through the service.
  Feature support and per-request readiness/capacity are different conditions.
- UI, keyboard and Lua use one service path. Expose pending/results through the
  confirmed projection and invalidate media caches on committed content revisions.
  Preserve unsupported-backend capability tests; replace production refusal tests
  with success coverage only when that route is implemented.

## Development verification strategy

The user's latest 2026-11-05 instruction establishes two sequential phases:
complete the implementation and local acceptance in this environment first;
only then start an iterative CI correction loop. This supersedes the earlier
milestone-push/CI-monitoring strategy, historical CI-routing instructions in the
execution record, and repository guidance to run complete gates before every
behavior commit. Feature scope and final acceptance criteria remain unchanged.

**Phase A — Complete the work locally.** Carry F1–F7 and all locally executable
F8 acceptance work through to completion in this environment. Use targeted local
checks while iterating, then broaden to the applicable local integration,
production-capacity, packaged-application and regression gates as the finished
implementation requires. Milestones are commit points, not reasons to stop
implementation or switch to CI monitoring. Keep implementing until the normal
application workflows and local acceptance evidence are complete.

**Phase B — Correct and validate with CI.** After Phase A is complete, reconcile
the target, run affected local checks, push the complete implementation and begin
the final CI/review loop. Inspect required failures and artifacts, fix their
causes here, run affected local checks, push corrections and rerun affected CI.
Repeat until fresh applicable CI and the final acceptance gates pass on the
actual delivered source/base. CI findings that require implementation changes
reopen the affected local work before that correction is pushed.

- For each meaningful increment, run the smallest relevant native package/test
  filter. Reuse existing fixtures and independent media/state oracles. Add a test
  only for a new contract or a concrete regression; do not duplicate the same
  assertion at every layer or add tests that merely mirror implementation details.
- Use Q=32 and one representative callback partition for the main development
  path. Add a boundary quantum/partition only when the changed logic depends on
  it. Reserve the broad cross-platform combinations for Phase B; exercise relevant
  combinations locally when needed to complete or verify the implementation.
  Do not build a Cartesian product into every new test.
- Storage/writer increments need the directly affected correctness test and,
  where ownership or admission changes, a focused heap/pressure/retirement test.
  Protocol increments need the affected codec/admission case. Service and client
  increments need the affected public-action case. Run a small integration group
  when crossing those boundaries, rather than the complete workspace suite.
- Format changed Rust and use warning-denying compilation of the affected test
  target. Run the relevant tracing/test-attribute/parity checks when their inputs
  change. Do not run a workspace build, full native suite, browser packaging or
  benchmark suite as a routine pre-commit step.
- Commit meaningful milestones locally. Defer CI-driven pushes, CI status checks,
  log/artifact inspection and reruns until Phase A is complete. Do not poll CI at
  startup, after a milestone, or between implementation steps during Phase A.
  Existing or automatically triggered runs may finish unobserved; they do not
  redirect this implementation phase. In Phase B, use CI for required
  Windows/macOS, Wasm/Node/browser, native-driver/carrier and broad regression
  coverage, and rerun affected failed checks after each verified correction.
- Perform production-capacity measurements when the writer/pool design is usable
  and again if that design or its limits change. Do not remeasure unchanged
  foundations after every edit. Use a representative native end-to-end path at
  integration milestones and reserve packaged native/browser acceptance for F8.
- Start Phase B only after the complete implementation and applicable local
  acceptance pass, not after an internal kernel milestone. At final delivery,
  require fresh applicable CI and the public application evidence below before
  closing P04/P05. Record exact source/base, failures and skips. Pending or unrun
  checks are never described as green; do not rerun successful checks without a
  relevant change or unresolved concern.

For example, a category-index change should start with the
`rt_runtime` `midi_state::indexed::store::tests` filter; packet-storage changes
with `resource::midi_packets::tests`. Run the neighboring index/packet group when
their shared interface changes. These are narrow development checks, not a
replacement for the final production-route acceptance tests.

Ordinary flat Recording can now prepare a long MIDI prefix in bounded callback
steps while the same private semantic root captures every new input tick. The
original recorder remains authoritative until the prefix is complete. An early
Replacing request refuses before switching modes; cancellation or Stop discards
only the private preparation. At the accepted cut, audio forks the current flat
snapshot and the complete MIDI writer becomes fixed in place. A guarded resource
oracle checks incomplete-publication guards, prefix/tail contents, one-tick erase,
retained views and exact cleanup. The engine scenario records beyond the history
horizon, cancels/retries preparation, replaces one tick, verifies exact paired
media and logical-zero projection, and stops another incomplete preparation.
All eight indexed-MIDI resource tests and 31 shared engine tests pass locally;
one opt-in callback measurement remains unrun. Global retained/live MIDI admission,
public protocol/service/client delivery and final acceptance are still open.

Atomic resolver admission now accepts distinct loop targets at one exact cut and
rolls the whole batch back before commitment when any member is invalid, refused
by resource preflight or superseded by a competing request. Clock pulses and
trigger effects are retried from committed state as well. Cancellation covers
all members, and retained terminal receipts cannot affect a new batch that reuses
a slot. A guarded three-loop scenario and all 17 scalar resolver regressions pass
locally. This is the settlement foundation for F4; protocol, resource staging and
application admission are not yet connected.

Paired input batch staging now acquires every target's immutable media, writer,
final snapshot reservations and ledger pins before submitting the atomic resolver
batch. The same cut resolves sync phase from RT state; retained dependency states
are fenced through preflight. Grab establishes a new identity channel layout while
preserving gain/preplay. A normal-constructor engine scenario verifies raw Grab,
atomic solo Stop, invalid last-target refusal, refusal after the first target has
prepared a growing writer, exact prefix continuation beyond the rolling horizon,
old exports and full cleanup. The bounded internal record codec rejects noncanonical
options, tags, padding and size. Twenty loop-library unit tests, all 17 resolver
and 19 runtime regressions, and 32 shared engine tests pass locally (one callback
measurement remains ignored). Public protocol/controller staging remains the next
F4 step; capabilities and P04/P05 remain disabled/OPEN.

Replacing preparation now accepts the loop's retained playback snapshots directly,
including media whose original resource owners have been removed. Trusted same-pool
MIDI forks immediately; loaded flat MIDI uses bounded import. Ordinary Recording
uses the continuously followed prefix path, while an already active shared writer
needs no controller-owned duplicate preparation. The normal-constructor batch
scenario now prepares current playback, enters/exits Replacing after one tick,
checks exact PCM/packets and immutable old exports, then resets the MIDI route and
verifies a retained source-generation fence refuses the entire later batch. The
long ordinary-recording scenario also uses this common preparation operation.
Affected warning-denying builds, the focused scenarios and the shared regression
group pass locally. This still does not enable application capabilities.

Capture takes now own their channel windows independently of monitored playback.
A fresh ordinary recording resets raw origins while retaining gain/preplay; an
old manually shifted take keeps its own windows during monitor-previous playback.
Publishing or exporting the new capture uses the capture layout, and selecting it
clears the displaced manual period override. The focused normal-constructor
scenario verifies exact monitored samples at the old offsets, identity new PCM/MIDI,
Replacing preparation/cancellation during that recording, preserved preplay and
complete resource recovery. All 19 loop runtime regressions and 32 shared engine
regressions pass locally, as does the new focused scenario; the opt-in measurement
remains unrun. Public protocol/controller work is still outstanding.

## Implementation stages

Dependencies: F0 -> F1 -> F2 -> F3 -> F4 -> F5 -> F6 -> F7 -> F8.
Verify each used interface with targeted local checks before building its
dependent layer. Complete implementation in this environment before inspecting
or correcting CI, following the two-phase strategy above. P04/P05 flags may
be enabled separately after their own public-route gates pass; final completion
requires both. Keep the checklist and evidence current throughout execution.

Public protocol version 13 now carries bounded input transaction begin/append,
prepare/commit, cancellation, status and finite-capacity records. The engine owns
preparation through terminal cleanup, retains requested/effective cuts and request
identity across controller disconnect, and commits all media targets/solo peers at
one RT cut. Delayed mode batches roll back clocks, triggers and old plans on any
member refusal. Normal-constructor public tests cover phase-aware mixed Grab,
current-media Replacing, delayed entry, exact old/current data, cancellation across
disconnect and late timing refusal with unchanged peers. A strict output diagnostic
caught restoration/merge overflow; playback, strip and host buffers now derive
separate bounded output capacity from state restoration plus routed input density.
Strip outputs forward their prepared merge input without a duplicate packet copy.
The public route passes with zero output overflows. Local engine tests pass (42,
one measurement intentionally ignored), together with targeted runtime/protocol
checks. Application adapter, musical finish plans, global MIDI owner protection,
production measurements and application acceptance remain open. No Phase A CI
inspection or remote push was performed for this milestone.

The RT application adapter now routes raw Grab and Replacing through that public
transaction protocol. Internal preparation RPCs retain one application mutation
sequence until the terminal result; later edits of those targets wait, while a
later mode action cancels a pending delayed transaction. Normal preparation
negotiates adapter Grab/Replacing capabilities; production service command
refusals and gesture policy still need removal after their own implementation.
Raw growing Grab can divide available audio credits across its batch and reserves
both continuation and a complete final export before entry. An ordinary adapter
scenario verifies one completion, pending identity, delayed cancellation, exact
old/current audio, zero MIDI output overflow and two simultaneous growing targets.
All sixteen adapter scenarios, two public engine input scenarios and eight audio
resource scenarios pass locally, with affected warnings-denied builds, formatting,
tracing and test-attribute checks. Musical gesture settlement and atomic finish
plans remain open; P04/P05 remain OPEN. No Phase A CI inspection or push occurred.

### F0 — Establish the new delivery

- [x] Refresh the merged foundation, target instructions and parent status; record
  actual source/base SHAs. Create the new feature branch and open/push its PR as
  described above, preserving unrelated work. Retain the actual base for final
  Phase B CI validation; historical early CI is not a new monitoring requirement.
- [x] Inventory every refusal, capability gate, mode conversion and production
  constructor named above; include service media/persistence guards, adapter
  canonical-import caches and configuration/family capture exclusions.

Verification: the new branch contains the merged foundation and current fixed
kernel, and the new PR has the correct live parent. No closed-PR continuation or
legacy production backend is used.

### F1 — Freeze app behavior and production limits

- [x] Extract a compact behavior-to-test matrix from reference `grab_targets_inner`,
  `grab_window`, `adoption_window`, BasicLoop and audio/MIDI channel replacement.
  Include explicit versus selected targets, all global policies, phase/length,
  startup/missing-history padding, empty/no-sync cases, trigger tracks, monitoring,
  pending transitions, wrapping and writer-to-writer mode changes.
- [x] Port meaningful reference expectations into production test scenarios:
  `grab_policy_covers_targeted_selection_solo_and_immediate_completion`,
  `production_keyboard_default_action_covers_stopped_cancel_and_grab_policy`,
  relevant `expanded_loop_actions_route_modes_grab_and_balance`,
  `fake_grab_preflights_every_target`, and channel replace/erase/wrap tests.
  Keep unrelated dry/wet expectations under the delivered processor contract;
  dependent composite modes remain P17.
- [x] Specify request ownership, requested/effective cuts, preparation/finalization,
  cancellation, insufficient history, admission failure and pressure outcomes.
  Select production budgets/defaults with a calculation for default and largest
  supported track/channel pools, 30-second retention and loaded MIDI density.

Verification: each acceptance criterion maps to a concrete public-route test and
named lower-layer proof, reusing scenarios where they cover several criteria.
Develop policy using a focused parameterized test group, not repeated full app
runs. A material departure from reference behavior or an
additional feature-scope exclusion requires user approval, not a ledger footnote.

### F2 — Prepare and expose real input histories

- [x] Thread shared preparation through EngineDescription, host settings and the
  normal engine constructor used by dummy/CPAL/JACK/native child and AudioWorklet
  hosts. Version worklet configuration and surface finite limits/readiness.
- [x] Implement source-slot activation/reset/removal, global page/index/state
  allocation and bounded retirement. Preserve track/slot generations, full routing
  masks, independent MIDI input projection and source coverage across graph cuts.
- [x] Provide the intended retention without duplicating histories per loop.
  Resolve MIDI metadata scaling before choosing default capacities; account for
  retained cuts, simultaneous writers and existing ordinary recording/imports.

Verification: normal production constructors capture while all loops are stopped,
with one input observed once. At real default pool sizes, preparation fits the
documented budgets; 30-second and density-pressure cases are truthful. Dynamic
tracks, rerouting, MIDI reset during a partial tick and teardown retain all owners
and recover every credit. Cover audio-only, MIDI-only, mixed and silent inputs
without repeating each case across every host during development; CI covers the
host/platform combinations.

### F3 — Finish the continuous RT writer and mode lifecycle

- [x] Add fixed-domain/engine Replacing transitions and state without confusing
  them with the existing one-shot content Replace operation. Update cycle/phase,
  sync, repetition, pending transitions and mode-dependent output decisions.
- [x] Add growing capture from an adopted prefix, preserving duration/data/state
  and consuming the first live tick exactly once. Retain fixed-history, long loaded
  and previously replaced bases through the ordinary resource/media interfaces.
- [x] Implement the continuous COW audio/MIDI writer described above. Own every
  admitted tick; handle erasure, wrap, logical-zero projection and suffix state.
  Preserve old views and make complete new views available at valid exit cuts.
- [x] Prove bounded preparation debt and guaranteed exit resources. Cover long
  loaded MIDI with more suffix events than one optional preparation allowance,
  sustained maximum admitted input, multiple active loops, immediate exit after
  one tick and many wraps. Fix representation/scheduling limits uncovered here;
  do not proceed with a writer that loses input while CandidateStatus is Building.
- [x] Handle Stop/Playing/Recording, scheduled cancellation, capture exhaustion,
  clear/removal and terminal fault; emit truthful terminal outcomes and release all
  private state. Keep unrelated playback and ordinary recording working.

Verification: develop against a focused allocator-guarded fixed-execution group
with independent samples/packets/state at Q=32 and one callback partition. Add a
targeted boundary case locally when a change depends on that boundary. Final
Phase B CI covers Q=1/16/32/64/128 and whole/one-frame/irregular callbacks.
Replacing output matches reference monitoring semantics. Prefix continuation grows
beyond the history horizon; replacement stays fixed-length. Required work survives
exhausted ordinary transfer budgets, and every cancellation/fault frees its credits.

### F4 — Deliver engine operations through the public protocol and adapter

- [x] Add negotiated Grab/Replacing support and bounded batch preparation/commit,
  status, cancellation and completion records to `shoop_rt_protocol`; update codecs,
  limits, carrier bounds and malformed/old-peer tests. Keep public control actions
  stable where possible; extend internal backend intent when needed to carry an
  RT-resolved target phase instead of a stale sampled position.
- [x] Implement engine admission and settlement for all selected sources/targets
  at one cut, including post-mode/position, prefix continuation, finish plans and
  solo effects. Preflight snapshots, writers, media, receipts and transitions;
  rollback the whole refused batch before changing any loop or peer.
- [x] Implement `shoop_rt_backend::grab_loops`, Replacing conversion, pending job
  ownership, completion correlation and mirror/cache updates. Support typed, socket,
  supported shared-memory and MessagePort carriers with identical semantics.

Verification: actual protocol/adapter requests drive ordinary production hosts.
One invalid/stale/full target leaves every target and solo peer unchanged. Admission,
completion and resync agree on mode, next mode, phase, length, revision and cut.
Old peers remain correctly unsupported; messages never exceed negotiated bounds.
Use one native carrier for development integration and focused codec/admission
tests for changed messages; final Phase B CI verifies the remaining required
socket/shared-memory/MessagePort coverage.

### F5 — Restore authoritative service policy

- [x] Implement GrabClicked, default recording Grab, global controls and explicit
  Grab/AdoptRingbuffers commands through one service-owned job path. Port target,
  selection, sync/apply-N, solo and play-after-record policy, including unsynced
  adopted-prefix recording and its scheduled completion.
- [x] Admit Replacing through explicit/triggered/scheduled mode commands; include
  it in the appropriate writer, cancellation, solo, sync and media guards. Keep
  explicit loop lists distinct from selection-aware gestures and preserve ordering.
- [x] Fence pending work against deletion, slot reuse, routing/source reset, target
  length changes and session replacement. Reconcile UI loss after admission and
  before completion without resending or recomputing the accepted operation.

Verification: production service + RT adapter passes the F1 behavior matrix, not
just FakeBackend operation assertions. Real audio/MIDI exports prove correct
windows and modes. Failure preserves confirmed state; a reattached client observes
the same admitted operation and final media exactly once.
Use a compact policy group and one representative real-media scenario while
iterating; exercise the complete public sequence at F8.

### F6 — Enable the actual UI, keyboard and Lua workflows

- [x] Derive each capability from negotiated, implemented host support; remove its
  production service/adapter refusal only after the corresponding route works.
  Keep per-request warmup/capacity errors distinct from unsupported capability.
- [x] Wire normal loop/global Grab controls, default-action settings, targeting,
  keyboard bindings and Lua Grab/AdoptRingbuffers/LoopMode_Replacing through that
  service path. Make Replacing discoverable in the appropriate normal controls;
  show its actual mode, pending transitions, results and media refresh behavior.
- [x] Update production UI/runtime tests to assert capabilities and enabled
  controls after normal startup. Preserve refusal tests for unsupported peers and
  other parity features. Exercise real clicks, bundled keyboard script and public
  Lua functions rather than injecting internal domain operations.

Verification: a normal launched application can create a supported track, receive
input while stopped, click Grab, enter/exit Replacing, and play/export the result.
Equivalent UI, keyboard and Lua actions produce equivalent confirmed data/state.
No test manually installs histories or changes the capability map to pass.
Test action routing with existing client/service fixtures. Reuse the production
media oracle instead of repeating long media scenarios for every input method;
complete locally supported browser/application acceptance in F8 and use final
Phase B CI for the remaining required browser/platform execution.

### F7 — Persistence, recovery and sustained-capacity acceptance

- [x] Audit active and scheduled Replacing across media reads, save/export,
  imports, family updates and full session replacement. Pin complete immutable
  exports or refuse according to the existing active-writer policy; never save a
  partial index or stale canonical-import cache as current content.
- [x] Round-trip grabbed, prefix-continued and repeatedly replaced media and
  supported default-Grab settings. Reload stopped loops, then grab/replace again.
  Retain old exports during conflicting imports and configuration replacement.
- [x] Exercise detach/reconnect, controller loss, abandoned preparation, saturated
  results, source removal, writer pressure, reset/overflow during partial input,
  runtime fault and final shutdown. Count all pages, versions, MIDI state/index
  slots, jobs, receipts and retirement through sustained simultaneous capture.
- [x] Measure prepared memory and complete callbacks at production defaults and
  admitted worst shapes/densities. Preserve tails, maxima, all timing misses and
  output overflow diagnostics; resolve configured work/storage violations without
  undocumented capacity increases or weaker functional assertions.

Verification: public-route save/load/edit and recovery retain exact supported
media/state, old owners and request identity. No histories or writer internals enter
file formats. Repeated operations reach a stable bounded resource usage and fully
recover at teardown. Reports identify actual source, configuration and limits.
Combine related lifecycle checks in a small native integration group. Add a
separate fault/pressure test only when it exercises a distinct failure contract;
do not multiply the full workflow across all media/carrier/platform combinations.

### F8 — End-to-end delivery and parity closure

- [x] Complete Phase A with one representative complete public sequence in a
  packaged native app here, plus the browser equivalent wherever this environment
  supports it. Required browser coverage is also verified in final Phase B CI:
  stopped input -> selected/targeted Grab -> unsynced prefix continuation ->
  playback -> continuous audio/MIDI Replacing across wraps -> scheduled exit ->
  old/current exports -> save/load -> repeat edit -> UI detach/reattach -> teardown.
  Cover normal button, keyboard and Lua routes, stereo, MIDI-only/SysEx, mixed
  tracks and multiple sources/users across a small complementary set of scenarios;
  do not rerun the entire sequence for every combination. Verify actual
  packets/samples and outcomes.
- [x] Verify configuration-based firmware support: the shipped disabled policy
  negotiates unavailable capabilities, while a documented enabled policy uses the
  common engine/service path and passes exact Grab/Replacing, owned completion,
  persistence/re-edit and teardown on both supported board memory profiles.
- [ ] Only after the complete implementation and local acceptance are finished,
  start Phase B: refresh/reconcile the target, run affected local checks and push
  the complete implementation. Obtain fresh applicable CI on the final
  source/base: warnings denied, full Rust suites, native hosts/drivers/carriers,
  required Node and Chrome libraries, real session/AudioWorklet/browser workflows,
  portable builds, formatting, tracing, test-attribute, parity and dependency gates.
  Inspect failures/artifacts and fix causes in an iterative correction loop:
  reproduce or explain each required failure, implement the correction here, run
  affected local checks, push and rerun affected CI until the required gates pass.
  Distinguish any later unvalidated consolidation explicitly.
- [ ] Inspect incoming automated reviews; address applicable feedback and rerun
  affected checks until approval. If no review arrives, record that without claiming
  approval. Resolve threads and align PR title/body with the final application work.
- [ ] Update `docs/native_session_recovery/feature-parity.md`: put implementation
  and production UI/Lua/service/protocol/RT evidence directly in P04 and P05 rows,
  mark both COMPLETE, reconcile capability coverage/open-row totals and replace the
  foundation-only remaining-work paragraph. Link an app-facing evidence document,
  final CI/artifacts, actual budgets and supported-feature boundaries. Update the
  shared/persistence contracts and previous plan handoffs to point to this delivery.
- [ ] Verify a clean normal app actually exposes and executes both features on
  the delivered tree. Record final head/base, CI/review status, honest skips and
  remaining unrelated parity items. Leave the PR unmerged for the user.

Verification routing: finish implementation and local acceptance in Phase A
without CI polling or correction detours. Only then enter Phase B's iterative CI
correction loop for final broad/cross-platform validation. Honor standing user
validation/waiver preferences where applicable and record actual outcomes. Prior
foundation CI does not establish application acceptance. No pending or unexecuted
check is labelled green.

P04/P05 must remain OPEN until their respective application acceptance gates pass.
Finish all stages before reporting full integration; partial RT progress must be
reported as partial progress, with the remaining user-facing behavior explicit.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone locally; keep implementing
  through local completion before starting CI inspection/correction.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
