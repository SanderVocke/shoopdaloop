# P03: composite loops through the production session service

## Status and delivery

Implementation in progress on `feat/p03-composites`. Written using
[write_plan.md](.agents/prompts/write_plan.md) for P03 in the
[feature parity ledger](docs/native_session_recovery/feature-parity.md).

- Inspected implementation base: `feat/fixed-quantum-runtime` at
  `bbf027e98c4d1449f775019585acafd44728bd71`.
- Reference: `master` at `92079fb6f00308be0f6b407b7e8c02229662e420`, with
  executable application behavior retained in `shoop_app_reference`.
- Create `feat/p03-composites` from the current `feat/fixed-quantum-runtime` head
  when execution starts. Target **`feat/fixed-quantum-runtime`** with the PR;
  recheck the base and any integration-branch changes before starting. Do not
  target `master` or merge without instructions.
- The user-approved execution contract below replaces source-tick-driven
  composite progression. Reference code is evidence for preserved editor,
  control and persistence behavior; it does not override this timing decision.
- Execution was authorized by the user's goal to complete this plan. The branch
  starts at the inspected base; the planning checkpoint is commit `18f7a5b1`.

## Goals, scope and dependencies

Restore regular and explicit-mode/script composites, UI editing, Lua composition
and control, composite-aware auto-arm, persistence and UI recovery through the
production client/service/RT stack. A composite is a loop whose content is an
arrangement: it has a prepared frame timeline that keeps moving independently of
the sync source once started. Synchronization aligns launches and repeats.

Include conversion of an empty primitive slot to a composite, serial/parallel
composition, positioned insertion, nesting, stable event identities, group
move/copy/delete, duration overrides, kind and event-mode changes, transport,
live edits, child independence, confirmed state and all corresponding file paths.
Copying an editor event is in P03; duplicating recorded media remains P11.

Include the shared primitive/composite repeat-alignment policy required by the
agreed contract. Current replacement primitives wrap independently, so this is
real loop-library and production work, not an assumption that primitive parity
already implements it. Preserve unrelated recording, playback and media behavior.

Dependencies and closure boundaries:

- P01/P02 already provide capture, document conversion and atomic session
  replacement. Extend them for composites; closing their other dependencies is
  unnecessary for this work.
- P05/P06 supply replacing and dry/wet modes accepted by reference script events;
  P06/P07 supply the corresponding track/processor topology. The current loop
  library exposes only stopped/playing/recording. Deliver and validate the core
  direct-track composite path first, then integrate those modes when their
  underlying features are available. Keep per-mode capability diagnostics.
  **Do not mark full P03 parity COMPLETE while these composite-mode interactions
  are absent.** Record a named partial restoration, with P03 OPEN, until the
  dependencies are restored or the user explicitly approves narrower closure.
- Lua VMs remain client-local. An accepted composition or scheduled transition is
  shared work; future commands from a lost VM are not accepted work. This plan
  does not restore VMs, restart scripts automatically, or require P16 to restore
  ordinary Lua composition commands.
- Retain existing exclusions: host-crash recovery/disk durability, browser-page
  crash recovery, simultaneous controlling clients, remote deployment and
  shared-memory reattachment. Do not add DSP time-stretching, tempo-following
  arrangements, arbitrary live processor construction or a second scheduler.

## Immutable acceptance criteria

Goals and acceptance criteria require explicit user approval to change.

1. **One authoritative clock.** Every composite progresses on rendered engine
   frames, settled at the configured quantum. Neither source pulses, application
   elapsed time, UI polling, Lua execution nor child positions advance its internal
   timeline. The configured quantum and host buffering remain the timing limits.
2. **Stable prepared timing.** The accepted plan fixes event frame spans, its time
   unit and total duration. A sync-source stop, restart, phase change, reassignment
   or length change cannot stretch, freeze, seek, truncate or silently recompile
   the current arrangement. Child media is not time-stretched. A new timing basis
   requires an explicit accepted arrangement timing update.
3. **Launch and repeat alignment.** Immediate and source-delayed commands retain
   their distinct admission/delivery semantics. A regular composite completes its
   own pass before repeating. With repeat alignment disabled or the source inactive,
   it repeats independently; with alignment enabled and the source playing, it
   waits at its end for the next source boundary. A coincident end/source boundary
   repeats without a gap; earlier source boundaries do not cut the pass short.
   Stopping the source while waiting releases the wait at the next settled quantum.
   The same policy applies to primitives. Reattachment does not retrigger anything.
4. **Deterministic execution.** Starts and nested starts establish the first child
   snapshot at the effective frame. Stop, seek, retrigger, clear, simultaneous
   source/control/program events and competing parents have deterministic existing
   precedence. Preserve contiguous same-mode playback when no gap/retrigger is
   required. An empty arrangement starts as a stopped success; explicit-mode
   programs finish one pass and stop. Regular composites retain their Play/Stop
   outer-mode restriction; explicit events may record children.
