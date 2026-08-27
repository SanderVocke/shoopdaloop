# Scalar latency compensation reimplementation plan

## Status and execution contract

This document is the implementation contract for reimplementing end-to-end latency compensation with one frozen alignment value per audio or MIDI channel. Implementation starts from current `master`; it does not continue the existing latency implementation branch.

Reference implementation:

- branch: `feature/latency-compensation`
- reference commit: `279308a6345858e85859c51c1da1532a5c227f19`
- pull request: `#797`, "Implement end-to-end latency compensation"

Use the reference implementation for tests, frame-domain behavior, provider research, diagnostics, and troubleshooting. Port production code only after checking that it fits the scalar design below. Do not merge or wholesale cherry-pick its implementation commits.

Execution contract:

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

Implementation workflow:

- Create a new implementation branch from an updated `master`, not from `feature/latency-compensation`.
- Keep the reference commit available locally for `git show`, focused diffs, and test/fixture transfer.
- Transfer tests before implementing their behavior. Minimal API scaffolding may make tests compile, but unsupported behavior must fail explicitly rather than silently returning zero or success.
- Record the expected failing-test inventory after test transfer. Make test groups green in dependency order and rerun already-green lower layers after every stage.
- Do not ignore, weaken, or delete a transferred test merely to make an intermediate stage green. Revise a test only when it depends on forbidden piecewise behavior or when current `master` has changed the surrounding contract.

## Goals

1. Preserve immediate live audio and MIDI monitoring without compensation-induced delay.
2. Align newly recorded direct, dry, and wet material to the logical timeline using one frozen capture-alignment value per channel.
3. Render dry material early enough for delayed processor output to land on its intended wet frame.
4. Keep observations, user policy, frozen take alignment, and current render advance distinct and inspectable.
5. Retain raw ordinary captures and enough bounded pre/post material to apply the scalar alignment non-destructively.
6. Report exact, ranged, estimated, manual-only, changed, and unknown provider states truthfully.
7. Preserve native, browser, session, resampling, realtime, transactional, and import/export contracts.
8. Recover the useful validation coverage from the reference branch without carrying over its piecewise timeline model.
9. Reduce implementation duplication by sharing scalar mapping and conversion logic where boundary constraints permit.

## Scope

### In scope

- Audio and MIDI capture latency for direct, dry, and wet channels.
- Ordinary record/play, prerecord, postroll, grab, planned preplay, dry-through-wet, dry-into-wet, and existing low-level replacement mode.
- Independent external-capture, processor, cue/output, backend-buffering, and manual policy components.
- Frozen per-take observations and changed/incomplete/variable warnings.
- JACK, Carla, OxiSynth, CPAL/midir, dummy, Web Audio, and Web MIDI capability reporting.
- Native and browser/worklet transport.
- Session persistence, exact media, deterministic resampling, logical/raw import and export, UI, settings, diagnostics, and tests.
- Atomic consolidation of a complete channel from its scalar raw mapping to canonical logical content.

### Out of scope

- Multiple alignment regions within one channel.
- Region precedence, overlapping mappings, non-monotonic mappings, or per-region policy editing.
- Segment-wise correction of a grab that spans provider revisions.
- Automatically delaying faster live-monitoring paths to align them with slower live paths.
- Automatic acoustic round-trip calibration.
- A new generic replacement-recording GUI workflow.
- Treating processor tails, SoundFont attack, or musical predelay as transport latency.

## Immutable acceptance criteria

