# Finish application Grab and Replacing parity

## Status and delivery

Execution started on `feat/finish-grab-replace` from fixed-runtime head
`2e19210bdbb5e32510322f8b8344715ce7cbefe5`. F0 is complete; F1 behavior and capacity proofs are in progress. Written using
[write_plan.md](.agents/prompts/write_plan.md). This plan finishes **P04 and P05
in the normal application**, including capability advertisement and parity-row
closure. Completing another internal engine demonstration does not finish it.

Authoring checkout: `f9075999e996ca56cfd30ff561cf85626c0df85d`.
[Foundation PR #927](https://github.com/SanderVocke/shoopdaloop/pull/927) is merged
into `feat/fixed-quantum-runtime` at `bfea527b9ed12798c0f05a17606a2c81ad45b5db`.
Its parent [PR #930](https://github.com/SanderVocke/shoopdaloop/pull/930) is still
open against `feat/backend-replacement` at inspection. Refresh this information
before implementation. Create `feat/finish-grab-replace` from the latest fixed
target, or from `feat/backend-replacement` if that target has landed. Open a new
PR against the branch containing that architecture; do not reuse closed #927.
Retarget onto `feat/backend-replacement` after the parent lands, reconciling its
actual final changes.

This plan succeeds [PLAN_RECONCILE_GRAB.md](PLAN_RECONCILE_GRAB.md) and
[PLAN_SHARED_MIDI.md](PLAN_SHARED_MIDI.md) for application delivery. Retain their
shared ownership, exact media, fixed-clock and bounded-work contracts. Their green
checks validate the foundation, not completion of this application work.

## Execution record

F0: refreshed the parent on 2026-11-04. PR #930 remains open; the merged
foundation and BufferedProcessor delivery (#937) are present in target
`2e19210b`. The buffering layer is additive and leaves the production loop hooks
unchanged; request/effective-time tests will include its declared timing when used.
The new branch preserves the untracked plan from the prior checkout. Draft
[PR #940](https://github.com/SanderVocke/shoopdaloop/pull/940) targets the refreshed
fixed branch. [The application contract](docs/native_session_recovery/grab-replace.md)
records the gate inventory and reference behavior. A checked portable musical
window resolver pins the cycle/phase, prefix-continuation, explicit padding and
u64 range semantics before engine settlement uses them. It does not enable a
capability or publish resources. F1 remains open until production routes and the
capacity/admission policy are verified. Builds and
tests run in CI under the user's standing instruction; local Cargo formatting is
editing only. P04/P05 remain OPEN during implementation.

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

## Findings that determine the work

| Layer | Existing support and remaining gap |
| --- | --- |
| `rt_runtime` / `shoop_loops` | Shared audio/MIDI history, immutable grabs, private edits, indexed imports and fenced publication exist. The resolver has Stopped/Playing/Recording only. A private MIDI candidate must become Ready before another patch; suffix preparation is callback-budgeted. This is not a continuous replacement writer. |
| Production preparation | `shoop_rt_engine::prepare` passes empty `SharedSettings`. Native and worklet hosts use that path. The foundation's short histories and large MIDI metadata reservations are test configurations, not production defaults. |
| Engine protocol / adapter | No negotiated Grab or Replacing capability/operation exists. `shoop_rt_backend::grab_loops` refuses Grab; its mode conversion refuses Replacing. |
| Service / clients | Grab, AdoptRingbuffers, GrabClicked, default-recording Grab and Replacing remain refused. UI and Lua consequently cannot reach the primitives. Several media/read/cache guards recognize Recording only. |
| Documentation | P04/P05 correctly remain OPEN. Their remaining-work descriptions must be replaced with actual application evidence before either status changes. |

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

## Goals, scope and immutable acceptance criteria

Goals and these criteria require explicit user approval to change.

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
existing limits and refusals for composites, FX/dry-wet, latency/timing editing,
duplication and unsupported MIDI initial/preroll state under their own parity
rows. Do not enable those capabilities as a side effect. This plan adds neither
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

## Implementation stages

Dependencies: F0 -> F1 -> F2 -> F3 -> F4 -> F5 -> F6 -> F7 -> F8.
Verify each used interface before building its dependent layer. P04/P05 flags may
be enabled separately after their own public-route gates pass; final completion
requires both. Keep the checklist and evidence current throughout execution.

### F0 — Establish the new delivery

- [x] Refresh the merged foundation, target instructions and parent status; record
  actual source/base SHAs. Create the new feature branch and open/push its PR as
  described above, preserving unrelated work. Establish CI on the actual base.
- [x] Inventory every refusal, capability gate, mode conversion and production
  constructor named above; include service media/persistence guards, adapter
  canonical-import caches and configuration/family capture exclusions.

Verification: the new branch contains the merged foundation and current fixed
kernel, and the new PR has the correct live parent. No closed-PR continuation or
legacy production backend is used.

### F1 — Freeze app behavior and production limits

- [ ] Extract a compact behavior-to-test matrix from reference `grab_targets_inner`,
  `grab_window`, `adoption_window`, BasicLoop and audio/MIDI channel replacement.
  Include explicit versus selected targets, all global policies, phase/length,
  startup/missing-history padding, empty/no-sync cases, trigger tracks, monitoring,
  pending transitions, wrapping and writer-to-writer mode changes.
- [ ] Port meaningful reference expectations into production test scenarios:
  `grab_policy_covers_targeted_selection_solo_and_immediate_completion`,
  `production_keyboard_default_action_covers_stopped_cancel_and_grab_policy`,
  relevant `expanded_loop_actions_route_modes_grab_and_balance`,
  `fake_grab_preflights_every_target`, and channel replace/erase/wrap tests.
  Keep unrelated dry/wet expectations under P06.
- [ ] Specify request ownership, requested/effective cuts, preparation/finalization,
  cancellation, insufficient history, admission failure and pressure outcomes.
  Select production budgets/defaults with a calculation for default and largest
  supported track/channel pools, 30-second retention and loaded MIDI density.

Verification: each acceptance criterion maps to a concrete public-route test and
named lower-layer proof. A material departure from reference behavior or an
additional feature-scope exclusion requires user approval, not a ledger footnote.

### F2 — Prepare and expose real input histories

- [ ] Thread shared preparation through EngineDescription, host settings and the
  normal engine constructor used by dummy/CPAL/JACK/native child and AudioWorklet
  hosts. Version worklet configuration and surface finite limits/readiness.
- [ ] Implement source-slot activation/reset/removal, global page/index/state
  allocation and bounded retirement. Preserve track/slot generations, full routing
  masks, independent MIDI input projection and source coverage across graph cuts.
- [ ] Provide the intended retention without duplicating histories per loop.
  Resolve MIDI metadata scaling before choosing default capacities; account for
  retained cuts, simultaneous writers and existing ordinary recording/imports.

Verification: normal production constructors capture while all loops are stopped,
with one input observed once. At real default pool sizes, preparation fits the
documented budgets; 30-second and density-pressure cases are truthful. Dynamic
tracks, rerouting, MIDI reset during a partial tick and teardown retain all owners
and recover every credit. Test audio-only, MIDI-only, mixed and silent inputs.

### F3 — Finish the continuous RT writer and mode lifecycle

- [ ] Add fixed-domain/engine Replacing transitions and state without confusing
  them with the existing one-shot content Replace operation. Update cycle/phase,
  sync, repetition, pending transitions and mode-dependent output decisions.
- [ ] Add growing capture from an adopted prefix, preserving duration/data/state
  and consuming the first live tick exactly once. Retain fixed-history, long loaded
  and previously replaced bases through the ordinary resource/media interfaces.
- [ ] Implement the continuous COW audio/MIDI writer described above. Own every
  admitted tick; handle erasure, wrap, logical-zero projection and suffix state.
  Preserve old views and make complete new views available at valid exit cuts.
- [ ] Prove bounded preparation debt and guaranteed exit resources. Cover long
  loaded MIDI with more suffix events than one optional preparation allowance,
  sustained maximum admitted input, multiple active loops, immediate exit after
  one tick and many wraps. Fix representation/scheduling limits uncovered here;
  do not proceed with a writer that loses input while CandidateStatus is Building.
- [ ] Handle Stop/Playing/Recording, scheduled cancellation, capture exhaustion,
  clear/removal and terminal fault; emit truthful terminal outcomes and release all
  private state. Keep unrelated playback and ordinary recording working.

Verification: allocator-guarded production fixed execution agrees with independent
samples/packets/state for Q=1/16/32/64/128 and whole/one-frame/irregular callbacks.
Replacing output matches reference monitoring semantics. Prefix continuation grows
beyond the history horizon; replacement stays fixed-length. Required work survives
exhausted ordinary transfer budgets, and every cancellation/fault frees its credits.

### F4 — Deliver engine operations through the public protocol and adapter

- [ ] Add negotiated Grab/Replacing support and bounded batch preparation/commit,
  status, cancellation and completion records to `shoop_rt_protocol`; update codecs,
  limits, carrier bounds and malformed/old-peer tests. Keep public control actions
  stable where possible; extend internal backend intent when needed to carry an
  RT-resolved target phase instead of a stale sampled position.
- [ ] Implement engine admission and settlement for all selected sources/targets
  at one cut, including post-mode/position, prefix continuation, finish plans and
  solo effects. Preflight snapshots, writers, media, receipts and transitions;
  rollback the whole refused batch before changing any loop or peer.
- [ ] Implement `shoop_rt_backend::grab_loops`, Replacing conversion, pending job
  ownership, completion correlation and mirror/cache updates. Support typed, socket,
  supported shared-memory and MessagePort carriers with identical semantics.

Verification: actual protocol/adapter requests drive ordinary production hosts.
One invalid/stale/full target leaves every target and solo peer unchanged. Admission,
completion and resync agree on mode, next mode, phase, length, revision and cut.
Old peers remain correctly unsupported; messages never exceed negotiated bounds.

### F5 — Restore authoritative service policy

- [ ] Implement GrabClicked, default recording Grab, global controls and explicit
  Grab/AdoptRingbuffers commands through one service-owned job path. Port target,
  selection, sync/apply-N, solo and play-after-record policy, including unsynced
  adopted-prefix recording and its scheduled completion.
- [ ] Admit Replacing through explicit/triggered/scheduled mode commands; include
  it in the appropriate writer, cancellation, solo, sync and media guards. Keep
  explicit loop lists distinct from selection-aware gestures and preserve ordering.
- [ ] Fence pending work against deletion, slot reuse, routing/source reset, target
  length changes and session replacement. Reconcile UI loss after admission and
  before completion without resending or recomputing the accepted operation.

Verification: production service + RT adapter passes the F1 behavior matrix, not
just FakeBackend operation assertions. Real audio/MIDI exports prove correct
windows and modes. Failure preserves confirmed state; a reattached client observes
the same admitted operation and final media exactly once.

### F6 — Enable the actual UI, keyboard and Lua workflows

- [ ] Derive each capability from negotiated, implemented host support; remove its
  production service/adapter refusal only after the corresponding route works.
  Keep per-request warmup/capacity errors distinct from unsupported capability.
- [ ] Wire normal loop/global Grab controls, default-action settings, targeting,
  keyboard bindings and Lua Grab/AdoptRingbuffers/LoopMode_Replacing through that
  service path. Make Replacing discoverable in the appropriate normal controls;
  show its actual mode, pending transitions, results and media refresh behavior.
- [ ] Update production UI/runtime tests to assert capabilities and enabled
  controls after normal startup. Preserve refusal tests for unsupported peers and
  other parity features. Exercise real clicks, bundled keyboard script and public
  Lua functions rather than injecting internal domain operations.

Verification: a normal launched application can create a supported track, receive
input while stopped, click Grab, enter/exit Replacing, and play/export the result.
Equivalent UI, keyboard and Lua actions produce equivalent confirmed data/state.
No test manually installs histories or changes the capability map to pass.

### F7 — Persistence, recovery and sustained-capacity acceptance

- [ ] Audit active and scheduled Replacing across media reads, save/export,
  imports, family updates and full session replacement. Pin complete immutable
  exports or refuse according to the existing active-writer policy; never save a
  partial index or stale canonical-import cache as current content.
- [ ] Round-trip grabbed, prefix-continued and repeatedly replaced media and
  supported default-Grab settings. Reload stopped loops, then grab/replace again.
  Retain old exports during conflicting imports and configuration replacement.
- [ ] Exercise detach/reconnect, controller loss, abandoned preparation, saturated
  results, source removal, writer pressure, reset/overflow during partial input,
  runtime fault and final shutdown. Count all pages, versions, MIDI state/index
  slots, jobs, receipts and retirement through sustained simultaneous capture.
- [ ] Measure prepared memory and complete callbacks at production defaults and
  admitted worst shapes/densities. Preserve tails, maxima, all timing misses and
  output overflow diagnostics; resolve configured work/storage violations without
  undocumented capacity increases or weaker functional assertions.

Verification: public-route save/load/edit and recovery retain exact supported
media/state, old owners and request identity. No histories or writer internals enter
file formats. Repeated operations reach a stable bounded resource usage and fully
recover at teardown. Reports identify actual source, configuration and limits.

### F8 — End-to-end delivery and parity closure

- [ ] Run the complete public sequence in packaged native and browser apps:
  stopped input -> selected/targeted Grab -> unsynced prefix continuation ->
  playback -> continuous audio/MIDI Replacing across wraps -> scheduled exit ->
  old/current exports -> save/load -> repeat edit -> UI detach/reattach -> teardown.
  Include normal button, keyboard and Lua routes, stereo, MIDI-only/SysEx, mixed
  tracks and multiple sources/users. Verify actual packets/samples and outcomes.
- [ ] Push meaningful milestones and obtain fresh applicable CI on the final
  source/base: warnings denied, full Rust suites, native hosts/drivers/carriers,
  required Node and Chrome libraries, real session/AudioWorklet/browser workflows,
  portable builds, formatting, tracing, test-attribute, parity and dependency gates.
  Inspect failures/artifacts and fix causes. Refresh/reconcile the target before
  final validation; distinguish any later unvalidated consolidation explicitly.
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

Verification routing: use CI for builds/tests, as requested by the user. Do not
duplicate checks locally or substitute prior foundation CI. Honor standing user
validation/waiver preferences where applicable and record actual outcomes. The
previous final-consolidation no-wait instruction does not establish application
validation for this new feature work. Additional waivers must be explicit; no
unexecuted checks are labelled green. Writing this plan runs no implementation CI.

P04/P05 must remain OPEN until their respective application acceptance gates pass.
Finish all stages before reporting full integration; partial RT progress must be
reported as partial progress, with the remaining user-facing behavior explicit.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