5. **Editor and Lua behavior.** Existing public intents and unchanged Lua
   composition/control calls reach the service and the same confirmed state.
   Serial/parallel order, positioned events, natural/forced durations, nesting,
   stable instance IDs and group relative positions survive edits and round trips.
   Invalid identities, cycles, modes, arithmetic and capacities refuse atomically.
6. **Independent children.** Playing, stopping, recording or seeking a child
   independently cannot move or stop its parent's clock. An explicit winning child
   command wins arbitration at that frame; later authored parent actions can still
   affect it. Unrelated loops and primitive repeat policies remain unchanged by
   composite commands. Clear/removal respects reference ownership rules.
7. **Live edits.** Retain immediate stopped/pending edits, safe future-only adoption
   before wrap, deferred edits at an actual pass restart, and explicit structural
   reconfiguration behavior. Pending and active versions are distinguishable.
   Rejected, cancelled, stale and superseded edits leave the working arrangement
   intact and produce reliable outcomes. No old-plan child action may leak at an
   atomic activation boundary.
8. **Auto-arm.** Automatic monitoring demand follows actual admitted recording
   sections, pending starts and active plan versions, including nested/overlapping
   parents and repeat waits. Preserve manual/prearmed monitoring; release only
   automatically owned demand. UI loss and slow UI polling cannot miss the first
   captured quantum or leave stale automatic ownership after completion/removal.
9. **Authoritative operations and projection.** Service admission, RT staging,
   scheduled application and terminal results are distinguishable. Snapshots and
   resync contain documents, timing basis, mode, playhead, iteration/pass state,
   repeat wait, next scheduled transition, active/pending versions and children
   coherently. UI and Lua never claim a refused candidate is active.
10. **Persistence.** Structure-only and media-bearing save/load preserve nested
    arrangements, event IDs, effective prepared timing and repeat policy. Fresh
    live IDs remap all references; loaded loops start stopped. Sample-rate
    conversion preserves durations in seconds within quantum alignment. Legacy
    documents receive deterministic migration. Unsupported/invalid/oversized
    documents and pre-commit failures leave the running session intact.
11. **Recovery and lifetime.** Accepted playback, delayed commands, edits and
    automatic monitoring continue under service/host ownership when the UI is
    lost. Socket reconnect reconciles results and projection without replay, VM
    reconstruction or automatic script restart. Retired/stale generations cannot
    control reused slots; clear, removal, New Session and shutdown reclaim owners.
12. **Bounded, portable production support.** Prepared limits cover programs,
    entries/segments, relations, candidates, outcomes and retirement. At-limit
    supported requests succeed; excess work refuses before acceptance. Render and
    control-endpoint paths remain allocation/free/lock/wait/I/O-free. Native typed,
    supported native carriers and browser/worklet behavior pass the final checks.
    Capability publication and ledger closure follow evidence, including the
    dependency qualification above.

## Composite execution contract

Implement and document this contract in the loop library, production contracts
and tests. It intentionally supersedes statements that a stopped source stalls
a running program or that nominal units cannot advance programs.

### Preparation and frame progression

- Resolve authored cycle positions, explicit durations and natural child durations
  off-render into an immutable sparse program. Make `nominal_unit_frames` the real
  prepared duration of one iteration, or rename it consistently to reflect that
  meaning. Retain indexed segments; do not expand durations into per-frame actions.
- Capture an explicit timing basis. For cycle-authored arrangements, use the
  stored basis or a valid sync length at initial preparation. Ordinary editor
  operations retain that basis; the current source length is not a live tempo
  input. Expose a deliberate timing-basis update through authoritative admission.
- Serial/parallel Lua composition without an established cycle grid uses section
  durations from the maximum child frame length, with sequential sections summed.
  Use a quantum-aligned unit capable of representing those spans exactly, including
  the quantum itself. A missing/empty source must not make preparation impossible;
  use an explicit stored basis, or the prepared quantum for a newly gridless plan.
  Display that basis honestly rather than inventing a musical bar duration.
- Keep all effective positions, units and spans quantum-aligned using the existing
  validation/diagnostic policy. Never use a one-frame fallback in a Q=32 engine or
  silently round an incompatible document. Preserve requested/effective timing.
- On applied start/retrigger at frame F, enter iteration zero and emit its
  authoritative child snapshot at F. Advance after each prepared unit of rendered
  time. Reuse the resolver frame and `CompositeState.entered_at`; add only state
  needed for position/repeat waiting and transactional correctness.
