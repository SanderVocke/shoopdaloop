# P17 — Complete composite mode integration

Status: in progress on `feat/composite-completion`.

Base: `feat/backend-replacement`, inspected at
`862101eb9cd65e79208ab8e7746633a5ac64b3a0` on 2026-11-06.
Follow `.agents/prompts/write_plan.md` and the branch's current agent instructions
when executing this plan. Recheck the base before implementation.

## Goal and scope

Close [P17](https://github.com/SanderVocke/shoopdaloop/blob/862101eb9cd65e79208ab8e7746633a5ac64b3a0/docs/native_session_recovery/feature-parity.md#p17-composite-mode-integration): make authored composite
`Replacing`, `PlayingDryThroughWet` and `RecordingDryIntoWet` events executable
through production UI/Lua, service, adapter and render paths. Support the
corresponding Direct, External dry/wet, built-in FX and OxiSynth interactions
within each host's negotiated topology and processor capabilities, including
reference-supported replacing on dry/wet tracks.

Preserve the completed P03 direct-track core and integrate the existing P05/P06/P07
foundations. Include nested regular/explicit programs, editing, monitoring,
capture, files and accepted-work recovery. P07's remaining Carla work is not a
prerequisite. Carla, bus FX, general latency/take-alignment controls, duplication,
script-resource ownership and expanded recovery guarantees remain separate items.

## Current implementation and gaps

| Area | Existing foundation / required change |
| --- | --- |
| Compiler/resolver | `shoop_loop_plan::Mode`, staging tags and resolver transitions already represent all three modes. Service `composites/prepare.rs` still rejects their input mappings. Audit execution semantics rather than introducing duplicate mode types. |
| Admission/topology | Service `composites/mod.rs` rejects dependent event modes and non-Direct tracks. Adapter `shoop_rt_backend/src/composites.rs` has matching topology restrictions, incomplete `wire_mode`, and a legacy Replacing refusal. |
| Engine/replacement | `shoop_rt_engine/src/composite.rs` and `replacement.rs` reject dry/wet spans and processor-owned targets/containers. Live edits and private file replacement must gain equivalent validated support. |
| Replacing resources | Primitive input transactions prepare `WriterPrepareReplacing`; authored composite transitions do not establish the same preparation. `shoop_loops/src/runtime/shared.rs` explicitly rejects dry/wet writer shapes, and planner take preparation needs a prepared live writer for Replacing. |
| Monitoring | `shoop_rt_engine/src/monitoring.rs` derives composite recording demand for ordinary Record. Extend demand and role routing without conflating external-input capture with wet rerecord. |
| UI/files | The editor already has dependent-mode controls, but `shoop_app_client/src/runtime.rs` advertises only Play/Stop/Record. Persistence writes dependent tags but rejects them in parsing and whole-document diagnostics. |

These observations are source findings, not validation of the current branch.
Retained reference fixtures supply behavioral expectations; their old tick-driven
scheduler does not replace P03's frame-clock contract.

## Acceptance criteria — immutable without explicit user approval

1. **Public execution:** production editor and Lua routes author, edit and execute
   each restored event mode on every reference-supported topology in its original
   host scope. Explicit Play remains ordinary playback; default UI/Lua actions
   resolve the current track preference. No unsupported mode is silently converted.
2. **Composition/timing:** nested regular/explicit combinations, serial/parallel
   sections, mixed selections, child independence and mode edits preserve the
   existing arbitration, seek/retrigger, continuation, repeat-wait and frozen-timing
   contract. Source stop/resize/restart does not retime an accepted arrangement.
   Composite outer transport remains Play/Stop.
3. **Exact capture and roles:** Replacing preserves the supported overwrite,
   erasure and MIDI-state semantics; dry-through-wet plays retained dry content;
   wet rerecord preserves dry media and updates wet media/settings at the actual
   take boundary. Audio samples, MIDI payload/order and dry-audio/dry-MIDI/wet-audio
   identities survive execution and round trips, including unequal channel widths.
4. **Monitoring and ownership:** render-owned starts/finishes and processor demand
   remain correct without client/service polling. Manual monitoring is preserved;
   demand combines across loops and releases correctly on Stop, waits, cancellation
   and structural changes. UI loss does not cancel accepted work.
5. **Atomicity/resources:** unsupported, stale, cyclic or over-capacity edits and
   loads refuse intact. Preparation, activation, cancellation, supersession,
   disconnect and teardown return every owner/reservation correctly. Callback
   work is prepared, bounded, nonblocking and allocation/deallocation-free.
   Resource exhaustion reports truthful capture outcomes without damaging unrelated
   playback or introducing timing delays to conceal missing preparation.
6. **Files/recovery:** save/load/save preserves authored IDs, modes, role-ordered
   media, recorded processor state and frozen effective geometry. Loaded live IDs
   are fresh and transport is stopped. Active/scheduled/nested future capture
   retains the existing save/export refusal policy. Socket recovery reconciles
   actual active/pending versions and operations without replay or VM restart.
7. **Evidence/delivery:** focused reference and production tests plus applicable
   native, browser/worklet and firmware/platform checks pass on the final PR head.
   Contracts and P17 status match shipped support. Missing or incomplete checks
   remain explicit; earlier P03/P05/P06 results do not certify this change.

## Design rules

- Keep one render-owned timing authority and P03's immutable quantum/frozen frame
  geometry. Do not restore source-tick scheduling or application timer fallbacks.
- Preserve service-owned authored documents, accepted operations and confirmed
  projection; adapters own identity mappings and render state owns execution.
- Reuse shared audio/semantic-MIDI stores and prepared continuous writers.
  Preparation must cover future authored starts, restarts and edits; readiness
  must not depend on a client issuing a command at each event boundary.
- Keep general recording predicates separate from external-input monitoring and
  role-specific capture predicates. Wet rerecord forces the reference routing
  policy; it is not ordinary Record with another tag.
- Validate event mode, target kind, topology and negotiated capability together.
  A nested composite receives supported outer Play/Stop controls; dependent modes
  are authored on eligible primitive descendants. Preserve DAG/version/generation
  fences and whole-operation refusal.
- Resolve mixed-selection gesture filtering against retained reference behavior
  separately from exact-list service/Lua command validation. Do not weaken atomic
  validation to enable a gesture or give composites a new outer transport mode.
- Keep each dependent mode gated until its public, runtime and persistence paths
  work end to end. If schemas or resource contracts change, version and test all
  affected native/socket/worklet codecs and maintained artifacts together; do not
  bump versions merely to enable already-defined tags.
- Preserve finite media/processor/composite budgets and disabled-host behavior.
  Do not expand platform capabilities or add a legacy production dependency.

## Implementation stages

Keep gates closed until their corresponding paths and verification are complete;
do not expose intermediate partial support as P17. Runtime dry/wet admission and
tests from Stage 3 may precede writer changes in Stage 2: they establish the role
topology against which writer work is verified. Public admission stays gated.

### Supported interaction matrix

| Target | Authored Play/Stop/Record | Authored Replacing | Authored dry playback / wet rerecord |
| --- | --- | --- | --- |
| Direct primitive | Existing P03 behavior | P05 semantics with autonomous preparation | Unsupported; refuse the whole edit/load |
| External dry/wet primitive | Role-aware recording and recorded wet playback | Paired dry/wet input overwrite | Native hosts advertising External topology |
| Built-in FX / OxiSynth primitive | Role-aware recording and recorded wet playback | Paired dry/wet input overwrite | Native/browser hosts advertising that processor |
| Composite, standalone or track-owned | Play/Stop only; Record is invalid | Invalid outer mode | Invalid outer mode |

Regular programs author inherited ordinary Play. A default UI/Lua playback action
resolves current track preferences; it does not rewrite explicitly authored Play.
Mixed user gestures filter ineligible composite outer targets only where the
reference does so; an exact-list service/Lua command containing an invalid target
refuses atomically. A dry/wet track-owned composite carries timing/lifecycle only,
with no media bindings; Clear recreates its primitive role bindings.

### Stage 1 — Baseline and interaction contract

- [x] Create `feat/composite-completion` from the current
  `origin/feat/backend-replacement`; record the actual base and baseline failures.
- [ ] Define the event-mode × primitive/composite-target × topology/host matrix,
  including container conversion/clear, default actions, valid mixed gestures and
  exact-list refusal. Map each acceptance criterion to focused tests.
- [ ] Inspect retained reference script-event/default-action/role fixtures,
  including `mixed_selection_applies_primitive_only_modes_without_faulting_the_composite`
  and `dry_wet_track_default_action_is_dynamic_while_explicit_play_stays_explicit`.
- [ ] Verify: run the relevant current composite, primitive replacing and dry/wet
  cases; identify independent baseline failures before changing behavior. Add
  focused regression/oracle cases for the missing interactions.

### Stage 2 — Prepared authored Replacing and role-aware writers

- [ ] Integrate writer preparation/reservations with composite admission and
  render-owned activation. Cover future starts, Record→Replace, contiguous spans,
  gaps, repeats, seeks, direct overrides and edited plans without per-event UI work.
- [ ] Extend the existing writer/media machinery for required dry/wet shapes,
  preserving paired capture, per-role timing and coherent immutable cuts. Document
  reservation lifetime, capacity accounting and invalidation by topology/media edits.
- [ ] Verify: use exact stereo/MIDI overwrite and erasure/state oracles, dry/wet
  role assertions, simultaneous writers, finite-capacity failures, cancellation,
  resync and teardown. Exercise callback guards and return to a stable resource
  baseline; compare complete output across supported callback partitions.

### Stage 3 — Compile and admit dependent modes/topologies

- [ ] Complete service compiler, adapter prepared/legacy conversion and protocol
  mappings. Replace blanket Direct-only refusals with the supported matrix.
- [ ] Update live composite and private-replacement engine validation together,
  including processed-track containers and eligible child bindings. Preserve
  primitive/composite identity conversion and action/sync cycle checks.
- [ ] Audit resolver snapshots, seek/retrigger, same-mode continuation and
  dry-through-wet/rerecord transitions; correct only demonstrated interaction gaps.
- [ ] Verify: run compiler/codec and guarded engine/adapter tests for each matrix
  entry, nested regular/explicit programs, same-boundary arbitration, partial
  callbacks, source loss/resize and atomic malformed/stale/over-budget refusal.

### Stage 4 — Monitoring, processor capture and UI/Lua

- [ ] Extend actual and upcoming composite demand for Replacing and wet rerecord,
  including nested starts and direct-child overrides. Preserve manual policy,
  aggregate track demand and pending-edit activation rules.
- [ ] Verify processor activation, dry send/wet return, ordinary wet playback,
  wet-only rerecord and recorded-settings capture at actual render boundaries.
- [ ] Advertise executable event modes in production details and route existing
  editor/Lua controls through service admission. Preserve explicit/default-action
  distinctions and the Stage 1 mixed-selection contract.
- [ ] Verify: exercise public typed and encoded UI/Lua routes, dynamic defaults,
  mode edits, child independence, multiple loops on one track and recording with
  no service/client polling. Check disabled capabilities still refuse cleanly.

### Stage 5 — Persistence, recovery and lifecycle integration

- [ ] Enable dependent-mode parsing and capability-aware file diagnostics; update
  private session replacement and role/state mapping without weakening atomic load.
- [ ] Preserve capture refusal for actual/armed/nested future recording and exact
  confirmed cuts while non-recording edits await restart. Retain frozen geometry
  through source changes and consented sample-rate/quantum conversion.
- [ ] Extend accepted-work recovery and structural lifecycle coverage to these
  modes: waiting edits, activation/cancel races, Clear/removal, New Session,
  shutdown, disconnect and fresh-generation reuse.
- [ ] Verify: exact save/load/save media, role order, event IDs, settings and timing;
  fresh stopped identities; unsupported whole-document refusal; socket UI-loss
  recovery without mutation replay or automatic Lua restart.

### Stage 6 — End-to-end acceptance and delivery

- [ ] Run packaged production scenarios for all three modes with nested/mixed
  arrangements, edits, child controls, monitoring, exact capture and file recovery.
  Exercise native typed/socket and actual hosted/self-contained AudioWorklet paths
  where supported; exercise External topology on its native host scope.
- [ ] Run formatting, warning-denying builds, test-policy/parity/dependency-boundary
  checks and applicable broad native suites per `VALIDATION.md`. Run affected
  Node/browser/worklet/artifact and firmware/platform suites through CI; use real
  host facilities for tests that require them. Select ownership-focused Miri checks
  when writer/lifetime changes warrant them, without treating timeouts as passes.
- [ ] Update `docs/native_session_recovery/composites.md`, affected processor/media
  contracts and the P17 ledger. Record revision-specific commands, failures,
  capacity observations and acceptance results in PR/CI artifacts. Correct related
  P01/P02 caveats without claiming their unrelated remaining work complete.
- [ ] Commit and push milestones; open a PR targeting `feat/backend-replacement`.
  Check applicable final-head CI through completion, fix failures attributable to
  this work and rerun affected checks. Reconcile target-branch updates before final
  acceptance; do not substitute green checks from an earlier head.
- [ ] Inspect automated reviews and repeatedly address substantive feedback through
  reviewer approval. Report absent/unavailable reviewers explicitly rather than
  inventing approval. Leave the PR unmerged and the worktree clean; mark P17 complete
  only when the acceptance criteria have evidence.

## Verification strategy

During iteration, run only the owning packages and targeted interaction cases:
`shoop_loop_plan`, `shoop_loops`, `shoop_rt_protocol`, `shoop_rt_engine`,
`shoop_rt_backend`, `shoop_app`, `shoop_app_client`, `shoop_egui`,
`shoop_persistence_codec` and `shoop_session`, plus relevant retained reference
fixtures. Use independent expected PCM/MIDI/role results, not just mode telemetry
or successful admission. Broaden at writer/topology integration milestones and
before delivery; avoid repeating the entire platform matrix after routine edits.

Follow current `.agents/info/build.md`, `.agents/info/test.md` and `VALIDATION.md`
for commands and feature selections. Local targeted checks provide fast feedback;
CI supplies broad regression and platform coverage. Actual AudioWorklet execution
is required for browser acceptance; Wasm compilation alone is insufficient.
Distinguish new failures from the known hosted-processor/independent-fixture
baseline limitations. Keep incomplete checks visible and do not close P17 on a
qualified partial result without explicit scope approval.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
