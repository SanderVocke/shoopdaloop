# P17 duplication and cross-kind clear-then-copy

Status: implementation in progress on `feat/p17-clear-then-copy`.

Written using [.agents/prompts/write_plan.md](.agents/prompts/write_plan.md).
Basis: the [P11 investigation](docs/investigations/p11-backend-parity.md) and the
user's agreed scope: restore P17 definition copying, and make cross-kind copying
equivalent to clearing the destination and then copying into it. MIDI state and
history work is deferred to a separate session.

## Goals and scope

Support existing Clone/Clone into routes for P17 regular, scripted and nested
composite definitions on eligible Direct, External, built-in FX and OxiSynth
tracks. Preserve the existing same-track destination restriction and ordinary
empty-slot/aligned-row behavior. Regular and scripted definitions both have the
composite kind; cross-kind means primitive versus composite.

For an occupied destination of a different kind, compose the normal Clear and
Copy operations in the service. Wait for complete clear confirmation before
starting the copy. A successful clear is not rolled back if copying later fails.
Same-kind copies retain their existing behavior and failure guarantees.

The scope includes public UI/Lua behavior, operation results and UI-detach
recovery, and save/load of the supported copied result. Existing MIDI/media
support remains the regression floor. Expanded MIDI initial/preroll/seek state or
parameter history, general latency/alignment controls, Carla and new cross-track
copying are outside this plan. P11 remains open for its deferred MIDI requirements.

## Immutable acceptance criteria

1. P17 definitions copy with their authored modes, explicit/default distinction,
   timing basis, frozen geometry, sync and repeat intent preserved. The copied
   definition has fresh event identities and prepared-version ownership. Editing
   it does not edit the source definition; existing child references remain shared.
2. Primitive-to-occupied-composite and composite-to-occupied-primitive copies have
   the same supported behavior and effects as normal Clear followed by normal Copy.
   Existing dependency restrictions still apply; this does not promise success for
   configurations refused by those operations. Copy starts only after the entire
   clear lifecycle, including dependent-definition work, has confirmed success.
3. Successful copies retain the destination's public slot identity, selection and
   targeting, copy supported metadata/settings, and finish stopped at position
   zero. Primitive copies retain supported role media and recorded processor state.
   Source content and unrelated active recordings retain their existing protections.
4. Same-kind copying and composite copying into an already empty slot do not gain
   a destructive clear phase. Their current before/after-commit guarantees,
   empty-slot search and aligned-row behavior remain intact.
5. A failed or unknown clear outcome does not start copying. After a confirmed
   clear, a copy failure leaves the state produced by the normal copy path and
   reports that the destination was cleared. Failure before copied-payload commit
   leaves it cleared; failure after that commit may leave copied content with
   incomplete controls/metadata. There is no restoration of the pre-clear payload.
6. One service-owned accepted operation tracks the sequence through UI loss and
   reattachment. Each phase executes at most once; unknown mutation outcomes are
   not replayed. Existing cancellation behavior, where available, does not undo
   an acknowledged clear. UI detachment is not cancellation.
7. Normal identity, capability, recording/future-write, pending-edit, capacity and
   cycle checks remain effective. Reject obvious invalid identities, including
   self-copy, before destructive work. Validate all authored P17 events, including
   events shadowed out of compiled spans, without translating unsupported modes.
8. Supported copies work without a file provider. Session save/load preserves the
   copied definitions, role media and supported settings, and existing persistence
   rules still apply. Documentation distinguishes same-kind preservation from
   cross-kind clear-then-copy and keeps MIDI history deferred.

## Design rules

- Keep orchestration in `shoop_app::Service`. Reuse its duplication job, normal
  clear/lifecycle helpers, composite validation/compiler and prepared activation.
  Clients submit one action and observe service results; they do not execute the
  clear/copy sequence themselves.
- Share validation with composite authoring. Remove the copy-only Direct-track and
  Play/Stop/Record restrictions while preserving eligibility checks and closure
  traversal for nested definitions and affected ownership.
- Route by confirmed kind/state. After a clear, revalidate source and destination
  and use current backend mappings. Do not carry stale engine generations or a
  source definition snapshot invalidated by the clear into the copy phase.
- Reuse clear's existing parent handling and refusals. Do not add graph-wide atomic
  publication, a new slot-replacement protocol, private backup/rollback storage or
  special conversion semantics. Existing engine empty-slot guards remain valid.
- Factor a small shared lifecycle helper if needed to compose jobs. Do not recursively
  submit top-level Clear/Copy commands that conflict with their own busy guards.
  Route completion/error receipts to the owning duplication operation and wait for
  terminal lifecycle success, not merely command admission or a reset receipt.
  After clear acknowledgement, wait for periodic loop telemetry to project the
  empty destination; a stale projection is not a clear failure.
- Definition duplication copies the definition, not its child loops or processors.
  Keep ordinary role-aware primitive copying separate from definition copying.