- Frame progression and actions occur in preview scratch. Capture/resource/graph
  refusal and retries cannot advance time, mutate committed cursors, emit duplicate
  actions or publish a candidate. Finish advances one whole quantum; zero callbacks
  do nothing and callback-end settlement belongs to the next nonempty callback.
- A composite's own start/pass-wrap edge is distinct from incoming alignment
  pulses. Internal iteration advancement must not fabricate synchronization pulses
  for other loops. Audit relay/trigger expectations under this distinction.

### Launch, pass end and repeat waiting

- Keep configured sync source, scheduled launch timing and repeat-alignment policy
  distinct. Do not encode repeat enablement solely by deleting the source relation.
  Preserve the existing `(skip + 1)` source-edge countdown meaning for delayed
  commands. A delayed command awaiting a stopped source remains pending under its
  command contract; this never stalls an already-running frame timeline.
- A normal pass advances regardless of source state. Only its end consults repeat
  alignment. An enabled, actively playing source can hold the loop at its end;
  an absent/inactive source cannot. Reassignment/length/phase changes affect future
  alignment edges without modifying the prepared plan.
- Represent waiting explicitly while retaining a playing transport mode and an
  end-position playhead. End outgoing authored occurrences at their declared
  boundaries, using ordinary child arbitration. Recording must end at its authored
  boundary, even if restart waits. Do not accidentally let children run through a
  gap or stop independently controlled children outside the declared actions.
- Increment completed-pass counts once at pass end. Enter iteration zero only
  when a new pass actually starts; idle waiting is not repeated zero-entry.
  At a coincident end/restart, preserve legitimate same-mode continuations. A real
  gap ends them and requires a new entry snapshot when playback resumes.
- Stop clears the wait and relevant pending control; seek/retrigger establishes
  a fresh authoritative cursor/phase. Explicit one-pass and empty programs never
  enter the regular-repeat wait. A waiting old plan may adopt an edit at its real
  restart or after Stop; waiting alone does not authorize playing its new first
  section. Define any stopped/finished candidate publication separately from start.
- Use the same end/alignment policy for primitives and composites, including the
  case where a source stops while a follower is already waiting. Recording itself
  keeps accumulating frames until an explicit or scheduled transition ends it;
  changing/stopping the source never implicitly ends a take.

### Edits, dependencies and ownership

- Source length changes never trigger automatic composite retiming. Ordinary
  child-content changes must not mutate an active program in place: when natural
  duration dependencies need refresh, prepare a versioned service-owned update and
  activate it only at the permitted boundary. Preserve the active timing snapshot.
- Classify edits from compiled old/new frame spans and dependency shape. Adopt a
  future-only edit before wrap only if it preserves the past/current state and the
  protected next prepared unit. Otherwise defer to an actual pass restart.
  Structural identity/node-kind changes use an atomic configuration transaction;
  explicit stop/restart effects must be reported, never hidden as a timing refresh.
- Preserve boundary arbitration, full seek snapshots, removed-target cleanup and
  direct-child precedence. Reuse owning `ProgramActivate`/`ProgramGraphActivate`
  and configuration transactions, extending their eligibility rules as needed.
  Actual restart probes must understand frame progression and repeat waiting.
- Keep supersession/cancellation bounded, including the library's single retained
  program owner. Collect each displaced/cancelled owner and terminal result;
  cancellation racing activation must reconcile the version actually installed.
- The service owns authored documents, timing basis, repeat intent, operation
  records and automatic-monitoring ownership. The adapter owns app/engine mappings;
  the resolver owns execution. UI drafts and Lua VMs are client-local. No execution
  fallback to `advance_script_compositions()` or application elapsed-time pumping.
- Maintain reference precedence and document the exact preservation/difference
  matrix. Retain old-stack fixtures as historical evidence; update new-library
  tests whose expected tick-stall behavior is deliberately superseded. Do not
  delete meaningful coverage or rewrite old evidence as if it tested the new contract.

## Implementation map and design constraints

| Layer | Required changes |
| --- | --- |
| `shoop_loops` | Prepared units, frame cursor/progression, shared repeat policy and wait state, own boundary events, seek/nesting, program-change eligibility and guarded tests/examples/docs. |
| `shoop_rt_protocol` | Bounded composite lifecycle/configuration/control, repeat policy, limits, versioned staging records, receipts, telemetry/resync and replacement records. |
| `shoop_rt_engine` | Composite-capable model and timing encoder, distinct primitive bindings, prepared program capacity, configuration/program jobs, projection and replacement. |
| `shoop_rt_backend` | Composite IDs/slot mappings, former unsupported methods, staged configuration/results, confirmed mirror/resync, persistence and retirement. |
| `shoop_app_protocol` / `shoop_app` | Bounded editor commands, canonical composite data, active/pending timing versions, admission/results, auto-arm, lifecycle, capture and replacement mappings. |
| `shoop_app_client` / `shoop_egui` / `shoop_scripting` | Existing intent routing, composite details/playhead/wait state, meaningful capability checks, public UI/Lua behavior. |
| `shoop_session` / `shoop_persistence_codec` | Preserve/migrate timing basis and repeat intent, validate nested references, resample prepared timing and remove composite refusal only when supported end to end. |
| Native/worklet hosts and setup | Carry negotiated capacities/config versions, retain callback ownership and all existing carrier/recovery boundaries. |

