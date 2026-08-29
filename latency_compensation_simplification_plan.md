# Latency Compensation Simplification Plan

## Execution contract

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

## Goals

- Reduce latency compensation to the smallest dependable workflow for a typical user: determine one effective recording offset, latch it when an operation starts, retain enough raw media to apply it, and preserve the resulting take alignment through playback, save/load, resampling, and normal export.
- Keep processor render advance only where dry-through-wet and dry-into-wet behavior requires it, separate from recording alignment.
- Prefer reliable automatic backend observations with a manual fallback over a generalized provider, route, component, and provenance model.
- Remove advanced diagnostics, historical reconstruction, raw-margin recovery/export, and other supporting surfaces that do not contribute directly to ordinary compensated recording and playback.
- Remove the abandoned differentiating terminology, in every capitalization, from tracked names, tests, comments, documentation, and other repository text; the single-offset design needs no distinguishing qualifier.
- Rework the test suite around the retained behavior and remove tests whose only purpose is to enforce deleted functionality.

## Scope

### Retain

- One effective signed recording offset per track, sourced from a reliable backend observation plus a manual override or trim.
- One immutable signed capture alignment per completed take; setting or provider changes affect future operations only.
- Manual correction of the effective track value and completed take alignment.
- Operation-boundary latching and callback-safe publication without realtime allocation or locking.
- Automatic bounded pre-record and post-record retention sufficient to produce a complete corrected take; publish content only after postroll settles.
- One clear, atomic failure when the required retention cannot be prepared or completed; never publish partially compensated content.
- Correct audio and MIDI recording, playback, grab/replacement where retained, loop-wrap behavior, save/load, sample-rate conversion, and normal logical export.
- Processor render advance and its callback/transition safety only for supported dry/wet operations.
- JACK automatic latency where dependable, with a manual path on unsupported or uncertain backends.
- Native and browser operation, with browser latency allowed to be manual-only if its automatic values cannot be represented without restoring the removed provider model.

### Remove or defer

- The five-component recipe and per-operation component applicability model, including cue-followed state, cue-output selection, path aggregation, path ambiguity, automatic interval identity, range-point selection, and per-component enable/mode controls.
- Separate certainty, range, source, interval, revision, warning, and component provenance once an effective offset has been resolved.
- Per-take observation ranges, revisions, variable-history flags/counts, changed-during-operation comparison, current-versus-frozen comparison, and applied-during-render provenance that is not required to interpret media.
- Retained observation-history reconstruction and stable/variable/unavailable history selection; latch the effective value at the operation boundary instead.
- Latency-specific raw audio/MIDI export including retained margins, and latency-specific consolidation/bake/recovery workflows.
- Diagnostic counters, plots, revision comparisons, provider forensics, and the advanced latency panel; replace them with compact effective-value controls and actionable failure/status feedback.
- Automatic Carla adapter, OxiSynth phase-range, browser timing, CPAL/midir, and Web MIDI provider modeling unless a provider can supply the reduced effective value directly without special policy/provenance infrastructure.
- Incomplete compensated takes as a recoverable persistent state.

## Immutable acceptance criteria

1. A typical user can set or obtain an effective recording offset, record audio or MIDI, and hear the completed loop aligned without manually moving its content.
2. Every operation latches its effective offset at its start. Later policy, route, buffer-size, or provider changes do not move an existing take; they apply only to future operations.
3. A completed take persists only the timing data required to reproduce it, including its signed capture alignment. Save/load and sample-rate conversion preserve the same timing in seconds within rounding tolerance.
4. Positive and supported negative alignments retain all raw media required for the complete logical loop. A take is not published or saved as settled until required postroll finishes.
5. If required retention cannot be reserved or completed, the operation fails atomically with a concise actionable error and does not expose partially corrected content.
6. Normal playback and normal audio/MIDI export use the logical compensated window. No latency-specific raw-margin export or consolidation UI/API remains.
7. Supported dry-through-wet and dry-into-wet operations apply processor render advance exactly once and do not delay live monitoring.
8. Automatic latency is limited to providers that can supply the reduced effective value truthfully. Every supported runtime has a manual fallback and does not invent a precise automatic value.
9. Latency settings expose only the reduced effective recording value, its manual adjustment, any retained processor value, and concise pending/error state; deleted component, cue-route, provenance, and diagnostic controls are absent.
10. Audio callback paths remain bounded, allocation-free after preparation, and free of blocking synchronization for latency work.
11. Audio and MIDI behavior remains deterministic across callback sizes, loop boundaries, immediate/planned supported transitions, and session round trips.
12. A case-insensitive repository search for the abandoned differentiating term returns no tracked occurrences, including identifiers, test names, comments, and documentation.
13. Tests cover every retained acceptance criterion and no test, fixture, snapshot, protocol field, or compatibility assertion requires removed functionality.
14. The final implementation has materially fewer changed production and test lines than this branch at the start of the work. The final audit records path-based added/deleted LOC and explains any retained large subsystem.

## Design rules and constraints

