# Simple loop duplication plan

Implement primitive and P03 composite Clone and Clone into by composing existing host-owned capture, content/definition-apply and control operations. Use a fresh implementation branch and a new PR targeting `feat/backend-replacement`, as an alternative to [PR #953](https://github.com/SanderVocke/shoopdaloop/pull/953), rather than stacking on its transaction machinery.

Planning base: `888bd8a66143a6a6a675883823578a7c1d7e4bee`. Follow `.agents/prompts/write_plan.md`, `.agents/rules/mandates.md` and `VALIDATION.md`. Implementation base is the same commit; work is on `feat/simple-loop-duplication`.

Scope includes supported Direct, dry/wet, built-in FX and synth primitive role media, plus the user-requested P03 composite core: Direct-track regular and explicit-mode definitions using Play, Stop and Record, including supported nesting. Composite copies retain child references rather than cloning children. Copying a composite into an empty primitive slot and replacing an existing P03 composite are included. Occupied cross-kind replacement is deferred because the existing operations require destructive clearing before application; refuse it before mutation. P17 dependent modes and MIDI initial-state, preroll, seek and parameter-history restoration remain outside this work. Existing supported MIDI content is copied exactly. This is a deliberate partial P11 delivery.

The new PR adopts the sequential failure contract below in place of the earlier plan's combined media/control/topology transaction. The existing PR remains available for comparison; closing or merging it requires the user's instruction.

## Goals and immutable acceptance criteria

1. **Reuse existing operations.** Implement one service-owned sequence over loop capture, existing media application and ordinary setters. Introduce no engine wire messages/version changes, runtime configuration operations, temporary runtime loop owners or new retirement mechanism. A narrow backend payload-loading entry point/shared helper is permitted where the existing public update shape lacks a captured field.
2. **Destination selection.** Clone chooses the first empty non-composite slot below the source on its track, adding slots through the existing aligned-row policy when needed. Clone into accepts an explicit compatible destination on the same track: primitive-to-primitive, P03 composite-to-composite, or composite-to-empty-primitive conversion, including playing same-kind destinations when safe to stop. Preserve the sync-loop exclusion. Reject self-copy, stale identities, incompatible shapes, unsupported modes and occupied cross-kind replacement before modifying the destination.
3. **Exact primitive payload and controls.** Copy complete supported raw audio/MIDI, channel roles/order, per-channel lengths, offsets, capture alignment, preplay, logical length and recorded processor state. Copy name, gain/balance, repeat and sync settings through existing host operations. Cover empty, timing-only, audio-only, MIDI-only, mixed and processed loops. Do not round-trip through export codecs, resample, truncate raw tails or retain destination channels absent from the source.
4. **Independent stopped result.** Success produces an independent destination stopped at position zero, with no pending transition copied from the source. Keep the existing destination's public identity and its own selection/targeting; do not copy the source's transient UI state. Editing or clearing either loop's own media/definition cannot change the other's. Composite children intentionally remain shared: editing a referenced child affects both arrangements. Unrelated playback and recording continue.
5. **Protect existing destination content.** Pin the source and validate compatibility before stopping an occupied destination. Use existing atomic media application or acknowledged prepared-composite replacement; never clear existing destination content in preparation for loading it. A failure before the content/definition commit preserves that content. The destination may remain stopped, and newly added empty rows may remain visible. Full rollback of transport, metadata and row layout is not promised.
6. **Report the real outcome.** Once media application or composite-definition activation is confirmed, the new content is committed. If subsequent control/metadata work fails, publish the actual confirmed destination and return an explicit message identifying that content was copied and which remaining step failed. Do not restore old content or repeat the copy automatically. Unknown backend outcomes remain unknown rather than being reported as unchanged or successful.
7. **Safe admission and ownership.** Refuse affected active, armed or scheduled recording/replacing, including known composite-driven writes, without stopping that recording. Reuse existing session/driver/persistence conflict checks and bounded resource limits. An accepted job stays on the host across UI disconnect/reconnect, with one operation ID and one terminal result, without replaying load or row creation. Broader host/engine crash recovery is excluded.
8. **Exact P03 definition.** Copy the active confirmed document, kind, timing basis, frozen effective geometry, explicit sync and repeat intent. Allocate fresh event identities and remap frozen geometry to them, using existing service allocators/compiler and independent prepared-version ownership. Preserve child loop references and authored order/modes. Reject unsettled edits, unsupported P17 modes/topologies anywhere in the authored dependency closure, and cycles introduced at the destination, before mutation. Inspect authored events even when later events shadow them in the compiled program.
9. **Bounded scope and public support.** Both paths work without a filesystem provider. UI eligibility and refusal messages expose supported P03 copies and the occupied cross-kind/P17 limits. Primitive copying into an existing slot must not require a spare runtime loop slot; composites use existing negotiated program budgets. Document the sequential failure semantics and remaining parity gaps; keep P11 OPEN.

Goals and acceptance criteria must not be changed without explicit user approval.

## Design rules and reuse map

| Existing facility | Planned use / gap to check |
| --- | --- |
| `Backend::capture_loop_content_async`, `shoop_rt_backend::capture_core` | Capture one coherent raw source cut, including role media and recorded processor state. Settle pending source metadata edits before pinning the matching service fields. |
| `Backend::replace_loop_content_async`, `shoop_rt_backend::import_content` | Reuse validation, upload, budget accounting, `ContentApply`, receipts and cleanup. The public update shape lacks recorded processor state and normally preserves parts of the destination; a clone must supply a complete payload and explicit empty channels/state. |
| Existing `replace_loop_family_async` | Check whether existing full-content loading can be reused naturally. Do not pull unrelated sync-family mutation into a single-loop copy merely to avoid a small helper. |
| Existing loop transition, gain/balance, timing, repeat and sync setters | Stop the destination and apply copied controls using existing acknowledgements. Confirm stopped position-zero behavior in the vertical slice. |
| Service composite submission/compiler and backend prepared-composite APIs | Copy a confirmed definition through `submit_composite_geometry`, `configure_prepared_composite_loop` or `convert_empty_loop_to_prepared_composite`. Reuse dependency checks, candidate ownership, activation acknowledgement and cleanup. Existing conversion accepts only an empty stopped primitive; do not bypass that guard. |
| Service `Command::AddLoop` and reference aligned-row selection | Compose existing structural operations and retain their confirmed results. A later failure can leave known empty additions. |
| Service pending operations, mutation completions and persistence exclusion | Use a small explicit job with stages; reuse host operation ownership and reconciliation. Avoid a new general transaction, lock or recovery framework. |

- Start from the latest base during implementation. Treat PR #953 as a source of behavioral examples and selected independent tests, not as a dependency or a bulk cherry-pick.
- Prefer one shared content-apply builder used by import and copying. Fill the recorded-state/complete-payload gap without duplicating the upload pipeline or adding a second backend copy engine.
- Keep service data and backend raw data in memory on the host. Do not require a client download/upload, file picker, persistence provider or whole-session save/load.
- Use existing coarse busy/fencing rules while a copy runs. It is acceptable to refuse competing edits briefly; do not build per-entity concurrency infrastructure. Recheck target identity, format and capture conflicts at the existing apply boundary.
- Make the phases explicit: **capture media or pin confirmed document and validate → resolve/add destination → stop and confirm → apply media/definition and confirm → copy controls/metadata → finish**. Use two payload variants in one host job, not a new transaction framework. Track issued work and acknowledgements so polling or reconnect cannot issue a stage twice.
- Honor existing cancellation only before content/definition apply is submitted. Once apply is in flight, reconcile its outcome and finish or report partial/unknown completion; add no new cancellation protocol. A submitted composite candidate is not success until its activation is confirmed.
- Keep intermediate engine state honest in the confirmed projection, with existing pending/error presentation. Do not imply that a stopped destination, visible empty row or acknowledged media apply has rolled back.
- Reuse existing MIDI cleanup in Stop and existing content ownership in import. Keep RT allocation/nonblocking constraints and all existing fixtures, watchdogs and assertions.
- For composite destinations, await the ordinary parent Stop and existing child/MIDI cleanup before replacing the definition. Respect existing dependency/owner exclusions; do not directly stop shared children or invent new owner-retirement behavior. Reuse frozen geometry rather than recomputing durations from changed child lengths.
- Keep unrelated native-driver cleanup, browser/CI infrastructure fixes and other parity work out of the feature PR. Establish evidence for an independent base failure and handle it separately instead of accumulating fixes here.

## Stage 1 Establish the fresh branch and prove the small path

- [x] Fetch `feat/backend-replacement`, create `feat/simple-loop-duplication` from its latest head, and carry this plan onto it. Record the implementation base and applicable guidance; leave PR #953 untouched.
- [x] Map copied fields and destination selection against `duplicate_loop_below`, `duplicate_loop_into` and `finish_primitive_loop_duplicate` in the retained reference. Write down intentional differences in atomicity and occupied cross-kind/P17 support.
- [x] Build a focused vertical slice using existing capture/apply operations: copy a known primitive into an empty stopped slot; confirm exact content, independent edits, stopped position zero and no filesystem dependency.
- [x] Demonstrate ordinary occupied-slot media replacement and failure preservation using the existing importer before designing extensions. Audit recorded processor state, explicit channel clearing and sync/repeat field gaps. Prove prepared P03 replacement and empty-slot conversion preserve the old definition/slot on pre-activation refusal and expose an acknowledgement usable by the job.
- [x] Review the production diff and subsystem boundaries. Choose the smallest shared payload helper that closes demonstrated gaps. If this requires new RT/wire machinery, report the blocker and revisit the design with the user before expanding scope.

Stage 1 design decision: use a narrow complete-payload entry point sharing the single-loop importer, because partial updates omit recorded processor state and preserve unspecified channels. Existing prepared-composite conversion/replacement supplies candidate ownership and activation receipts. Reference row selection and copied fields are retained; row/control atomicity and occupied cross-kind conversion are intentionally excluded as specified above. No RT protocol extension is needed.

## Stage 2 Reuse exact captured-payload loading

Depends on Stage 1.

- [x] Map a captured primitive to a complete destination payload, preserving every supported raw field and explicitly replacing/clearing all audio, MIDI and recorded-state fields. Validate roles, format and capacities before destination mutation.
- [x] If required, expose a thin backend operation accepting captured payloads or refactor the existing import helper so both routes use the same validation/upload/`ContentApply`/cleanup code. Keep ordinary partial-import behavior intact.
- [x] Verify exact samples, packet bytes/order and channel timing for empty, timing-only, audio/MIDI/mixed and processed cases. Include copying empty/opposite-media sources over occupied content and recorded-state replacement/clearing.
- [x] Verify retained readers, edit independence, existing importer regressions, exhaustion before apply and copying into an existing destination when runtime loop slots are full. No temporary loop-owner allocation is allowed.

## Stage 3 Compose the service-owned copy sequence

Depends on Stage 2.

- [x] Add one small host job for `Duplicate` and `DuplicateTo`, reusing existing operation IDs, receipts and conflict handling. Pin source metadata with the captured content; reserve the selected destination against competing commands.
- [x] Resolve the first empty slot or compose existing row additions. Stop a playing primitive destination and await confirmation, then load the payload and apply copied controls/metadata through existing setters.
- [x] Record the media-apply submission and confirmed commit boundaries. Finish each stage once; publish the actual confirmed state and stage-specific failure message. Do not clear existing media first, replay an uncertain load or implement compensating media rollback.
- [x] Test capture/validation refusal, row-add failure, stop failure, apply refusal/exhaustion and post-apply control failure. Assert source preservation, old media before commit, copied media after commit, allowed stopped/empty-row side effects and truthful partial/unknown results.
- [x] Test busy admission, stale destinations, pending transitions and scheduled/composite-driven recording. Verify unrelated recording/playback continues. Exercise duplicate delivery and UI disconnect/reconnect before and after apply, proving one copy and one terminal outcome.

## Stage 4 Add P03 definition copying through the existing editor

Depends on Stage 3.

- [x] Add the definition payload variant: pin only the active confirmed P03 document and its frozen geometry; require conflicting edits/candidates to settle or refuse. Allocate fresh event IDs and remap geometry, preserving basis, kind, authored order/modes and existing child references. Check the recursive authored closure for Direct/P03 eligibility, including shadowed unsupported events.
- [x] Preflight destination-substituted dependency/cycle checks and existing capacity/version fences. Support P03-to-P03 replacement and conversion of an empty stopped primitive through existing submission APIs. Refuse occupied cross-kind replacement before Stop or row creation; add no Clear/reset sequence or temporary owner.
- [x] Extend the same host job to Stop/confirm a composite destination, submit exactly one prepared definition and await the existing activation outcome before applying metadata/controls. Reuse candidate cleanup and parent/child Stop ownership. Preserve old confirmed definitions on pre-activation failure; report post-activation control failure as committed content, just like the primitive path.
- [x] Verify empty, regular, explicit Play/Stop/Record and nested P03 definitions; fresh IDs, frozen geometry despite changed child lengths, child-reference preservation and independent edits. Cover playing destinations, parent/child cleanup, cycles, unsettled candidates, shadowed P17 refusal, capacity exhaustion, activation failure and reconnect without resubmission. Assert unsupported conversions preserve the entire destination.

## Stage 5 Expose the feature and document its contract

Depends on Stage 4.

- [x] Connect existing Clone/Clone into gestures through `shoop_egui` and `shoop_app_client` to the host job. Preserve existing sync-loop eligibility; offer supported P03 actions and explain unsupported cross-kind/P17 cases consistently with service admission.
- [x] Advertise `LoopDuplication` only for hosts that support this contract, independently of file import/export provider availability; require existing composite capability for composite actions. Do not add a scripting API; verify existing Lua queries/control can observe and edit primitive and P03 copies.
- [x] Update the duplication/persistence contract and P11 coverage: primitive and P03 copying supported, occupied cross-kind conversions/P17 duplication deferred, sequential failure semantics explicit, MIDI restoration still open. Document shared child references. Do not claim full duplication parity.
- [x] Verify encoded command parity, UI eligibility, confirmed pending/error state, unsupported-host refusal, selection/target independence and the visible result of post-apply failure.

## Stage 6 Validate end to end

Depends on Stages 2–5.

- [x] Run production native host/transport workflows: raw source → empty copy → occupied overwrite → independent edit/clear → play → save/load/resave, plus P03 empty-slot conversion, nested/script definition replacement and safe cross-kind refusal. Compare persistent media, authored definitions, geometry and copied fields, stable slot identity and stopped result. Include accepted-operation reconnect without replay.
- [ ] Rebuild and run the actual browser/application AudioWorklet workflow with direct and supported processed primitive media and P03 copies. Exercise public gestures/dispatch, provider independence, independent definition editing and shared-child playback, save/load and refusal/partial-failure presentation; check content/output and render-memory bounds.
- [ ] Run formatting, the warning-denying workspace build, native nextest and the applicable policy/parity/dependency/Python checks from `VALIDATION.md`. Use CI for other supported platforms and existing firmware/JACK/ownership gates.
- [x] Review the final diff against the implementation base: no feature changes in `shoop_rt_protocol`, `shoop_rt_engine`, `shoop_loops` or `rt_runtime`; no unrelated host/CI fixes or copied transaction framework. Explain any narrow backend API/helper extension.

## Stage 7 Deliver the new PR and verify CI/review

Depends on Stage 6 for final readiness. A draft PR may be opened while final browser/base comparison checks run so CI and automated review can progress in parallel; final readiness still requires validation of the delivered head.

- [ ] Commit completed milestones, push `feat/simple-loop-duplication`, and open a separate PR targeting `feat/backend-replacement`. Link PR #953 as the alternative and explain the scope/failure-contract differences and production-versus-test diff size.
- [ ] Work applicable CI regressions to green and verify the delivered head. Diagnose failures with logs/artifacts before rerunning; preserve existing media fixtures, watchdogs and assertions. Demonstrate and report independently blocked/base failures instead of claiming a pass or absorbing unrelated repairs.
- [ ] Check automated review after pushes, address applicable feedback and repeat through approval. Explicitly report absent or unavailable review rather than inventing approval.
- [ ] Update this checklist and leave a clean worktree and the new PR unmerged. Do not close or merge PR #953, or merge the new PR, autonomously.

## Testing time versus confidence

Use the smallest real-backend vertical slice to validate the reuse premise before building orchestration, including a small prepared-composite replacement proof. During development, concentrate parameterized tests on exact payloads, definition/identity mapping and the few irreversible boundaries; use existing builders and selected reference/#953 examples rather than copying its entire test suite. Failure injection belongs mainly in service tests, with real adapter tests proving actual media/definition-apply behavior, Stop cleanup and independence. Include one authored-versus-compiled shadowing regression rather than a broad new P17 campaign.

Run the broad native gates at integration, and actual native/browser workflows once the public path is executable. Repeat a broad gate when changes, failures or unresolved concerns justify it. Existing CI supplies cross-platform and RT ownership coverage; unchanged RT layers do not call for a new Miri/stress campaign. Keep revision-specific results in PR/CI artifacts and ignored `work/`, not a committed chronological results ledger.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

## Source anchors

- [Plan instructions](.agents/prompts/write_plan.md), [validation guide](VALIDATION.md) and [previous transaction plan](https://github.com/SanderVocke/shoopdaloop/blob/d5dc97dd1d0d737e7542dc1601a7ce5b98b7ed4f/PLAN_DUP.md).
- [Base capture](https://github.com/SanderVocke/shoopdaloop/blob/888bd8a66143a6a6a675883823578a7c1d7e4bee/src/rust/shoop_rt_backend/src/capture.rs), [single-loop import](https://github.com/SanderVocke/shoopdaloop/blob/888bd8a66143a6a6a675883823578a7c1d7e4bee/src/rust/shoop_rt_backend/src/import.rs) and [existing family import](https://github.com/SanderVocke/shoopdaloop/blob/888bd8a66143a6a6a675883823578a7c1d7e4bee/src/rust/shoop_rt_backend/src/family.rs).
- [Backend captured/update shapes](https://github.com/SanderVocke/shoopdaloop/blob/888bd8a66143a6a6a675883823578a7c1d7e4bee/src/rust/shoop_backend/src/lib.rs), [service commands/slot creation](https://github.com/SanderVocke/shoopdaloop/blob/888bd8a66143a6a6a675883823578a7c1d7e4bee/src/rust/shoop_app/src/service/commands.rs) and [reference behavior](https://github.com/SanderVocke/shoopdaloop/blob/888bd8a66143a6a6a675883823578a7c1d7e4bee/src/rust/shoop_app_reference/src/lib.rs).
- [Service composite submission](https://github.com/SanderVocke/shoopdaloop/blob/888bd8a66143a6a6a675883823578a7c1d7e4bee/src/rust/shoop_app/src/service/composites/mod.rs), [frozen-geometry compiler](https://github.com/SanderVocke/shoopdaloop/blob/888bd8a66143a6a6a675883823578a7c1d7e4bee/src/rust/shoop_app/src/service/composites/prepare.rs), [prepared backend/empty-conversion guards](https://github.com/SanderVocke/shoopdaloop/blob/888bd8a66143a6a6a675883823578a7c1d7e4bee/src/rust/shoop_rt_backend/src/composites.rs) and [composite execution contract](https://github.com/SanderVocke/shoopdaloop/blob/888bd8a66143a6a6a675883823578a7c1d7e4bee/docs/native_session_recovery/composites.md).