The production encoder currently emits primitive-only topologies, prepares
primitive bindings for every slot, and allocates program-entry/segment limits of
one. Replace those assumptions deliberately. Composites are timing-only: no audio,
MIDI capture/playback nodes or host ports belong to the composite itself. Converting
an empty primitive slot must retire its primitive bindings atomically; stale
generations and failed conversion must leave no orphaned processors or metadata.

Select and document finite defaults and aggregate memory/graph/staging budgets
before implementation. Include nesting/action relations, live plus candidate plans,
retained results and displaced owners. Negotiate actual limits to UI/Lua and file
preflight. Do not silently shrink existing media/loop/bus capacities, impose musical
wave/event quotas, renew callback budgets per quantum, or claim deadline guarantees.
Keep compilation/allocation/destruction off-render. `rt_runtime` remains generic
and free of application or musical policy.

Persist the accepted timing basis/effective spans sufficiently to reproduce a
plan after source changes and save/load. The existing cycle-only document is not
enough by itself once its source length diverges. Extend the existing format and
migration path as required; legacy missing-basis documents derive it deterministically
from saved session information, not the currently running destination session.
Ordinary save/load or rename must never rebase timing. Retain file capability
diagnostics, active-recording save refusal and private atomic replacement staging.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

## Staged implementation

Execute in order. Stages 1-3 establish library/engine contracts; Stages 4-6 add
authoritative application ownership; Stage 7 completes persistence; Stage 8 exposes
the supported production feature. Stage 9 is the full-P03 dependency gate. Stage 10
validates and delivers the final implementation. Partial delivery must retain OPEN
status and its named limits.

### Stage 0 — planning checkpoint

- [x] Read repository rules, planning/build/test guidance and architecture; inspect
  reference, loop library and production refusal paths; recheck the feature head.
- [x] Record the agreed frame-driven progression and shared repeat-alignment contract,
  immutable criteria, cross-feature dependencies and staged verification.
- [x] Verify this document's local links, coverage and `git diff --check`; record
  documentation checks separately from implementation evidence.

### Stage 1 — branch, contract fixtures and capacity decisions

- [x] Create the implementation branch from the refreshed target head, preserve
  unrelated changes, and commit/push the plan. Reconcile this baseline with any
  intervening changes before editing production code.
- [x] Write the maintained composite contract and a preservation/difference matrix
  covering frame timing, repeat waits, pulse meaning, live edits and mode dependencies.
  Link it from the parity ledger and architecture.
- [x] Define immutable prepared timing, explicit repeat policy, wait/projection state,
  version/fencing rules, finite capacity defaults and resource/graph budgets.
- [x] Add an independent frame-based scalar oracle and table-driven contract cases.
  Register meaningful native/Wasm tests using the repository test attribute.

Verification: oracle examples include unequal section lengths, Q=1 and Q=32,
source stop/length/phase changes, an end coincident with a source edge, and Stop
while waiting. Review every immutable criterion against a stage/test destination.

### Stage 2 — loop resolver, shared repetition and owning program changes

- [x] Implement frame-driven program advancement, explicit playhead/wait state and
  start/pass boundary publication. Preserve snapshot/arbitration behavior and sparse
  compilation; avoid parallel frame-clock/source-clock execution modes.
- [x] Implement shared primitive/composite repeat alignment, active-source fallback,
  release of an existing wait when the source stops, and complete outgoing actions
  at pass end. Keep delayed commands independent from internal progression.
- [x] Update nesting, iteration seeks, current-position reporting, clear/Stop,
  cancellation, exhaustion, supersession and capture preflight for the new cursor.
- [x] Extend safe future-only adoption and real-restart/stopped publication, including
  waiting-state probes, empty candidates and atomic program/graph updates.
- [x] Update affected new-library fixtures, independent loop hosts/examples and
  current guides. Record intentional contract differences; keep historical fixtures.

Verification: debug/release `shoop_loops` composites, program changes/boundaries,
runtime/configuration, capture and graph tests. Compare state, audio/MIDI and ordered
actions with the scalar oracle across whole/single/irregular callbacks, all supported
quanta, nonaligned host partitions and zero callbacks. Cover nested/multiple parents,
direct child override, recording across source changes, alignment gaps, pending
commands, candidate refusal/retry and stale generations under active heap guards.
Run the affected independent native and Wasm/Node/browser loop scenarios. No
callback-size-dependent expected values or unexplained media rebaselines are allowed.