- Use a single signed frame mapping for take capture alignment. Give it a durable domain name such as `FrameMapping` or `CaptureFrameMapping`, without terminology referring to abandoned alternatives.
- Separate capture alignment from processor render advance; never combine them into a value that can be applied twice.
- Resolve and validate control-path values before publishing a compact callback-facing snapshot.
- Do not allocate, lock, perform unbounded work, or rebuild the graph in the audio callback merely because a latency value changes.
- Prepare retention capacity before arming an operation. Keep retention internal except for concise pending/failure status.
- Prefer aborting an unsupported or incomplete operation over persisting ambiguous content.
- Store frames in session data with the sample rate needed for deterministic conversion; do not persist provider-forensic metadata.
- Preserve compatibility only where an existing released session format requires it. Branch-only latency fields may be removed rather than migrated indefinitely.
- Do not preserve generalized abstractions solely for hypothetical future multi-region, multi-path, or automatic-provider functionality.
- Keep native and browser protocols symmetric only for behavior both runtimes actually support; manual-only browser latency is acceptable.
- Avoid unrelated refactors except for the repository-wide terminology cleanup required by acceptance criterion 12.

## Staged implementation

### Stage 0: Baseline inventory and deletion map

- [x] Record the current branch commit, merge-base, path-based LOC, latency-related files, public API/protocol/session fields, and the full case-insensitive terminology inventory.
- [x] Map every retained behavior to its implementation and tests; classify all latency tests as retain, rewrite, or delete.
- [x] Identify whether any branch-only session/wire compatibility surface can be removed outright and document any released compatibility that must remain.
- [x] Turn the scope above into a deletion map ordered from domain types outward through engine, backend, protocols, app, UI, persistence, documentation, and tests.

Verification:

- [x] Review the inventory against every acceptance criterion and confirm each retained behavior has at least one planned test owner.
- [x] Save reproducible `git diff --numstat`, `rg`, and test-list commands in the implementation audit.

### Stage 1: Reduce the domain and terminology

Depends on Stage 0.

- [x] Replace the qualified frame-mapping name and related names with durable single-offset terminology; remove every remaining tracked use of the abandoned term, including generic helper/test/comment occurrences elsewhere in the repository.
- [ ] Reduce the latency domain to checked signed frame mapping, bounded effective recording offset, and a separate bounded processor render advance.
- [ ] Remove component kinds and policies, recording references, cue applicability, path aggregation/ambiguity, observation interval/source identities, ranges/certainty where no provider boundary still needs them, resolved recipes, and forensic take snapshots.
- [ ] Define the minimal control-path and callback-facing values, errors, and invariants needed by subsequent stages.

Verification:

- [ ] Add focused domain tests for signed mapping, bounds, overflow, manual override/trim semantics, and separation of capture alignment from render advance.
- [ ] Run the latency-domain crate tests and `cargo clippy`/format checks applicable to the touched crates.
- [x] Confirm the case-insensitive tracked terminology audit has no matches.

### Stage 2: Simplify engine latching, retention, and channel behavior

Depends on Stage 1.

- [ ] Replace recipe latching with one validated effective recording offset and, only for dry/wet modes, one processor render-advance value.
- [ ] Remove retained observation spans, historical selection, variable-revision handling, recipe/component diagnostics, and incomplete-take recovery state.
- [ ] Prepare bounded pre/post retention before record, grab, or replacement operations; keep postroll content unsettled until finalization and abort atomically on capacity/finalization failure.
- [ ] Apply the frozen take alignment during audio and MIDI playback and the processor advance exactly once in supported render modes.
- [ ] Preserve immediate live monitoring, topology stability, callback safety, loop-wrap correctness, and transactional take edits.

Verification:

- [ ] Rewrite channel/runtime tests around start-latched values, later setting changes, complete pre/post windows, atomic insufficient-capacity failure, postroll settlement, loop wrap, and no callback allocations.
- [ ] Retain deterministic audio/MIDI oracles for ordinary record/play and supported dry/wet modes; delete component/history matrices.
- [ ] Run focused engine unit and integration tests under representative callback sizes and sample rates.

### Stage 3: Collapse backend and provider policy

Depends on Stage 2.

- [ ] Replace component recipes and provider provenance with one effective recording-offset control and one optional processor-advance control.
- [ ] Keep JACK automatic reporting only where it maps truthfully to the effective value; make unsupported backends explicitly manual rather than estimated.
- [ ] Remove cue-route resolution, ambiguity tracking, Carla adapter/provenance integration, OxiSynth range modeling, and other automatic providers outside the reduced contract.
- [ ] Simplify native backend commands, snapshots, state mirrors, and errors to the minimal values/status needed by the app and engine.

Verification:

- [ ] Rewrite JACK/backend tests for automatic effective values, manual fallback, latching, and unsupported capability behavior.
- [ ] Remove provider compatibility fixtures and patches that no retained provider consumes.
- [ ] Run native backend tests and assert latency updates neither rebuild topology nor block callback work.

### Stage 4: Simplify browser and cross-process protocols

Depends on Stage 3.

