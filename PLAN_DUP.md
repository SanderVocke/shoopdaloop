# Loop duplication implementation plan

Restore the duplication portion of P11 on `feat/backend-replacement`: Clone into a slot below the source and duplicate into an explicit destination. Preserve supported primitive content and P03 composite definitions through the production service, native and browser hosts, persistence and accepted-operation recovery.

P11 remains OPEN for MIDI initial-state, preroll, seek and parameter-history restoration. This work copies existing supported MIDI content exactly; it does not extend those semantics or remove their validation gates. P17 dependent composite modes, Carla, latency editing and broader recovery guarantees retain their existing scopes.

Planning base: `0b6812259ffc00b9a524bb6095cfc2c6b2112847`. Follow `.agents/prompts/write_plan.md`, `.agents/rules/mandates.md` and `VALIDATION.md` on the implementation branch. Progress is recorded in the stage checklists below.

## Goals and immutable acceptance criteria

1. **Destination behavior.** `LoopAction::Duplicate` uses the first empty non-composite slot below the source on the same track. If none exists, add an aligned row using the existing application row policy. `DuplicateTo` replaces an explicit same-track destination. Reject self-copy, stale identities, cross-track destinations and composite cycles before mutation. Preserve current UI eligibility rules, including the sync-loop restriction.
2. **Exact primitive copy.** Preserve name, logical length, gain/balance, complete supported raw audio and MIDI content, channel roles/order, per-channel lengths, offsets, capture alignment, preplay, repeat settings and recorded processor state. Include empty, timing-only, audio-only, MIDI-only, mixed and supported dry/wet loops. Preserve an existing destination's slot identity. Do not normalize raw media through an export codec or introduce resampling.
3. **Independent stopped destination.** The destination starts stopped at position zero, without copied selection, targeting or pending transitions. Resource ownership, content revisions and new composite/event identities belong to the destination. Editing, replacing or clearing either loop cannot alter the other. Unrelated selection, targeting, routing and playback remain coherent.
4. **Supported composite copy.** Copy the active confirmed P03 definition, timing basis, frozen geometry, explicit sync and repeat intent. Preserve references to existing child loops rather than recursively cloning children. Give the destination independent document/runtime ownership and consistent identity mappings. Support primitive-to-composite and composite-to-primitive replacement. Preserve P17 refusal and cycle/dependency validation.
5. **Atomic operation and recovery.** Pin source media and metadata from one coherent cut. Revalidate destination and session ownership before commit. Reserve slots, resources, receipts and retirement ownership before changing live state. Failure before commit leaves source, destination and row layout intact. An admitted operation has one terminal outcome across UI loss and reconnect, without replay or duplicate row creation.
6. **Safe conflicts and finite work.** Refuse affected active, armed or scheduled recording/replacing and conflicting edits without stopping recording. Supported playback continues during preparation; replacing a playing destination performs required MIDI cleanup at activation. Unrelated recording continues. Preparation, transport, rendering and cleanup respect negotiated capacities and existing RT allocation/nonblocking constraints.
7. **Public support and partial P11 closure.** UI Clone and explicit-destination gestures use the same service operation and confirmed projection. Advertise `LoopDuplication` only when the complete supported path exists. Verify native typed/socket and actual browser/worklet behavior, exact persistence round trips and accepted-operation recovery. Update P11 to describe duplication as complete while retaining its MIDI gap.

Goals and acceptance criteria must not be changed without explicit user approval.

## Design rules and implementation boundaries

- The service owns destination selection, row creation, source metadata, composite documents, admission and completion. The client sends intent and displays confirmed results. Duplication must not depend on a filesystem provider or client round-tripping media.
- Reuse immutable loop capture, prepared content ownership, role-aware upload/application and P03 compilation. Share immutable media or use COW where existing lifetime rules permit; otherwise make a bounded control-side copy. Prove edit independence and account for peak retained ownership in either case.
- Capture only the source and dependencies needed for validation. Do not use whole-session save/load as a copying mechanism: unrelated playback, recording and identities must survive.
- Stage content, controls, composite replacement and row additions privately, then publish their complete result coherently. Do not clear a destination, create visible rows or apply name/gain changes before the remaining work can succeed.
- Keep MIDI packets, event order and frame positions intact. Unsupported initial/preroll forms retain explicit refusal before mutation. Introduce no new MIDI reconstruction model or history-replay algorithm in this work.
- Clone the active confirmed composite version. Conflicting pending edits must settle or cause explicit refusal. Reuse P03 structural validation and compilation, including checks for composites referencing the destination being replaced.
- Keep public API changes minimal. `Duplicate` and `DuplicateTo` already exist. Add backend/protocol operations only where the prepared transaction cannot express the required atomic cut; cover any wire/version changes across hosts.
- Preserve legacy fixtures as executable reference evidence, without production dependencies on `shoop_app_reference`. The current Lua API has no dedicated duplication function; verify Lua visibility and control of cloned loops without adding a new scripting API.