1. **Single alignment invariant.** Every audio and MIDI channel has at most one signed frozen `capture_alignment_frames` value. No runtime, backend, application, wire, persistence, or UI type contains alignment-region data.
2. **Monitoring remains immediate.** Enabling compensation does not add intentional buffering or delay to live monitoring.
3. **Deterministic capture alignment.** A deterministic source delayed by `N` frames plays at its intended logical frame when compensation is enabled and remains delayed by `N` when disabled.
4. **Deterministic wet alignment.** Live wet recording accounts for selected capture, processor, backend-hop, cue, and manual components exactly once.
5. **Dry render-ahead.** Dry audio and MIDI are dispatched early by the selected current processor path so wet output lands on the intended frame across callback and loop boundaries.
6. **No wet double compensation.** Dry-into-wet writes canonical wet timing and leaves no processor contribution to apply during later ordinary wet playback.
7. **Independent policy control.** Each meaningful component can be disabled, manually replaced, or automatically selected with signed trim without changing the detected observation shown to the user.
8. **Frozen takes.** Provider, graph, device, or buffer changes do not retime existing takes. Changes during an operation retain the latched scalar and set a persistent warning.
9. **Complete bounded windows.** Positive alignment retains required postroll; bounded negative alignment retains required prerecord. Insufficient media fails or reports incomplete explicitly.
10. **Scalar grab semantics.** A stable grab uses its observed scalar. A grab spanning revisions uses one documented selected observation for the complete channel, persists `variable=true` and the revision count, and never creates segments.
11. **Scalar replacement semantics.** Existing low-level replacement may proceed when its resolved alignment equals the channel alignment. A differing alignment is rejected before mutation and requires consolidation before retry.
12. **Atomic consolidation.** Consolidation maps each complete audio/MIDI channel to logical coordinates, preserves loop length and MIDI state/order, resets its alignment to zero, and commits all channels or none.
13. **Provider honesty.** JACK and supported Carla runtimes publish measured path information; OxiSynth publishes characterized behavior; unsupported CPAL, browser, or Carla values remain unknown/manual rather than becoming zero.
14. **Persistence and resampling.** Save/load and sample-rate conversion preserve scalar alignment, observations, policy, retained margins, warnings, and logical timing with checked deterministic rounding.
15. **Explicit I/O.** Standard export uses the logical compensated view; raw export is explicit. Imports without metadata use zero applied alignment with unknown provenance, and manual offsets work for empty and non-empty media.
16. **Realtime safety.** Callback work adds no heap allocation, ordinary mutex, I/O, logging, content-sized sorting, or unbounded iteration. Numeric latency changes do not rebuild graph topology.
17. **Transactional safety.** Failed load, driver switch, replacement, consolidation, provider refresh, or content finalization leaves prior usable state intact.
18. **Cross-target validation.** Native, Wasm, browser, dummy, JACK, and Carla tests pass wherever their declared capabilities apply.

## Design rules

### Scalar time model

Use one sign convention throughout:

```text
raw_frame = logical_frame + capture_alignment_frames
processor_dispatch_frame = target_wet_frame - render_advance_frames
```

- `start_offset` remains media-layout geometry and is never reused as latency.
- `capture_alignment_frames` is one signed, frozen value for an entire channel data set.
- `render_advance_frames` is current operation state and is never persisted as another raw-media mapping.
- Component observations and the policy used to derive the scalar remain available as provenance, but playback uses the resolved scalar.
- Ordinary playback uses the frozen scalar only. Current providers affect future operations, not existing takes.
- Checked arithmetic rejects unsupported values; it does not silently clamp them.

### Dynamic operations

- Resolve and latch observations at the operation boundary.
- A provider revision change during record, postroll, render, or grab marks the operation changed/variable but does not alter its mapping.
- A revision-spanning grab selects the newest fully available observation unless evidence supports a different documented rule. The complete grabbed channel uses that one scalar.
- A latency-aware replacement is preflighted before mode change. Differing old/new scalars produce a consolidation-required error without mutation.
- Dry-into-wet is a canonical render operation, not a source of mixed mappings.

### Architecture

- Keep authoritative policy resolution in a low-level Wasm-compatible domain crate.
- Share one scalar raw/logical mapping implementation among audio, MIDI, export, import, consolidation, and test oracles.
- Distinct realtime, wire, and persistence representations are allowed where atomicity, bounded encoding, or compatibility requires them. Use `From`/`TryFrom`, shared helpers, or generated mapping to avoid repeated handwritten semantics.
- Prepare policy, storage, commands, and provider snapshots off the callback thread.
- Callback-facing values use fixed-capacity or atomic publication with bounded work.
- Unsupported provider behavior must remain explicit in API results and diagnostics.
- Keep latency code in focused modules rather than extending already-large general `lib.rs` files where practical.

## Test transfer policy

The reference branch adds roughly 136 Rust test functions and 8,800 Rust test/support lines. Preserve tests by behavior, not blindly by source text.

### Transfer substantially unchanged

