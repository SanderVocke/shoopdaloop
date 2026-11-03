# Scoped runtime execution and shared loop callback plans

Status: In progress. Scoped execution, shared callback media, both real-media
canaries and guarded adversarial cases are implemented. Complete native,
browser/worklet and delivery gates are underway. See the
[API and reproduction guide](scoped-processing.md) and
[execution contract](scoped-processing-contract.md).

Branch: `feat/scoped-loop-processing`, created from `feat/backend-replacement`
at `e68b15ac551ff1899554311ddfe2d5e4dda1d3ea` (merged
[PR #926](https://github.com/SanderVocke/shoopdaloop/pull/926)).

## Goal and scope

Establish the scheduling foundation for full-block DSP alongside sample-accurate
loop processing. Scopes determine where planner subdivisions apply; shared loop
plans let coordinated playback and capture execute at different positions in the
signal graph without advancing musical time twice.

Implement this in `rt_runtime` and `shoop_loops`, with the minimal engine/host
adaptations needed to preserve existing production behavior. Prove both
`loop region -> full-block DSP` and `dry playback -> full-block DSP -> wet capture`
using ordinary deterministic test processors and real loop media/resources.

This phase is independent of DSP PRs #915/#922. General buses, their controls,
routing, ordering, persistence and recovery are already restored by #926 and
remain supported. Production bus FX slots, Carla hosting/editor/state integration,
application dry/wet modes, new plugin algorithms, network/IPC DSP bindings and
board drivers are subsequent work. Do not enable BusFx/TrackFx capabilities or
mark those parity rows complete through this foundation alone.

## Findings and rationale

- The current [executor](../../src/rust/rt_runtime/src/executor.rs) chooses an
  interval from scheduler/planner boundaries and visits every active graph step.
  A node without a decision binding still receives those subdivisions.
- The [planner contract](../../src/rust/rt_runtime/src/planner.rs) supports
  interleaved `plan -> process -> finish`, including observation-dependent
  decisions. Preserve that capability inside an explicit execution scope.
- The [loop planner](../../src/rust/shoop_loops/src/runtime/planner.rs) currently
  requires playback/capture observations and commits capture reservations before
  advancing each interval. Callback planning requires a deliberate separation of
  timing, media execution and completion; synthetic observations are insufficient.
- Master at `92079fb6` separates channel preparation, loop timing and channel
  execution. Audio timing queues transfers that channel nodes apply later against
  callback buffers. Its
  [dry/FX/wet ordering test](https://github.com/SanderVocke/shoopdaloop/blob/92079fb6f00308be0f6b407b7e8c02229662e420/src/rust/shoop_engine/src/graph_build.rs#L397)
  places dry and wet channels of one loop on opposite sides of FX. MIDI is
  processed differently there and needs its own proof in the new design.
- Nested scopes alone solve the downstream-bus case, but a single atomic child
  containing all loop media consumers cannot surround an intervening FX stage.
  Planning authority and execution-region membership must be distinct.

## Immutable acceptance criteria

1. In a real `GraphRuntime`, planner boundaries subdivide only their declared
   execution scope. An admitted full-parent-block processor receives exactly one
   call covering `[F, F+N)` for each nonempty parent invocation. At the root, this
   is the actual host callback. Unrelated scheduled actions cannot silently
   subdivide that call. Zero-frame calls perform no planning or time advancement.
2. A bus-shaped canary renders subdivided loop output through full-block DSP with
   exact samples and MIDI. A dry/wet canary sends actual prerecorded media through
   DSP into actual capture, using real `shoop_loops` timing and runtime resources.
   Both require positive processed/captured output, independent sample/event
   oracles and invocation-size assertions; empty output or unrelated scalar FX
   routes cannot satisfy them.
3. Playback and capture separated by DSP can consume one coordinated timing plan.
   Define how user-visible loops, primitive LoopIds and media phases map in the
   fixture. Shared logical owners advance once, and source identities, sync and
   transition frames remain correct. Unrelated planner copies or fake completion
   observations cannot substitute for that coordination. Public dry/wet UI/modes
   need not be restored in this phase.
4. Supported feed-forward graphs produce same-callback output with no extra
   scheduling block of latency. A gain probe must match its direct oracle; any
   delay probe must match its declared algorithmic delay. This does not promise
   adaptation of an arbitrary plugin quantum to a different device block size.
5. A genuinely observation-dependent generic planner continues to work through
   local interleaved execution while an external full-block node retains its call
   cadence. Callback planning is not mandatory for every runtime planner.
6. Scope-local sample-accurate actions and compatible graph activations preserve
   their effective frames. Cross-scope changes have explicit admission and
   execution rules. No silent event rounding, global cuts through full-block
   processors, stale-plan reuse or premature reclamation is permitted. Preserve
   existing promised behavior for configurations that remain admitted.
7. Capture preflight/refusal, media snapshots, atomic capture groups and truthful
   completion remain intact. Failure before publication leaves the relevant live
   state/music unchanged. A post-publication fault does not fabricate successful
   capture, roll back time or replay already rendered frames.
8. Audio and MIDI retain identities and offsets across regions and subdivisions.
   Include DSP-produced MIDI captured downstream, initial MIDI state at capture
   boundaries, overflow and reserved recovery. No truncation or duplicate traffic
   is introduced. Input-dependent metadata is obtained when its input is ready.
9. Preparation validates signal and declared planning/control dependencies, scope
   membership, generations and block contracts. Incompatible grouping and
   same-block causal cycles are refused explicitly. All plans, reservations and
   scratch have checked prepared limits; exhaustion cannot silently drop valid
   musical boundaries or leave partially published plans.
10. Rendering, including failure, is bounded and performs no heap allocation,
    deallocation, locking, waiting or I/O. One outer frame authority and callback
    budget govern all scopes. Preparation and final reclamation remain off-render.
    Portable libraries retain `no_std` plus off-render `alloc`, with no new native,
    Linux, socket/thread, Carla or legacy-engine dependency.
11. Existing runtime/loop semantics and production bus behavior remain covered by
    regression tests. Runtime/loop libraries build for Cortex-A32, Cortex-A64,
    Cortex-M and WebAssembly; affected native and browser hosts remain functional.

## Design rules and constraints

- Implement one prepared execution model. A nested-graph API may compile into
  flat scope metadata and steps; embedding independently owned runtimes is not
  required. Limit the initial surface to the scopes and staged consumers exercised
  by the acceptance cases. General recursive/multi-rate/parallel scheduling is
  outside this phase.
- A full-block contract names its parent scope. Placement beneath a subdividing
  ancestor must be validated against that contract. Region grouping must respect
  dependency order; planning authority may serve several media execution regions.
- Validate execution dependencies separately from musical sync relations. Existing
  resolvable sync cycles remain valid; a dependency requiring not-yet-produced
  media across a full-block boundary needs explicit admission/refusal semantics.
- Plans describe absolute frame intervals and retain validated source/resource
  ownership. Separately executed consumers use slices of the same parent block,
  not later callbacks or independently advanced clocks.
- Keep musical timing and loop semantics in `shoop_loops`; generic execution,
  dependency validation, event delivery and ownership belong in `rt_runtime`.
  No Carla, bus-specific or network-specific scheduling rules enter the runtime.
- Separate loop timing decisions, resource reservation, media execution and
  completion explicitly. Determine which observations are input-dependent before
  choosing which work can safely be planned ahead. Preserve the existing capture
  failure and MIDI checkpoint guarantees rather than copying master's internals.
- Scheduled processor/planner actions, MIDI injection and graph activation are
  part of the scope contract. Document supported/refused cases before coding,
  including full-block-target actions and topology changes across scope boundaries.
  Scope-local activation and parent-plan lifetimes require separate authority.
- Callback-wide ingress, validation, transfers, receipts, recovery and cleanup
  allowances are not replenished per scope or interval. Plans and diagnostics
  have separate capacities; diagnostics loss cannot change musical execution.
- Size worst-case plan storage from prepared limits and document its cost,
  including dense one-frame boundaries. Prefer compact spans/shared records where
  appropriate. Capacity refusal must occur at a defined safe boundary.
- Keep existing public APIs where practical; document necessary migrations and
  update all in-tree consumers. Current generic planner examples remain meaningful.
  Production adaptation must preserve #926's negotiated bus limits and behavior.

## Implementation stages

### 0. Establish the baseline and contracts

- [x] Inspect the runtime executor/planner, loop resource lifecycle, master phase
      separation and merged bus work.
- [x] Create the independent implementation branch from the updated backend base.
- [x] Recheck branch/base changes and repository guidance before implementation;
      record baseline runtime/loop tests and current native/browser bus canaries.
- [x] Write `scoped-processing-contract.md`: scopes/cadence, frame authority,
      dependency validation, plan/resource ownership, failure table and the exact
      action/activation admission matrix. Include both required signal paths.
- [x] Specify prepared limits and compatibility/migration rules. Commit the
      contracts before implementing execution changes.

Verification: contracts explain every acceptance criterion, including how actual
capture/MIDI observations fit planning ahead and how full-block nodes avoid cuts
from both planners and scheduled actions.

### 1. Establish executable canaries and independent oracles

Depends on Stage 0.

- [x] Add a deterministic full-block test processor with recorded frame origins,
      call sizes and nontrivial known audio/MIDI behavior. Keep it independent of
      Carla and the DSP-host stack.
- [x] Add the bus-shaped scope canary and capture the current global-subdivision
      failure before correcting execution.
- [x] Define the real loop-media dry/FX/wet fixture and independent expected
      samples/events. Exercise internal wraps and a recording transition within
      a parent block; account for any declared DSP delay explicitly.
- [x] Cover multiple channels, shared timing, repeated callbacks, irregular host
      partitions and origins beyond 32 bits. Compare subdivision variants against
      the same absolute-frame oracle rather than deriving expectations from plans.

Verification: canaries require positive DSP/capture results and fail on fragmented
full-block calls, shifted media, missing MIDI or duplicate timing advancement.

### 2. Implement generic scoped runtime execution

Depends on Stages 0-1; retains existing interleaved planner semantics.

- [x] Add prepared scope/cadence metadata and validation for node membership,
      interfaces, block requirements and signal/planning dependency order.
- [x] Implement local `plan -> process -> finish` execution and full-parent-block
      processing in one executor with one outer callback envelope.
- [x] Implement scoped action delivery and activation according to the contract;
      preserve MIDI offsets, receipt frames, generation checks and owner lifetimes.
- [x] Add a planner whose next decision depends on real processing observations.
      Verify that it stays interleaved and does not subdivide an external probe.
- [x] Test invalid grouping/cycles, incompatible block placement, scope-local
      activation, stale candidates, capacity refusal and off-render retirement.

Verification: the bus-shaped canary and existing runtime suites pass; heap guards
cover successful and failing execution, and callback budgets remain shared.

Progress: the generic feedback canary reproduced nine DSP calls for one 17-frame
callback before cadence support, and now observes one call with exact audio/MIDI.
Region contraction and incremental receiver validation refuse incompatible
grouping. Existing runtime/loop suites and the warning-denying workspace build
pass. The real-media bus canary and action/activation adversarial cases follow;
this generic probe alone does not satisfy the real-media acceptance criterion.

### 3. Implement shared loop callback plans and staged media execution

Depends on the Stage 0 ownership contract and Stage 2 execution support.

- [x] Introduce bounded callback-plan storage for coordinated loop timing and
      prepared media operations. Retain source snapshots and required reservations
      until their consumers complete; avoid copying mutable planner authorities.
- [x] Refactor playback/capture execution and observation collection so consumers
      can run on opposite sides of DSP using the same timing plan.
- [x] Preserve capture-start preflight, refusal, group atomicity, actual media
      completion and MIDI initial-state handling. Never advance the old interval
      API using synthetic consumer observations.
- [x] Make the real dry/FX/wet canary pass, including DSP-produced MIDI captured
      downstream and one advancement of each coordinated logical owner.
- [x] Update in-tree loop examples/adapters and document the separation between
      timing ownership and media execution regions.

Verification: exact media/call-count assertions pass in both canaries; sync and
transition behavior remain consistent across callback partitions and region order
permitted by the dependency graph.

Implementation decision: the opt-in `runtime::callback` adapter uses the real
resolver, media ledger, renderers and atomic capture groups. The existing
`LoopPlanner` remains interleaved, preserving its observation-dependent capture
and publication behavior. Playback and capture phases share one primitive LoopId;
explicit capture windows supply the low-level dry/wet fixture. The callback
adapter requires worst-case plan storage during preparation, rather than admitting
callbacks that might exhaust a smaller span allowance after time has advanced.

The dry/wet test passes exact stereo and MIDI oracles for `[17]`, `[3,4,1,9]`
and seventeen one-frame callbacks, including wraps, an internal capture start/end,
and the DSP-generated held-note checkpoint. Fault, capacity, activation and
production/browser delivery checks remain outstanding.

### 4. Prove failures, compatibility and prepared bounds

Depends on Stages 2-3.

- [x] Exercise capture exhaustion/refused transitions, plan exhaustion, delayed
      consumer completion and post-publication faults. Assert truthful receipts,
      retained old snapshots and correct cleanup without replay or fake success.
- [x] Test MIDI overflow/recovery, capture-start checkpoints, consumer offsets and
      events exactly on scope, loop, callback and graph-activation boundaries.
- [x] Verify commands targeting one scope do not fragment unrelated full-block
      nodes. Test compatible child graph changes and documented cross-scope
      refusals without silently quantizing existing operations.
- [x] Run dense-boundary cases and record plan/scratch storage and work counts.
      Confirm no per-scope budget renewal, allocation/deallocation or unbounded
      ready-work draining, including teardown and replacement paths.
- [x] Adapt production engine/host consumers as needed and run existing bus,
      persistence, recovery, program, topology and capture regressions. Keep
      application FX capabilities disabled.

Verification: debug and release correctness/heap guards pass; admission and failure
behavior match the frozen contract and #926's bus behavior remains intact.

Baseline evidence: core suites and seven native bus/session canaries passed before
implementation; #926 head `fe897c4d54ce5032e18d5c5655781ba539ea0b4b`
has successful web debug/release checks. The backend base remains `e68b15ac` on
recheck. New debug tests cover actual DSP/capture failures, missing observations,
capture exhaustion with retained owning exports, MIDI overflow and reserved host
recovery, synchronized owners, exact child activation and parent-interface refusal.
Plan capacity refusal is tested during preparation; valid dense callbacks retain
the full worst-case allowance. The example reports 8,704 plan bytes per owner for
32 spans on the native evidence host and one DSP call for all 32 local boundaries.

### 5. End-to-end validation and delivery

Depends on Stages 0-4.

- [ ] Provide runnable examples for both paths and reproduce them from a clean
      checkout. Record source/commands, independent audio/MIDI checks, FX call
      sizes, timing advancement, storage and dense-case work in a new evidence
      directory. Any elapsed-time measurements are descriptive, not fragile CI
      timing thresholds or physical-device latency guarantees.
- [ ] Run repository formatting, Rust test policy, warning-denying workspace
      build, focused debug/release suites, complete native suite, tracing and
      dependency/boundary checks using `.agents/info/build.md` and `test.md`.
- [ ] Build runtime/loop libraries for `armv7a-none-eabi`,
      `aarch64-unknown-none`, `thumbv7em-none-eabihf` and `wasm32-unknown-unknown`;
      run affected engine/service/host builds and existing browser/worklet checks.
- [ ] Add correctness-focused CI coverage for both canaries, including real media,
      MIDI and full-block invocation counts. Retain existing native/browser and
      bare-metal gates and coverage thresholds.
- [x] Document the resulting APIs, limits, event/activation matrix and migration
      points for the DSP PR stack and later bus/Carla integration. Preserve prior
      benchmark/evidence archives and completed parity records.
- [ ] Push this branch and open a separate PR against `feat/backend-replacement`.
      Inspect failures before targeted corrections and monitor CI to green.
- [ ] Inspect automated review, address justified findings and repeat checks until
      successful. Resolve completed discussions and obtain approval from any
      participating human reviewers; record actual review status and final
      evidence here without inferring human approval from automated checks.
- [ ] Mark this plan complete only after its acceptance criteria and delivery
      gates pass. Production FX integration remains a subsequent milestone.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