- Preserve current capability gates, finite capacities and RT scheduling/ownership
  contracts. No engine, protocol or file-format changes are anticipated. The
  baseline oracle exposed a loop-library conversion guard that rejected stopped,
  cleared recorded loops because their historical capture-frame counter remained
  nonzero. Check stopped mode and current content length instead; existing
  configuration preflight still validates capture-resource idleness. This narrow
  prerequisite fix adds no conversion protocol or transaction. If an
  existing operation cannot provide the agreed behavior, document a reproducible
  blocker and revise the smallest necessary implementation step.
- No new cancellation API, automatic retries of unknown outcomes, host-crash
  recovery or deployment scope is introduced by this feature.

## Code and evidence entry points

The replacement sources were inspected at `9aee74966fa66e896e7b6c26ce4893e8c532735b`
on `feat/backend-replacement`; master reference was
`9134eb20b7d1ee34e71b5745927645c9c2bd8d2a`. Refresh this evidence against the actual
implementation base before editing. Implementation base is the same replacement
commit, `9aee74966fa66e896e7b6c26ce4893e8c532735b`; the plan and investigation were
preserved when switching from the master-based planning checkout.

Paths below refer to the replacement branch:

- `src/rust/shoop_app/src/service/duplication.rs`: copy admission, dependency
  checks, source capture/definition cloning, phase progression and terminal errors.
- `src/rust/shoop_app/src/service/composites/{mod,prepare,structure}.rs`: authored
  mode validation, frozen geometry, normal clear and receipt-driven parent lifecycle.
- `src/rust/shoop_app/tests/support/{duplication_cases,composite_cases}.rs`:
  production copy and composite fixtures, including encoded delivery and failures.
- `src/rust/shoop_rt_backend/tests/{content_copy,composites,persistence}.rs`:
  existing content/role, prepared-composite and persistence contracts.
- `docs/native_session_recovery/{feature-parity,persistence-contract,composites}.md`:
  delivered scope, copying guarantees and P17 execution behavior.