- Shared observation, policy, range, overlap, and arithmetic tests.
- Deterministic monitoring, record/play, grab, preplay, dry-through-wet, and dry-into-wet matrices.
- Audio/MIDI retained-window, state, ordering, and no-allocation tests.
- Provider tests for JACK, Carla, OxiSynth, dummy, CPAL/browser capability fallback, and dynamic revisions.
- Backend/application policy, protocol, persistence, UI, diagnostics, stress, and cross-target tests.
- Review regressions concerning production wiring, transactionality, MIDI ordering, target-channel selection, media offsets, and atomic multi-channel updates where they remain meaningful with one scalar.

### Do not transfer

- `piecewise_alignment_regions_select_the_newest_matching_raw_mapping`
- `piecewise_alignment_regions_select_the_newest_matching_midi_mapping`
- `piecewise_state_restore_ignores_raw_earlier_logical_future_events`

Do not transfer helpers or assertions for region capacity, region ordering, region revision precedence, shadowed events, or non-monotonic mapping.

### Transfer with scalar fixtures

Rewrite these tests to retain their non-region purpose:

- Logical/raw audio export uses one scalar alignment.
- Logical/raw MIDI export verifies scalar timing, boundary state, and equal-frame order.
- Standard import verifies unknown defaults and one manual scalar offset, including empty media.
- Mixed audio/MIDI consolidation bakes each channel's scalar and commits atomically.
- Session latency documents round-trip and reject malformed scalar observations and bounds.
- Same-rate and cross-rate replay preserve scalar timing.
- Collapsed range resampling preserves truthful range certainty without region metadata.

### Add missing regressions

- Successful consolidation invalidates cached waveform and MIDI detail data before refresh.
- A manual import offset on zero-duration audio or MIDI is valid and creates no interval-like metadata.
- Repository/API checks prove that forbidden alignment-region types and fields are absent.

## Staged implementation

### Stage 0 — Establish the clean baseline and reference inventory

Dependencies: none.

- [ ] Create the implementation branch from an updated `master` and record its base commit.
- [ ] Record the immutable reference commit and PR review findings used during transfer.
- [ ] Run the current native and portable baseline suites before adding latency code.
- [ ] Inventory every reference test, helper, fixture, provider patch, protocol field, and persistence field to transfer, rewrite, or omit under the policy above.
- [ ] Record baseline behavior for monitoring, ordinary recording/playback, grab, preplay, dry/wet modes, and low-level replacement where current tests do not already pin it.

Verification:

- [ ] Baseline tests pass on the implementation branch before feature changes.
- [ ] The transfer inventory accounts for all reference latency tests and the two missing regressions.
- [ ] No production commit from the reference branch has been wholesale cherry-picked.

### Stage 1 — Transfer tests, fixtures, and compile-time API contracts

Dependencies: Stage 0.

- [ ] Transfer the deterministic latency harness and applicable tests before implementing behavior.
- [ ] Transfer inline tests in coherent subsystem groups, resolving current-`master` conflicts by preserving test intent.
- [ ] Remove the three piecewise-only tests and convert the seven region-bearing fixtures to scalar fixtures.
- [ ] Add the cache-invalidation, empty-import, and no-region architecture regressions.
- [ ] Add minimal scalar domain/backend/application/wire/session API shapes needed for compilation.
- [ ] Stub unavailable behavior with explicit unsupported or unresolved results; do not return invented zero observations or false success.
- [ ] Make the workspace build and all transferred tests compile.
- [ ] Record the expected failing tests by subsystem and expected frame/result.

Verification:

- [ ] `cargo check --workspace` succeeds.
- [ ] Test discovery contains the expected transferred tests, with only documented region tests omitted.
- [ ] Failures are behavioral assertions or explicit unsupported results, not compilation failures, panics from missing scaffolding, or hangs.
- [ ] No `AlignmentRegion`, `alignment_regions`, piecewise latency map, or equivalent API exists.

### Stage 2 — Implement shared latency observations and scalar policy resolution

Dependencies: Stage 1 API contracts.

- [ ] Implement checked exact/range/estimated/manual-only/unknown observations.
- [ ] Implement component kinds, policy modes, range selection, source/path identity, revisions, and overlap prevention.
- [ ] Resolve direct/dry/wet record, dry-through-wet, dry-into-wet, grab, and replacement recipes to one checked scalar total.
- [ ] Encode conditional cue/output semantics in the resolver.
- [ ] Implement frozen take snapshots and changed/incomplete/variable status without region fields.
- [ ] Keep shared types Wasm-compatible and independent of backend/UI representations.