### Stage 3 — RT protocol and production engine

- [x] Add versioned composite lifecycle, bounded plan staging/control, repeat-policy,
  outcome and resync records. Carry finite capacities through engine/host/worklet
  preparation and client negotiation; reject malformed or incompatible versions.
- [x] Generalize primitive-only model/encoder assumptions and binding/configuration
  preparation. Use timing-only composite slots and actual action/sync dependency
  validation; handle empty-slot conversion, removal and generation reuse atomically.
- [x] Connect existing program/configuration/graph transactions to composite jobs,
  effective-frame execution and owning returns. Preserve unrelated request progress
  while an edit waits, and reclaim failed/retired candidates off-render.
- [x] Publish coherent playheads, active children, repeat waits, active/pending plan
  versions, scheduled transitions and complete resync state. Extend private session
  replacement preparation without publishing partial plans or graphs.

Verification: typed/binary codec equivalence, invalid/count/size/version/epoch cases;
guarded engine creation/conversion/control/edit/removal tests; at-limit and excess
capacity; live and staged ownership pressure; simultaneous graph edits and recording;
audio/MIDI output and complete resync. Exercise native and actual worklet record
delivery. Confirm unchanged callback-wide budgets and generic dependency boundaries.

### Stage 4 — RT backend adapter and confirmed mappings

- [ ] Implement composite creation/configuration/transport/options/removal methods
  currently returning `CompositeLoops` unavailable. Reserve app IDs and generational
  slots until confirmation, and map primitive/composite targets without ambiguity.
- [x] Extend request/result accounting, mirror/resync and active/pending plan metadata;
  retain scheduled/accepted work and return terminal supersession/refusal correctly.
- [ ] Implement conversion rollback, clear/removal/track-removal cleanup, source
  disappearance handling and capture/replacement mappings without stale references.

Verification: adapter tests through typed and encoded delivery for delayed activation,
refusal, out-of-order observations, resync, slot reuse, cancellation racing activation,
displaced-owner reclamation and preservation of unrelated loops/media/controls.

Adapter milestone: negotiated prepared-plan admission/conversion, stable public IDs,
fresh runtime generations, bounded 32-record streaming, logical mutation tickets,
exact activation-frame receipts, confirmed version projection, cancellation,
supersession, resync, independent repeat alignment and composite-to-primitive Clear
are implemented. An accepted waiting edit yields its RT owner for structural work,
then restages its unchanged spans against the confirmed topology; source
reassignment updates the relation without recompiling or retiming those spans.
Stop cancels scheduled transport but preserves accepted owning plan edits. A plan
that atomically changes its own source may install while stopped or at a restart;
it cannot use early future-only adoption. Complete pending ingress metadata/spans
and RT admission state are included in reliable resync.

Milestone verification: warning-denying full-feature workspace build and native
suite passed (2,767 tests, 7 skipped). The independent Node/Wasm run passed loop
191, adapter 47, engine 80, worklet host 18 and protocol 5 tests. Chrome passed the
same counts, after adapter unit-test browser registration was made explicit so
its three assembly/refusal helpers run in Chrome too.
Test-policy/parity/tracing and the four-target dependency boundaries passed.

Public `CompositeLoops` capability and the legacy creation entry point remain gated
until service/editor/monitoring/persistence support is complete. The negotiated
prepared-plan API is exercised directly; this is not yet public P03 restoration.
Lifecycle dependency cleanup and session capture/replacement integration remain.

### Stage 5 — service documents, editor operations and admission

- [x] Add canonical composite documents, stored timing basis/repeat intent and active/
  pending plan versions to service state. Preserve stable public loop and event IDs;
  use variant/mapped backend identities instead of assuming every loop is primitive.
- [x] Add bounded app-protocol commands and service handlers for conversion,
  serial/parallel/positioned composition, group move/copy/delete, duration overrides,
  kind/mode changes and deliberate timing-basis updates. Prevalidate complete target
  sets, recursion/dependencies, modes, arithmetic, identities and capacity.
- [x] Prepare immutable plans from admitted service data; distinguish authored/pending
  changes from active execution. Coalesce/supersede only through explicit outcomes
  and owning cancellation/collection. Keep results through UI connection loss.
- [ ] Restore selection/target/default-action and explicit transport behavior,
  clear/removal/reference protection, New Session and shutdown. Project coherent
  details, playheads, waits, next operations and active children for UI and Lua.

Verification: service production tests using the RT adapter, including both app
protocol delivery shapes, invalid multi-target requests without partial mutation,
stale event/loop IDs, nested cycles, edit failure and version reconciliation. Compare
preserved editor/control semantics with retained reference fixtures; test the new
timing contract separately. Do not enable the capability yet.