The [completed simple-duplication plan](https://github.com/SanderVocke/shoopdaloop/blob/9aee74966fa66e896e7b6c26ce4893e8c532735b/PLAN_SIMPLE_DUP.md)
records the existing implementation and native/browser acceptance. Reuse its
fixtures and harnesses. This plan extends that delivered path; it does not revive
the earlier general transaction proposal or absorb unrelated host/CI repairs.

## Stage 1 — Prepare the branch and establish the behavior oracle

This stage precedes all implementation. Stages 2 and 3 follow it; Stage 4 requires
both. Stage 5 validates and delivers the integrated result.

- [x] Preserve this plan and existing investigation, fetch
  `origin/feat/backend-replacement`, and create an implementation branch such as
  `feat/p17-clear-then-copy` from it. Read the branch's AGENTS/index/mandates,
  style, build and test guidance. Record the base SHA here; do not implement on
  the master-based planning checkout.
- [x] Refresh the entry-point review and identify the normal user Clear route,
  its complete terminal-success signal, and how lifecycle receipts/busy guards
  interact with the existing duplication job. Read `VALIDATION.md` and the completed
  duplication plan to select the current commands and existing acceptance harnesses.
- [x] Establish a reusable fixture that performs normal Clear to completion and
  then normal Copy. Use it as the semantic oracle for cross-kind copying; compare
  payload, kind, settings, parent effects and results while allowing fresh internal
  event/version identities to differ.
- [x] Run focused existing duplication and relevant clear/composite lifecycle
  fixtures as a baseline. Record failures separately from new-work regressions.

Verification: confirm the implementation base is the replacement branch; the
manual-sequence fixture exercises production service/backend paths and can delay
clear completion. Commit the completed preparation/test milestone.

## Stage 2 — Enable P17 definition copying

- [x] Share authored-event validation with ordinary composite submission and
  replace the copy-only topology/mode allowlists. Preserve nested readiness,
  capability, recording/future-write and cycle checks.
- [x] Reuse definition cloning, fresh event IDs/prepared versions and frozen
  geometry. Keep child references and explicit versus dynamic-default modes.
- [x] Extend production fixtures for Replacing, dry playback and wet rerecord on
  supported track topologies, regular/scripted nesting, empty-slot cloning and
  composite-to-composite replacement. Verify independent definition edits and
  shared-child behavior.
- [x] Update the old combined cross-kind/P17-refusal fixture: retain valid busy,
  cycle and unsupported-capability assertions; replace obsolete blanket mode
  refusals with supported-copy cases. Keep an unsupported authored event hidden
  by overlapping spans to prove validation examines the authored document.

Verification: focused copy/composite tests pass through typed and encoded service
delivery. Existing P03, role-aware primitive copying and no-file-provider cases
remain green. Commit this stage independently of Stage 3.

## Stage 3 — Compose Clear and Copy for cross-kind destinations

- [x] Add a clear-needed decision only for occupied differing kinds. Keep the
  existing same-kind and empty-slot routes unchanged.
- [x] Compose the normal clear lifecycle into the duplication job using shared
  helpers and one owning service operation. Track clear progress/error without
  allowing copy advancement or premature operation completion.
- [x] After full clear confirmation, re-read confirmed state and enter the normal
  copy path, including ordinary source capture/definition preparation and any
  required existing empty-slot conversion. Revalidate mappings and dependencies.
- [x] Use existing lifecycle locks/admission to prevent conflicting edits between
  phases. Preserve ordinary Stop behavior and UI detach/reattach reconciliation.
- [x] Track whether clear was confirmed so terminal messages distinguish clear
  failure/unknown outcome, cleared-but-copy-failed, and copied-content-with-later-
  failure. Preserve same-kind error guarantees. Apply existing cancellation rules
  without adding a new cancellation workflow or restoring pre-clear content.
- [x] Add manual-sequence comparisons for both directions, Direct and eligible
  processed tracks, empty/timing-only cases and supported parent references.
  Match existing manual-sequence refusal where references prevent conversion;
  do not add a new graph rewrite to make those cases succeed.
- [x] Delay/fail clear admission and lifecycle completion, source capture, copy
  preparation/application and controls. Assert no copy before clear confirmation,
  no copy after clear failure/unknown outcome, no rollback of a confirmed clear,
  no replay, and no added clear for same-kind copies.

Verification: focused service lifecycle/copy tests and existing adapter copy tests
pass. A held clear completion cannot deadlock the composed job or incorrectly
finish its outer operation. Commit the stage with before/after-clear fault evidence.

## Stage 4 — Verify integration, persistence and detach behavior

- [x] Exercise the public UI Clone/Clone into actions and existing Lua read/edit route
  against the production service, confirming one accepted operation and accurate
  progress/results for P17 and both cross-kind directions.
- [x] Detach and reattach before clear confirmation, between phases, during copy
  application and after commit. Verify the same retained operation settles once
  and neither mutation phase is resubmitted.
- [x] Save/load supported copied definitions and primitive role media, checking
  authored modes, frozen geometry, fresh loaded identities, shared child references
  and supported recorded processor state. Reuse current persistence fixtures;
  retain existing MIDI limits.
- [x] Recheck ordinary primitive/P03 copies, first-empty-slot/aligned-row behavior,
  source preservation and unrelated recording. Verify refused identity/capability/
  cycle/busy cases retain their established effects.
- [x] Update the persistence contract and parity ledger with clear-then-copy
  failure semantics and duplication evidence. Keep P11's deferred MIDI requirements
  explicitly open and align the investigation with the final implementation.

Verification: targeted public-route, encoded service, persistence and retained-work
tests pass; copied definitions are executable after load. Commit integration and
documentation as a meaningful milestone.

## Stage 5 — Final end-to-end validation, PR and review

- [ ] Run the implementation branch's documented formatting, warning-denying build
  and native suite in its prescribed environment. Use Nix's development shell
  where required. Run `python3 scripts/check_shoop_test_usage.py` if Rust tests
  changed and relevant dependency-boundary checks if layering changed.
- [ ] Exercise packaged native typed and socket delivery, applicable shared-memory
  delivery, and an actual browser AudioWorklet using the repository's existing
  acceptance harnesses. Cover a P17 definition copy, both cross-kind directions,
  a copy failure after clear, service reattachment and a saved/reloaded result.
  Compare with normal Clear followed by Copy; record exact commands and outcomes.
- [ ] Record the tested source SHA and material validation limits in the PR and
  parity evidence. Run relevant retained reference duplication fixtures as well
  as production acceptance; do not treat reference-only coverage as completion.
- [ ] Push the implementation branch and create a PR targeting
  `feat/backend-replacement`, describing same-kind preservation, cross-kind
  destructive-clear semantics, delivered P17 coverage and deferred MIDI scope.
- [ ] Monitor required CI until green. Inspect automated PR review; address
  feedback, rerun affected checks, push fixes and check again until the automated
  reviewer approves. Document unavailable checks or unresolved external failures
  explicitly instead of claiming successful validation. Reproduce independent base
  failures and report them separately; do not expand this PR into host/CI repairs.
- [ ] Check off completed criteria with final-head evidence and give the user
  the PR and any remaining limits. Do not merge autonomously.

Testing strategy: use focused service/composite/copy fixtures while iterating and
add meaningful delayed-completion, fault and semantic-oracle tests at the changed
boundaries. Broaden to integration and platform acceptance once both features are
present, then run the full required gates at the final milestone. Repeat or widen
tests after new changes or unresolved failures, not after unchanged successful runs.
Follow the fetched branch's test guide rather than copying commands from master:
its workspace features and browser host target differ in this migration.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