Verification:

- [ ] All shared-domain and scalar recipe tests pass on native and Wasm-compatible targets.
- [ ] Unknown automatic values remain unresolved; disabled unknown values contribute zero explicitly.
- [ ] Component toggles, trims, bounds, overlap rejection, and range selection have direct tests.

### Stage 3 — Add provider observations and callback-safe scalar contracts

Dependencies: Stage 2.

- [ ] Add per-port and per-processor latency observations with coherent revisions.
- [ ] Add callback-readable atomic/fixed-size publication for observations and one latched recipe per operation.
- [ ] Port the deterministic delayed audio/MIDI processor fixture.
- [ ] Publish current and latched scalar state through engine mirrors.
- [ ] Ensure numeric updates do not trigger graph topology rebuilds.

Verification:

- [ ] Observation publication and latching tests pass.
- [ ] Dynamic changes increment revisions and mark active operations without retiming them.
- [ ] Publication and deterministic processor paths pass no-allocation/no-lock checks.

### Stage 4 — Implement scalar capture windows and ordinary playback

Dependencies: Stages 2–3.

- [ ] Separate media layout, frozen capture alignment, and current render advance in audio and MIDI channels.
- [ ] Centralize checked scalar raw/logical mapping and use it for both channel types.
- [ ] Reserve bounded prerecord/postroll storage before arming.
- [ ] Continue finalization until required postroll is available while keeping content mutations unsettled transactionally.
- [ ] Map ordinary playback through the single scalar across callbacks and loop wrap.
- [ ] Preserve MIDI start state, equal-frame order, and events crossing retained boundaries.
- [ ] Implement deterministic readiness/defer behavior for play-after-record and advances at or above one loop.

Verification:

- [ ] Ordinary record/play matrices pass for direct, dry, and wet audio/MIDI.
- [ ] Positive, zero, and bounded negative alignments select exact expected raw frames.
- [ ] Final events survive postroll; prerecord material supports negative alignment.
- [ ] Start/stop/restart, callback crossings, loop wrap, and play-after-record tests pass.
- [ ] Armed record/finalization remains allocation-free and lock-free.

### Stage 5 — Implement scalar dry render-ahead and wet rerecording

Dependencies: Stage 4 and delayed processor fixture.

- [ ] Apply current processor render advance independently of frozen capture alignment.
- [ ] Start planned dry-through-wet dispatch early enough for exact target-frame output.
- [ ] Implement explicit defer/warn behavior for immediate transitions lacking lead time.
- [ ] Restore and clean MIDI state across early dispatch, wrap, stop, and latency changes.
- [ ] Implement dry-into-wet canonical writes and `applied_during_render` provenance with no remaining processor playback contribution.
- [ ] Keep live monitoring on the uncompensated shortest path.

Verification:

- [ ] Planned-preplay, dry-through-wet, and dry-into-wet matrices pass for audio and MIDI.
- [ ] Exact processor delay lands on target at start, steady state, wrap, stop, and restart.
- [ ] Wet rerecord followed by ordinary playback proves no double compensation.
- [ ] Monitoring equivalence remains sample-for-sample unchanged.

### Stage 6 — Implement scalar grab, replacement, and consolidation

Dependencies: Stages 4–5.

- [ ] Retain bounded latency-observation history alongside input ring history.
- [ ] Use one stable observation for stable grabs.
- [ ] For revision-spanning grabs, select the documented newest observation for the complete channel and persist variable/revision warnings.
- [ ] Preflight low-level replacement before entering replacement mode.
- [ ] Permit replacement only when the resolved incoming alignment equals the existing channel scalar.
- [ ] Reject differing replacement alignment before mutation with a consolidation-required error.
- [ ] Consolidate complete audio and MIDI channels into logical coordinates, reset scalar alignment to zero, and commit all channels atomically.
- [ ] Preserve undo/content snapshot behavior and invalidate application media caches after consolidation.

Verification:

- [ ] Stable and variable grab tests pass without producing segment metadata.
- [ ] Insufficient history fails before target mutation.
- [ ] Compatible replacement preserves one scalar; incompatible replacement leaves mode and content unchanged.
- [ ] Mixed audio/MIDI consolidation preserves samples, event ordering, start state, and loop length atomically.
- [ ] Consolidated waveform/MIDI detail data is refreshed rather than served from stale caches.