## Stage 1 Establish the branch and operation contract

- [x] Fetch the latest base, create `feat/p11-loop-duplication` from `feat/backend-replacement`, read applicable agent/build/test guidance and carry `PLAN_DUP.md` onto that branch.
- [x] Map reference destination selection, aligned-row policy, copied fields and composite identities to production structures. Use `duplicate_loop_below`, `duplicate_loop_into`, `finish_primitive_loop_duplicate` and `duplicate_uses_the_first_empty_slot_below_and_drop_actions_target_and_swap` in `shoop_app_reference/src/lib.rs` as behavioral anchors.
- [x] Specify destination reservation, source capture, private preparation, admission, activation, publication, retirement and terminal outcomes. Define revision checks, recording conflicts and capacity accounting, including complete row expansion. Source edits after a successful pin must not change the copy.
- [x] Audit capture/import/composite transactions and identify the smallest missing atomic operation before implementation.
- [x] Verify the retained reference fixture and focused baseline service/persistence/composite tests. Distinguish existing failures from regressions.

### Prepared transaction contract

The service reserves one duplication operation and blocks conflicting mutations while source metadata/media are pinned. The backend prepares a targeted candidate containing the destination replacement and any aligned-row additions; unchanged owners keep their identities and live state. Preparation reserves media, topology, graph and retirement ownership. Activation rechecks affected capture state and publishes only the changed owners. Occupied destinations reserve a fresh runtime slot; the old owner remains private through the atomic Stop boundary so MIDI cleanup and composite child-stop actions execute before its later retirement. Application slot identity stays stable. Before activation, failure retires the private candidate; afterwards, the service publishes copied metadata/layout once and retains the operation result for reconnect reconciliation. Pending source composite edits refuse rather than copying an unconfirmed definition.

## Stage 2 Prepare and activate exact primitive copies

Depends on Stage 1.

- [x] Implement private preparation in `shoop_rt_backend`, preserving raw media, timing, roles, controls and recorded processor state.
- [x] Extend `shoop_backend`, `shoop_rt_protocol` and `shoop_rt_engine` only as needed to activate destination content, stopped transport and controls atomically. Preserve old exports until readers release them and retire owners through existing control-side paths.
- [x] Preflight slots, media/pages/indexes, receipts and retirement ownership, including retained old exports. Support cancellation and failed-preparation cleanup without changing live content.
- [x] Verify exact samples, MIDI bytes/order/initial packets and timing across the primitive acceptance matrix. Exercise edit independence in both directions, playing destinations, retained exports, exhaustion, stale requests and repeated cancellation/teardown with allocation guards active.

## Stage 3 Implement service duplication and recovery

Depends on Stage 2.

- [x] Replace the unconditional refusal in `shoop_app/src/service/commands.rs` with one host-owned asynchronous operation supporting both duplication intents. Integrate operation IDs, reliable admission/results, command conflicts and persistence ownership.
- [x] Select or privately reserve the destination and complete aligned-row expansion. Explicit occupied destinations use the same commit path. Publish layout, metadata, media, controls and stopped state coherently.
- [x] Fence stale session/destination identities and conflicting changes. Preserve pre-admission cancellation and post-admission host ownership across detach/reconnect. A pinned source cut remains stable if the source changes later.
- [x] Verify slot skipping, row creation, explicit overwrite, invalid destinations, source changes, recording conflicts and unrelated activity. Inject preparation/activation failures and assert either unchanged destination/layout or exactly one complete publication.

## Stage 4 Integrate supported composite duplication

Depends on Stage 3 and the existing completed P03 core.

- [x] Prepare an independent destination document/program from the confirmed source. Consistently rebind document/event/runtime identities while preserving child references, timing basis, frozen geometry, explicit sync and repeat intent.
- [x] Route primitive/composite replacement through the same atomic ownership/publication contract. Safely retire previous destination programs, active-child ownership and pending work without affecting unrelated owners.
- [x] Reuse structural, dependency, cycle, unsupported-mode and capacity checks. Refuse invalid replacements and unsettled conflicting composite edits before mutation.
- [x] Verify nested regular/explicit composites, edit independence, child-reference preservation, both replacement directions, playing destinations, retained old programs and failure cleanup. Keep P17 modes refused intact.

## Stage 5 Enable public workflows and capabilities