### Stage 6 — composite-aware automatic monitoring

- [ ] Move automatic-monitoring ownership/reconciliation into the service, using
  confirmed active/pending plans, frame phase and recording demand. Aggregate
  nested/overlapping parents and independent primitive recording demand.
- [ ] Preserve manual/prearmed monitoring and other input policies; release only
  automatic demand after section/pass completion, waits, Stop, removal or replacement.
- [ ] Arrange required first-quantum monitoring changes through bounded host/RT work
  when necessary. Correctness must not depend on a UI/Lua poll arriving before a
  recording boundary. Reconcile failed changes and reconnect from actual state.

Verification: port the reference auto-arm scenarios to production, including current/
upcoming sections, pending starts, nested explicit programs, conflicting controls,
pending plan versions, source changes, alignment waits and prearmed tracks. Stall/
disconnect the UI across recording starts and ends; inspect first-quantum media and
confirmed monitoring, ownership cleanup and failure recovery.

### Stage 7 — coherent capture, document migration and atomic load

- [ ] Extend the app persistence image, codec and session conversion with composites,
  timing basis/effective timing and repeat intent. Implement legacy migration and
  sample-rate/alignment handling; retain stable event identity and nested references.
- [ ] Capture one coherent active service/RT view. Define pending-edit save behavior
  explicitly: persist the confirmed active plan, report that newer edits are pending,
  and never mix an authored candidate with active timing. Keep existing recording
  capture refusal; ordinary playing export/save must not stop playback.
- [ ] Extend private service/adapter/engine replacement candidates, fresh source/live
  mappings, validation, rollback and retirement. Load all composites stopped; reject
  an unsupported document as a whole rather than discarding events or dependent modes.
- [ ] Remove composite file refusal only for fully supported document requirements.
  Extend New Session and repeated load teardown without requiring extra live capacity
  beyond the documented replacement budget.

Verification: structure-only and media-bearing save/load/save, nested plans and stable
event IDs, sync source resized/stopped/removed before save, gridless Lua arrangements,
pending edits, changed sample rate with consent, absent host ports and capacity limits.
Inject failures during staging/commit/cancellation and UI loss. Compare complete
documents, exported child media and preserved old-session output after refusal.

### Stage 8 — public UI/Lua routes and supported-core recovery

- [ ] Route every retained composite intent and Lua composition operation through
  the service. Populate composite details instead of `composite: None`; retain
  editor interactions and show timing basis, pending edits and repeat wait honestly.
- [ ] Enable `CompositeLoops` only when the supported core is complete across
  service, adapter, RT, UI and files. Keep unavailable per-event modes/topologies
  gated by their own capabilities and report partial P03 scope explicitly.
- [ ] Exercise the unchanged public Lua API and actual editor: conversion, drag/drop,
  serial/parallel append, positioned insert, grouped copy/move/delete, kind/mode/
  duration changes, play/stop/seek and independent child control.
- [ ] Add native host/socket UI-loss scenarios during playback, delayed start,
  repeat waiting, recording sections and staged edits. Reattach with fresh authority
  and reconcile existing operation IDs/results without replay or script restart.

Verification: production client/service/UI tests and native typed/socket workflows;
supported shared-memory first-attachment delivery without adding reattachment.
Run hosted and self-contained browser/AudioWorklet editor/Lua/audio-MIDI/save-load
scenarios. Test source stop/resume/resize mid-pass and callbacks continuing while
the client is absent; check state and audible/rendered output, not only enabled UI.

### Stage 9 — dependent mode integration and full-parity gate

- [ ] Integrate replacing and dry/wet script-event modes with the production
  capabilities restored under P05/P06/P07. Extend loop/RT mode representation and
  default-action/role mapping as those workstreams require, without silently
  translating unsupported modes into ordinary playback or recording.
- [ ] Validate nested explicit/regular combinations, mixed selections, event-mode
  edits, capture/monitoring, child independence, persistence and recovery for every
  restored dependent mode and its original platform scope.
- [ ] Audit the complete master/reference composite surface against this plan's
  intentional timing changes. Record remaining dependencies as partial scope;
  keep P03 OPEN until this gate is satisfied or narrower closure is user-approved.

Verification: dependent feature suites plus production composite interaction tests,
including whole-document refusal until support exists. No processor/track feature
is advertised solely because its mode tag can be encoded.

### Stage 10 — final end-to-end validation, evidence and delivery

- [ ] Run the complete regression gates below and final-head native platform matrix,
  Node/Wasm suites, actual browser/worklet scenarios and selected ownership/capture
  Miri cases. Record exact source pins, commands, profiles, capacities and limits.