### Stage 7 — Integrate backend and application policy

Dependencies: Stages 2–6.

- [ ] Extend backend snapshots and commands with per-port observations, track policy, one channel/take scalar, warnings, and consolidation.
- [ ] Resolve policy off the realtime thread and transfer bounded prepared recipes to callbacks.
- [ ] Integrate native, dummy/test, and application model paths without duplicating recipe semantics.
- [ ] Reconcile optimistic UI/application edits against authoritative callback-latched state.
- [ ] Keep multi-channel take edits and content operations transactional.

Verification:

- [ ] Fake, engine, and native backend contract tests pass.
- [ ] Settings affect future operations but not frozen takes.
- [ ] Unsupported, pending, accepted, latched, changed, and failed policy updates are distinguishable.
- [ ] Driver/processor changes warn without silently moving existing content.

### Stage 8 — Implement native provider support

Dependencies: Stages 3 and 7.

- [ ] Reuse the reference branch's provider characterization evidence, revalidating it against current dependencies.
- [ ] Implement JACK capture/playback observation and latency callback propagation through fixed-capacity callback-safe route snapshots.
- [ ] Account for verified external send/return callback-cycle latency as a separately identified component.
- [ ] Implement the version-gated Carla Rack/Patchbay adapter and in-process/subprocess publication without importing region concepts.
- [ ] Publish OxiSynth's characterized phase-dependent event latency range consistently on native and Wasm.
- [ ] Report CPAL/midir values only where their APIs provide defensible semantics; otherwise use estimated/manual/unknown states.

Verification:

- [ ] JACK range, route filtering, external-hop, graph-change, and port-retirement tests pass on a real software server.
- [ ] Carla zero/nonzero Rack, branched Patchbay, Patchbay16, unsupported-runtime, and worker-restart tests pass where facilities exist.
- [ ] OxiSynth offset/event/callback characterization and compensation tests pass.
- [ ] Provider callbacks satisfy realtime allocation/lock constraints.

### Stage 9 — Carry scalar latency through browser and protocols

Dependencies: Stages 2, 7, and provider semantics needed by the worklet.

- [ ] Add bounded scalar policy, observation, frozen-take, warning, and error records to audio worklet/client protocols.
- [ ] Bump and validate protocol versions and message capacities.
- [ ] Publish supported Web Audio output properties and keep unavailable capture/output values unknown/manual.
- [ ] Preserve Web MIDI's existing coarse timing claim.
- [ ] Transfer waveform/MIDI detail latency metadata as one scalar and reject inconsistent chunks.
- [ ] Preserve frozen takes across browser device restart or permission changes.

Verification:

- [ ] Wire round-trip, capacity, stale-generation, missing-property, restart, and media-detail tests pass.
- [ ] Native and worklet backends expose equivalent scalar behavior where capabilities overlap.
- [ ] Node and browser Wasm suites pass for completed packages.

### Stage 10 — Implement persistence, resampling, import, and export

Dependencies: Stages 6–9 define the complete persisted state.

- [ ] Add versioned session/exact-media documents for observations, policy, one scalar alignment, margins, operation provenance, and warnings.
- [ ] Migrate older sessions to explicit zero-applied/unknown provenance.
- [ ] Validate all bounds and relationships before backend mutation.
- [ ] Resample unsigned observations/margins and signed alignment/trims with documented checked rounding.
- [ ] Export logical audio/MIDI through the shared scalar mapping and preserve equal-frame MIDI order and boundary state.
- [ ] Keep raw export explicit and metadata-preserving.
- [ ] Import standard media with zero-applied/unknown provenance and optional manual scalar offset, including zero-duration media.
- [ ] Preserve or intentionally reset scalar provenance across duplicate, clone, composite, and session-switch flows.

Verification:

- [ ] Same-rate save/load preserves raw bytes/events, scalar metadata, and replayed logical timing.
- [ ] Cross-rate replay follows documented rounding without changing component identity.
- [ ] Malformed, overflowing, and future-version data fails transactionally.
- [ ] Logical/raw audio and MIDI exports have exact expected content and do not mutate the take.
- [ ] Empty and non-empty manual-offset imports pass without interval metadata.