Depends on Stages 3 and 4.

- [x] Advertise negotiated `LoopDuplication` only when the complete supported path is available. Duplication must work without a local file provider. Keep import and click-insertion capability decisions independent.
- [x] Connect existing Clone and explicit-destination gestures through `shoop_egui`, `shoop_app_client` and application transports. Use confirmed projection and existing progress/error presentation without optimistic content/layout changes.
- [x] Replace production refusal expectations, including `public_core_file_capabilities_keep_duplication_refused`, with supported-path and unavailable-host coverage. Preserve refusal tests for unsupported configurations and MIDI forms.
- [x] Verify UI gestures, encoded command parity, confirmed destination state and Lua query/control behavior after cloning.
- [x] Exercise cancellation, stale callbacks, duplicate operation delivery and accepted-operation reconnect. Assert one copy and one terminal outcome.

## Stage 6 Validate end to end and deliver

Depends on all previous stages.

- [x] Run native typed/socket workflows: create supported media, clone, overwrite occupied slots, duplicate composites, edit either copy, play and save/load/save. Compare exact media and persistent fields. Verify stopped destinations, fresh load identities and preserved unrelated playback/recording.
- [x] Run actual browser/application AudioWorklet workflows covering direct, role-aware and supported composite copies. Verify content/output and bounded render memory. Exercise supported UI reattachment while retaining the browser-page-crash exclusion. Use CI for other supported operating systems and applicable firmware checks.
- [x] Run formatting, a warning-denying workspace build, the native workspace nextest command and test-usage, feature-parity, dependency-boundary and Python checks documented in `VALIDATION.md`. At the planning base the consolidated boundary command is `python3 scripts/check_dependency_boundaries.py --scope runtime --scope loops --scope engine`.
- [x] Run relevant loop/runtime ownership hosts and selected Miri checks if those layers change. Do not expand to unrelated stress suites without a concrete concern.
- [x] Update the persistence contract and P11 backlog with supported duplication behavior, capacities, refusal rules and implementation links. Keep P11 OPEN for MIDI restoration and reconcile `LoopDuplication` coverage without changing other parity scopes. Put revision-specific results in PR/CI artifacts.
- [x] Commit milestones, push and open a PR targeting `feat/backend-replacement`.
- [x] Resolve applicable CI failures and verify the final head. Report independently blocked checks truthfully rather than claiming a pass.
- [x] Check automated review after pushes, address feedback and repeat through approval. Explicitly report unavailable or absent review. Leave the PR unmerged with a clean worktree; do not merge autonomously.

## Testing strategy

Use focused reference, adapter, service and UI tests at the stage where each behavior becomes executable. Concentrate failure injection on publication, ownership and recovery boundaries, with independent sample/packet expectations. Run broad workspace/platform gates at integration, repeating them when subsequent changes or failures justify it. Native dummy/offline tests provide deterministic content evidence; actual browser execution and CI establish the corresponding host paths. No new physical-device qualification is required for this control/media feature.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

## Source anchors

- [Branch plan instructions](https://github.com/SanderVocke/shoopdaloop/blob/0b6812259ffc00b9a524bb6095cfc2c6b2112847/.agents/prompts/write_plan.md)
- [Feature backlog and closure requirements](https://github.com/SanderVocke/shoopdaloop/blob/0b6812259ffc00b9a524bb6095cfc2c6b2112847/docs/native_session_recovery/feature-parity.md)
- [Reference duplication behavior](https://github.com/SanderVocke/shoopdaloop/blob/0b6812259ffc00b9a524bb6095cfc2c6b2112847/src/rust/shoop_app_reference/src/lib.rs#L6572)
- [Production service refusal](https://github.com/SanderVocke/shoopdaloop/blob/0b6812259ffc00b9a524bb6095cfc2c6b2112847/src/rust/shoop_app/src/service/commands.rs#L761)
- [Role-aware content import](https://github.com/SanderVocke/shoopdaloop/blob/0b6812259ffc00b9a524bb6095cfc2c6b2112847/src/rust/shoop_rt_backend/src/import.rs)
- [Persistence contract](https://github.com/SanderVocke/shoopdaloop/blob/0b6812259ffc00b9a524bb6095cfc2c6b2112847/docs/native_session_recovery/persistence-contract.md)
- [Composite execution contract](https://github.com/SanderVocke/shoopdaloop/blob/0b6812259ffc00b9a524bb6095cfc2c6b2112847/docs/native_session_recovery/composites.md)
- [Validation commands](https://github.com/SanderVocke/shoopdaloop/blob/0b6812259ffc00b9a524bb6095cfc2c6b2112847/VALIDATION.md)