- [ ] Remove component observations, provenance, history, diagnostics, cue routing, and deleted commands from audio protocol, worklet, client, and plugin protocol surfaces.
- [ ] Carry only effective recording offset, optional processor advance, frozen take alignment, and concise pending/error/finalizing state.
- [ ] Use manual-only browser latency unless browser APIs provide a truthful value under the reduced contract without special certainty/range semantics.
- [ ] Remove obsolete wire compatibility code and update transport versioning if the protocol contract requires it.

Verification:

- [ ] Rewrite protocol round-trip and worklet tests for the reduced state and commands.
- [ ] Run Wasm/worklet tests and browser smoke coverage for manual compensation, recording finalization, playback, and failure reporting.

### Stage 5: Minimize app model, persistence, and exports

Depends on Stages 3 and 4.

- [ ] Replace app/API track policy with effective value, manual adjustment, optional processor value, and concise pending/error state.
- [ ] Reduce per-take state and session documents to signed alignment plus timing/status data required for correct settled content; remove forensic provenance and persistent incomplete state.
- [ ] Preserve save/load and resampling of the frozen alignment with checked rounding and bounds.
- [ ] Remove latency-specific raw-margin export and consolidate/bake intents, commands, transformations, confirmations, and recovery behavior; keep normal logical audio/MIDI export correct.
- [ ] Ensure snapshots and saves wait for postroll settlement or return one explicit retry/failure result without mixed generations.

Verification:

- [ ] Rewrite app and session tests for track edits, future-operation semantics, per-take manual alignment, round trips, sample-rate conversion, settlement, and logical exports.
- [ ] Delete raw-margin/consolidation/provenance tests and confirm no orphaned API or serialized fields remain.
- [ ] Run app, session archive, resampling, and export test suites, including malformed/out-of-range input cases.

### Stage 6: Replace the advanced UI and documentation

Depends on Stage 5.

- [ ] Replace the component grid, cue selector, frozen-provenance comparison, diagnostic counters/plots, and recovery controls with compact effective-value and per-take alignment controls.
- [ ] Show only actionable automatic/manual capability, pending postroll, and atomic failure feedback.
- [ ] Remove raw-margin export and latency consolidation actions from loop menus.
- [ ] Rewrite user, settings, session-format, port-model, browser, Carla, and troubleshooting documentation to describe only retained behavior and provider support.
- [ ] Update or replace the latency UI smoke example and visual validation assets.

Verification:

- [ ] Run UI unit/smoke tests and inspect native and browser layouts for direct, FX, unsupported/manual, pending, and failure states.
- [ ] Take required screenshots of the perceptible web UI change and verify controls remain usable at supported sizes.
- [ ] Confirm removed terms and workflows no longer appear in user-facing text or documentation.

### Stage 7: Test-suite reconciliation and dead-code audit

Depends on Stages 1–6.

- [ ] Audit every latency-related unit, integration, browser, provider, session, UI, and characterization test against the immutable acceptance criteria.
- [ ] Consolidate oversized matrices to pairwise/boundary coverage while retaining explicit tests for audio/MIDI, callback size, sample rate, loop wrap, latching, postroll, dry/wet exact-once behavior, save/load, export, and atomic failures.
- [ ] Delete fixtures, mocks, snapshots, patches, feature flags, protocol helpers, and test-only APIs used exclusively by removed functionality.
- [ ] Run compiler/dependency analysis and repository searches to remove dead types, fields, conversions, counters, and documentation links.
- [ ] Update tracing coverage only for retained latency operations.

Verification:

- [ ] Run `python3 scripts/check_shoop_test_usage.py` after modifying Rust tests.
- [ ] Run formatting, clippy, dependency checks, and all focused latency suites with warnings denied.
- [ ] Confirm every retained acceptance criterion maps to passing automated coverage and every deleted test maps to an explicitly removed behavior.
- [ ] Confirm the case-insensitive tracked terminology audit returns no matches.

### Stage 8: End-to-end validation and size audit

Depends on all prior stages.

- [ ] Run the complete native and browser test suites using the repository-prescribed commands and `RUSTFLAGS="-D warnings"` where applicable.
- [ ] Validate an end-to-end direct recording with automatic JACK latency, a manual-only backend/browser recording, MIDI recording/playback, save/reopen, sample-rate conversion, and normal exports.
- [ ] Validate a provider/setting change after recording leaves the existing take fixed and affects only the next take.
- [ ] Validate positive and supported negative boundaries, insufficient retention, interrupted postroll, callback/loop boundaries, and supported dry/wet transitions without partial publication or double application.
- [ ] Run realtime allocation/locking checks and verify latency updates remain topology-stable.
- [ ] Run final repository audits for removed APIs, fields, files, documentation, compatibility artifacts, and all case variants of the abandoned term.
- [ ] Compare final path-based additions/deletions and test/production LOC with the Stage 0 baseline; document the achieved reduction and justify any remaining large latency subsystem.
- [ ] Update this plan with checked stages and any approved design-rule revisions, then ensure each stage or meaningful milestone has its own commit.

Final verification:

- [ ] All immutable acceptance criteria are demonstrably satisfied.
- [ ] The working tree is clean, full required CI-equivalent checks pass, user documentation matches the shipped behavior, and the size audit demonstrates a material reduction.
