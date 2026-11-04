# Reconcile shared audio/MIDI grab and replacement with fixed execution

## Status, baseline and delivery

R0–R7 are implemented with passing native functional and release evidence at
`480a77a6`. Target refresh through `820fb239` is consolidated. R8 delivery requires
green checks on the resulting head; [PR #927](https://github.com/SanderVocke/shoopdaloop/pull/927)
records its exact final SHA and validation results. Written using
[write_plan.md](.agents/prompts/write_plan.md).

- Shared audio/MIDI baseline: `feat/shared-midi-persistence` at
  `837a7dd0567bcfff12971c2cc1fdc8c688851183`, including the audio foundation from
  [PR #909](https://github.com/SanderVocke/shoopdaloop/pull/909).
- Target: `feat/fixed-quantum-runtime`, inspected at
  `cb79706e95350539638368bcca9f50b1601795ae`; its parent delivery is
  [PR #930](https://github.com/SanderVocke/shoopdaloop/pull/930).
- Continue the existing feature branch and retarget
  [PR #927](https://github.com/SanderVocke/shoopdaloop/pull/927) to
  **`feat/fixed-quantum-runtime`**, replacing its current
  `feat/backend-replacement` base. Refresh both heads before execution and before
  final validation. Leave merging to the user.
- The common ancestor inspected was `e68b15ac`. Six files change on both sides;
  textual three-way checks found conflicts in `docs/tracing_coverage.csv` and
  `shoop_persistence_codec/src/codec.rs` test-module registration. This is not a
  compiled merge result. Fixed execution does not modify the shared-store
  algorithms; most integration work concerns lifecycle, clocks and proofs.
- The old head's [green CI](https://github.com/SanderVocke/shoopdaloop/actions/runs/41669618880)
  is historical evidence, not validation of this reconciliation.

This plan supersedes the execution and delivery direction in
[PLAN_SHARED_MIDI.md](PLAN_SHARED_MIDI.md). Preserve its storage semantics and the
[audio](src/rust/rt_runtime/docs/reference/shared-audio.md) and
[MIDI](src/rust/rt_runtime/docs/reference/shared-midi.md) ownership contracts.
The target has consolidated its fixed execution and conversion contracts into
[PLAN_FIXED_QUANTUM_CLEANUP.md](PLAN_FIXED_QUANTUM_CLEANUP.md), the runtime guides
and [core persistence contract](docs/native_session_recovery/persistence-contract.md).
Its original fixed/conversion plan files were retired during cleanup; their
functional requirements and this plan's acceptance criteria remain in force.

## Execution record and acceptance coverage

R0: both heads were refreshed at execution start on 2026-11-04. Shared head
`837a7dd0` is retained as `backup/shared-grab-before-fixed-quantum`; target
`cb79706e` is the actual fetched head of open PR #930. PR #927 remains open.
The real Git merge found one tracing-inventory conflict; codec test registration
merged automatically. Both inventories and shared codec tests are retained.
The integrated engine uses `GraphRuntime::prepare_fixed_with_planner_and_resources`
and direct `process_fixed` loop consumers.
The merged baseline is `8c810ef8`; fixed-base workflow filters are enabled and
PR #927 has been retargeted. Actual validation is pending fresh CI. The experimental `shoop_loops::fixed`
bank is not a replacement production path for this reconciliation.

Target refresh on 2026-11-04 found `435eb2a2f13550b9f3f8e934e558c1f3e24b486d`,
including the fixed-runtime cleanup and the current backend/host integration.
The merge preserves its removal of the prototype, adaptive resolver and borrowed
decision APIs. Shared history now uses `Planner::begin_tick`/`finish_tick` and
`Processor::process`; all resource and resolver preparation supplies immutable
`Quantum` explicitly. Q=1 storage/reference fixtures remain scalar fixed ticks.
The new callback measurement is independent of the removed prototype benchmarks.

Implemented milestones: prepared source histories and common completion watermark;
paired source-fenced cuts; private audio/MIDI candidates; fair callback-wide edit
and loaded-index import progress; complete fenced publication through the media
ledger; ownership recovery for unread results; production Q/partition oracles;
failure/route/upload/shutdown fixtures; codec/reload/family/configuration sequence;
prepared capacity diagnostics and full-callback raw CI artifacts.

CI on `df6922fa` passed bare-metal and native JACK production checks. Its broader
test build exposed an outdated fixture reply pattern, fixed in `dde23efb`.
That head's test compile exposed two fixture type/authority mistakes, fixed in
`1d84380d`. These are partial historical results. The target advanced before fresh
PR checks could start; final acceptance requires checks on the new merged head.

The `435eb2a2` merge is `b1d8dc84`. Its new integrated CI found the expanded
1024-port host mask (source fencing now compares all words) and the target's
remaining 64-port host-buffer admission limit. The latter prevented dynamic JACK
engine preparation; buffered audio now admits the engine's 1024-port maximum,
with finite stack reference arrays. A dedicated foundation CI job runs the four
affected library suites and repository policies independently of host-matrix
startup; it supplements the complete required matrix.

Source review additionally found that ordinary writer claims alone do not exclude
pending uploads. Publication now scans the finite prepared version table, counts
that render validation and refuses uploads/capture writers. Stopped shared cleanup
cancels unfinished uploads on its returned owners before removing them. The
unread-terminal-result fixture retains such an upload and checks full recovery.

Target refresh at `d43871fb965ea6fc59d53dfb41b60139a2e22735` is merged in
`090ef6b8`. Its buffered `AudioIo` interface supersedes the temporary enlarged
stack arrays; no port cap or prototype path from this branch remains. Structural
ramp/stop/activation now follows the target's separate quantum ticks. Source
fencing compares the complete control-side host mask and stages a compact changed
flag under the next graph epoch, rather than copying routing tables into commands.

Reconciliation compile fixes include the scalar resolver finish signature, current
preview/port-publication fixture metadata, and supported format-conversion quanta.
Source review restored the resolver's committed/quiescent boundary guards in the
shared same-length replacement helper and added a regression fixture. Ready/private
writers also validate a checked preparation scan bound. CI remains the sole test
execution route. No completed reconciliation behavior or timing result is claimed
until the consolidated foundation/full suites run successfully.

CI [6207](https://github.com/SanderVocke/shoopdaloop/actions/runs/41886008906)
at `10d57c09` passed native JACK acceptance and bare-metal preparation. The four
foundation packages compile and their storage/reference/codec suites pass. The
engine unit binary passed 19 tests, including the Q/partition oracles, identical
admitted-cut comparison, independent audio/MIDI routing projection, faults,
recording conflicts, saturated receipts and loaded bases beyond the history
horizon. The persistence sequence reached family publication, where its fixture
incorrectly reused consumed upload source tokens. It now provides distinct sources
for each family member and configuration replacement; final validation is pending.

The pending final proofs also assert every version/snapshot/index/capture credit,
shared reservation and queued retirement after shutdown. An additional production
fixture exhausts control and reader background-copy credits before required
capture/playback, private MIDI progress and ready publication in later ticks of
the same callback. `SharedWork` exposes remaining background credits for that
assertion. Release measurement artifacts retain per-callback MIDI overflow counts,
configuration source hashes and their complete timing/pool/work observations.

The consolidated foundation job in [run 6210](https://github.com/SanderVocke/shoopdaloop/actions/runs/41887790851/job/126776831485)
passed all four packages with warnings denied and repository policies. Its engine
binary passed **21 tests, zero failures, one explicitly ignored release
measurement**. This includes the complete persistence/family/configuration
sequence, all-credit shutdown assertions, identical admitted-cut oracles and the
background-copy exhaustion proof. Native JACK, bare-metal and all three process
inspection platforms passed on that head. These are native functional proofs;
the broader matrix, release observations and latest target merge need fresh checks.

Target `3ca192e10552afe9ca9dfe6f4482c8b5081602bc` adds delayed MIDI-output
invalidation during recovery, plus regressions at larger quanta. It is merged
without changing that behavior. The release measurement now runs in the focused
foundation job, includes admitted 64-event and refused 65-event MIDI ticks,
checks truthful recovery, and records work maxima and callback-end pool extrema
in addition to every raw callback. Those extrema are observations after callback
maintenance; they are not claimed as intra-callback high-water marks. The prepared
reservations and multi-tick maximum-host workload establish the finite capacity
bound separately.

Run 6212 passed the refreshed native functional/policy suites, JACK and bare-metal.
Its expanded release fixture found a stimulus-count mistake: a 64-event burst had
not subtracted regular packets elsewhere in the same larger-Q tick. The fixture
now counts the entire tick before choosing its burst, preserving the 64/65-event
expectations. Persistence additionally saves and retains the loaded-and-reedited
third revision through subsequent family and full replacement. Both updates need
fresh CI; no failed measurement is accepted as evidence. Capacity review also
removed a double count of the MIDI store value: its own metadata report already
includes that inline storage, unlike the audio report. Reservations and actual
allocations are unchanged.

Run [6214](https://github.com/SanderVocke/shoopdaloop/actions/runs/41890301190)
at `480a77a669b7e1ea1cf3354cddd14fab238da54a` passed the focused functional,
policy and release-measurement job, native JACK and bare-metal builds. The
[measurement/evidence record](docs/native_session_recovery/evidence/shared-fixed-media.md)
links raw artifact `13353826518`, its source/configuration attribution and complete
reported limits: 71,965 callbacks, 616 deadline misses, 2,844 MIDI overflow
diagnostics in the Q=1/128-frame case, and none in the other cases. All 128-frame
cases had zero deadline misses. Prepared metadata, reservations, work maxima and
callback-end pool extrema are documented without inferring device acceptance.
All measured configurations recover every checked credit after teardown.

The final target refresh found `820fb23994df2290ceec7cb83c6b500aafe68bce`,
merged as `59695256`. Its physical-port retirement ownership fix is preserved;
the removed ignored `Budgets::subranges` field has no remaining initialization
in our fixtures. Fixed-tick diagnostics still report tick counts. This merge and
the completion documentation require fresh final-head checks; the implementation
and measurement fixture are otherwise unchanged. No PR reviews or inline review
threads were present at the 2026-11-04 inspection. Check again before delivery;
absence is not approval. The PR remains open against the fixed target.

| Criteria | Implementation/proof owner |
| --- | --- |
| 1 | R1/R2/R5: production fixed hooks; fixed shared-media integration fixture |
| 2–3 | Existing shared store/resource tests plus R3/R5 mixed history/oracles |
| 4–5 | R3: input tap, completed watermark, boundary grab and gap fixtures |
| 6 | R4/R5: private wrapping edits and independent raw media/state oracle |
| 7 | R5: fenced candidate selection, MIDI discontinuity, replacement races |
| 8–9 | R4/R6/R7: callback work schedule, RT guards, capacity and retirement |
| 10 | R6: ordinary resource/media, codec conversion and replacement fixtures |
| 11 | R5/R8: Q/host-partition mixed end-to-end fixture |
| 12 | R1/R8: fixed-base CI triggers, final head, review and delivery |

Port inventory: `shoop_loops/tests/shared_audio.rs` and `shared_midi.rs` manually
compose interval resolver/capture/render operations. Retain their independent
sample/packet/state expectations while moving integrated execution to fixed
hooks. `tests/runtime_topology.rs` and `support/configuration_*` shared backing
fixtures need explicit Q>1 coverage; old convenience Q=1 passes alone do not
prove the new integration. Pure shared-store and codec tests remain applicable.

## Delivery record

The implementation/evidence input is
`1979fa895bb2f8fcb8c9b50f7d827e528cfde638`; the included fixed target is
`820fb23994df2290ceec7cb83c6b500aafe68bce`. This completion record is committed
on top of that input. Its resulting exact head and applicable CI/artifact links
are recorded in [PR #927](https://github.com/SanderVocke/shoopdaloop/pull/927),
which remains open against `feat/fixed-quantum-runtime`. This keeps the record
prepared before final validation without requiring a self-referential Git SHA.
The checklist records completed implementation, fixtures and validation launch;
**delivery is accepted only when that exact head's applicable checks pass**.

Run [6217](https://github.com/SanderVocke/shoopdaloop/actions/runs/41891916353)
at `76d13d2e` passed the foundation/policy/release, native JACK, bare-metal,
Linux/Windows inspection and complete Linux coverage: **2,723 passed, seven
skipped**. The focused release measurement runs separately from the ignored
ordinary-suite entry; physical-device tests and opt-in failure canaries retain
their existing scope. [Refreshed raw artifact](https://github.com/SanderVocke/shoopdaloop/actions/runs/41891916353/artifacts/13355430264)
records the same 71,965 callbacks, 193 individual deadline misses (all one-frame
callbacks), zero misses at 128 frames, and the same 2,844 Q=1/128 MIDI overflow
diagnostics. Timing variation across CI runs is not a performance improvement
claim. The earlier detailed measurement record remains explicitly attributed to
its own source. Final non-macOS platform, Node/Chrome, app/worklet/browser,
CodeQL and policy results must pass on the documentation head and are linked in
the PR; macOS alone is waived by the user's instruction above.

No reviewer submission or inline review thread was present at the latest
inspection; inspect again at delivery and resolve any arriving feedback. This
records absence, not reviewer approval. P04/P05 application actions/modes, service
selection/sync policy, UI/Lua integration and continuous replacement recording
remain open. Physical-device timing/lossless output beyond prepared quotas is
not certified. The audio dependency from #909 and fixed parent #930 remain explicit.

## Goals and scope

Carry the complete shared audio/MIDI foundation into the fixed production runtime:
continuous history, overlapping retained grabs, independently editable takes,
immutable exports, bounded replacement preparation, ordinary loop playback,
full-loop/configuration replacement and persistence. Demonstrate the same finished
black-box functionality through fixed execution, rather than a manual interval
composition or a second playback backend.

Fixed ticks are the sole execution architecture. Port or remove our obsolete
interval-driven integration and tests. Do not retain an adaptive fallback, backend
selector, borrowed interval-decision lifecycle or observation acknowledgements for
this work. Q=1 remains a genuine fixed-tick reference configuration. Independent
storage/state oracles remain useful without preserving the old execution path.

This remains foundation work. It does not enable application P04/P05 capability
flags, global grab/selection/sync policy, UI or Lua actions, a continuous Replacing
recording mode, dry/wet routing, new latency compensation, arbitrary synthesizer
state reconstruction or a physical-device deadline guarantee. Prepared engine/test
hooks and bounded internal operations may be added; new public host/service grab
commands are not required for completion.

## Immutable acceptance criteria

Goals and these criteria require explicit user approval to change.

User-approved delivery qualification, 2026-11-04: macOS checks may be ignored and
work may proceed as though they finished. Queued macOS jobs are therefore waived
for this delivery, not reported as passing. Linux, Windows, Wasm/browser, portable
and policy checks remain required. The main platform matrix now starts
independently of process inspection, preserving every inspection and platform job
while preventing a queued macOS runner from blocking independent validation.

1. **Sole fixed execution:** all integrated proofs use the current production
   fixed kernel/domain hooks, at Q=16/32/64/128 and with a fixed Q=1 reference.
   Existing fixed-engine recording, sync/program, routing, slot, recovery and
   content-family behavior remains intact.
2. **Shared ownership:** overlapping grabs retain immutable payload pages;
   independently editable siblings and old exports survive history eviction,
   edits, source removal, superseding uploads and session/configuration changes.
   Grabbing/forking does not copy a complete history window or replay its prefix.
3. **Exact media semantics:** preserve multichannel audio, partial pages, logical
   windows, loaded bases longer than history, complete MIDI packets/SysEx, stable
   equal-frame order, explicit empty duration and all 16 channels. MIDI cuts own
   state strictly before the cut; originals at the cut remain at local zero.
4. **Fixed clock and alignment:** accepted live cuts/publications use eligible
   completed core ticks, with truthful requested/effective times and the target's
   same-tick ordering. Logical loop lengths and seeks obey Q. MIDI timestamps and
   stored edit coordinates retain frame precision; no hidden rounding, padding,
   cropping or graph splitting is introduced to make a grab fit.
5. **Continuous, truthful history:** configured input histories advance while
   loops are stopped, without duplicate source appends or duplicate observation
   by the same MIDI tracker. Audio/MIDI cuts use a common completed watermark and
   valid coverage. Density pressure, storage refusal, rerouting and partial-tick
   faults never advertise missing input as complete history or replay callbacks.
6. **Replacement semantics:** fixed-length replacement/erase, including wrapping,
   preserves outside samples or packet bytes/order and existing owners. Touching
   MIDI logical zero refreshes initial state from input strictly before that
   portion. Controller changes/deleted note-offs affect the suffix naturally;
   stored edits add no synthetic restoration packets at splice boundaries.
7. **Coherent live publication:** a candidate has complete audio/MIDI sources,
   duration, initial state and indexes before selection. Refusal/cancellation or
   stale resource, loop, topology or format fences preserves the live editable
   owner and playback. Publication changes the complete media view at one tick;
   old exports remain immutable and MIDI cursor/discontinuity handling is valid.
8. **Bounded execution and readiness:** capture, reads, cuts, edits, preparation,
   publication and retirement allocate/free nothing, never block and have
   configured work/storage limits. MIDI readiness is explicit; no whole-suffix
   rebuild or next-tick readiness is assumed. Required capture/playback and ready
   publication work with ordinary background transfer allowances exhausted.
9. **Callback budgets and capacity:** ingress, optional preparation and maintenance
   retain explicit callback-wide bounds across all ticks. Capacity accounting
   includes pages, index paths, MIDI state copies, candidate/snapshot credits and
   queued retirement. Bounded cleanup eventually returns all credits, including
   after cancellation, saturated receipts, a terminal fault and shutdown.
10. **Ordinary interfaces and persistence:** resource IDs, snapshots, byte/MIDI
    cursors, bounded seek, NodePairSnapshot, Adopt/Select, exports, configuration
    replacement and the target's family publication accept shared-backed media.
    Same-format session/exact-media reload preserves supported media/state;
    explicit rate/Q conversion follows the target's conversion contract. No
    storage identity or pending edit enters file formats; existing SMF losses
    and unsupported MIDI-state diagnostics remain explicit.
11. **Integrated evidence:** independent audio/packet/state oracles agree across
    whole, one-frame and irregular host partitions at identical admitted cuts.
    Include overlapping users, ongoing mixed capture/playback, wrapped edits,
    output overflow, stale replacement, persistence and complete teardown.
12. **Delivery:** the reconciled final head includes the current target, has green
    required native/Wasm/browser/portable CI, and has addressed applicable arriving
    automated reviews. No inherited green check or Q=1-only fixture establishes
    completion. Application P04/P05 rows remain open and the PR remains unmerged.

## Design rules and constraints

- Retain `ResourceStore`, shared backing dispatch, scoped/generation-checked
  handles, immutable snapshots, render accounting and fair bounded retirement.
  Keep `no_std + alloc` and current runtime/loop/host dependency boundaries.
  Keep conversion, file I/O and application policy outside rendering.
- Use prepared per-input/tap history bindings, independent of loop-slot lifetime.
  Compose a fixed history processor/tap into the real graph, including when no
  loop records or plays. Prepare scratch for Q; host capacity is a separate
  limit. Preserve signal routing and the target's declared host buffering delay.
- Observe MIDI once in each authoritative history tracker. A separate loop
  recording tracker may still observe its own input. Do not append our
  `MidiCapture::append_history` helper after an observation by that same tracker.
  Capture initial projections before consuming the corresponding input interval.
- Distinguish host ingress, completed core ticks, logical loop position and
  audible output time. A callback shorter than Q may complete no tick. A request
  received after an earlier tick's input started accumulating cannot act
  retroactively on that tick. Zero-frame callbacks advance nothing.
- Use checked frame/range, byte-count, revision and capacity arithmetic. Preserve
  monotonic u64 history coordinates across u32 boundaries; overflow refuses
  before any published mutation rather than wrapping a time or identity.
- Expose history only through a completed watermark. If a tap writes before a
  later graph fault, uncompleted time is not an eligible common cut. Preserve old
  owners; use explicit new segments after a gap/reroute. Add bounded audio history
  restart if necessary rather than manufacturing silence or falsifying continuity.
  Reclamation pressure can delay recovery; never retry consumed input as new input.
- Route musical grabs through bounded scheduled domain settlement with one common
  cut and all needed owner/snapshot credits preflighted. Explicit reads of already
  complete retained ranges need no arbitrary musical delay. The executor currently
  boundary-gates BeginCapture/StopCapture/Snapshot, not AudioGrab/MidiGrab: define
  their interaction explicitly instead of treating callback ingress as publication.
- An edited resource root is not a playback publication. Stage edits in private
  candidate takes/forks, snapshot complete revisions, then use ordinary media
  admission and tick selection. Keep resource revisions separate from resolver
  content revisions, slot generations, topology/session epochs and rate/Q fences.
  Recheck fences at final selection; failed selection discards private candidates.
- Reuse Adopt/Select and media activation preflight. Reuse SelectFamily for actual
  dependency-family changes, respecting its membership/capture restrictions;
  do not use whole-session replacement for a local edit or pretend unrelated
  loops form a sync family. Audio and MIDI of one published content view change
  together. Source changes use the normal MIDI cleanup/seek/restoration policy;
  that playback policy is distinct from inserting packets into stored splice data.
- Retain MIDI prepare/progress/Ready/commit/cancel. Its event-step allowance is
  distinct from the audio quantum Q. Use a prepared fair job schedule with a
  reserved callback-wide work allowance and bounded transaction count. No loop
  may drain an arbitrary suffix in one tick or consume another loop's guaranteed
  capture budget. Ordinary transfer exhaustion does not starve admitted work.
- Readiness and the requested publication cut are separate. At a requested cut,
  publish only a complete Ready candidate; otherwise report a defined refusal or
  explicitly reschedule with a new effective cut. Never expose a partial index or
  silently claim the old deadline. Compare partitioned runs at the same admitted
  cuts; separately test readiness differences under different callback counts.
- Use the target's explicit offline format conversion for incompatible loaded
  media, including retained channels/history and sync dependencies. Live history
  chooses valid logical windows; it does not invoke silent resampling/retiming.
  Preserve originals until conversion and fenced publication succeed.

## Staged implementation

Dependencies: R0 -> R1 -> R2 -> R3 -> R4 -> R5. R6 follows R5; R7 follows R6;
R8 is the final gate. Within a stage, implement and verify its used interfaces
before building the next layer. Update the plan when target development changes
the named hooks; do not restore superseded execution to avoid reconciliation.

### R0 — Refresh the baseline and establish delivery

- [x] Read current target instructions/plans, refresh both branches and record
  exact SHAs, merge base, parent PR state and changed-file overlap. Inspect the
  current production preparation and fixed hooks rather than relying on earlier
  prototype documentation.
- [x] Continue `feat/shared-midi-persistence` in a clean checkout/worktree; create
  a named backup branch at its pre-reconciliation head. Preserve unrelated edits
  and retain the audio/MIDI foundation commits while merging the current target.
- [x] Record a contract-to-fixture matrix for acceptance criteria 1–12 and an
  inventory of interval-dependent fixtures/glue to port or remove.

Verification: the recorded base is the actual target head, both foundations are
present, execution scope is fixed-only, and every criterion has an owner/test.

### R1 — Reconcile source and make the new PR target testable

- [x] Resolve the merge using fixed execution as authoritative. Retain both tracing
  inventories and shared-MIDI codec tests; adapt codec helpers and source fixtures
  to current APIs. Preserve fixed conversion/family publication and the shared
  retirement fix that scans prepared stores rather than all configured slots.
- [x] Remove/port our obsolete execution glue; preserve generic backing, logical
  export and immutable ownership APIs. Do not reintroduce a production interval
  executor, old observation lifecycle or a selectable compatibility backend.
- [x] Add `feat/fixed-quantum-runtime` to relevant PR base filters in
  `.github/workflows/build_and_test.yml`, `docs.yml` and `codeql.yml`. Preserve
  the full matrix, required Node suites and pinned-Chrome library proofs.
- [x] Commit/push the reconciled baseline and retarget PR #927 to
  `feat/fixed-quantum-runtime`; ensure a supported event starts fresh checks.
  If the existing PR is no longer usable, create its replacement against this
  same target and link the superseded deliveries/dependencies.

Verification through CI: the source builds with warnings denied, storage/codec
and existing fixed tests compile/run, and checks belong to the reconciled head
and intended base. A base edit or old status alone is not CI evidence.

Implementation checkboxes in R1–R6 record completed source/fixture work. Their
native verification gates passed in run 6210 at `9f5f5886`. Portable/native JACK
checks also passed. Browser and final refreshed-head gates remain in R8; R7
requires retained release observations before its remaining items close.

### R2 — Small fixed-runtime compatibility proof

- [x] Add a Q=32 fixture using the production fixed kernel and loop processors.
  Prepare shared audio and MIDI resources, select their immutable snapshots using
  ordinary media operations, then play, seek, wrap, stop and export them.
- [x] Retain an old export and sibling owner, edit a private candidate and select
  its complete snapshots at a known tick. Assert source-change/MIDI restoration,
  unchanged siblings/exports, content revisions and correct continued playhead.
- [x] Exercise direct divisible processing and buffered host partitions, including
  callbacks shorter than Q; verify the declared delay and exact intra-tick MIDI
  offsets against independent expected output. Guard RT execution against heap use.

Verification: CI demonstrates shared backing works through actual fixed playback
and publication before expanding history integration. No fixture may succeed by
using the old process/decision/observation execution adapter.

### R3 — Continuous fixed-tick history and coherent grabs

- [x] Add prepared audio/MIDI history bindings and a fixed input tap with Q-sized
  scratch, stable source identity and explicit capacity. Wire it through engine
  preparation/graph lifetime without enabling application mode flags or policies.
- [x] Append exactly once per source/tick, including stopped loops and event-free
  MIDI intervals. Maintain pre-input state and a completed common audio/MIDI
  watermark; independent loops must share history rather than duplicate writes.
- [x] Implement boundary-aware grab settlement and all-or-nothing source admission
  for each audio/MIDI content view. Validate coverage, logical length, Q alignment,
  source identity and requested/effective cut; release partial private admissions
  on refusal without exposing a half-created result.
- [x] Handle storage gaps, density eviction, input resets/reroutes and partial-tick
  faults explicitly. Supply bounded segment restart/recovery where missing and
  retain old exports while queued cleanup settles.

Verification: CI covers overlapping users, stopped-state capture, empty intervals,
events at Q-1/Q and the cut, partial host input, requested cuts before/after
eligibility, audio-only/MIDI-only/mixed shapes, capacity refusal and recovery.
Include horizon/u32 crossings and timestamp overflow. No cut may include
uncompleted or partially advertised simultaneous MIDI input.

### R4 — Bounded private replacement and preparation scheduling

- [x] Adapt audio edit scratch/admission to the Q-sized runtime while retaining
  exact stored interval semantics, multichannel/page wrapping and zero edits.
  Build candidates without changing the live selected source or editable mapping.
- [x] Own MIDI patch bytes and pre-input state at preparation. Retain incremental
  suffix rebuilding and complete-index readiness, including logical-zero refresh,
  erased intervals, long loaded bases and competing/cancelled edits.
- [x] Add a prepared fair progress schedule and explicit callback-wide allowance.
  Keep capture/playback/ready commit independent of ordinary transfer credits;
  account event steps, state copies, index paths and private cleanup separately.
- [x] Define and implement publication-cut readiness/refusal/rescheduling outcomes.
  Preserve the committed revision until a complete candidate is accepted. This
  does not add a continuous replacement-recording writer or promise next-tick
  readiness for every admitted suffix.

Verification: CI exhausts ordinary background credits, varies callback/tick counts,
forces delayed readiness, cancellation, transaction/page/index exhaustion and
stale edits, and checks fair bounded progress without interrupting required audio.

### R5 — Atomic selection and fixed-only integration proofs

- [x] Preflight candidate snapshot/media ownership and target fences, then select
  the complete audio/MIDI view at one eligible tick through current domain media
  transactions. Preserve position/controls according to existing selection rules.
- [x] Recheck resource version/revision, loop generation/content revision, relevant
  topology/session epoch, length and format. Coordinate normal selection, family
  import and full configuration replacement; reject stale candidates intact.
- [x] Preserve MIDI renderer discontinuities, held-note cleanup/restoration,
  packet order and output-overflow recovery. Do not silently swap a cached MIDI
  index or mutate a snapshot already owned by playback/export.
- [x] Port interval-driven mixed-history/replacement fixtures to fixed execution;
  retain independent sample/packet/state oracles. Cover Q=1/16/32/64/128, host
  partitions 1/7/31 and larger/divisible blocks, several loops and repeated wraps.

Verification: CI proves no mixed revision is observable, unchanged owners survive,
and failed source admission/readiness/fences leave prior playback intact. Include
superseding uploads, slot reuse, same-length content changes, family replacement,
recording conflicts, same-tick ordering and callbacks with no complete tick.

### R6 — Persistence, conversion and existing lifecycle integration

- [x] Round-trip pinned original/intermediate/final shared media through ordinary
  exports, session archives, exact MIDI and audio/WAV; reload immutable indexed
  bases and edit them without changing the saved owners. Preserve SMF warnings.
- [x] Exercise the target's rate/Q conversion with shared-backed retained media,
  initial/preroll MIDI and sync dependents. Verify identity conversion separately
  from explicitly changed samples/timing; do not weaken unsupported-state checks.
- [x] Exercise NodePairSnapshot/Adopt/Select, bulk reads/MIDI initial queries,
  SelectFamily and ConfigurationReplace through the reconciled engine paths.
  Preserve unrelated playback/recording, current controls and accepted-operation
  ownership across cancellation, detach/reconnect and controller loss.
- [x] Complete source/tap removal, pending edit/import cancellation, saturated
  result handling and shutdown after a partial-tick fault. Drain ordinary/audio/
  MIDI retirement fairly and recover every reserved page/owner/transaction credit.
  Released handles/cursors invalidate immediately, before physical reclamation.

Verification: CI native and browser fixtures use ordinary media/transport paths,
retain exports across replacement, and assert exact supported data/state plus
bounded cleanup. Test-only preparation may install sources; it may not bypass
normal playback, selection, file conversion or ownership enforcement.

### R7 — Capacity, performance evidence and documentation

- [x] Measure prepared memory and worst-case work across Q and maximum host sizes,
  active sources/loops, retained exports, page boundaries and MIDI event density.
  Include enough ticks before callback-end maintenance to expose peak pressure.
- [x] Add reproducible full-callback measurements for history capture, repeated
  audio edits, MIDI preparation/publication and mixed playback with old exports.
  Report copy/state/index/reclamation counters, tails, maxima and every deadline
  miss; simple immutable-playback timings do not establish these costs.
- [x] Resolve violations of configured operation/storage bounds; document finite
  capacity/refusal and observed timing limits without claiming device acceptance.
- [x] Update runtime/loop documentation, tracing inventory, parity/persistence
  evidence and PLAN_SHARED_MIDI.md to link this fixed-only completion. Remove
  obsolete interval integration claims; keep application P04/P05 flags open.

Verification: CI artifacts identify source/configuration and preserve all results.
Counters and measured prepared bytes explain the retained ownership and Q-sized
work costs; no undocumented capacity increase or weakened deadline assertion
substitutes for evidence.

### R8 — Final end-to-end CI, review and delivery

- [x] Refresh/merge the latest fixed target, resolve any further changes and repeat
  affected validation. Confirm the PR base is `feat/fixed-quantum-runtime` and its
  diff retains fixed execution while containing only this foundation and necessary
  reconciliation. Keep the audio dependency explicit until present in the base.
- [x] Launch the full existing CI matrix on the delivery head: warning-denying
  native builds and Rust suite on supported platforms, required Node and Chrome
  library suites, production app/codec/worklet builds and browser workflows,
  portable bare-metal builds, Docs/CodeQL, formatting, test attributes, tracing,
  parity and dependency boundaries, with the user-approved macOS waiver above.
  Keep test execution separate from builds
  that overwrite test-enabled host executables. Record real skips/facility limits.
  Successful applicable checks on the exact delivered head remain the final
  verification gate; launching checks alone does not satisfy acceptance.
- [x] Run the mixed fixed-runtime end-to-end sequence: continuous history -> two
  overlapping grabs -> ordinary loop selection/playback -> delayed private wrapped
  edits -> coherent tick publication -> retained exports -> save/reload/edit ->
  full replacement and teardown. Assert independent oracles, correct clocks,
  old-owner survival, bounded work and complete credit recovery.
- [x] Inspect automated reviews after publication and fixes; address applicable
  arriving feedback and repeat affected CI until the reviewer approves. If none
  arrive, record that fact without claiming approval. Resolve review threads and
  keep the plan/PR description consistent with the final implementation.
- [x] Prepare the final source/base, evidence scope, CI/artifact links and remaining
  application gaps in this completion record before final CI. Record the resulting
  documentation-head SHA and its actual checks/artifacts in the PR description
  before delivery. Any later
  completion-record commit must itself receive applicable final-head checks.
  Leave the PR unmerged.

Validation routing: the user requested relying on CI. For this reconciliation,
CI runs the focused and full checks above; do not duplicate the matrix locally.
This overrides local-before-push defaults for this task while preserving all
functional gates. Follow the current branch's build/test commands and provisioned
CI tools. Failed checks require fixes and fresh evidence, not weaker assertions.

Completion requires the final end-to-end sequence through the sole fixed runtime,
on the delivered tree, with green applicable CI and resolved arriving review.
A textual merge, passing old CI, storage-only tests, Q=1-only execution, manual
renderer composition or an unfinished MIDI index builder is not completion.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