- [ ] Run a production scenario that records/imports children, authors a nested
  regular and explicit recording arrangement through UI/Lua, resizes/stops/restarts
  the source, waits/repeats, edits live, disconnects/reattaches the UI, saves, loads
  fresh IDs and repeats. Check media and monitoring as well as all projections.
- [ ] Add implementation/acceptance links to P03, P01/P02 and relevant dependency
  rows; update current architecture and public loop/program/host guides. Create
  `docs/native_session_recovery/evidence/composites.md` with criterion-to-test mapping
  and explicit differences from the old tick/fallback behavior. Reconcile capability
  coverage and feature counts; mark COMPLETE only after the dependency gate passes.
- [ ] Commit and push completed milestones, create/update the PR against the target
  branch and obtain green checks at the final head. Inspect automated review;
  address feedback, rerun affected checks and repeat until the reviewer approves.
  If no automated reviewer responds, record that fact without claiming approval.
  Leave merging to explicit instructions.

Verification: final evidence distinguishes tests actually run from inherited
reference evidence and unavailable facilities. Compile-only or model-only success
cannot replace public production, recovery, file and actual worklet acceptance.

## Regression gates and evidence

Use [.agents/info/build.md](.agents/info/build.md) to choose the toolchain environment
and [.agents/info/test.md](.agents/info/test.md) for current gates. At execution time,
recheck the workflow, native/browser runners and Miri inventory instead of relying
on historical commands in evidence. Omit missing-backend allowances for checks that
require real facilities.

```sh
cargo fmt --all -- --check
RUSTFLAGS="-D warnings" cargo build --workspace
SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend,shoop_backend/native-fx --profile ci
python3 scripts/check_shoop_test_usage.py
python3 scripts/check_feature_parity.py
python3 scripts/check_tracing_coverage.py --require-closed
python3 scripts/check_rt_runtime_boundary.py
python3 scripts/check_loops_boundary.py
python3 scripts/check_rt_engine_boundary.py
python3 scripts/check_wasm_test_dependencies.py --package shoop_loops --package shoop_rt_protocol --package shoop_rt_engine --package shoop_rt_backend --package shoop_rt_host_worklet
```

Run the relevant standalone loop host manifests/runners and documentation/example
checks in addition to the workspace suite. Build `shoopdaloop` and
`shoop_rt_host_worklet` for `wasm32-unknown-unknown`; run current Node and actual
browser/worklet acceptance commands from their maintained guides. Preserve the
existing no-default-feature/no-std target checks. Run focused Miri cases for
candidate swaps/returns, repeat waits with capture preflight, conversion/slot reuse
and reconnect ownership under the pinned repository setup. Keep heap guards active
and retain the normal/full test groups required before behavioral commits/pushes.

## Execution evidence

- Planning checkpoint: local Markdown links and whitespace checked; no runtime
  result claimed. Base refreshed to `bbf027e9` before branch creation.
- Environment: prepared Rust 1.99.1, nextest 0.9.116, Node 22.22.2 and Chrome/
  ChromeDriver 147.0.7727.137. Local activation is
  `/workspace/work/p03-tools/activate.sh`; compact debug/test profiles and disabled
  incremental compilation keep the complete workspace within available storage.
  Builds and tests deny Rust warnings.
- Loop execution milestone (Stages 1/2): frame-driven progression, shared repeat
  policy, explicit end waiting, own start/wrap pulses, future-only edits and paired
  program/graph deferral implemented. The independent dense oracle preserves every
  media sample and MIDI packet; its new semantic counts remove source-relay pulses
  deliberately superseded by the accepted contract.
- Validation at this milestone: complete workspace nextest **2675 passed / 6
  skipped** with `shoop_engine/app_backend,shoop_backend/native-fx`; loop release,
  Node/Wasm and Chrome/Wasm **184 passed each**; standalone loop-host **60 passed**;
  independent native replay and actual offline/live AudioWorklet replay passed.
  Debug loop cases are included in the full workspace run. The maintained program
  book example compiles and runs; runner Python tests **41 passed**.
- Guarded repeat tests inspect real audio/MIDI and recording across source stop,
  pass-end waiting and restart at all supported quanta and irregular/zero callback
  partitions. Program/configuration tests retain capture refusal/retry, nesting,
  arbitration, cancellation and displaced-owner coverage.
- Formatting, Rust test-attribute policy, parity/tracing inventories, generic
  dependency boundaries and whitespace checks pass. Production composite support
  remains unavailable: Stages 3–10 and the dependent-mode gate are not complete.