### Stage 11 — Implement UI, settings, and diagnostics

Dependencies: Stages 7 and 10.

- [ ] Register defaults for future operations without retiming frozen takes.
- [ ] Add component controls, range selection, signed trim, cue selection, scalar totals, and current/frozen comparison.
- [ ] Display raw/logical positions, retained bounds, exact/range/estimated/manual/unknown state, and changed/incomplete/variable warnings.
- [ ] Add consolidation and explicit raw-export controls.
- [ ] Expose bounded counters/plots for unresolved recipes, changes, insufficient margins, deferred transitions, finalization, ambiguity, and provider failures.
- [ ] Keep controls usable with keyboard and touch without hover-only requirements.

Verification:

- [ ] UI policy, warning, reconciliation, no-backend, cue, consolidation, cache-refresh, touch, and layout tests pass.
- [ ] Settings save/cancel/reset/migration behavior passes.
- [ ] Diagnostics identify actionable failures without realtime logging.
- [ ] Manual usability fixtures cover Direct, External, Carla, and Built-in Synth tracks.

### Stage 12 — Simplification and realtime audit

Dependencies: all runtime stages.

- [ ] Inventory latency representations and remove unnecessary conversion duplication.
- [ ] Confirm audio, MIDI, export, and consolidation use the same scalar mapping semantics.
- [ ] Confirm fake/native/worklet backends share policy resolution rather than reimplementing it.
- [ ] Audit callback paths for content-sized scans, sorting, allocation, locks, logging, and unbounded sub-block loops.
- [ ] Run maximum-value, rapid-policy, graph-churn, processor-change, driver-switch, loop-transition, and session-save stress tests.
- [ ] Remove scaffolding stubs and temporary expected-failure inventories.
- [ ] Verify forbidden region symbols and equivalent structures are absent from production and persistence APIs.

Verification:

- [ ] All transferred tests are green; no test remains ignored or weakened for implementation convenience.
- [ ] No callback work scales with complete take content because of latency compensation.
- [ ] Memory and callback work remain bounded at documented maxima.
- [ ] `git diff --check` passes and unrelated changes are absent.

### Stage 13 — Documentation and final end-to-end validation

Dependencies: all prior stages.

- [ ] Document the scalar sign convention, component meanings, cue/output condition, provider certainty, retained media, grab warning, replacement/consolidation rule, and dry/wet behavior.
- [ ] Update session, settings, port, Web MIDI, worklet, Carla, and troubleshooting contracts.
- [ ] Retain provider measurements and facility limitations as evidence rather than unsupported claims.
- [ ] Map every immutable acceptance criterion to direct tests or recorded provider validation.
- [ ] Compare the new branch's ordinary scalar frame oracles with the reference implementation and document intentional differences for variable grab and replacement.

Final verification:

- [ ] `cargo fmt --all -- --check`
- [ ] `RUSTFLAGS="-D warnings" cargo build --workspace`
- [ ] `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend --profile ci`
- [ ] `python3 scripts/check_shoop_test_usage.py`
- [ ] `python3 scripts/check_tracing_coverage.py --require-closed`
- [ ] Build the application and audio worklet for `wasm32-unknown-unknown`.
- [ ] Run the complete shared Wasm suite in Node and a supported browser.
- [ ] Run documented Chromium and Firefox AudioWorklet smoke tests where available.
- [ ] Run JACK tests without missing-backend allowance against a real JACK server.
- [ ] Run real Carla Rack, Patchbay, Patchbay16, and subprocess tests in the pinned environment.
- [ ] Run deterministic 44.1/48 kHz and 64/127-frame callback matrices for direct, dry, wet, grab, render-ahead, dry-into-wet, provider changes, save/load, resampling, and logical/raw export.
- [ ] Perform the manual latency-panel usability pass.
- [ ] Confirm no alignment-region symbols or serialized fields exist.
- [ ] Confirm all goals and immutable acceptance criteria have direct evidence.

## Completion definition

The reimplementation is complete when all transferred scalar-relevant tests and final gates pass, every channel uses exactly one frozen alignment value, provider uncertainty remains truthful, monitoring remains immediate, persisted takes reproduce their logical timing, variable grabs remain visibly scalar/inexact, incompatible replacement is transactional, and no piecewise mapping model exists in runtime, protocols, persistence, or UI.