- Loop execution checkpoint committed and pushed as `47add1c9`.
- RT preparation milestone: neutral sparse compiler extraction; engine protocol v8,
  host setup v7 and worklet configuration v4; finite negotiated entry/segment/relation
  budgets; timing-only slots with optional strip ownership; generation-fenced empty
  conversion; live plan/version staging and cancellation; coherent checked loop
  telemetry; repeat policy, iteration controls, complete active-plan resync and
  private replacement records. Application composite capability remains disabled.
- The compiler dependency allowance is deliberately limited to `shoop_loop_plan`.
  A separate four-target audit prohibits runtime, application and host dependencies
  in that crate, preserving adapter portability and the generic runtime boundary.
- At-limit staging now batches images larger than the command window. The guarded
  engine case installs 8192 sparse spans with two reliable credits and one pending
  transition, without allocating on endpoint delivery or rendering.
- RT validation: full native workspace **2686 passed / 6 skipped**; subsequent
  engine/protocol follow-ups **65 passed**, including the full composite-pool
  projection and signal-free smoothing cases. Node/Wasm: engine **60**, worklet
  host **18**, protocol **5**, all passed; Chrome runs the same **60 / 18 / 5**
  cases after enabling the existing preview module's browser configuration.
  Latest guarded composite cases **12 passed**, including coherent version
  publication. Warning-denying workspace build passed. The maintained browser
  artifact builder and Node worklet/codec artifact contracts passed; their
  MessagePort simulation is distinct from actual Web Audio acceptance.
- Remaining Stage 3 work includes final browser delivery evidence, the complete
  active-child projection consumed by the adapter/service, and auditing unrelated
  structural requests while a live program waits. No completion of Stages 4–10
  or of full P03 is claimed at this checkpoint.
- The target was refreshed during this checkpoint: `2e19210b` adds shared-media/
  replace foundations and buffered processors after the original `bbf027e9` base.
  The target is integrated in the current tree. Its buffered/shared unit modules
  declared two global allocators; one crate-level tracking allocator now covers
  both, preserving heap guards. Native integration verification: **2756 passed /
  7 skipped**; Node engine **79**, worklet host **18**, protocol **5**, all passed.
  Warning-denying workspace build and dependency/tracing/parity audits passed.
  Chrome and independent loop-host rechecks are in progress. Earlier results
  above identify the pre-integration checkpoints (`47add1c9`, `1a170847`).

- Service/editor milestone in progress: app protocol v21 carries canonical flat
  documents with stable event IDs, original sample-clock timing basis, frozen
  effective instance geometry and normalized spans, active/pending versions and
  exact observed frames. All retained editor operations and transport preflight
  run in the service; sample conversion quantizes each endpoint independently.
- Confirmed natural-length changes create a new version without rebasing timing.
  Source resizing does not recompile accepted spans. Deferred backend controls
  preserve compose-then-Play and clear-then-Record ordering across generations.
- Referenced Clear and track removal use a service-owned receipt-driven lifecycle:
  stop affected ancestors, install empty reference-removing programs, change the
  slot/track, then restore frozen surviving arrangements while stopped. Failures
  before structural publication restore the original plans. Child media survives
  composite Clear. Results explicitly list stopped ancestors, including after UI
  loss. Other structural/editor admissions wait behind this finite workflow.
- Production client details use accepted effective geometry and exact frame
  playheads without primitive media reads. The editor shows original timing,
  pending versions, cancellation, repeat waits and a deliberate sync-length update.
  Reference-only details retain their historical timing projection.
- Focused service/protocol/client/adapter/editor verification: **368 passed**.
  Warning-denying full-feature workspace build and native suite passed: **2,783
  passed / 7 skipped**. Test policy, parity and tracing inventory (**428 modules**)
  passed. Node/Chrome milestone checks are still being run. Automatic monitoring,
  files/New Session, the public capability and dependent-mode gate remain open.
- The backend checkpoint `4abb0f0c` passed its complete GitHub platform matrix,
  native JACK acceptance, coverage, documentation and CodeQL checks. No submitted
  reviews or inline review threads were present when checked. This is validation
  of that checkpoint, not approval or final-head P03 acceptance.
- The target advanced to `a192b0ce` (profiling and render optimizations). Its
  integration follows the service milestone; both workstreams independently
  changed protocol shapes, so merged versions must be reconciled before delivery.

- Service checkpoint `08c34392`: Node and Chrome both passed app **49**, client
  **11**, app protocol **17**, editor **232** and adapter **48** tests. Four-target
  dependency boundaries and Wasm test dependency isolation passed.
- Integrated target `a192b0ce`: retained profiling and render optimizations,
  reconciled app protocol **22** and engine protocol **9**, assigned profiling
  request tag **40** after the composite request range, and kept one shared test
  allocator for engine unit modules. Warning-denying full-feature build passed;
  focused app/protocol/adapter/engine integration tests **203 passed / 1 skipped**.
  The complete integrated-head gates are still pending.
